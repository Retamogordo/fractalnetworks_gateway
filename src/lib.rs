use async_trait::async_trait;
use ipnet::IpNet;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use std::collections::{BTreeMap, HashMap};
use std::net::SocketAddr;
use std::ops::{Deref, DerefMut};
use thiserror::Error;
use url::Url;
use wireguard_util::keys::{Privkey, Pubkey, Secret};

#[derive(Error, Debug)]
pub enum GatewayError {
    #[error("An unknown error has occured")]
    Unknown,
    #[cfg(feature = "client")]
    #[error("An error making the request has occured: {0:}")]
    Reqwest(#[from] reqwest::Error),
}

#[derive(Serialize, Deserialize)]
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

#[serde_as]
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NetworkState {
    #[serde_as(as = "DisplayFromStr")]
    pub private_key: Privkey,
    #[serde(default)]
    pub listen_port: u16,
    #[serde_as(as = "Vec<DisplayFromStr>")]
    pub address: Vec<IpNet>,
    #[serde_as(as = "BTreeMap<DisplayFromStr, _>")]
    pub peers: BTreeMap<Pubkey, PeerState>,
    pub proxy: HashMap<Url, Vec<SocketAddr>>,
}

#[serde_as]
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PeerState {
    #[serde(default)]
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub preshared_key: Option<Secret>,
    #[serde_as(as = "Vec<DisplayFromStr>")]
    pub allowed_ips: Vec<IpNet>,
    pub endpoint: Option<SocketAddr>,
}

#[async_trait]
pub trait Gateway {
    async fn config_set(&self, token: &str, state: &GatewayConfig) -> Result<(), GatewayError>;

    async fn config_get(&self, token: &str) -> Result<GatewayConfig, GatewayError>;

    async fn status_get(&self, token: &str) -> Result<(), GatewayError>;

    async fn traffic_get(&self, token: &str, since: Option<usize>) -> Result<(), GatewayError>;
}

#[async_trait]
impl Gateway for Url {
    async fn config_set(&self, token: &str, state: &GatewayConfig) -> Result<(), GatewayError> {
        unimplemented!()
    }

    async fn config_get(&self, token: &str) -> Result<GatewayConfig, GatewayError> {
        unimplemented!()
    }

    async fn status_get(&self, token: &str) -> Result<(), GatewayError> {
        unimplemented!()
    }

    async fn traffic_get(&self, token: &str, since: Option<usize>) -> Result<(), GatewayError> {
        unimplemented!()
    }
}
