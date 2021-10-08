use crate::types::*;
use anyhow::{anyhow, Context, Result};
use ipnet::{IpNet, Ipv4Net, Ipv6Net};
use log::*;
use rocket::serde::Deserialize;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use tokio::process::Command;

pub async fn netns_add(name: &str) -> Result<()> {
    info!("netns add {}", name);
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

pub async fn netns_exists(name: &str) -> Result<bool> {
    let success = Command::new("/usr/sbin/ip")
        .arg("netns")
        .arg("exec")
        .arg(name)
        .arg("/bin/true")
        .status()
        .await?
        .success();
    Ok(success)
}

pub async fn netns_del(name: &str) -> Result<()> {
    info!("netns del {}", name);
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
    let output = String::from_utf8(output.stdout).context("Parsing command output as string")?;
    let mut items: Vec<NetnsItem> = vec![];
    if output.len() > 0 {
        items = serde_json::from_str(&output).context("Pasing netns list output as JSON")?;
    }
    Ok(items)
}

pub async fn addr_add(netns: Option<&str>, interface: &str, addr: &str) -> Result<()> {
    info!("addr add {:?}, {}, {}", netns, interface, addr);
    let mut command = Command::new("/usr/sbin/ip");
    if let Some(netns) = netns {
        command.arg("-n").arg(netns);
    }
    let success = command
        .arg("addr")
        .arg("add")
        .arg(addr)
        .arg("dev")
        .arg(interface)
        .status()
        .await?
        .success();
    if !success {
        return Err(anyhow!("Error setting address"));
    }
    Ok(())
}

pub async fn bridge_add(netns: Option<&str>, interface: &str) -> Result<()> {
    info!("bridge_add({:?}, {})", netns, interface);
    let mut command = Command::new("/usr/sbin/ip");
    if let Some(netns) = netns {
        command.arg("-n").arg(netns);
    }
    let success = command
        .arg("link")
        .arg("add")
        .arg(interface)
        .arg("type")
        .arg("bridge")
        .status()
        .await?
        .success();
    if !success {
        return Err(anyhow!(
            "Error creating bridge {} in {:?}",
            interface,
            netns
        ));
    }
    Ok(())
}

pub async fn bridge_exists(netns: Option<&str>, name: &str) -> Result<bool> {
    let mut command = Command::new("/usr/sbin/ip");
    if let Some(netns) = netns {
        command.arg("-n").arg(netns);
    }
    let output = command
        .arg("link")
        .arg("show")
        .arg(name)
        .arg("type")
        .arg("bridge")
        .output()
        .await?;
    if output.status.success() && output.stdout.len() > 0 {
        Ok(true)
    } else {
        Ok(false)
    }
}

#[derive(Deserialize)]
pub struct InterfaceShow {
    ifindex: usize,
    ifname: String,
    mtu: Option<usize>,
    operstate: String,
}

pub async fn interface_down(netns: Option<&str>, interface: &str) -> Result<bool> {
    let mut command = Command::new("/usr/sbin/ip");
    command.arg("--json");
    if let Some(netns) = netns {
        command.arg("-n").arg(netns);
    }
    command.arg("link").arg("show").arg("dev").arg(interface);
    let output = command.output().await?;
    if !output.status.success() {
        return Err(anyhow!("Error checking interface state"));
    }
    let output = String::from_utf8(output.stdout)?;
    let items: Vec<InterfaceShow> = serde_json::from_str(&output)?;
    if items.len() == 1 {
        Ok(items[0].operstate == "DOWN")
    } else {
        Err(anyhow!("Did not return any interfaces"))
    }
}

pub async fn interface_set_up(netns: Option<&str>, interface: &str) -> Result<()> {
    info!("interface_up({:?}, {})", netns, interface);
    let mut command = Command::new("/usr/sbin/ip");
    if let Some(netns) = netns {
        command.arg("-n").arg(netns);
    }
    command.arg("link").arg("set").arg(interface).arg("up");
    if !command.status().await?.success() {
        return Err(anyhow!("Error setting interface up"));
    }
    Ok(())
}

#[derive(Deserialize, PartialEq, Debug)]
struct IpInterfaceAddr {
    addr_info: Vec<IpInterfaceAddrInfo>,
}

#[derive(Deserialize, PartialEq, Debug)]
struct IpInterfaceAddrInfo {
    local: IpAddr,
    prefixlen: u8,
}

#[test]
fn test_ip_addr() {
    use std::net::Ipv4Addr;
    let test = r#"[{"ifindex":58,"ifname":"wg0","flags":["POINTOPOINT","NOARP","UP","LOWER_UP"],"mtu":1420,"qdisc":"noqueue","operstate":"UNKNOWN","group":"default","txqlen":1000,"link_type":"none","addr_info":[{"family":"inet","local":"10.80.69.7","prefixlen":24,"scope":"global","label":"wg0","valid_life_time":4294967295,"preferred_life_time":4294967295}]}]"#;
    let output: Vec<IpInterfaceAddr> = serde_json::from_str(test).unwrap();
    assert_eq!(
        output,
        vec![IpInterfaceAddr {
            addr_info: vec![IpInterfaceAddrInfo {
                local: IpAddr::V4(Ipv4Addr::new(10, 80, 69, 7)),
                prefixlen: 24
            }],
        }]
    );
}

pub async fn addr_list(netns: Option<&str>, interface: &str) -> Result<Vec<IpNet>> {
    let mut command = Command::new("/usr/sbin/ip");
    command.arg("--json");
    if let Some(netns) = netns {
        command.arg("-n").arg(netns);
    }
    let output = command
        .arg("addr")
        .arg("show")
        .arg("dev")
        .arg(interface)
        .output()
        .await?;
    if !output.status.success() {
        return Err(anyhow!(
            "Error fetching addr for {} in {:?}",
            interface,
            netns
        ));
    }
    let output = String::from_utf8(output.stdout)?;
    let items: Vec<IpInterfaceAddr> = serde_json::from_str(&output)?;
    Ok(items
        .iter()
        .map(|addr| {
            addr.addr_info.iter().map(|info| match info.local {
                IpAddr::V4(addr) => IpNet::V4(Ipv4Net::new(addr, info.prefixlen).unwrap()),
                IpAddr::V6(addr) => IpNet::V6(Ipv6Net::new(addr, info.prefixlen).unwrap()),
            })
        })
        .flatten()
        .collect())
}

#[derive(Deserialize)]
struct LinkInfo {
    master: Option<String>,
}

pub async fn link_get_master(netns: Option<&str>, interface: &str) -> Result<Option<String>> {
    let mut command = Command::new("/usr/sbin/ip");
    command.arg("--json");
    if let Some(netns) = netns {
        command.arg("-n").arg(netns);
    }
    let output = command
        .arg("link")
        .arg("show")
        .arg("dev")
        .arg(interface)
        .output()
        .await?;
    if !output.status.success() {
        return Err(anyhow!(
            "Error checking interface {} master in {:?}",
            interface,
            netns
        ));
    }
    let output = String::from_utf8(output.stdout)?;
    if output.len() == 0 {
        return Ok(None);
    }
    let output: Vec<LinkInfo> = serde_json::from_str(&output)?;
    if output.len() == 0 {
        return Ok(None);
    }
    Ok(output[0].master.clone())
}

pub async fn link_set_master(netns: Option<&str>, interface: &str, master: &str) -> Result<()> {
    let mut command = Command::new("/usr/sbin/ip");
    command.arg("--json");
    if let Some(netns) = netns {
        command.arg("-n").arg(netns);
    }
    let status = command
        .arg("link")
        .arg("set")
        .arg("dev")
        .arg(interface)
        .arg("master")
        .arg(master)
        .status()
        .await?;
    if !status.success() {
        return Err(anyhow!(
            "Error setting interface {} master in {:?} to {}",
            interface,
            netns,
            master
        ));
    }
    Ok(())
}

pub async fn veth_add(netns: &str, outer: &str, inner: &str) -> Result<()> {
    info!("veth add {}, {}, {}", netns, outer, inner);
    if !Command::new("/usr/sbin/ip")
        .arg("link")
        .arg("add")
        .arg("dev")
        .arg(outer)
        .arg("type")
        .arg("veth")
        .arg("peer")
        .arg(inner)
        .arg("netns")
        .arg(netns)
        .status()
        .await?
        .success()
    {
        return Err(anyhow!(
            "Error creating veth interfaces {} and {} in {}",
            outer,
            inner,
            netns
        ));
    }
    Ok(())
}

pub async fn veth_exists(netns: &str, name: &str) -> Result<bool> {
    let output = Command::new("/usr/sbin/ip")
        .arg("-n")
        .arg(netns)
        .arg("link")
        .arg("show")
        .arg(name)
        .arg("type")
        .arg("veth")
        .output()
        .await?;
    if output.status.success() && output.stdout.len() > 0 {
        Ok(true)
    } else {
        Ok(false)
    }
}

pub async fn wireguard_create(netns: &str, name: &str) -> Result<()> {
    info!("wireguard create {}, {}", netns, name);
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

pub async fn wireguard_exists(netns: &str, name: &str) -> Result<bool> {
    let output = Command::new("/usr/sbin/ip")
        .arg("-n")
        .arg(netns)
        .arg("link")
        .arg("show")
        .arg(name)
        .arg("type")
        .arg("wireguard")
        .output()
        .await?;
    if output.status.success() && output.stdout.len() > 0 {
        Ok(true)
    } else {
        Ok(false)
    }
}

pub async fn wireguard_syncconf(netns: &str, name: &str) -> Result<()> {
    info!("wireguard syncconf {}, {}", netns, name);
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
