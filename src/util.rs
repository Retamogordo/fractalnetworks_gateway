use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};
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