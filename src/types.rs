use crate::wireguard::{WireguardPrivkey, WireguardPubkey, WireguardSecret};
use rocket::serde::{Deserialize, Serialize};
use std::net::IpAddr;

#[derive(Deserialize)]
pub struct NetworkState {
    #[serde(with = "crate::wireguard::from_str")]
    pub private_key: WireguardPrivkey,
    pub port: u16,
    pub peers: Vec<PeerState>,
}

#[derive(Deserialize)]
pub struct PeerState {
    #[serde(with = "crate::wireguard::from_str")]
    pub preshared_key: WireguardSecret,
    #[serde(with = "crate::wireguard::from_str")]
    pub public_key: WireguardPubkey,
    pub allowed_ip: IpAddr,
    pub endpoint: IpAddr,
    pub port: u16,
}

impl NetworkState {
    pub fn to_config(&self) -> String {
        let mut config = String::new();
        use std::fmt::Write;
        writeln!(config, "[Interface]").unwrap();
        writeln!(config, "ListenPort = {}", self.port).unwrap();
        writeln!(config, "PrivateKey = {}", self.private_key.to_string()).unwrap();

        for peer in &self.peers {
            writeln!(config, "\n{}", peer.to_config()).unwrap();
        }
        config
    }
}

impl PeerState {
    pub fn to_config(&self) -> String {
        let mut config = String::new();
        use std::fmt::Write;
        writeln!(config, "[Peer]").unwrap();
        writeln!(config, "PublicKey = {}", self.public_key.to_string()).unwrap();
        writeln!(config, "AllowedIPs = {}", self.allowed_ip).unwrap();
        writeln!(config, "Endpoint = {}:{}", self.endpoint, self.port).unwrap();
        config
    }
}
