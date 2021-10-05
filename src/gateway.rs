use crate::types::NetworkState;
use crate::util::*;
use anyhow::Result;
use std::path::Path;

const WIREGUARD_INTERFACE: &'static str = "ens0";

pub async fn create(network: &NetworkState) -> Result<String> {
    let pubkey = network.private_key.pubkey().to_string();
    let netns = format!("node-{}", network.port);

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
    println!("{:#?}", stats);

    Ok(pubkey)
}
