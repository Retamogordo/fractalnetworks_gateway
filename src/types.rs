use crate::wireguard::{WireguardPrivkey, WireguardPubkey, WireguardSecret};
use anyhow::anyhow;
use ipnet::IpNet;
use itertools::Itertools;
use rocket::serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::IpAddr;
use std::net::SocketAddr;
use std::str::FromStr;

pub const NETNS_PREFIX: &'static str = "network-";
pub const VETH_PREFIX: &'static str = "veth";

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
