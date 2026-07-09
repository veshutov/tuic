use anyhow::Result;
use common::await_shutdown;
use std::net::{Ipv4Addr, SocketAddr};
use std::str::FromStr;

mod config;
mod ip;
mod quic;
mod server;

use crate::config::VpnServerConfig;
use crate::server::VpnServer;

#[tokio::main]
async fn main() -> Result<()> {
    let config = VpnServerConfig::new(
        "utun12".to_string(),
        SocketAddr::from((Ipv4Addr::UNSPECIFIED, common::SERVER_PORT)),
        Ipv4Addr::from_str("10.0.0.0")?,
        24,
        common::MTU,
    );
    let vpn_server = VpnServer::new(config)?;

    tokio::select! {
        _ = await_shutdown() => {},
        _ = vpn_server.run() => println!("Server died, exiting..."),
    }

    vpn_server.shutdown().await;
    Ok(())
}
