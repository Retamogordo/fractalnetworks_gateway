use crate::types::*;
use crate::util::*;
use anyhow::{Context, Result};
use ipnet::{IpNet, Ipv4Net};
use lazy_static::lazy_static;
use log::*;
use rocket::futures::TryStreamExt;
use sqlx::{query, query_as, SqlitePool};
use std::collections::HashSet;
use std::net::Ipv4Addr;
use std::path::Path;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};
use tera::Tera;

const WIREGUARD_INTERFACE: &'static str = "ens0";
const BRIDGE_INTERFACE: &'static str = "ensbr0";
lazy_static! {
    pub static ref BRIDGE_NET: Ipv4Net = Ipv4Net::new(Ipv4Addr::new(172, 99, 0, 1), 16).unwrap();
    pub static ref TERA_TEMPLATES: Tera = {
        let mut tera = Tera::default();
        tera.add_raw_templates([
            ("iptables.save.tera", include_str!("../templates/iptables.save.tera")),
        ]).unwrap();
        tera
    };
}
const TRAFFIC_RETENTION: Duration = Duration::from_secs(24 * 60 * 60);

/// Given a new state, do whatever needs to be done to get the system in that
/// state.
pub async fn apply(state: &[NetworkState]) -> Result<String> {
    info!("Applying new state");

    // set up bridge
    apply_bridge(BRIDGE_INTERFACE, &vec![(*BRIDGE_NET).into()])
        .await
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
pub async fn apply_bridge(_name: &str, addr: &[IpNet]) -> Result<()> {
    if !bridge_exists(None, BRIDGE_INTERFACE).await? {
        bridge_add(None, BRIDGE_INTERFACE).await?;
    }

    apply_addr(None, BRIDGE_INTERFACE, &addr)
        .await
        .context("Setting up bridge interface")?;

    apply_interface_up(None, BRIDGE_INTERFACE)
        .await
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

    apply_interface_up(Some(&netns), WIREGUARD_INTERFACE)
        .await
        .context("Setting wireguard interface UP")?;

    // write wireguard config
    netns_write_file(
        &netns,
        Path::new("wireguard/ens0.conf"),
        &network.to_config(),
    )
    .await?;

    // set wireguard interface addresses to allow kernel ingress traffic
    apply_addr(Some(&netns), WIREGUARD_INTERFACE, &network.address)
        .await
        .context("Applying wireguard interface addresses")?;

    // sync config of wireguard netns
    wireguard_syncconf(&netns, WIREGUARD_INTERFACE).await?;

    // fetch stats to make sure interface is really up
    let _stats = wireguard_stats(&netns, WIREGUARD_INTERFACE).await?;

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
    let addr: Ipv4Net = network.veth_ipv4net().into();
    let addr: IpNet = addr.into();
    let addr = vec![addr];
    apply_addr(Some(&netns), &veth_name, &addr)
        .await
        .context("Applying veth addr")?;
    //apply_addr(None, &veth_name, &addr).await
    //    .context("Applying veth addr")?;
    apply_link_master(None, &veth_name, BRIDGE_INTERFACE)
        .await
        .context("Setting veth master")?;

    // make sure inner veth is up
    apply_interface_up(Some(&netns), &veth_name)
        .await
        .context("Making inner veth interface UP")?;
    apply_interface_up(None, &veth_name)
        .await
        .context("Marking outer veth interface UP")?;

    Ok(())
}

pub async fn apply_link_master(netns: Option<&str>, interface: &str, master: &str) -> Result<()> {
    let current = link_get_master(netns, interface).await?;
    if current.is_none() || current.as_deref() != Some(master) {
        link_set_master(netns, interface, master)
            .await
            .context("Setting master of interface")?;
    }
    Ok(())
}

pub async fn apply_forwarding(network: &NetworkState) -> Result<()> {
    let netns = network.netns_name();
    let mappings = network.port_mappings();
    Ok(())
}

/// Start watchdog process that repeatedly checks the state of the system, with
/// a configurable interval.
pub async fn watchdog(pool: &SqlitePool, duration: Duration) -> Result<()> {
    info!("Launching watchdog every {}s", duration.as_secs());
    let mut interval = tokio::time::interval(duration);
    loop {
        interval.tick().await;
        watchdog_run(&pool).await?;
    }
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

pub async fn watchdog_peer(
    pool: &SqlitePool,
    stats: &NetworkStats,
    peer: &PeerStats,
) -> Result<()> {
    // find most recent entry for this peer
    let prev: Option<(i64, i64, i64)> = query_as(
        "SELECT traffic_rx_raw, traffic_tx_raw, MAX(time) FROM gateway_traffic
            WHERE network_pubkey = ? AND device_pubkey = ?",
    )
    .bind(stats.public_key.as_slice())
    .bind(peer.public_key.as_slice())
    .fetch_optional(pool)
    .await?;
    let (traffic_rx, traffic_tx) = if let Some((traffic_rx_raw, traffic_tx_raw, _time)) = prev {
        let traffic_rx = peer.transfer_rx as i64;
        let traffic_tx = peer.transfer_tx as i64;
        if traffic_rx_raw < traffic_rx && traffic_tx_raw < traffic_tx {
            (traffic_rx - traffic_rx_raw, traffic_tx - traffic_tx_raw)
        } else {
            (0, 0)
        }
    } else {
        (0, 0)
    };
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?;
    query(
        "INSERT INTO gateway_traffic(
            network_pubkey,
            device_pubkey,
            time,
            traffic_rx,
            traffic_rx_raw,
            traffic_tx,
            traffic_tx_raw)
        VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(stats.public_key.as_slice())
    .bind(peer.public_key.as_slice())
    .bind(timestamp.as_secs() as i64)
    .bind(traffic_rx as i64)
    .bind(peer.transfer_rx as i64)
    .bind(traffic_tx as i64)
    .bind(peer.transfer_tx as i64)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn traffic(pool: &SqlitePool, start_time: usize) -> Result<TrafficInfo> {
    let mut traffic_info = TrafficInfo::new(start_time);
    let _network_pubkey: Vec<u8> = vec![];
    let _network_pubkey_str = "".to_string();
    let _device_pubkey: Vec<u8> = vec![];
    let _device_pubkey_str = "".to_string();

    let mut rows = query_as::<_, (Vec<u8>, Vec<u8>, i64, i64, i64)>("SELECT network_pubkey, device_pubkey, traffic_rx, traffic_tx, time FROM gateway_traffic WHERE time > ?")
        .bind(start_time as i64)
        .fetch(pool);

    while let Some((network, device, rx, tx, time)) = rows.try_next().await? {
        let traffic = Traffic::new(rx as usize, tx as usize);
        let time = time as usize;
        let network = base64::encode(&network);
        let device = base64::encode(&device);
        traffic_info.add(network, device, time, traffic);
        //traffic_info.add
    }

    Ok(traffic_info)
}

/// Garbage collector. This runs in a configurable interval (by default, once
/// per hour) and runs garbage_collect().
pub async fn garbage(pool: &SqlitePool, duration: Duration) -> Result<()> {
    info!("Launching garbage collector every {}s", duration.as_secs());
    let mut interval = tokio::time::interval(duration);
    loop {
        interval.tick().await;
        garbage_collect(&pool).await?;
    }
}

/// Deletes all traffic items in the database that are older than
/// TRAFFIC_RETENTION, and finally performs a VACUUM on the database to ensure
/// it is as compact as possible. Without this, the database file would keep
/// growing in size.
pub async fn garbage_collect(pool: &SqlitePool) -> Result<()> {
    info!("Running garbage collection");
    let time = SystemTime::now().duration_since(UNIX_EPOCH)?;
    let cutoff = time - TRAFFIC_RETENTION;
    let result = query("DELETE FROM gateway_traffic WHERE time < ?")
        .bind(cutoff.as_secs() as i64)
        .execute(pool)
        .await?;
    if result.rows_affected() > 0 {
        info!("Removed {} traffic data lines", result.rows_affected());
        query("VACUUM").execute(pool).await?;
        info!("Completed database vacuum");
    }
    Ok(())
}
