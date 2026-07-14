use anyhow::Result;
use clap::Parser;
use tracing::info;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::{Registry, fmt, prelude::*};
use tuic_common::await_shutdown;

mod config;
mod ip;
mod quic;
mod route;
mod server;
mod session;

use crate::config::VpnConfig;
use crate::server::VpnServer;

#[derive(Parser, Debug)]
#[command(version, about = "Tuic VPN server")]
struct Args {
    /// Path to the configuration file, defaults to "tuic"
    #[arg(value_name = "FILE")]
    config: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let stdout_layer = fmt::layer()
        .with_writer(std::io::stdout)
        .with_ansi(true)
        .with_filter(LevelFilter::INFO);
    Registry::default().with(stdout_layer).init();

    let args = Args::parse();
    let config_path = match args.config {
        Some(path) => path,
        None => "tuic".into(),
    };
    info!("Loading config from path: {config_path}");
    let config = VpnConfig::from_file(&config_path)?;
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
