use std::net::SocketAddr;

use config::{Config, ConfigError, File};
use serde::Deserialize;

/// VPN configuration
#[derive(Debug, Deserialize)]
pub struct VpnConfig {
    /// TUN device configuration
    pub tun: TunConfig,
    /// QUIC protocolconfiguration
    pub quic: QuicConfig,
    /// User configuration
    pub user: UserConfig,
}

/// TUN virtual interface configuration
#[derive(Debug, Deserialize)]
pub struct TunConfig {
    /// TUN device name, assigned by OS if empty
    pub name: Option<String>,
    /// Maximum Transmission Unit – max packet size in bytes, should not exceed QUIC connection max datagram size
    pub mtu: u16,
    /// Whether to setup routing to the device, see [route module](crate::route)
    pub setup_routes: bool,
}

/// QUIC protocol configuration, used to communicate with the server
#[derive(Debug, Deserialize)]
pub struct QuicConfig {
    /// Reconnection interval in milliseconds
    pub reconnect_interval_ms: u64,
    /// Server address
    pub server_address: SocketAddr,
    /// Server name
    pub server_name: String,
    /// QUIC endpoint address
    pub endpoint_address: SocketAddr,
    /// Receive buffer size in kilobytes, used to buffer QUIC datagrams
    pub receive_buffer_size_kb: usize,
    /// Send buffer size in kilobytes, used to buffer QUIC datagrams
    pub send_buffer_size_kb: usize,
    /// Path to the server certificate
    pub server_cert: Option<String>,
    /// Whether to load native certificates
    pub load_native_certs: bool,
}

/// Client configuration
#[derive(Debug, Deserialize)]
pub struct UserConfig {
    /// User name used for authentication
    pub name: String,
    /// User secret used for authentication
    pub secret: String,
}

impl VpnConfig {
    pub fn from_file(config_file: &str) -> Result<Self, ConfigError> {
        let setup_routes = if cfg!(target_os = "macos") {
            true
        } else {
            false
        };
        let config = Config::builder()
            .set_default("tun.mtu", 1150)?
            .set_default("tun.setup_routes", setup_routes)?
            .set_default("quic.reconnect_interval_ms", 1000)?
            .set_default("quic.receive_buffer_size_kb", 10240)?
            .set_default("quic.send_buffer_size_kb", 10240)?
            .set_default("quic.load_native_certs", true)?
            .add_source(File::with_name(config_file))
            .build()?;
        config.try_deserialize()
    }
}
