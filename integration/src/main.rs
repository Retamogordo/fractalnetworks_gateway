use anyhow::{anyhow, Result};
use futures::{SinkExt, StreamExt};
use gateway_client::*;
use ipnet::{IpAdd, IpNet, Ipv4Net};
use log::info;
use networking_wrappers::*;
use rand::{prelude::SliceRandom, thread_rng, Rng};
use std::collections::BTreeMap;
use std::net::{IpAddr, SocketAddr};
use std::ops::Range;
use std::path::PathBuf;
use structopt::StructOpt;
use tokio::net::{TcpListener, TcpStream};
use tokio::process::Command;
use tokio_tungstenite::{accept_async, tungstenite::protocol::Message, WebSocketStream};
use wireguard_keys::{Privkey, Pubkey, Secret};

#[derive(StructOpt, Clone, Debug)]
pub struct Options {
    #[structopt(
        long,
        short,
        default_value = "0.0.0.0:8000",
        env = "INTEGRATION_LISTEN"
    )]
    listen: SocketAddr,

    #[structopt(
        long,
        short,
        default_value = "gateway:8000",
        env = "INTEGRATION_GATEWAY"
    )]
    gateway: String,
}

const PORT_RANGE: Range<u16> = 50000..60000;
const NETWORK_MTU: usize = 1420;

fn generate_config(
    size: usize,
    peers: Range<usize>,
    peer_keys: &mut BTreeMap<Pubkey, Privkey>,
) -> GatewayConfig {
    let mut config = GatewayConfig::default();
    let mut rng = thread_rng();

    for _ in 0..size {
        let port = rng.gen_range(PORT_RANGE);
        let peers = rng.gen_range(peers.clone());
        let address: IpNet = "10.0.0.1/8".parse().unwrap();
        let mut network = NetworkState {
            private_key: Privkey::generate(),
            listen_port: port,
            mtu: NETWORK_MTU,
            address: vec!["10.0.0.1/8".parse().unwrap()],
            peers: Default::default(),
            proxy: Default::default(),
        };
        for n in 0..peers {
            let address = match address.addr() {
                IpAddr::V4(ipv4) => IpAddr::V4(ipv4.saturating_add(1 + n as u32)),
                IpAddr::V6(ipv6) => IpAddr::V6(ipv6.saturating_add(1 + n as u128)),
            };
            let address = IpNet::new(address, 32).unwrap();
            let privkey = Privkey::generate();
            let pubkey = privkey.pubkey();
            peer_keys.insert(pubkey.clone(), privkey);
            network.peers.insert(
                pubkey,
                PeerState {
                    allowed_ips: vec![address],
                    endpoint: None,
                    preshared_key: None,
                },
            );
        }

        config.insert(port, network);
    }

    config
}

fn generate_partial_config(
    add: usize,
    remove: usize,
    peers: Range<usize>,
    existing: Vec<u16>,
    peer_keys: &mut BTreeMap<Pubkey, Privkey>,
) -> GatewayConfigPartial {
    let mut config = GatewayConfigPartial::default();

    for (port, network) in generate_config(add, peers, peer_keys)
        .into_inner()
        .into_iter()
    {
        config.insert(port, Some(network));
    }

    let mut rng = thread_rng();
    for port in existing.choose_multiple(&mut rng, remove) {
        config.insert(*port, None);
    }

    config
}

