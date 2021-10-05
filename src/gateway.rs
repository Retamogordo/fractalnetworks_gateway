use crate::types::NetworkState;
use crate::util::*;
use anyhow::Result;
use std::path::Path;
use std::time::Duration;

const WIREGUARD_INTERFACE: &'static str = "ens0";

pub async fn create(network: &NetworkState) -> Result<String> {
    let pubkey = network.private_key.pubkey().to_string();
    let netns = format!("node-{}", network.listen_port);

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

    // sync config of wireguard netns
    wireguard_syncconf(&netns, WIREGUARD_INTERFACE).await?;

    let stats = wireguard_stats(&netns, WIREGUARD_INTERFACE).await?;

    Ok(pubkey)
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
        println!("{}: {:?}", &netns.name, stats);
    }
    Ok(())
}
