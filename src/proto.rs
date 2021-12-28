tonic::include_proto!("gateway");

macro_rules! wireguard_conversions {
    ($wgtype:ty, $prototype:ty) => {
        impl From<$wgtype> for $prototype {
            fn from(value: $wgtype) -> $prototype {
                Self {
                    data: value.to_vec(),
                }
            }
        }

        impl TryInto<$wgtype> for $prototype {
            type Error = wireguard_keys::ParseError;
            fn try_into(self) -> Result<$wgtype, Self::Error> {
                let data: &[u8] = &self.data;
                let key = data.try_into()?;
                Ok(key)
            }
        }
    };
}

wireguard_conversions!(wireguard_keys::Pubkey, Pubkey);
wireguard_conversions!(wireguard_keys::Privkey, Privkey);
wireguard_conversions!(wireguard_keys::Secret, Secret);

impl From<std::net::IpAddr> for IpAddr {
    fn from(value: std::net::IpAddr) -> IpAddr {
        match value {
            std::net::IpAddr::V4(ip) => IpAddr {
                version: 0,
                data: ip.octets().to_vec(),
            },
            std::net::IpAddr::V6(ip) => IpAddr {
                version: 1,
                data: ip.octets().to_vec(),
            },
        }
    }
}

impl TryInto<std::net::IpAddr> for IpAddr {
    type Error = anyhow::Error;
    fn try_into(self) -> Result<std::net::IpAddr, Self::Error> {
        match self.version {
            0 => {
                let data: [u8; 4] = self
                    .data
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("Illegal IPv4 length encountered"))?;
                Ok(data.into())
            }
            1 => {
                let data: [u8; 16] = self
                    .data
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("Illegal IPv6 length encountered"))?;
                Ok(data.into())
            }
            _ => Err(anyhow::anyhow!("Illegal IP address version")),
        }
    }
}

impl From<ipnet::IpNet> for IpNet {
    fn from(value: ipnet::IpNet) -> IpNet {
        match value {
            ipnet::IpNet::V4(ip) => IpNet {
                version: 0,
                prefix: ip.prefix_len() as u32,
                data: ip.addr().octets().to_vec(),
            },
            ipnet::IpNet::V6(ip) => IpNet {
                version: 1,
                prefix: ip.prefix_len() as u32,
                data: ip.addr().octets().to_vec(),
            },
        }
    }
}

impl TryInto<ipnet::IpNet> for IpNet {
    type Error = anyhow::Error;
    fn try_into(self) -> Result<ipnet::IpNet, Self::Error> {
        match self.version {
            0 => {
                let data: [u8; 4] = self
                    .data
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("Illegal IPv4 length encountered"))?;
                let ip: std::net::Ipv4Addr = data.into();
                let prefix: u8 = self.prefix.try_into()?;
                let ipnet = ipnet::Ipv4Net::new(ip, prefix)?;
                Ok(ipnet.into())
            }
            1 => {
                let data: [u8; 16] = self
                    .data
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("Illegal IPv6 length encountered"))?;
                let ip: std::net::Ipv6Addr = data.into();
                let prefix: u8 = self.prefix.try_into()?;
                let ipnet = ipnet::Ipv6Net::new(ip, prefix)?;
                Ok(ipnet.into())
            }
            _ => Err(anyhow::anyhow!("Illegal IP address version")),
        }
    }
}

impl From<std::net::SocketAddr> for SocketAddr {
    fn from(value: std::net::SocketAddr) -> SocketAddr {
        let addr: IpAddr = value.ip().into();
        SocketAddr {
            addr: Some(addr),
            port: value.port() as u32,
        }
    }
}

impl TryInto<std::net::SocketAddr> for SocketAddr {
    type Error = anyhow::Error;
    fn try_into(self) -> Result<std::net::SocketAddr, Self::Error> {
        let addr = self
            .addr
            .ok_or_else(|| anyhow::anyhow!("Missing address for SocketAddr"))?;
        let addr: std::net::IpAddr = addr.try_into()?;
        let port: u16 = self.port.try_into()?;
        Ok(std::net::SocketAddr::new(addr, port))
    }
}

impl From<crate::NetworkState> for NetworkConfig {
    fn from(value: crate::NetworkState) -> NetworkConfig {
        NetworkConfig {
            address: value.address.into_iter().map(|a| a.into()).collect(),
            // FIXME
            forwarding: std::collections::HashMap::new(),
            mtu: value.mtu as u32,
            peers: value
                .peers
                .into_iter()
                .map(|(pubkey, peer)| PeerConfig {
                    allowed_ips: peer.allowed_ips.into_iter().map(|ip| ip.into()).collect(),
                    endpoint: peer.endpoint.map(|e| e.into()),
                    preshared: peer.preshared_key.map(|k| k.into()),
                    pubkey: Some(pubkey.into()),
                })
                .collect(),
            privkey: Some(value.private_key.into()),
        }
    }
}

impl TryInto<crate::NetworkState> for NetworkConfig {
    type Error = anyhow::Error;
    fn try_into(self) -> Result<crate::NetworkState, Self::Error> {
        Ok(crate::NetworkState {
            listen_port: 0,
            mtu: self.mtu.try_into()?,
            // FIXME
            peers: std::collections::BTreeMap::new(),
            private_key: self
                .privkey
                .ok_or_else(|| anyhow::anyhow!("Missing private key"))?
                .try_into()?,
            // FIXME
            proxy: std::collections::HashMap::new(),
            address: self
                .address
                .into_iter()
                .map(|v| v.try_into())
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
}
