tonic::include_proto!("gateway");

impl From<wireguard_keys::Pubkey> for Pubkey {
    fn from(value: wireguard_keys::Pubkey) -> Pubkey {
        Pubkey {
            data: value.to_vec(),
        }
    }
}

impl TryInto<wireguard_keys::Pubkey> for Pubkey {
    type Error = wireguard_keys::ParseError;
    fn try_into(self) -> Result<wireguard_keys::Pubkey, Self::Error> {
        let data: &[u8] = &self.data;
        let key = data.try_into()?;
        Ok(key)
    }
}

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

impl From<std::net::SocketAddr> for SocketAddr {
    fn from(value: std::net::SocketAddr) -> SocketAddr {
        let addr: IpAddr = value.ip().into();
        SocketAddr {
            addr: Some(addr),
            port: value.port() as u32,
        }
    }
}
