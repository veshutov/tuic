use anyhow::Result;
use tracing::{error, info};
use tuic_common::await_shutdown;

mod config;
mod ip;
mod quic;
mod route;
mod server;

use crate::config::VpnConfig;
use crate::server::VpnServer;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let config = VpnConfig::new("config")?;
    info!("{:#?}", config);
    let vpn_server = VpnServer::new(config)?;

    tokio::select! {
        _ = await_shutdown() => {},
        _ = vpn_server.run() => error!("Server died, exiting..."),
    }

    vpn_server.shutdown().await;
    Ok(())
}
