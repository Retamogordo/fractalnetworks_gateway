use crate::gateway::BRIDGE_NET;
use crate::wireguard::{WireguardPrivkey, WireguardPubkey, WireguardSecret};
use anyhow::anyhow;
use ipnet::IpNet;
use ipnet::{IpAdd, Ipv4Net};
use itertools::Itertools;
use log::*;
use rocket::serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use std::collections::{BTreeMap, HashMap};
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use url::Url;

pub const NETNS_PREFIX: &'static str = "network-";
pub const VETH_PREFIX: &'static str = "veth";
pub const WIREGUARD_PREFIX: &'static str = "wg";
const PORT_MAPPING_START: u16 = 2000;

#[serde_as]
#[derive(Deserialize, Clone, Debug)]
pub struct NetworkState {
    #[serde_as(as = "DisplayFromStr")]
    pub private_key: WireguardPrivkey,
    #[serde(default)]
    pub listen_port: u16,
    #[serde_as(as = "Vec<DisplayFromStr>")]
    pub address: Vec<IpNet>,
    #[serde_as(as = "BTreeMap<DisplayFromStr, _>")]
    pub peers: BTreeMap<WireguardPubkey, PeerState>,
    pub proxy: HashMap<Url, Vec<SocketAddr>>,
}

#[serde_as]
#[derive(Deserialize, Clone, Debug)]
pub struct PeerState {
    #[serde(default)]
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub preshared_key: Option<WireguardSecret>,
    #[serde_as(as = "Vec<DisplayFromStr>")]
    pub allowed_ips: Vec<IpNet>,
    pub endpoint: Option<SocketAddr>,
}

#[derive(Serialize, Clone, Debug)]
pub struct PortConfig {
    interface_in: String,
    interface_out: String,
    ip_source: IpAddr,
    mappings: Vec<PortMapping>,
}

#[derive(Serialize, Clone, Debug)]
pub struct PortMapping {
    port_in: u16,
    port_out: u16,
    ip_out: IpAddr,
}

impl NetworkState {
    pub fn to_config(&self) -> String {
        let mut config = String::new();
        use std::fmt::Write;
        writeln!(config, "[Interface]").unwrap();
        writeln!(config, "ListenPort = {}", self.listen_port).unwrap();
        writeln!(config, "PrivateKey = {}", self.private_key.to_string()).unwrap();

        for (pubkey, peer) in &self.peers {
            writeln!(config, "\n{}", peer.to_config(pubkey)).unwrap();
        }
        config
    }

    pub fn netns_name(&self) -> String {
        format!("{}{}", NETNS_PREFIX, self.listen_port)
    }

    pub fn wgif_name(&self) -> String {
        format!("{}{}", WIREGUARD_PREFIX, self.listen_port)
    }

    pub fn veth_name(&self) -> String {
        format!("{}{}", VETH_PREFIX, self.listen_port)
    }

    pub fn veth_ipv4net(&self) -> Ipv4Net {
        let addr = BRIDGE_NET.network();
        let addr = addr.saturating_add(self.listen_port as u32);
        Ipv4Net::new(addr, BRIDGE_NET.prefix_len()).unwrap()
    }

    pub fn port_mappings(&self) -> Vec<(Url, u16, SocketAddr)> {
        self.proxy
            .iter()
            .map(|(url, addrs)| addrs.iter().map(|a| (url.clone(), a)))
            .flatten()
            .enumerate()
            .map(|(i, (url, addr))| (url, PORT_MAPPING_START + i as u16, *addr))
            .collect()
    }

    pub fn port_config(&self) -> PortConfig {
        PortConfig {
            interface_in: self.veth_name(),
            interface_out: self.wgif_name(),
            ip_source: self.address.first().unwrap().addr(),
            mappings: self
                .port_mappings()
                .iter()
                .map(|(_, port, sock)| PortMapping {
                    port_in: *port,
                    port_out: sock.port(),
                    ip_out: sock.ip(),
                })
                .collect(),
        }
    }
}

impl PeerState {
    pub fn to_config(&self, public_key: &WireguardPubkey) -> String {
        let mut config = String::new();
        use std::fmt::Write;
        writeln!(config, "[Peer]").unwrap();
        writeln!(config, "PublicKey = {}", public_key.to_string()).unwrap();
        writeln!(
            config,
            "AllowedIPs = {}",
            self.allowed_ips.iter().map(|ip| ip.to_string()).join(", ")
        )
        .unwrap();
        if let Some(endpoint) = self.endpoint {
            writeln!(config, "Endpoint = {}", endpoint).unwrap();
        }
        config
    }
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Forwarding {
    https_forwarding: BTreeMap<String, String>,
    https_upstream: BTreeMap<String, Vec<SocketAddr>>,
    http_forwarding: BTreeMap<String, SocketAddr>,
    ssh_forwarding: BTreeMap<String, SocketAddr>,
}

impl Forwarding {
    pub fn new() -> Self {
        Forwarding {
            ..Default::default()
        }
    }

    pub fn add(&mut self, network: &NetworkState) {
        for (url, port, _sock) in &network.port_mappings() {
            let sock = SocketAddr::new(network.veth_ipv4net().addr().into(), *port);
            match url.scheme() {
                "https" => self.add_https(url, sock),
                "http" => self.add_http(url, sock),
                "ssh" => self.add_ssh(url, sock),
                _other => error!("Unrecognized URL scheme: {}", url),
            }
        }
    }

