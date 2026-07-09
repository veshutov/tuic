use std::net::{Ipv4Addr, SocketAddr};

pub struct VpnServerConfig {
    pub device_name: String,
    pub listen_addr: SocketAddr,
    pub subnet_addr: Ipv4Addr,
    pub subnet_prefix: u8,
    pub mtu: u16,
}

impl VpnServerConfig {
    pub fn new(
        device_name: String,
        listen_addr: SocketAddr,
        subnet_addr: Ipv4Addr,
        subnet_prefix: u8,
        mtu: u16,
    ) -> Self {
        Self {
            device_name,
            listen_addr,
            subnet_addr,
            subnet_prefix,
            mtu,
        }
    }
}
