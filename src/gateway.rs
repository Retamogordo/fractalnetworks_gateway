use crate::types::*;
use crate::util::*;
use anyhow::{Context, Result};
use log::*;
use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;
use tokio::sync::RwLock;
use std::sync::Arc;
use std::collections::BTreeMap;
use sqlx::{query, query_as, SqlitePool};
use ipnet::{IpNet, Ipv4Net};
use std::net::{IpAddr, Ipv4Addr};
use lazy_static::lazy_static;

const WIREGUARD_INTERFACE: &'static str = "ens0";
const BRIDGE_INTERFACE: &'static str = "ensbr0";
lazy_static! {
    static ref BRIDGE_NET: IpNet = IpNet::V4(Ipv4Net::new(Ipv4Addr::new(172, 99, 0, 0), 16).unwrap());
}

/// Given a new state, do whatever needs to be done to get the system in that
/// state.
pub async fn apply(state: &[NetworkState]) -> Result<String> {
    info!("Applying new state");

    // set up bridge
    apply_bridge(BRIDGE_INTERFACE, &vec![BRIDGE_NET.clone()]).await
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
            netns_del(&netns).await?;
        }
    }

    // for the rest, apply config
    for network in state {
        apply_network(network).await?;
    }

    Ok("success".to_string())
}

/// Make sure the bridge interface exists, is up and has a certain address
/// set up.
pub async fn apply_bridge(name: &str, addr: &[IpNet]) -> Result<()> {
    if !bridge_exists(None, BRIDGE_INTERFACE).await? {
        bridge_add(None, BRIDGE_INTERFACE).await?;
    }

    apply_addr(None, BRIDGE_INTERFACE, &addr).await
        .context("Setting up bridge interface")?;

    apply_interface_up(None, BRIDGE_INTERFACE).await
        .context("Bringing bridge interface up")?;

    Ok(())
}

pub async fn apply_network(network: &NetworkState) -> Result<()> {
    apply_netns(network).await?;
    apply_wireguard(network).await?;
    apply_veth(network).await?;
    apply_forwarding(network).await?;
    Ok(())
}

pub async fn apply_netns(network: &NetworkState) -> Result<()> {
    let netns = network.netns_name();

    // make sure that netns exists
    if !netns_exists(&netns).await? {
        netns_add(&netns).await?;
    }
    Ok(())
}

pub async fn apply_wireguard(network: &NetworkState) -> Result<()> {
    let netns = network.netns_name();

    // make sure that the wireguard interface works
    if !wireguard_exists(&netns, WIREGUARD_INTERFACE).await? {
        info!("Wireguard network does not exist");
        // create wireguard config in netns
        wireguard_create(&netns, WIREGUARD_INTERFACE).await?;
    }

    apply_interface_up(Some(&netns), WIREGUARD_INTERFACE).await
        .context("Setting wireguard interface UP")?;

    // write wireguard config
    netns_write_file(
        &netns,
        Path::new("wireguard/ens0.conf"),
        &network.to_config(),
    )
    .await?;

    // set wireguard interface addresses to allow kernel ingress traffic
    apply_addr(Some(&netns), WIREGUARD_INTERFACE, &network.address).await
        .context("Applying wireguard interface addresses")?;

    // sync config of wireguard netns
    wireguard_syncconf(&netns, WIREGUARD_INTERFACE).await?;

    // fetch stats to make sure interface is really up
    let stats = wireguard_stats(&netns, WIREGUARD_INTERFACE).await?;

    Ok(())
}

pub async fn apply_addr(netns: Option<&str>, interface: &str, target: &[IpNet]) -> Result<()> {
    let current = addr_list(netns, interface).await?;
    for addr in target {
        if !current.contains(addr) {
            addr_add(netns, interface, &addr.to_string()).await?;
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

pub async fn apply_veth(network: &NetworkState) -> Result<()> {
    let netns = network.netns_name();

    // create veth pair
    let veth_name = network.veth_name();
    if !veth_exists(&netns, &veth_name).await? {
        veth_add(&netns, &veth_name, &veth_name).await?;
    }

    // make sure veth interfaces have addresses set
    let addr: IpAddr = network.veth_ipv4().into();
    let addr: IpNet = addr.into();
    let addr = vec![addr];
    info!("addresses {:?}", addr);
    apply_addr(Some(&netns), &veth_name, &addr).await
        .context("Applying veth addr")?;
    apply_addr(None, &veth_name, &addr).await
        .context("Applying veth addr")?;

    // make sure inner veth is up
    apply_interface_up(Some(&netns), &veth_name).await
        .context("Making inner veth interface UP")?;
    apply_interface_up(None, &veth_name).await
        .context("Marking outer veth interface UP")?;

    Ok(())
}

pub async fn apply_forwarding(network: &NetworkState) -> Result<()> {
    Ok(())
}

/// Start watchdog process that repeatedly checks the state of the system, with
/// a configurable interval.
pub async fn watchdog(pool: SqlitePool, duration: Duration) -> Result<()> {
    info!("Launching watchdog every {}s", duration.as_secs());
    let mut interval = tokio::time::interval(duration);
    loop {
        interval.tick().await;
        watchdog_run(&pool).await?;
    }
    Ok(())
}

pub async fn watchdog_run(pool: &SqlitePool) -> Result<()> {
    info!("Running watchdog");
    let netns_items = netns_list().await?;
    for netns in &netns_items {
        if netns.name.starts_with(NETNS_PREFIX) {
            watchdog_netns(pool, &netns.name).await?;
        }
    }
    Ok(())
}

pub async fn watchdog_netns(pool: &SqlitePool, netns: &str) -> Result<()> {
    let stats = wireguard_stats(&netns, WIREGUARD_INTERFACE).await?;
    for peer in stats.peers() {
        watchdog_peer(pool, &stats, &peer).await?;
    }
    Ok(())
}

pub async fn watchdog_peer(pool: &SqlitePool, stats: &NetworkStats, peer: &PeerStats) -> Result<()> {
    query(
        "INSERT OR IGNORE INTO gateway_network(network_pubkey)
            VALUES (?)")
        .bind(stats.public_key.as_slice())
        .execute(pool)
        .await?;
    query(
        "INSERT OR IGNORE INTO gateway_device(device_pubkey)
            VALUES (?)")
        .bind(peer.public_key.as_slice())
        .execute(pool)
        .await?;
    Ok(())
}
