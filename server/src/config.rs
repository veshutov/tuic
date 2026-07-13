use std::{collections::HashMap, net::SocketAddr};

use config::{Config, ConfigError, File};
use serde::Deserialize;

/// VPN configuration
#[derive(Debug, Deserialize, Clone)]
pub struct VpnConfig {
    /// TUN device settings
    pub tun: TunConfig,
    /// QUIC protocol settings
    pub quic: QuicConfig,
    /// Username to their secret map
    pub users: HashMap<String, String>,
}

/// TUN virtual interface configuration
#[derive(Debug, Deserialize, Clone)]
pub struct TunConfig {
    /// Device name, empty by default, will be assigned by the OS
    pub name: Option<String>,
    /// Device subnet, client addresses will be allocated from this subnet
    pub subnet: String,
    /// Maximum transmission unit – max packet size in bytes, should not exceed QUIC connection max datagram size
    pub mtu: u16,
    /// Whether to setup automatic routing, see [route module](crate::route)
    pub setup_nat: bool,
}

/// QUIC protocol configuration, used to communicate with the client
#[derive(Debug, Deserialize, Clone)]
pub struct QuicConfig {
    /// Server name, used for certificate validation
    pub server_name: String,
    /// QUIC endpoint address
    pub endpoint_address: SocketAddr,
    /// Receive buffer size in kilobytes, used to buffer QUIC datagrams
    pub receive_buffer_size_kb: usize,
    /// Send buffer size in kilobytes, used to buffer QUIC datagrams
    pub send_buffer_size_kb: usize,
    /// Path to the server certificate
    pub server_cert: String,
    /// Path to the server key
    pub server_key: String,
}

impl VpnConfig {
    pub fn from_file(config_file: &str) -> Result<Self, ConfigError> {
        let setup_nat = if cfg!(target_os = "linux") {
            true
        } else {
            false
        };
        let config = Config::builder()
            .set_default("tun.mtu", 1150)?
            .set_default("tun.setup_nat", setup_nat)?
            .set_default("tun.subnet", "10.0.0.0/24")?
            .set_default("quic.endpoint_address", "0.0.0.0:443")?
            .set_default("quic.reconnect_interval_ms", 1000)?
            .set_default("quic.receive_buffer_size_kb", 10240)?
            .set_default("quic.send_buffer_size_kb", 10240)?
            .add_source(File::with_name(config_file))
            .build()?;
        config.try_deserialize()
    }

    pub fn auth(&self, username: &str, password: String) -> bool {
        self.users.get(username) == Some(&password)
    }
}