async fn apply_config(
    websocket: &mut WebSocketStream<TcpStream>,
    config: GatewayConfig,
) -> Result<Result<String, String>> {
    websocket
        .send(Message::Text(serde_json::to_string(
            &GatewayRequest::Apply(config),
        )?))
        .await?;
    while let Some(Ok(message)) = websocket.next().await {
        match message {
            Message::Text(value) => {
                let value = serde_json::from_str(&value)?;
                match value {
                    GatewayResponse::Apply(status) => {
                        return Ok(status);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    Err(anyhow!("Missing apply config response"))
}

async fn apply_partial_config(
    websocket: &mut WebSocketStream<TcpStream>,
    config: GatewayConfigPartial,
) -> Result<Result<String, String>> {
    websocket
        .send(Message::Text(serde_json::to_string(
            &GatewayRequest::ApplyPartial(config),
        )?))
        .await?;
    while let Some(Ok(message)) = websocket.next().await {
        match message {
            Message::Text(value) => {
                let value = serde_json::from_str(&value)?;
                match value {
                    GatewayResponse::Apply(status) => {
                        return Ok(status);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    Err(anyhow!("Missing apply config response"))
}

async fn run_tests(global: &Global, websocket: &mut WebSocketStream<TcpStream>) -> Result<()> {
    info!("Applying empty config");
    let response = apply_config(websocket, Default::default()).await?;
    assert!(response.is_ok());

    let mut peer_keys = BTreeMap::new();
    let mut config = GatewayConfig::default();

    // create 10 networks, and verify that they are all reachable.
    for _ in 0..3 {
        info!("Applying config with 10 networks");
        config = generate_config(10, 0..3, &mut peer_keys);
        let response = apply_config(websocket, config.clone()).await?;
        assert!(response.is_ok());

        // make sure config is correct
        verify_config(global, &config, &peer_keys).await?;
    }

    // create 10 networks, and verify that the previous ones are not reachable.
    for _ in 0..3 {
        info!("Applying config with 10 networks and making sure old networks are not reachable");
        let new_config = generate_config(10, 0..3, &mut peer_keys);
        let response = apply_config(websocket, new_config.clone()).await?;
        assert!(response.is_ok());

        // FIXME: why does this break if we don't wait?
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;

        // make sure config is correct
        verify_old_config(global, &config, &peer_keys).await?;
        config = new_config;
    }

    for _ in 0..3 {
        info!("Applying partial config with 10 networks");
        let partial_config = generate_partial_config(10, 0, 0..3, vec![], &mut peer_keys);
        config.apply_partial(&partial_config);
        let response = apply_partial_config(websocket, partial_config).await?;
        assert!(response.is_ok());

        // make sure config is correct
        verify_config(global, &config, &peer_keys).await?;
    }

    for _ in 0..3 {
        info!("Applying partial config with 10 networks");
        let partial_config = generate_partial_config(
            0,
            10,
            0..3,
            config.keys().cloned().collect(),
            &mut peer_keys,
        );
        config.apply_partial(&partial_config);
        let response = apply_partial_config(websocket, partial_config).await?;
        assert!(response.is_ok());

        // make sure config is correct
        verify_config(global, &config, &peer_keys).await?;
    }

    info!("Applying empty config");
    let response = apply_config(websocket, Default::default()).await?;
    assert!(response.is_ok());

    Ok(())
}

pub const IP_PATH: &'static str = "ip";
pub const PING_PATH: &'static str = "ping";
async fn ping_host(netns: &str, host: IpAddr) -> Result<()> {
    let output = Command::new(IP_PATH)
        .arg("netns")
        .arg("exec")
        .arg(netns)
        .arg(PING_PATH)
        .arg("-f")
        .arg("-c")
        .arg("4")
        .arg("-W")
        .arg("0.1")
        .arg(host.to_string())
        .stderr(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .output()
        .await?;
    match output.status.success() {
        true => Ok(()),
        false => Err(anyhow!("Error pinging {host} in {netns}: {output:?}")),
    }
}

async fn verify_config(
    global: &Global,
    config: &GatewayConfig,
    peer_keys: &BTreeMap<Pubkey, Privkey>,
) -> Result<()> {
    for (port, network) in config.iter() {
        for (pubkey, peer) in network.peers.iter() {
            let netns = format!("network-{port}-{}", pubkey.to_hex());
            netns_add(&netns).await?;
            wireguard_create(Some(&netns), "wg0").await?;
            interface_up(Some(&netns), "wg0").await?;
            let addr = match peer.allowed_ips[0] {
                IpNet::V4(ipv4net) => IpNet::V4(Ipv4Net::new(ipv4net.addr(), 8)?),
                _ => unreachable!(),
            };
            addr_add(Some(&netns), "wg0", addr).await?;
            let config = [
                format!("[Interface]"),
                format!("PrivateKey = {}", peer_keys.get(pubkey).unwrap()),
                String::new(),
                format!("[Peer]"),
                format!("PublicKey = {}", network.private_key.pubkey()),
                format!("Endpoint = {}:{port}", global.gateway),
                format!("AllowedIPs = {}", network.address[0]),
                format!("PersistentKeepalive = 25"),
            ]
            .join("\n");
            netns_write_file(&netns, &PathBuf::from("wireguard/wg0.conf"), &config).await?;
            wireguard_syncconf(&netns, "wg0").await?;
            ping_host(&netns, network.address[0].addr()).await?;
            netns_del(&netns).await?;
        }
    }
    Ok(())
}

async fn verify_old_config(
    global: &Global,
    config: &GatewayConfig,
    peer_keys: &BTreeMap<Pubkey, Privkey>,
) -> Result<()> {
    for (port, network) in config.iter() {
        for (pubkey, peer) in network.peers.iter() {
            let netns = format!("network-{port}-{}", pubkey.to_hex());
            netns_add(&netns).await?;
            wireguard_create(Some(&netns), "wg0").await?;
            interface_up(Some(&netns), "wg0").await?;
            let addr = match peer.allowed_ips[0] {
                IpNet::V4(ipv4net) => IpNet::V4(Ipv4Net::new(ipv4net.addr(), 8)?),
                _ => unreachable!(),
            };
            addr_add(Some(&netns), "wg0", addr).await?;
            let config = [
                format!("[Interface]"),
                format!("PrivateKey = {}", peer_keys.get(pubkey).unwrap()),
                String::new(),
                format!("[Peer]"),
                format!("PublicKey = {}", network.private_key.pubkey()),
                format!("Endpoint = {}:{port}", global.gateway),
                format!("AllowedIPs = {}", network.address[0]),
                format!("PersistentKeepalive = 25"),
            ]
            .join("\n");
            netns_write_file(&netns, &PathBuf::from("wireguard/wg0.conf"), &config).await?;
            wireguard_syncconf(&netns, "wg0").await?;
            let result = ping_host(&netns, network.address[0].addr()).await;
            if result.is_ok() {
                return Err(anyhow::anyhow!("Network is reachable"));
            }
            netns_del(&netns).await?;
        }
    }
    Ok(())
}

struct Global {
    options: Options,
    gateway: IpAddr,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let options = Options::from_args();

    let global = Global {
        options: options.clone(),
        gateway: tokio::net::lookup_host(&options.gateway)
            .await?
            .next()
            .unwrap()
            .ip(),
    };

    let socket = TcpListener::bind(&options.listen).await?;
    let (stream, addr) = socket.accept().await?;
    info!("Got gateway connection from {addr}");
    let mut websocket = accept_async(stream).await?;

    let result = run_tests(&global, &mut websocket).await;
    info!("Test result: {result:?}");
    let _ = websocket
        .send(Message::Text(serde_json::to_string(
            &GatewayRequest::Shutdown,
        )?))
        .await;

    result
}
