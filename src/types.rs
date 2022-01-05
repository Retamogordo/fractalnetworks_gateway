use crate::gateway::BRIDGE_NET;
use anyhow::{anyhow, Context};
use gateway_client::{NetworkState, PeerState};
use ipnet::{IpAdd, IpNet, Ipv4Net};
use itertools::Itertools;
use log::*;
use rocket::serde::{Deserialize, Serialize};

use std::collections::BTreeMap;
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use url::Url;
use wireguard_keys::{Privkey, Pubkey, Secret};

pub const NETNS_PREFIX: &'static str = "network-";
pub const VETH_PREFIX: &'static str = "veth";
pub const WIREGUARD_PREFIX: &'static str = "wg";
const PORT_MAPPING_START: u16 = 2000;

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

pub trait NetworkStateExt {
    fn to_config(&self) -> String;
    fn netns_name(&self) -> String;
    fn wgif_name(&self) -> String;
    fn veth_name(&self) -> String;
    fn veth_ipv4net(&self) -> Ipv4Net;
    fn port_mappings(&self) -> Vec<(Url, u16, SocketAddr)>;
    fn port_config(&self) -> PortConfig;
}

impl NetworkStateExt for NetworkState {
    fn to_config(&self) -> String {
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

    fn netns_name(&self) -> String {
        format!("{}{}", NETNS_PREFIX, self.listen_port)
    }

    fn wgif_name(&self) -> String {
        format!("{}{}", WIREGUARD_PREFIX, self.listen_port)
    }

    fn veth_name(&self) -> String {
        format!("{}{}", VETH_PREFIX, self.listen_port)
    }

    fn veth_ipv4net(&self) -> Ipv4Net {
        let addr = BRIDGE_NET.network();
        let addr = addr.saturating_add(self.listen_port as u32);
        Ipv4Net::new(addr, BRIDGE_NET.prefix_len()).unwrap()
    }

    fn port_mappings(&self) -> Vec<(Url, u16, SocketAddr)> {
        self.proxy
            .iter()
            .map(|(url, addrs)| addrs.iter().map(|a| (url.clone(), a)))
            .flatten()
            .enumerate()
            .map(|(i, (url, addr))| (url, PORT_MAPPING_START + i as u16, *addr))
            .collect()
    }

    fn port_config(&self) -> PortConfig {
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

pub trait PeerStateExt {
    fn to_config(&self, public_key: &Pubkey) -> String;
}

impl PeerStateExt for PeerState {
    fn to_config(&self, public_key: &Pubkey) -> String {
        let mut config = String::new();
        use std::fmt::Write;
        writeln!(config, "[Peer]").unwrap();
        writeln!(config, "PublicKey = {}", public_key.to_string()).unwrap();
        writeln!(
            config,
            "AllowedIPs = {}",
            self.allowed_ips
                .iter()
                .map(|ip| ip.trunc().to_string())
                .join(", ")
        )
        .unwrap();
        if let Some(preshared_key) = &self.preshared_key {
            writeln!(config, "PresharedKey = {}", preshared_key.to_string()).unwrap();
        }
        if let Some(endpoint) = self.endpoint {
            writeln!(config, "Endpoint = {}", endpoint).unwrap();
        }
        writeln!(config, "PersistentKeepalive = 25").unwrap();
        config
    }
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Forwarding {
    https_forwarding: BTreeMap<String, String>,
    https_upstream: BTreeMap<String, Vec<SocketAddr>>,
    http_forwarding: BTreeMap<String, String>,
    http_upstream: BTreeMap<String, Vec<SocketAddr>>,
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

    pub fn add_http(&mut self, url: &Url, socket: SocketAddr) {
        let host = url.host_str().unwrap();
        let upstream = self
            .http_forwarding
            .entry(host.to_string())
            .or_insert_with(|| {
                format!(
                    "http_{}",
                    base32::encode(
                        base32::Alphabet::RFC4648 { padding: false },
                        host.as_bytes()
                    )
                )
            });
        let servers = self
            .http_upstream
            .entry(upstream.to_string())
            .or_insert_with(|| vec![]);
        servers.push(socket);
    }

    pub fn add_ssh(&mut self, _url: &Url, _socket: SocketAddr) {}

    pub fn add_custom(&mut self, url: &Url, socket: SocketAddr) {
        match url.scheme() {
            "https" => self.add_https(url, socket),
            "http" => self.add_http(url, socket),
            _other => error!("Unrecognized URL scheme: {}", url),
        }
    }
}

#[derive(Clone, Debug)]
pub struct NetworkStats {
    private_key: Privkey,
    pub public_key: Pubkey,
    pub listen_port: u16,
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
            private_key: Privkey::from_str(components[0])?,
            public_key: Pubkey::from_str(components[1])?,
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

    pub fn listen_port(&self) -> u16 {
        self.listen_port
    }
}

#[derive(Clone, Debug)]
pub struct PeerStats {
    pub public_key: Pubkey,
    pub preshared_key: Option<Secret>,
    pub endpoint: Option<SocketAddr>,
    pub allowed_ips: Vec<IpNet>,
    pub latest_handshake: Option<SystemTime>,
    pub transfer_rx: usize,
    pub transfer_tx: usize,
    pub persistent_keepalive: Option<usize>,
}

impl FromStr for PeerStats {
    type Err = anyhow::Error;
    fn from_str(output: &str) -> Result<Self, Self::Err> {
        let components: Vec<&str> = output.split('\t').collect();
        if components.len() != 8 {
            return Err(anyhow!("Wrong network stats line len"));
        }
        Ok(PeerStats {
            public_key: Pubkey::from_str(components[0])?,
            preshared_key: if components[1] == "(none)" {
                None
            } else {
                Some(Secret::from_str(components[1])?)
            },
            endpoint: if components[2] == "(none)" {
                None
            } else {
                Some(components[2].parse().context("Parsing endpoint")?)
            },
            allowed_ips: if components[3] == "(none)" {
                vec![]
            } else {
                components[3]
                    .split(',')
                    .map(|ipnet| ipnet.parse())
                    .collect::<Result<Vec<_>, _>>()
                    .context("Parsing IpNet")?
            },
            latest_handshake: {
                let timestamp: u64 = components[4].parse()?;
                if timestamp > 0 {
                    Some(
                        UNIX_EPOCH
                            .checked_add(Duration::from_secs(timestamp))
                            .ok_or(anyhow!("Error parsing latest handshake time"))?,
                    )
                } else {
                    None
                }
            },
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
