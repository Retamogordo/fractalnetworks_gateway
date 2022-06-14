//! # Gateway
//!
//! This crate implements a gateway daemon that manages wireguard connections.
//! It is meant to run on leaf machines and controlled via a centralized
//! manager.
//!
//! It uses [rocket] to server a REST HTTP API with support for HTTP/2. It uses
//! [tokio] as the async runtime, and [sqlx] to talk to a local SQLite database
//! to store traffic data.
//!
//! At runtime, it initialises the database if needed and launches the REST
//! API. When it gets a request to apply some state via the [api::apply]
//! endpoint (via a POST request to `/api/v1/state.json`), it differentially
//! applies that state, meaning that any items (network namespaces, interfaces,
//! networks, peers, addresses, port mappings) that are not in the new config
//! are removed, and new ones are added. Applying the same config twice should
//! not result in any change or disruption to connections.
//!
//! For monitoring purposes, there are two endpoints: the [api::traffic]
//! endpoint allows for monitoring of traffic data for every network, device
//! and the gateway as a whole. Polling this endpoint is recommended. It allows
//! for filtering traffic data by timestamp, such that only newer data is read.

pub mod gateway;
pub mod types;
pub mod watchdog;
pub mod websocket;

use anyhow::{anyhow, Context, Result};
use event_types::{broadcast::BroadcastEmitter, emitter::EventCollector};
use gateway_client::GatewayEvent;
use gateway_client::TrafficInfo;
use humantime::parse_duration;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use structopt::StructOpt;
use tokio::sync::broadcast::{channel, Sender};
use tokio::sync::Mutex;
use url::Url;

/// Broadcast queue length for traffic data.
const BROADCAST_QUEUE_TRAFFIC: usize = 16;

/// Broadcast queue length for events.
const BROADCAST_QUEUE_EVENTS: usize = 16;

/// Command-line options for running gateway (either as REST or a gRPC service).
#[derive(StructOpt, Clone, Debug)]
pub struct Options {
    /// Security token used to authenticate API requests.
    #[structopt(long, short, env = "GATEWAY_TOKEN")]
    token: String,

    /// Interval to run watchdog at.
    #[structopt(long, short, default_value="60s", parse(try_from_str = parse_duration))]
    watchdog: Duration,

    /// Add custom HTTPS forwarding
    #[structopt(long, env = "GATEWAY_CUSTOM_FORWARDING", parse(try_from_str = parse_custom_forwarding), use_delimiter = true)]
    custom_forwarding: Vec<(Url, SocketAddr)>,

    /// Where to connect to get the manager
    #[structopt(long, short, env = "GATEWAY_MANAGER")]
    manager: Url,

    /// Name of this gateway. Passed on to manager as part of a HTTP
    /// header. This is used so that a single account can host multiple
    /// gateways.
    #[structopt(long, short, env = "GATEWAY_IDENTITY")]
    identity: String,
}

/// Given a forwarding scheme like `https://domain.com=127.0.0.1:8000`, parse it
/// into URL and SocketAddr.
fn parse_custom_forwarding(text: &str) -> Result<(Url, SocketAddr)> {
    let mut parts = text.split("=");
    let url_part = parts.next().ok_or(anyhow!("Missing URL part"))?;
    let url = Url::parse(url_part).context("While parsing forwarding URL")?;
    let socket_part = parts.next().ok_or(anyhow!("Missing socket part"))?;
    let socket = SocketAddr::from_str(socket_part)?;
    Ok((url, socket))
}

/// Global state.
///
/// This struct is made available to all parts of the gateway.
#[derive(Clone)]
pub struct Global {
    /// Config application lock.
    ///
    /// This lock is held while a new configuration is being applied.
    lock: Arc<Mutex<()>>,
    /// IPtables application lock.
    ///
    /// IPtables rules cannot be applied simultaneously.
    iptables_lock: Arc<Mutex<()>>,
    /// Command-line options.
    options: Options,
    /// Watchdog duration.
    ///
    /// The watchdog process runs on intervals and polls wireguard traffic and peer
    /// statistics and turns them into events.
    watchdog: Duration,
    /// Traffic Stream.
    traffic: EventCollector<TrafficInfo>,
    /// Broadcast queue for sending traffic data.
    traffic_broadcast: Sender<TrafficInfo>,
    /// Events stream for gateway. These events are sent out on the gRPC socket.
    events: EventCollector<GatewayEvent>,
    /// Underlying channel that events are sent on.
    events_broadcast: Sender<GatewayEvent>,
    /// JWT or ApiKey used to connect to manager.
    token: String,
    /// Where to connect to for the manager
    manager: Url,
}

impl Global {
    pub fn lock(&self) -> &Mutex<()> {
        &self.lock
    }

    pub async fn event(&self, event: &GatewayEvent) -> Result<()> {
        self.events.event(event).await
    }

    pub fn iptables_lock(&self) -> &Mutex<()> {
        &self.iptables_lock
    }

    pub fn options(&self) -> &Options {
        &self.options
    }

    /// launch watchdog, which after the interval will pull in traffic stats
    /// and make sure that everything is running as it should.
    pub async fn watchdog(&self) {
        let global = self.clone();
        tokio::spawn(async move {
            loop {
                match watchdog::watchdog(&global).await {
                    Ok(_) => {}
                    Err(e) => log::error!("{}", e),
                }
            }
        });
    }
}

impl Options {
    pub async fn global(&self) -> Result<Global> {
        // set up resilient traffic event emitter
        let (traffic_broadcast, _) = channel(BROADCAST_QUEUE_TRAFFIC);
        let mut traffic = EventCollector::new();
        traffic.emitter(BroadcastEmitter::new(traffic_broadcast.clone()));

        // set up resilient event emitter
        let (events_broadcast, _) = channel(BROADCAST_QUEUE_EVENTS);
        let mut events = EventCollector::new();
        events.emitter(BroadcastEmitter::new(events_broadcast.clone()));

        let global = Global {
            lock: Arc::new(Mutex::new(())),
            iptables_lock: Arc::new(Mutex::new(())),
            options: self.clone(),
            watchdog: self.watchdog,
            traffic,
            traffic_broadcast,
            events,
            events_broadcast,
            token: self.token.clone(),
            manager: self.manager.clone(),
        };

        Ok(global)
    }
}
