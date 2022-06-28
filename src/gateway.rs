use crate::types::*;
use crate::Global;
use crate::Options;
use anyhow::anyhow;
use anyhow::{Context, Result};
use fractal_gateway_client::{GatewayConfig, GatewayConfigPartial, NetworkState};
use ipnet::{IpNet, Ipv4Net};
use lazy_static::lazy_static;
use log::*;
use networking_wrappers::*;
use regex::Regex;
use std::borrow::Cow;
use std::collections::HashSet;
use std::net::Ipv4Addr;
use std::path::Path;
use tera::Tera;

/// Name of the bride network interface to use
const BRIDGE_INTERFACE: &'static str = "ensbr0";

/// Path of the NGINX modules configuration
const NGINX_MODULE_PATH: &'static str = "/etc/nginx/modules-enabled/gateway.conf";

/// Path of the NGINX site configuration
const NGINX_SITE_PATH: &'static str = "/etc/nginx/sites-enabled/gateway.conf";

lazy_static! {
    pub static ref BRIDGE_NET: Ipv4Net = Ipv4Net::new(Ipv4Addr::new(172, 99, 0, 1), 16).unwrap();
    pub static ref TERA_TEMPLATES: Tera = {
        let mut tera = Tera::default();
        tera.add_raw_templates([
            (
                "iptables.save",
                include_str!("../templates/iptables.save.tera"),
            ),
            ("nginx.conf", include_str!("../templates/nginx.conf.tera")),
            (
                "sites.nginx.conf",
                include_str!("../templates/sites.nginx.conf.tera"),
            ),
        ])
        .unwrap();
        tera
    };
    pub static ref IPTABLES_PACKET_COUNTER_REGEX: Regex = Regex::new(r"\[\d+:\d+\]$").unwrap();
}

/// Called on a fresh start, initialize NGINX config if needed.
pub async fn startup(options: &Options) -> Result<()> {
    let module_path = Path::new(NGINX_MODULE_PATH);
    if !module_path.is_file() {
        for (url, socket) in &options.custom_forwarding {
            info!("Custom forwarding: {} => {:?}", url.to_string(), socket);
        }
        apply_nginx(&[], options).await?;
    }

    Ok(())
}

/// Given a new state, do whatever needs to be done to get the system in that
/// state.
pub async fn apply(global: &Global, config: &GatewayConfig) -> Result<()> {
    info!("Applying new state");
    let mut state = global.lock().lock().await;
    *state = config.clone();

    // turn config into list of network states
    let state: Vec<NetworkState> = config
        .iter()
        .map(|(port, state)| {
            let mut state = state.clone();
            state.listen_port = *port;
            state
        })
        .collect();

    // set up bridge
    apply_bridge(BRIDGE_INTERFACE, &vec![(*BRIDGE_NET).into()])
        .await
        .context("Creating bridge interface")?;

    // find out which netns exist right now
    let netns_list: HashSet<String> = netns_list()
        .await?
        .into_iter()
        .map(|netns| netns.name)
        .collect();

    // find out which we are expecting to exist
    let netns_expected: HashSet<String> =
        state.iter().map(|network| network.netns_name()).collect();

    // ones that exist but shouldn't, we delete them.
    for netns in netns_list.difference(&netns_expected) {
        if netns.starts_with(NETNS_PREFIX) {
            netns_del(&netns)
                .await
                .context("Removing surplus network namespace")?;
        }
    }

    for network in &state {
        apply_network(global, network).await?;
    }

    apply_nginx(&state, global.options())
        .await
        .context("Applying nginx configuration")?;

    Ok(())
}

/// Apply a partial config, this is only a diff.
pub async fn apply_partial(global: &Global, config: &GatewayConfigPartial) -> Result<()> {
    info!("Applying new partial state");
    let mut state = global.lock().lock().await;

    // set up bridge
    apply_bridge(BRIDGE_INTERFACE, &vec![(*BRIDGE_NET).into()])
        .await
        .context("Creating bridge interface")?;

    // find out which netns exist right now
    let netns_list: HashSet<String> = netns_list()
        .await?
        .into_iter()
        .map(|netns| netns.name)
        .collect();

    for (port, config) in config.iter() {
        match config {
            None => {
                state.remove(port);
                let netns = format!("{NETNS_PREFIX}{port}");
                if netns_list.contains(&netns) {
                    netns_del(&netns).await?;
                }
            }
            Some(network) => {
                apply_network(global, network).await?;
                state.insert(*port, network.clone());
            }
        }
    }

    let networks: Vec<_> = state.iter().map(|(_port, state)| state.clone()).collect();

    apply_nginx(&networks, global.options())
        .await
        .context("Applying nginx configuration")?;

    Ok(())
}

/// Make sure the bridge interface exists, is up and has a certain address
/// set up.
pub async fn apply_bridge(_name: &str, addr: &[IpNet]) -> Result<()> {
    if !bridge_exists(None, BRIDGE_INTERFACE).await? {
        bridge_add(None, BRIDGE_INTERFACE).await?;
    }

    apply_addr(None, BRIDGE_INTERFACE, &addr)
        .await
        .context("Setting up bridge interface")?;

    apply_interface_up(None, BRIDGE_INTERFACE)
        .await
        .context("Bringing bridge interface up")?;

    Ok(())
}

/// Apply a given network state.
pub async fn apply_network(global: &Global, network: &NetworkState) -> Result<()> {
    apply_netns(network).await?;
    apply_wireguard(network).await?;
    apply_veth(network).await?;

    let _lock = global.iptables_lock().lock().await;
    apply_forwarding(network).await?;
    Ok(())
}

