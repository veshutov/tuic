use std::net::SocketAddr;

use config::{Config, ConfigError, File};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct VpnConfig {
    pub tun: TunConfig,
    pub quic: QuicConfig,
}

#[derive(Debug, Deserialize)]
pub struct TunConfig {
    pub name: String,
    pub subnet: String,
    pub mtu: u16,
}

#[derive(Debug, Deserialize)]
pub struct QuicConfig {
    pub endpoint_address: SocketAddr,
    pub receive_buffer_size_kb: usize,
    pub send_buffer_size_kb: usize,
    pub server_cert: String,
    pub server_key: String,
}

impl VpnConfig {
    pub fn new(config_file: &str) -> Result<Self, ConfigError> {
        let config = Config::builder()
            .add_source(File::with_name(config_file))
            .build()?;
        config.try_deserialize()
    }
}
