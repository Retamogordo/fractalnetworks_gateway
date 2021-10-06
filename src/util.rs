use crate::types::*;
use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use tokio::process::Command;

pub async fn netns_add(name: &str) -> Result<()> {
    let success = Command::new("/usr/sbin/ip")
        .arg("netns")
        .arg("add")
        .arg(name)
        .status()
        .await?
        .success();
    match success {
        true => Ok(()),
        false => Err(anyhow!("Error creating netns")),
    }
}

pub async fn netns_del(name: &str) -> Result<()> {
    let success = Command::new("/usr/sbin/ip")
        .arg("netns")
        .arg("del")
        .arg(name)
        .status()
        .await?
        .success();
    match success {
        true => Ok(()),
        false => Err(anyhow!("Error deleting netns")),
    }
}

pub async fn netns_write_file(netns: &str, filename: &Path, data: &str) -> Result<()> {
    let mut path = PathBuf::from("/etc/netns");
    path.push(netns);
    if let Some(parent) = filename.parent() {
        path.push(parent);
    }
    tokio::fs::create_dir_all(&path).await?;
    path.push(filename.file_name().unwrap());
    tokio::fs::write(path, data.as_bytes()).await?;
    Ok(())
}

pub async fn netns_list() -> Result<Vec<NetnsItem>> {
    let output = Command::new("/usr/sbin/ip")
        .arg("--json")
        .arg("netns")
        .arg("list")
        .output()
        .await?;
    if !output.status.success() {
        return Err(anyhow!("Error fetching wireguard stats"));
    }
    let output = String::from_utf8(output.stdout)?;
    let items: Vec<NetnsItem> = serde_json::from_str(&output)?;
    Ok(items)
}

pub async fn wireguard_create(netns: &str, name: &str) -> Result<()> {
    if !Command::new("/usr/sbin/ip")
        .arg("link")
        .arg("add")
        .arg("dev")
        .arg(name)
        .arg("type")
        .arg("wireguard")
        .status()
        .await?
        .success()
    {
        return Err(anyhow!("Error creating wireguard interface"));
    }
    if !Command::new("/usr/sbin/ip")
        .arg("link")
        .arg("set")
        .arg(name)
        .arg("netns")
        .arg(netns)
        .status()
        .await?
        .success()
    {
        return Err(anyhow!("Error moving wireguard interface"));
    }
    Ok(())
}

pub async fn wireguard_syncconf(netns: &str, name: &str) -> Result<()> {
    if !Command::new("/usr/sbin/ip")
        .arg("netns")
        .arg("exec")
        .arg(netns)
        .arg("wg")
        .arg("syncconf")
        .arg(name)
        .arg(format!("/etc/wireguard/{}.conf", name))
        .status()
        .await?
        .success()
    {
        return Err(anyhow!("Error syncronizing wireguard config"));
    }
    Ok(())
}

pub async fn wireguard_stats(netns: &str, name: &str) -> Result<NetworkStats> {
    let result = Command::new("/usr/sbin/ip")
        .arg("netns")
        .arg("exec")
        .arg(netns)
        .arg("wg")
        .arg("show")
        .arg(name)
        .arg("dump")
        .output()
        .await?;
    if !result.status.success() {
        return Err(anyhow!("Error fetching wireguard stats"));
    }
    let result = String::from_utf8(result.stdout)?;
    let stats = NetworkStats::from_str(&result)?;
    Ok(stats)
}
