use crate::types::*;
use crate::util::*;
use anyhow::{Context, Result};
use gateway_client::{GatewayConfig, NetworkState, Traffic, TrafficInfo};
use ipnet::{IpNet, Ipv4Net};
use lazy_static::lazy_static;
use log::*;
use rocket::futures::TryStreamExt;
use sqlx::{query_as, SqlitePool};
use std::collections::HashSet;
use std::net::Ipv4Addr;
use std::path::Path;
use tera::Tera;
use wireguard_keys::Pubkey;

const BRIDGE_INTERFACE: &'static str = "ensbr0";
const NGINX_MODULE_PATH: &'static str = "/etc/nginx/modules-enabled/gateway.conf";
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
}

/// Given a new state, do whatever needs to be done to get the system in that
/// state.
pub async fn apply(config: &GatewayConfig) -> Result<String> {
    info!("Applying new state");

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

    // for the rest, apply config
    for network in &state {
        apply_network(network).await.context("Applying network")?;
    }

    apply_nginx(&state)
        .await
        .context("Applying nginx configuration")?;

    Ok("success".to_string())
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
pub async fn apply_network(network: &NetworkState) -> Result<()> {
    apply_netns(network).await?;
    apply_wireguard(network).await?;
    apply_veth(network).await?;
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
        wireguard_create(&netns, &wgif).await?;
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
    if interface_down(netns, interface).await? {
        interface_set_up(netns, interface).await?;
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

/// Apply the forwarding configuration by writing out an iptables state and restoring it.
pub async fn apply_forwarding(network: &NetworkState) -> Result<()> {
    let netns = network.netns_name();
    let config = network.port_config();
    let context = tera::Context::from_serialize(&config)?;
    let savefile = TERA_TEMPLATES.render("iptables.save", &context)?;
    iptables_restore(Some(&netns), &savefile).await?;

    Ok(())
}

/// Apply an nginx configuration by writing out config files and restarting nginx.
pub async fn apply_nginx(networks: &[NetworkState]) -> Result<()> {
    let mut forwarding = Forwarding::new();
    for network in networks {
        forwarding.add(network);
    }
    let context = tera::Context::from_serialize(&forwarding)?;
    let config = TERA_TEMPLATES.render("nginx.conf", &context)?;
    tokio::fs::write(Path::new(NGINX_MODULE_PATH), config.as_bytes()).await?;

    let config = TERA_TEMPLATES.render("sites.nginx.conf", &context)?;
    tokio::fs::write(Path::new(NGINX_SITE_PATH), config.as_bytes()).await?;

    nginx_reload().await?;
    Ok(())
}

/// Grab traffic data from the database.
pub async fn traffic(pool: &SqlitePool, start_time: usize) -> Result<TrafficInfo> {
    let mut traffic_info = TrafficInfo::new(start_time);
    let mut rows = query_as::<_, (Vec<u8>, Vec<u8>, i64, i64, i64)>(
        "SELECT network_pubkey, device_pubkey, traffic_rx, traffic_tx, time
            FROM gateway_traffic
            JOIN gateway_network ON gateway_network.network_id = gateway_traffic.network_id
            JOIN gateway_device ON gateway_device.device_id = gateway_traffic.device_id
            WHERE time > ?",
    )
    .bind(start_time as i64)
    .fetch(pool);

    while let Some((network, device, rx, tx, time)) = rows.try_next().await? {
        let traffic = Traffic::new(rx as usize, tx as usize);
        let time = time as usize;
        let network = Pubkey::try_from(&network[..])?;
        let device = Pubkey::try_from(&device[..])?;
        traffic_info.add(network, device, time, traffic);
    }

    Ok(traffic_info)
}
