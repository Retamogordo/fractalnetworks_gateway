use crate::types::NetworkState;
use crate::util::*;
use anyhow::Result;
use std::path::Path;

pub async fn create(network: &NetworkState) -> Result<String> {
    let pubkey = network.private_key.pubkey().to_string();
    // create netns
    netns_add(&pubkey).await?;

    // write wireguard config
    netns_write_file(
        &pubkey,
        Path::new("wireguard/node1.conf"),
        &network.to_config(),
    )
    .await?;

    wireguard_create(&pubkey, "node1").await?;
    wireguard_syncconf(&pubkey, "node1").await?;
    Ok(pubkey)
}
