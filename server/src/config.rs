use std::{collections::HashMap, net::SocketAddr};

use config::{Config, ConfigError, File};
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct VpnConfig {
    pub tun: TunConfig,
    pub quic: QuicConfig,
    pub users: HashMap<String, String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct TunConfig {
    pub name: String,
    pub subnet: String,
    pub mtu: u16,
    pub setup_nat: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct QuicConfig {
    pub server_name: String,
    pub endpoint_address: SocketAddr,
    pub receive_buffer_size_kb: usize,
    pub send_buffer_size_kb: usize,
    pub server_cert: String,
    pub server_key: String,
}

impl VpnConfig {
    pub fn from_file(config_file: &str) -> Result<Self, ConfigError> {
        let config = Config::builder()
            .add_source(File::with_name(config_file))
            .build()?;
        config.try_deserialize()
    }

    pub fn auth(&self, username: &str, password: String) -> bool {
        self.users.get(username) == Some(&password)
    }
}
