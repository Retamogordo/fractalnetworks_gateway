use crate::types::*;
use crate::util::*;
use anyhow::Result;
use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;
use log::*;

const WIREGUARD_INTERFACE: &'static str = "ens0";

pub async fn create(network: &NetworkState) -> Result<String> {
    let pubkey = network.private_key.pubkey().to_string();
    let netns = network.netns_name();

    // create netns
    netns_add(&netns).await?;

    // write wireguard config
    netns_write_file(
        &netns,
        Path::new("wireguard/ens0.conf"),
        &network.to_config(),
    )
    .await?;

    // create wireguard config in netns
    wireguard_create(&netns, WIREGUARD_INTERFACE).await?;

    // create veth pair
    veth_add(&netns, &network.veth_name(), "veth0").await?;

    for address in &network.address {
        addr_add(&netns, WIREGUARD_INTERFACE, &address.to_string()).await?;
    }

    // sync config of wireguard netns
    wireguard_syncconf(&netns, WIREGUARD_INTERFACE).await?;

    let stats = wireguard_stats(&netns, WIREGUARD_INTERFACE).await?;

    Ok(pubkey)
}

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
        if !netns_list.contains(&network.netns_name()) {
            create(network).await?;
        }
        let create = netns_list.contains(&network.netns_name());
        apply_network(network, create).await?;
    }
    Ok("okay".to_string())
}

pub async fn apply_network(state: &NetworkState, create: bool) -> Result<()> {
    Ok(())
}

pub async fn watchdog(duration: Duration) -> Result<()> {
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
