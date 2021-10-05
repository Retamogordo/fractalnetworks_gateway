use crate::api::NetworkCreate;
use anyhow::{anyhow, Result};
use tokio::process::Command;
use std::path::{Path, PathBuf};
use crate::util::*;

pub async fn create(network: &NetworkCreate) -> Result<String> {
    let pubkey = network.private_key.pubkey().to_string();
    // create netns
    netns_add("node-1").await?;

    // write wireguard config
    netns_write_file("node-1", Path::new("wireguard/node1.conf"), &network.to_config()).await?;

    wireguard_create("node-1", "node1").await?;
    wireguard_syncconf("node-1", "node1").await?;
    Ok(pubkey)
}