/// Given a network state, make sure the network namespace associated with it exists.
pub async fn apply_netns(network: &NetworkState) -> Result<()> {
    let netns = network.netns_name();

    // make sure that netns exists
    if !netns_exists(&netns).await? {
        netns_add(&netns).await?;
    }

    Ok(())
}

/// Apply the wireguard configuration associated with a network state.
pub async fn apply_wireguard(network: &NetworkState) -> Result<()> {
    let netns = network.netns_name();
    let wgif = network.wgif_name();

    // make sure that the wireguard interface works
    if !wireguard_exists(&netns, &wgif).await? {
        info!("Wireguard network does not exist");
        // create wireguard config in netns
        wireguard_create(Some(&netns), &wgif).await?;
    }

    let show = interface_show(Some(&netns), &wgif).await?;
    let mtu = show
        .mtu
        .ok_or(anyhow!("Missing MTU for WireGuard network"))?;
    if mtu != network.mtu {
        interface_mtu(Some(&netns), &wgif, network.mtu).await?;
    }

    apply_interface_up(Some(&netns), &wgif)
        .await
        .context("Setting wireguard interface UP")?;

    // write wireguard config
    netns_write_file(
        &netns,
        Path::new(&format!("wireguard/{}.conf", &wgif)),
        &network.to_config(),
    )
    .await?;

    // set wireguard interface addresses to allow kernel ingress traffic
    apply_addr(Some(&netns), &wgif, &network.address)
        .await
        .context("Applying wireguard interface addresses")?;

    // sync config of wireguard netns
    wireguard_syncconf(&netns, &wgif).await?;

    Ok(())
}

/// Given an interface and a network namespace, apply the address.
pub async fn apply_addr(netns: Option<&str>, interface: &str, target: &[IpNet]) -> Result<()> {
    // FIXME: this will not remove addresses.
    let current = addr_list(netns, interface).await?;
    for addr in target {
        if !current.contains(addr) {
            addr_add(netns, interface, *addr).await?;
        }
    }
    Ok(())
}

/// Make sure that an interface in a given network namespace (or in the root
/// namespace if none is supplied) is not DOWN.
pub async fn apply_interface_up(netns: Option<&str>, interface: &str) -> Result<()> {
    let status = interface_show(netns, interface).await?;
    if status.is_down() {
        interface_up(netns, interface).await?;
    }
    Ok(())
}

/// Given a network state, apply the veth configuration by creating the veth pair.
pub async fn apply_veth(network: &NetworkState) -> Result<()> {
    let netns = network.netns_name();

    // create veth pair
    let veth_name = network.veth_name();
    if !veth_exists(&netns, &veth_name).await? {
        veth_add(&netns, &veth_name, &veth_name).await?;
    }

    // make sure veth interfaces have addresses set
    let addr: Ipv4Net = network.veth_ipv4net().into();
    let addr: IpNet = addr.into();
    let addr = vec![addr];
    apply_addr(Some(&netns), &veth_name, &addr)
        .await
        .context("Applying veth addr")?;
    //apply_addr(None, &veth_name, &addr).await
    //    .context("Applying veth addr")?;
    apply_link_master(None, &veth_name, BRIDGE_INTERFACE)
        .await
        .context("Setting veth master")?;

    // make sure inner veth is up
    apply_interface_up(Some(&netns), &veth_name)
        .await
        .context("Making inner veth interface UP")?;
    apply_interface_up(None, &veth_name)
        .await
        .context("Marking outer veth interface UP")?;

    Ok(())
}

pub async fn apply_link_master(netns: Option<&str>, interface: &str, master: &str) -> Result<()> {
    let current = link_get_master(netns, interface).await?;
    if current.is_none() || current.as_deref() != Some(master) {
        link_set_master(netns, interface, master)
            .await
            .context("Setting master of interface")?;
    }
    Ok(())
}

/// Clean iptables save file
fn clean_iptables(input: &str) -> String {
    let mut cleaned: String = input
        .lines()
        // filter comments
        .filter(|line| line.chars().next() != Some('#'))
        // filter empty lines
        .filter(|line| line.chars().next() != None)
        .map(|line| IPTABLES_PACKET_COUNTER_REGEX.replace(line, "[0:0]"))
        .collect::<Vec<Cow<'_, str>>>()
        .join("\n");
    cleaned.push('\n');
    cleaned
}

/// Apply the forwarding configuration by writing out an iptables state and restoring it.
pub async fn apply_forwarding(network: &NetworkState) -> Result<()> {
    let netns = network.netns_name();
    let config = network.port_config();
    let context = tera::Context::from_serialize(&config)?;
    let savefile = TERA_TEMPLATES.render("iptables.save", &context)?;
    let savefile = clean_iptables(&savefile);
    let current = iptables_save(Some(&netns)).await?;
    let current = clean_iptables(&current);

    if savefile != current {
        iptables_restore(Some(&netns), &savefile).await?;
    }

    Ok(())
}

/// Apply an nginx configuration by writing out config files and restarting nginx.
pub async fn apply_nginx(networks: &[NetworkState], options: &Options) -> Result<()> {
    let mut forwarding = Forwarding::new();
    for network in networks {
        forwarding.add(network);
    }

    // add custom forwarding from command-line options
    for (url, socket) in &options.custom_forwarding {
        forwarding.add_custom(url, *socket);
    }

    // fill NGINX template
    let context = tera::Context::from_serialize(&forwarding)?;
    let config = TERA_TEMPLATES.render("nginx.conf", &context)?;
    tokio::fs::write(Path::new(NGINX_MODULE_PATH), config.as_bytes()).await?;

    let config = TERA_TEMPLATES.render("sites.nginx.conf", &context)?;
    tokio::fs::write(Path::new(NGINX_SITE_PATH), config.as_bytes()).await?;

    nginx_reload().await?;

    Ok(())
}
