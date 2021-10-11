use crate::gateway::BRIDGE_NET;
use crate::wireguard::{WireguardPrivkey, WireguardPubkey, WireguardSecret};
use anyhow::anyhow;
use ipnet::IpNet;
use ipnet::{IpAdd, Ipv4Net};
use itertools::Itertools;
use rocket::serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;

pub const NETNS_PREFIX: &'static str = "network-";
pub const VETH_PREFIX: &'static str = "veth";
const PORT_MAPPING_START: u16 = 2000;

#[derive(Deserialize, Clone, Debug)]
pub struct NetworkState {
    #[serde(with = "crate::wireguard::from_str")]
    pub private_key: WireguardPrivkey,
    #[serde(default)]
    pub listen_port: u16,
    #[serde(with = "serde_with::rust::seq_display_fromstr")]
    pub address: Vec<IpNet>,
    pub peers: Vec<PeerState>,
    pub proxy: HashMap<String, Vec<SocketAddr>>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct PeerState {
    //#[serde(with = "crate::wireguard::from_str")]
    //pub preshared_key: WireguardSecret,
    #[serde(with = "crate::wireguard::from_str")]
    pub public_key: WireguardPubkey,
    #[serde(with = "serde_with::rust::seq_display_fromstr")]
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

        for peer in &self.peers {
            writeln!(config, "\n{}", peer.to_config()).unwrap();
        }
        config
    }

    pub fn netns_name(&self) -> String {
        format!("{}{}", NETNS_PREFIX, self.listen_port)
    }

    pub fn veth_name(&self) -> String {
        format!("{}{}", VETH_PREFIX, self.listen_port)
    }

    pub fn veth_ipv4net(&self) -> Ipv4Net {
        let addr = BRIDGE_NET.network();
        let addr = addr.saturating_add(self.listen_port as u32);
        Ipv4Net::new(addr, BRIDGE_NET.prefix_len()).unwrap()
    }

    pub fn port_mappings(&self) -> Vec<(u16, SocketAddr)> {
        self.proxy
            .iter()
            .map(|(_, addrs)| addrs.iter())
            .flatten()
            .enumerate()
            .map(|(i, addr)| (PORT_MAPPING_START + i as u16, *addr))
            .collect()
    }

    pub fn port_config(&self) -> PortConfig {
        PortConfig {
            interface_in: self.veth_name(),
            interface_out: crate::gateway::WIREGUARD_INTERFACE.to_string(),
            ip_source: self.address.first().unwrap().addr(),
            mappings: self
                .port_mappings()
                .iter()
                .map(|(port, sock)| PortMapping {
                    port_in: *port,
                    port_out: sock.port(),
                    ip_out: sock.ip(),
                })
                .collect(),
        }
    }
}

impl PeerState {
    pub fn to_config(&self) -> String {
        let mut config = String::new();
        use std::fmt::Write;
        writeln!(config, "[Peer]").unwrap();
        writeln!(config, "PublicKey = {}", self.public_key.to_string()).unwrap();
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
