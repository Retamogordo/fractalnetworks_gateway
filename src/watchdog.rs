use crate::types::*;
use crate::util::*;
use crate::Global;
use anyhow::{Context, Result};
use event_types::{GatewayEvent, GatewayPeerConnectedEvent};
use gateway_client::{Traffic, TrafficInfo};
use log::*;
use sqlx::{query, query_as, SqlitePool};
use std::collections::{BTreeMap, HashSet};
use std::net::SocketAddr;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};
use wireguard_keys::{Privkey, Pubkey};

/// Minimum amount of traffic to be recorded. This exists because we don't
/// need to store a traffic entry if no traffic has occured. But because of
/// the PersistentKeepalive, there will always be some amount of traffic.
/// Hence, we can set a lower limit of 1000 bytes, below which we don't store
/// it. The traffic will still accumulate, so no information is lost.
pub const TRAFFIC_MINIMUM: usize = 1024;

type PeerCache = BTreeMap<u16, BTreeMap<Pubkey, PeerStats>>;

/// Start watchdog process that repeatedly checks the state of the system, with
/// a configurable interval.
pub async fn watchdog(global: &Global) -> Result<()> {
    info!("Launching watchdog every {}s", global.watchdog.as_secs());
    let mut interval = tokio::time::interval(global.watchdog);
    interval.tick().await;
    let mut peer_cache = PeerCache::new();
    loop {
        interval.tick().await;
        watchdog_run(&global, &mut peer_cache).await?;
    }
}

pub async fn watchdog_run(global: &Global, cache: &mut PeerCache) -> Result<()> {
    info!("Running watchdog");
    let netns_items = netns_list().await.context("Listing network namespaces")?;
    let mut traffic = TrafficInfo::new(0);
    for netns in &netns_items {
        if netns.name.starts_with(NETNS_PREFIX) {
            match watchdog_netns(global, &mut traffic, cache, &netns.name).await {
                Ok(_) => {}
                Err(e) => error!("Error in watchdog_netns: {:?}", e),
            }
        }
    }
    global.traffic.send(traffic)?;
    global
        .event(&GatewayEvent::PeerConnected(GatewayPeerConnectedEvent {
            endpoint: "127.0.0.1:9000".parse().unwrap(),
            network: Privkey::generate().pubkey(),
            peer: Privkey::generate().pubkey(),
        }))
        .await?;
    Ok(())
}

pub async fn watchdog_netns(
    global: &Global,
    traffic: &mut TrafficInfo,
    cache: &mut PeerCache,
    netns: &str,
) -> Result<()> {
    // pull wireguard stats
    let wgif = format!("wg{}", &netns[8..]);
    let stats = wireguard_stats(&netns, &wgif)
        .await
        .context("Fetching wireguard stats")?;

    // if not exists, create and fetch cache for this wireguard network
    let mut entry = cache
        .entry(stats.listen_port())
        .or_insert_with(|| BTreeMap::new());

    // fetch handle peer stats
    let mut peers = HashSet::new();
    for peer in stats.peers() {
        peers.insert(peer.public_key);
        match watchdog_peer(global, traffic, entry, &stats, &peer).await {
            Ok(_) => {}
            Err(e) => error!("Error in watchdog_peer: {:?}", e),
        }
    }

    // determine which peers are dead
    let mut dead_peers = Vec::new();
    for (peer, _) in entry.iter() {
        if !peers.contains(peer) {
            dead_peers.push(*peer);
        }
    }

    // remove dead peers from cache
    for peer in dead_peers {
        entry.remove(&peer);
    }

    Ok(())
}

pub async fn watchdog_peer(
    global: &Global,
    traffic: &mut TrafficInfo,
    cache: &mut BTreeMap<Pubkey, PeerStats>,
    stats: &NetworkStats,
    peer: &PeerStats,
) -> Result<()> {
    if let Some(previous) = cache.get(&peer.public_key) {
        let time = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs() as usize;
        if previous.transfer_rx > peer.transfer_rx || previous.transfer_tx > peer.transfer_tx {
            error!(
                "Cache invalid for network {} peer {}",
                stats.public_key, peer.public_key
            );
        } else {
            // how much traffic has been generated in total?
            let difference = (previous.transfer_rx - peer.transfer_rx)
                + (previous.transfer_tx - peer.transfer_tx);

            // only send out traffic if traffic has occured
            if difference > 0 {
                let traffic_item = Traffic::new(
                    peer.transfer_rx - previous.transfer_rx,
                    peer.transfer_tx - previous.transfer_tx,
                );
                traffic.add(stats.public_key, peer.public_key, time, traffic_item);
            }
        }
    } else {
    }

    cache.insert(peer.public_key, peer.clone());
    Ok(())
}
