use crate::types::*;
use crate::util::*;
use crate::Global;
use anyhow::{Context, Result};
use event_types::{GatewayEvent, GatewayPeerConnectedEvent};
use gateway_client::TrafficInfo;
use log::*;
use sqlx::{query, query_as, SqlitePool};
use std::net::SocketAddr;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};
use wireguard_keys::Privkey;

/// Minimum amount of traffic to be recorded. This exists because we don't
/// need to store a traffic entry if no traffic has occured. But because of
/// the PersistentKeepalive, there will always be some amount of traffic.
/// Hence, we can set a lower limit of 1000 bytes, below which we don't store
/// it. The traffic will still accumulate, so no information is lost.
pub const TRAFFIC_MINIMUM: usize = 1024;

/// Start watchdog process that repeatedly checks the state of the system, with
/// a configurable interval.
pub async fn watchdog(global: &Global) -> Result<()> {
    info!("Launching watchdog every {}s", global.watchdog.as_secs());
    let mut interval = tokio::time::interval(global.watchdog);
    interval.tick().await;
    loop {
        interval.tick().await;
        watchdog_run(&global).await?;
    }
}

pub async fn watchdog_run(global: &Global) -> Result<()> {
    info!("Running watchdog");
    let netns_items = netns_list().await.context("Listing network namespaces")?;
    let mut traffic_info = TrafficInfo::new(0);
    for netns in &netns_items {
        if netns.name.starts_with(NETNS_PREFIX) {
            match watchdog_netns(global, &netns.name).await {
                Ok(_) => {}
                Err(e) => error!("Error in watchdog_netns: {:?}", e),
            }
        }
    }
    global.traffic.send(traffic_info)?;
    global
        .event(&GatewayEvent::PeerConnected(GatewayPeerConnectedEvent {
            endpoint: "127.0.0.1:9000".parse().unwrap(),
            network: Privkey::generate().pubkey(),
            peer: Privkey::generate().pubkey(),
        }))
        .await?;
    Ok(())
}

pub async fn watchdog_netns(global: &Global, netns: &str) -> Result<()> {
    let wgif = format!("wg{}", &netns[8..]);
    let stats = wireguard_stats(&netns, &wgif)
        .await
        .context("Fetching wireguard stats")?;
    for peer in stats.peers() {
        match watchdog_peer(global, &stats, &peer).await {
            Ok(_) => {}
            Err(e) => error!("Error in watchdog_peer: {:?}", e),
        }
    }
    Ok(())
}

pub async fn watchdog_peer(global: &Global, stats: &NetworkStats, peer: &PeerStats) -> Result<()> {
    let mut conn = global.database.acquire().await?;
    // insert network pubkey
    query(
        "INSERT OR IGNORE INTO gateway_network(network_pubkey)
            VALUES (?)",
    )
    .bind(&stats.public_key[..])
    .execute(&mut conn)
    .await?;
    let network_id: (i64,) = query_as(
        "SELECT network_id FROM gateway_network
            WHERE network_pubkey = ?",
    )
    .bind(&stats.public_key[..])
    .fetch_one(&mut conn)
    .await
    .context("Looking up network_id")?;
    let network_id = network_id.0;

    query(
        "INSERT OR IGNORE INTO gateway_device(device_pubkey)
            VALUES (?)",
    )
    .bind(&peer.public_key[..])
    .execute(&mut conn)
    .await?;
    let device_id: (i64,) = query_as(
        "SELECT device_id FROM gateway_device
            WHERE device_pubkey = ?",
    )
    .bind(&peer.public_key[..])
    .fetch_one(&mut conn)
    .await
    .context("Looking up device_id")?;
    let device_id = device_id.0;

    // find most recent entry for this peer
    let prev: Option<(i64, i64, i64)> = query_as(
        "SELECT traffic_rx_raw, traffic_tx_raw, MAX(time) FROM gateway_traffic
            WHERE network_id = ? AND device_id = ?",
    )
    .bind(network_id)
    .bind(device_id)
    .fetch_optional(&mut conn)
    .await?;

    // find out how much traffic has occured since last watchdog run
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

    // if there has been less than the minimum amount of traffic recorded,
    // don't record it yet.
    if ((traffic_rx + traffic_tx) as usize) < TRAFFIC_MINIMUM {
        return Ok(());
    }

    // insert entry
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?;
    query(
        "INSERT INTO gateway_traffic(
            network_id,
            device_id,
            time,
            traffic_rx,
            traffic_rx_raw,
            traffic_tx,
            traffic_tx_raw)
        VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(network_id)
    .bind(device_id)
    .bind(timestamp.as_secs() as i64)
    .bind(traffic_rx as i64)
    .bind(peer.transfer_rx as i64)
    .bind(traffic_tx as i64)
    .bind(peer.transfer_tx as i64)
    .execute(&mut conn)
    .await?;
    Ok(())
}
