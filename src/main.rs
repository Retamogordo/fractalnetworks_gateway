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

mod api;
mod garbage;
mod gateway;
#[cfg(feature = "grpc")]
mod grpc;
mod token;
mod types;
mod util;
mod watchdog;

use anyhow::{anyhow, Context, Result};
use event_types::{broadcast::BroadcastEmitter, emitter::EventCollector, GatewayEvent};
use gateway_client::TrafficInfo;
use sqlx::SqlitePool;
use std::net::SocketAddr;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use structopt::StructOpt;
use token::Token;
use tokio::fs::File;
use tokio::sync::broadcast::{channel, Receiver, Sender};
use tokio::sync::Mutex;
use url::Url;

/// Broadcast queue length for traffic data.
const BROADCAST_QUEUE_TRAFFIC: usize = 16;

/// Broadcast queue length for events.
const BROADCAST_QUEUE_EVENTS: usize = 16;

/// Command-line options for gateway.
#[derive(StructOpt, Clone, Debug)]
pub enum Command {
    /// Run gateway.
    Run(Options),
    /// Generate OpenAPI documentation.
    #[cfg(feature = "openapi")]
    Openapi,
    /// Run as gRPC service.
    #[cfg(feature = "grpc")]
    Grpc(Options),
    /// Migrate database.
    Migrate {
        /// Database to migrate.
        database: String,
    },
}

/// Command-line options for running gateway (either as REST or a gRPC service).
#[derive(StructOpt, Clone, Debug)]
pub struct Options {
    /// What database file to use to log traffic data to.
    #[structopt(long, short, env = "GATEWAY_DATABASE")]
    database: Option<String>,

    /// Security token used to authenticate API requests.
    #[structopt(long, short, env = "GATEWAY_TOKEN")]
    secret: String,

    /// Interval to run watchdog at.
    #[structopt(long, short, default_value="60s", parse(try_from_str = parse_duration::parse::parse))]
    watchdog: Duration,

    /// Interval to run garbage collection at.
    #[structopt(long, short, default_value="1h", parse(try_from_str = parse_duration::parse::parse))]
    garbage: Duration,

    /// Duration for which network data is retained.
    #[structopt(long, short, default_value="24h", parse(try_from_str = parse_duration::parse::parse))]
    retention: Duration,

    /// Add custom HTTPS forwarding
    #[structopt(long, env = "GATEWAY_CUSTOM_FORWARDING", parse(try_from_str = parse_custom_forwarding), use_delimiter = true)]
    custom_forwarding: Vec<(Url, SocketAddr)>,
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
    /// Traffic data retention.
    retention: Duration,
    garbage: Duration,
    /// Connection to the database.
    ///
    /// The database is used only to store traffic data.
    database: SqlitePool,
    /// Broadcast queue for sending traffic data.
    traffic: Sender<TrafficInfo>,
    /// Events stream for gateway. These events are sent out on the gRPC socket.
    events: EventCollector<GatewayEvent>,
    /// Underlying channel that events are sent on.
    events_channel: Sender<GatewayEvent>,
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
        rocket::tokio::spawn(async move {
            loop {
                match watchdog::watchdog(&global).await {
                    Ok(_) => {}
                    Err(e) => log::error!("{}", e),
                }
            }
        });
    }

    /// launch garbage collector, which prunes old traffic stats from database
    pub async fn garbage(&self) {
        let database = self.database.clone();
        let garbage = self.garbage.clone();
        let retention = self.retention.clone();
        rocket::tokio::spawn(async move {
            loop {
                match garbage::garbage(&database, garbage, retention).await {
                    Ok(_) => {}
                    Err(e) => log::error!("{}", e),
                }
            }
        });
    }
}

impl Options {
    pub async fn global(&self) -> Result<Global> {
        let (traffic, _) = channel(BROADCAST_QUEUE_TRAFFIC);
        let (events_channel, _) = channel(BROADCAST_QUEUE_TRAFFIC);
        let mut events = EventCollector::new();
        events.emitter(BroadcastEmitter::new(events_channel.clone()));
        let global = Global {
            lock: Arc::new(Mutex::new(())),
            iptables_lock: Arc::new(Mutex::new(())),
            options: self.clone(),
            watchdog: self.watchdog,
            retention: self.retention,
            garbage: self.garbage,
            database: self.database().await?,
            traffic,
            events,
            events_channel,
        };

        Ok(global)
    }

    pub async fn database(&self) -> Result<SqlitePool> {
        // create database if not exists
        if let Some(database) = &self.database {
            let database = Path::new(&database);
            if !database.exists() {
                File::create(database).await?;
            }
        }

        let database_string = self.database.as_deref().unwrap_or_else(|| ":memory:");

        // connect and migrate database
        let pool = SqlitePool::connect(&database_string).await?;
        sqlx::migrate!().run(&pool).await?;

        Ok(pool)
    }

    pub async fn run(self) -> Result<()> {
        let global = self.global().await?;

        global.watchdog().await;
        global.garbage().await;
        gateway::startup(&self).await?;

        // launch REST API
        rocket::build()
            .mount("/api/v1", api::routes())
            .manage(Token::new(&self.secret))
            .manage(global.database.clone())
            .manage(global)
            .manage(self.clone())
            .launch()
            .await?;

        Ok(())
    }
}

#[rocket::main]
async fn main() -> Result<()> {
    env_logger::init();
    let command = Command::from_args();

    match command {
        #[cfg(feature = "openapi")]
        Command::Openapi => {
            let openapi = api::openapi_json();
            println!("{}", serde_json::to_string(&openapi)?);
            Ok(())
        }
        #[cfg(feature = "grpc")]
        Command::Grpc(options) => grpc::run(&options).await,
        Command::Migrate { database } => {
            let pool = SqlitePool::connect(&database).await?;
            sqlx::migrate!().run(&pool).await?;
            Ok(())
        }
        Command::Run(options) => {
            options.run().await?;
            Ok(())
        }
    }
}
