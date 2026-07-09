use std::net::{Ipv4Addr, SocketAddr};

pub struct VpnServerConfig {
    pub device_name: String,
    pub listen_addr: SocketAddr,
    pub subnet_addr: Ipv4Addr,
    pub subnet_prefix: u8,
    pub mtu: u16,
}