    pub fn add_https(&mut self, url: &Url, socket: SocketAddr) {
        let host = url.host_str().unwrap();
        let upstream = self
            .https_forwarding
            .entry(host.to_string())
            .or_insert_with(|| {
                format!(
                    "https_{}",
                    base32::encode(
                        base32::Alphabet::RFC4648 { padding: false },
                        host.as_bytes()
                    )
                )
            });
        let servers = self
            .https_upstream
            .entry(upstream.to_string())
            .or_insert_with(|| vec![]);
        servers.push(socket);
    }

    pub fn add_http(&mut self, _url: &Url, _socket: SocketAddr) {}

    pub fn add_ssh(&mut self, _url: &Url, _socket: SocketAddr) {}
}

#[derive(Clone, Debug)]
pub struct NetworkStats {
    private_key: WireguardPrivkey,
    pub public_key: WireguardPubkey,
    listen_port: u16,
    fwmark: Option<u16>,
    peers: Vec<PeerStats>,
}

impl FromStr for NetworkStats {
    type Err = anyhow::Error;
    fn from_str(output: &str) -> Result<Self, Self::Err> {
        let mut lines = output.lines();
        let network_stats = lines.next().ok_or(anyhow!("Missing network line"))?;
        let components: Vec<&str> = network_stats.split('\t').collect();
        if components.len() != 4 {
            println!("{:?}", components);
            return Err(anyhow!("Wrong network stats line len"));
        }
        Ok(NetworkStats {
            private_key: WireguardPrivkey::from_str(components[0])?,
            public_key: WireguardPubkey::from_str(components[1])?,
            listen_port: components[2].parse()?,
            fwmark: if components[3] == "off" {
                None
            } else {
                Some(components[3].parse()?)
            },
            peers: lines
                .map(|line| PeerStats::from_str(line))
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
}

impl NetworkStats {
    pub fn peers(&self) -> &[PeerStats] {
        &self.peers
    }
}

#[derive(Clone, Debug)]
pub struct PeerStats {
    pub public_key: WireguardPubkey,
    preshared_key: Option<WireguardSecret>,
    endpoint: SocketAddr,
    allowed_ips: Vec<IpNet>,
    latest_handshake: usize,
    pub transfer_rx: usize,
    pub transfer_tx: usize,
    persistent_keepalive: Option<usize>,
}

impl FromStr for PeerStats {
    type Err = anyhow::Error;
    fn from_str(output: &str) -> Result<Self, Self::Err> {
        let components: Vec<&str> = output.split('\t').collect();
        if components.len() != 8 {
            return Err(anyhow!("Wrong network stats line len"));
        }
        Ok(PeerStats {
            public_key: WireguardPubkey::from_str(components[0])?,
            preshared_key: if components[1] == "(none)" {
                None
            } else {
                Some(WireguardSecret::from_str(components[1])?)
            },
            endpoint: components[2].parse()?,
            allowed_ips: if components[3] == "(none)" {
                vec![]
            } else {
                components[3]
                    .split(',')
                    .map(|ipnet| ipnet.parse())
                    .collect::<Result<Vec<_>, _>>()?
            },
            latest_handshake: components[4].parse()?,
            transfer_rx: components[5].parse()?,
            transfer_tx: components[6].parse()?,
            persistent_keepalive: if components[7] == "off" {
                None
            } else {
                Some(components[4].parse()?)
            },
        })
    }
}

impl PeerStats {
    pub fn transfer(&self) -> (usize, usize) {
        (self.transfer_rx, self.transfer_tx)
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct NetnsItem {
    pub name: String,
    pub id: Option<usize>,
}

#[derive(Serialize, Clone, Debug)]
pub struct Traffic {
    rx: usize,
    tx: usize,
}

impl Traffic {
    pub fn new(rx: usize, tx: usize) -> Self {
        Traffic { rx, tx }
    }

    pub fn add(&mut self, other: &Traffic) {
        self.rx += other.rx;
        self.tx += other.tx;
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct TrafficInfo {
    start_time: usize,
    stop_time: usize,
    traffic: Traffic,
    networks: BTreeMap<String, NetworkTraffic>,
}

impl TrafficInfo {
    pub fn new(start_time: usize) -> Self {
        TrafficInfo {
            start_time,
            stop_time: start_time,
            traffic: Traffic::new(0, 0),
            networks: BTreeMap::new(),
        }
    }

    pub fn add(&mut self, network: String, device: String, time: usize, traffic: Traffic) {
        self.traffic.add(&traffic);
        let network_traffic = self
            .networks
            .entry(network)
            .or_insert(NetworkTraffic::new());
        self.stop_time = self.stop_time.max(time);
        network_traffic.add(device, time, traffic);
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct NetworkTraffic {
    traffic: Traffic,
    devices: BTreeMap<String, DeviceTraffic>,
}

impl NetworkTraffic {
    pub fn new() -> Self {
        NetworkTraffic {
            traffic: Traffic::new(0, 0),
            devices: BTreeMap::new(),
        }
    }

    pub fn add(&mut self, device: String, time: usize, traffic: Traffic) {
        self.traffic.add(&traffic);
        let device_traffic = self.devices.entry(device).or_insert(DeviceTraffic::new());
        device_traffic.add(time, traffic);
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct DeviceTraffic {
    traffic: Traffic,
    times: BTreeMap<usize, Traffic>,
}

impl DeviceTraffic {
    pub fn new() -> Self {
        DeviceTraffic {
            traffic: Traffic::new(0, 0),
            times: BTreeMap::new(),
        }
    }

    pub fn add(&mut self, time: usize, traffic: Traffic) {
        self.traffic.add(&traffic);
        self.times.insert(time, traffic);
    }
}
