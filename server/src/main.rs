use anyhow::Result;
use common::await_shutdown;

mod config;
mod ip;
mod quic;
mod server;

use crate::config::VpnConfig;
use crate::server::VpnServer;

#[tokio::main]
async fn main() -> Result<()> {
    let config = VpnConfig::new("config")?;
    println!("{:#?}", config);
    let vpn_server = VpnServer::new(config)?;

    tokio::select! {
        _ = await_shutdown() => {},
        _ = vpn_server.run() => println!("Server died, exiting..."),
    }

    vpn_server.shutdown().await;
    Ok(())
}
