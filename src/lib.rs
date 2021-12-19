#[cfg(feature = "api")]
use async_trait::async_trait;
use ipnet::IpNet;
#[cfg(feature = "api")]
use reqwest::Client;
#[cfg(feature = "schema")]
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::net::SocketAddr;
use std::ops::{Add, AddAssign, Deref, DerefMut};
use thiserror::Error;
use url::Url;
use wireguard_keys::{Privkey, Pubkey, Secret};

/// Possible errors that can happen when making a request to the gateway.
#[derive(Error, Debug)]
pub enum GatewayError {
    #[error("An unknown error has occured")]
    Unknown,
    #[cfg(feature = "api")]
    #[error("An error making the request has occured: {0:}")]
    Reqwest(#[from] reqwest::Error),
}

/// Represents the entire configuration state of the gateway.
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct GatewayConfig(BTreeMap<u16, NetworkState>);

impl Deref for GatewayConfig {
    type Target = BTreeMap<u16, NetworkState>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for GatewayConfig {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Represents the configuration state of one particular WireGuard network.
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NetworkState {
    /// WireGuard private key
    pub private_key: Privkey,
    /// UDP port this network is reachable on
    #[serde(default)]
    pub listen_port: u16,
    /// Subnet for this network.
    pub address: Vec<IpNet>,
    /// Configuration state for peers in this network
    pub peers: BTreeMap<Pubkey, PeerState>,
    /// Forwarding settings for this network
    pub proxy: HashMap<Url, Vec<SocketAddr>>,
}

/// Represents the configuration state of one particular peer of a WireGuard network.
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PeerState {
    /// Preshared key for this peer
    #[serde(default)]
    pub preshared_key: Option<Secret>,
    /// Allowed IP addresses of this peer
    pub allowed_ips: Vec<IpNet>,
    /// Last connected endpoint, used to resume talking to peer
    pub endpoint: Option<SocketAddr>,
}

/// Represents a single traffic item, consisting of received and sent bytes.
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[derive(Serialize, Deserialize, Copy, Clone, Debug, Default)]
pub struct Traffic {
    /// Received bytes
    pub rx: usize,
    /// Sent bytes
    pub tx: usize,
}

impl Traffic {
    pub fn new(rx: usize, tx: usize) -> Self {
        Traffic { rx, tx }
    }
}

impl Add for Traffic {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Self {
            rx: self.rx + rhs.rx,
            tx: self.tx + rhs.tx,
        }
    }
}

impl AddAssign for Traffic {
    fn add_assign(&mut self, other: Self) {
        self.tx += other.tx;
        self.rx += other.rx;
    }
}

impl Traffic {
    pub fn add(&mut self, other: &Traffic) {
        self.rx += other.rx;
        self.tx += other.tx;
    }
}

/// Traffic data from the gateway for one particular time slice.
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TrafficInfo {
    /// Stat of time slice, as UNIX timestamp
    start_time: usize,
    /// End of time slice, as UNIX timestamp
    stop_time: usize,
    /// Sum of all traffic occuring in this time slice.
    traffic: Traffic,
    /// Traffic by network
    networks: BTreeMap<Pubkey, NetworkTraffic>,
}

impl TrafficInfo {
    pub fn new(start_time: usize) -> Self {
        TrafficInfo {
            start_time,
            stop_time: start_time,
            traffic: Traffic::default(),
            networks: BTreeMap::new(),
        }
    }

    pub fn add(&mut self, network: Pubkey, device: Pubkey, time: usize, traffic: Traffic) {
        self.traffic += traffic;
        let network_traffic = self
            .networks
            .entry(network.clone())
            .or_insert(NetworkTraffic::default());
        self.stop_time = self.stop_time.max(time);
        network_traffic.add(device, time, traffic);
    }
}

/// Traffic that occured within one particular network.
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct NetworkTraffic {
    /// Total traffic occuring in this network.
    traffic: Traffic,
    /// Traffic per device.
    devices: BTreeMap<Pubkey, DeviceTraffic>,
}

impl NetworkTraffic {
    pub fn add(&mut self, device: Pubkey, time: usize, traffic: Traffic) {
        self.traffic += traffic;
        let device_traffic = self
            .devices
            .entry(device)
            .or_insert(DeviceTraffic::default());
        device_traffic.add(time, traffic);
    }
}

/// Traffic occuring from one particular peer in the network.
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct DeviceTraffic {
    /// Total traffic from this peer
    traffic: Traffic,
    /// Map of timestamps and traffic generated
    times: BTreeMap<usize, Traffic>,
}

impl DeviceTraffic {
    pub fn add(&mut self, time: usize, traffic: Traffic) {
        self.traffic += traffic;
        self.times.insert(time, traffic);
    }
}

/// Client for the gateway. These methods can be called to interface with a
/// gateway server.
#[cfg(feature = "api")]
#[async_trait]
pub trait GatewayClient {
    /// Apply a new configuration to a gateway.
    async fn config_set(
        &self,
        client: &Client,
        token: &str,
        state: &GatewayConfig,
    ) -> Result<(), GatewayError>;

    /// Get the currently active configuration from the gateway.
    async fn config_get(&self, client: &Client, token: &str)
        -> Result<GatewayConfig, GatewayError>;

    /// Get the current status of the networks.
    async fn status_get(&self, client: &Client, token: &str) -> Result<(), GatewayError>;

    /// Get the traffic since the provided timestamp.
    async fn traffic_get(
        &self,
        client: &Client,
        token: &str,
        since: Option<usize>,
    ) -> Result<TrafficInfo, GatewayError>;
}

#[cfg(feature = "api")]
#[async_trait]
impl GatewayClient for Url {
    async fn config_set(
        &self,
        client: &Client,
        token: &str,
        state: &GatewayConfig,
    ) -> Result<(), GatewayError> {
        let url = self
            .join(&"/api/v1/config.json")
            .map_err(|_| GatewayError::Unknown)?;
        let result = client
            .post(url)
            .header("Token", token)
            .json(state)
            .send()
            .await?;
        match result.status() {
            status if status.is_success() => Ok(()),
            _ => Err(GatewayError::Unknown),
        }
    }

    async fn config_get(
        &self,
        client: &Client,
        token: &str,
    ) -> Result<GatewayConfig, GatewayError> {
        unimplemented!()
    }

    async fn status_get(&self, client: &Client, token: &str) -> Result<(), GatewayError> {
        unimplemented!()
    }

    async fn traffic_get(
        &self,
        client: &Client,
        token: &str,
        since: Option<usize>,
    ) -> Result<TrafficInfo, GatewayError> {
        let url = self
            .join(&"/api/v1/traffic.json")
            .map_err(|_| GatewayError::Unknown)?;
        let result = client.get(url).header("Token", token).send().await?;
        match result.status() {
            status if status.is_success() => Ok(result.json().await?),
            _ => Err(GatewayError::Unknown),
        }
    }
}
