use ipnet::IpNet;
#[cfg(feature = "schema")]
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::ops::{Add, AddAssign, Deref, DerefMut};
use thiserror::Error;
use url::Url;
use wireguard_keys::{Privkey, Pubkey, Secret};

/// Peer connected to the gateway.
///
/// This event is emitted on the gateway's event stream whenever a peer connects to a gateway.
/// The gateway polls the wireguard interface's status periodically and emits this event whenever
/// it detects a change.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GatewayPeerConnectedEvent {
    pub network: Pubkey,
    pub peer: Pubkey,
    pub endpoint: SocketAddr,
}

/// Peer disconnected from the gateway.
///
/// This event is emitted when the last packet received from the peer is older than the keepalive
/// packet interval.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GatewayPeerDisconnectedEvent {
    pub network: Pubkey,
    pub peer: Pubkey,
}

/// Peer endpoint has changed.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GatewayPeerEndpointEvent {
    pub network: Pubkey,
    pub peer: Pubkey,
    pub endpoint: SocketAddr,
}

/// Gateway event types
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum GatewayEvent {
    PeerConnected(GatewayPeerConnectedEvent),
    PeerDisconnected(GatewayPeerDisconnectedEvent),
    Endpoint(GatewayPeerEndpointEvent),
}

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

impl GatewayConfig {
    pub fn into_inner(self) -> BTreeMap<u16, NetworkState> {
        self.0
    }

    pub fn apply_partial(&mut self, partial: &GatewayConfigPartial) {
        for (port, network) in partial.iter() {
            match network {
                None => self.remove(port),
                Some(network) => self.insert(*port, network.clone()),
            };
        }
    }
}

/// Represents a partial configuration of the gateway. All ports are listed,
/// but those containing a `None` value did not change.
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct GatewayConfigPartial(BTreeMap<u16, Option<NetworkState>>);

impl GatewayConfigPartial {
    pub fn into_inner(self) -> BTreeMap<u16, Option<NetworkState>> {
        self.0
    }
}

impl Deref for GatewayConfigPartial {
    type Target = BTreeMap<u16, Option<NetworkState>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for GatewayConfigPartial {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Default MTU for WireGuard networks.
fn default_mtu() -> usize {
    1420
}

/// Requests coming in for the gateway
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum GatewayRequest {
    /// Apply entire new config to gateway
    Apply(GatewayConfig),
    /// Apply partial config to gateway
    ApplyPartial(GatewayConfigPartial),
    /// Shut gateway down.
    Shutdown,
}

/// Responses sent back out by gateway
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum GatewayResponse {
    /// Send out traffic data
    Traffic(TrafficInfo),
    /// Send out events
    Event(GatewayEvent),
    /// Result for the last apply operation
    Apply(Result<String, String>),
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
    /// MTU (maximum packet size) for network.
    #[serde(default = "default_mtu")]
    pub mtu: usize,
    /// Subnet for this network.
    pub address: Vec<IpNet>,
    /// Configuration state for peers in this network
    pub peers: BTreeMap<Pubkey, PeerState>,
    /// Forwarding settings for this network
    pub proxy: BTreeMap<Url, Vec<SocketAddr>>,
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
    pub start_time: usize,
    /// End of time slice, as UNIX timestamp
    pub stop_time: usize,
    /// Sum of all traffic occuring in this time slice.
    pub traffic: Traffic,
    /// Traffic by network
    pub networks: BTreeMap<Pubkey, NetworkTraffic>,
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
    pub traffic: Traffic,
    /// Traffic per device.
    pub devices: BTreeMap<Pubkey, DeviceTraffic>,
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
    pub traffic: Traffic,
    /// Map of timestamps and traffic generated
    pub times: BTreeMap<usize, Traffic>,
}

impl DeviceTraffic {
    pub fn add(&mut self, time: usize, traffic: Traffic) {
        self.traffic += traffic;
        self.times.insert(time, traffic);
    }
}
