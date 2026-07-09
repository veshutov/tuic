use anyhow::Result;
use common::await_shutdown;
use std::net::{Ipv4Addr, SocketAddr};

mod config;
mod ip;
mod quic;
mod server;

use crate::config::VpnServerConfig;
use crate::server::VpnServer;

#[tokio::main]
async fn main() -> Result<()> {
    let config = VpnServerConfig {
        device_name: "utun12".into(),
        listen_addr: SocketAddr::from((Ipv4Addr::UNSPECIFIED, common::SERVER_PORT)),
        subnet_addr: Ipv4Addr::new(10, 0, 0, 0),
        subnet_prefix: 24,
        mtu: common::MTU,
    };
    let vpn_server = VpnServer::new(config)?;

    tokio::select! {
        _ = await_shutdown() => {},
        _ = vpn_server.run() => println!("Server died, exiting..."),
    }

    vpn_server.shutdown().await;
    Ok(())
}
