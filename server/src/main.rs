use anyhow::Result;
use tracing::info;
use tuic_common::await_shutdown;

mod config;
mod ip;
mod quic;
mod route;
mod server;
mod session;

use crate::config::VpnConfig;
use crate::server::VpnServer;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let config_path = std::env::args().nth(1).unwrap_or_else(|| "tuic".into());
    info!("Loading config from path: {config_path}");
    let config = VpnConfig::new(&config_path)?;
    info!("{:#?}", config);
    let vpn_server = VpnServer::new(config)?;

    let run_handle = {
        let server = vpn_server.clone();
        tokio::spawn(async move { server.run().await })
    };

    let result = tokio::select! {
        _ = await_shutdown() => Ok(()),
        r = run_handle => r.unwrap_or_else(|e| Err(e.into())),
    };

    vpn_server.shutdown().await;
    result
}
