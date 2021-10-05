use rocket::*;
use anyhow::Result;
use std::net::IpAddr;
use rocket::serde::json::Json;
use rocket::serde::{Deserialize, Serialize};
use crate::wireguard::{WireguardPrivkey, WireguardPubkey, WireguardSecret, ToBase64};
use crate::gateway;

#[derive(Deserialize)]
pub struct NetworkCreate {
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
    pub public_key: WireguardSecret,
    pub allowed_ip: IpAddr,
    pub endpoint: IpAddr,
    pub port: u16,
}

#[post("/networks/create", data = "<data>")]
async fn networks_create(data: Json<NetworkCreate>) -> String {
    gateway::create(&data).await.unwrap()
}

#[get("/networks")]
async fn networks() -> &'static str {
    "Hello, world"
}

#[get("/network/<public_key>")]
async fn network_get(public_key: &str) -> &'static str {
    "Network"
}

pub fn routes() -> Vec<rocket::Route> {
    routes![networks, networks_create, network_get]
}

impl NetworkCreate {
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
