use crate::types::*;
use crate::util::*;
use anyhow::Result;
use log::*;
use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;
use tokio::sync::RwLock;
use std::sync::Arc;
use std::collections::BTreeMap;
use sqlx::SqlitePool;

const WIREGUARD_INTERFACE: &'static str = "ens0";

pub async fn apply(state: &[NetworkState]) -> Result<String> {
    info!("Applying new state");

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
    Ok("okay".to_string())
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

    if interface_down(Some(&netns), WIREGUARD_INTERFACE).await? {
        interface_set_up(Some(&netns), WIREGUARD_INTERFACE).await?;
    }

    // write wireguard config
    netns_write_file(
        &netns,
        Path::new("wireguard/ens0.conf"),
        &network.to_config(),
    )
    .await?;

    // set wireguard interface addresses to allow kernel ingress traffic
    let addresses = addr_list(&netns, WIREGUARD_INTERFACE).await?;
    for address in &network.address {
        if !addresses.contains(address) {
            addr_add(&netns, WIREGUARD_INTERFACE, &address.to_string()).await?;
        }
    }

    // sync config of wireguard netns
    wireguard_syncconf(&netns, WIREGUARD_INTERFACE).await?;

    // fetch stats to make sure interface is really up
    let stats = wireguard_stats(&netns, WIREGUARD_INTERFACE).await?;

    Ok(())
}

pub async fn apply_veth(network: &NetworkState) -> Result<()> {
    let netns = network.netns_name();

    // create veth pair
    let veth_name = network.veth_name();
    if !veth_exists(&netns, &veth_name).await? {
        veth_add(&netns, &veth_name, &veth_name).await?;
    }

    // make sure inner veth is up
    if interface_down(Some(&netns), &veth_name).await? {
        interface_set_up(Some(&netns), &veth_name).await?;
    }

    // make sure outer veth is up
    if interface_down(None, &veth_name).await? {
        interface_set_up(None, &veth_name).await?;
    }

    Ok(())
}

pub async fn apply_forwarding(network: &NetworkState) -> Result<()> {
    Ok(())
}

pub async fn watchdog(pool: SqlitePool, duration: Duration) -> Result<()> {
    let mut interval = tokio::time::interval(duration);
    loop {
        interval.tick().await;
        watchdog_run().await?;
    }
    Ok(())
}

pub async fn watchdog_run() -> Result<()> {
    let netns_items = netns_list().await?;
    for netns in &netns_items {
        let stats = wireguard_stats(&netns.name, WIREGUARD_INTERFACE).await?;
        info!("{}: {:?}", &netns.name, stats);
    }
    Ok(())
}
