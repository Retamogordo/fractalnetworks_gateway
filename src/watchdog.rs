use crate::types::*;
use crate::util::*;
use anyhow::{Context, Result};
use log::*;
use sqlx::{query, query_as, SqlitePool};
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

/// Start watchdog process that repeatedly checks the state of the system, with
/// a configurable interval.
pub async fn watchdog(pool: &SqlitePool, duration: Duration) -> Result<()> {
    info!("Launching watchdog every {}s", duration.as_secs());
    let mut interval = tokio::time::interval(duration);
    interval.tick().await;
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
            match watchdog_netns(pool, &netns.name).await {
                Ok(_) => {}
                Err(e) => error!("{:?}", e),
            }
        }
    }
    Ok(())
}

pub async fn watchdog_netns(pool: &SqlitePool, netns: &str) -> Result<()> {
    let wgif = format!("wg{}", &netns[8..]);
    let stats = wireguard_stats(&netns, &wgif).await?;
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
    // insert network pubkey
    query(
        "INSERT OR IGNORE INTO gateway_network(network_pubkey)
            VALUES (?)",
    )
    .bind(stats.public_key.as_slice())
    .execute(pool)
    .await?;
    let network_id: (i64,) = query_as(
        "SELECT network_id FROM gateway_network
            WHERE network_pubkey = ?",
    )
    .bind(stats.public_key.as_slice())
    .fetch_one(pool)
    .await
    .context("Looking up network_id")?;
    let network_id = network_id.0;

    query(
        "INSERT OR IGNORE INTO gateway_device(device_pubkey)
            VALUES (?)",
    )
    .bind(peer.public_key.as_slice())
    .execute(pool)
    .await?;
    let device_id: (i64,) = query_as(
        "SELECT device_id FROM gateway_device
            WHERE device_pubkey = ?",
    )
    .bind(peer.public_key.as_slice())
    .fetch_one(pool)
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
    .execute(pool)
    .await?;
    Ok(())
}
