use anyhow::Result;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info};
use tuic_common::{CLOSE_CODE_NORMAL, await_shutdown};

mod config;
mod quic;
mod route;
mod tunnel;

use crate::config::VpnConfig;
use crate::quic::make_client_endpoint;
use crate::tunnel::run_tunnel;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let config_path = std::env::args().nth(1).unwrap_or_else(|| "tuic".into());
    info!("loading config from path: {config_path}");
    let config = VpnConfig::from_file(&config_path)?;
    let quic_config = &config.quic;
    info!("{:#?}", config);

    let endpoint = make_client_endpoint(&quic_config)?;
    let backoff = Duration::from_millis(quic_config.reconnect_interval_ms);

    let main_task = async {
        loop {
            let connecting =
                match endpoint.connect(quic_config.server_address, &quic_config.server_name) {
                    Ok(c) => c,
                    Err(e) => {
                        error!("connect failed: {e:#}, retrying in {backoff:?}");
                        sleep(backoff).await;
                        continue;
                    }
                };
            let connection = match connecting.await {
                Ok(c) => c,
                Err(e) => {
                    error!("handshake failed: {e:#}, retrying in {backoff:?}");
                    sleep(backoff).await;
                    continue;
                }
            };
            info!("connected to server {}", connection.remote_address());
            match run_tunnel(&connection, &config).await {
                Ok(_) => error!("session ended, reconnecting..."),
                Err(e) => error!("error running tunnel: {e:#}"),
            };
            connection.close(CLOSE_CODE_NORMAL, &[]);
            sleep(backoff).await;
        }
    };

    tokio::select! {
        _ = await_shutdown() => {},
        _ = main_task => {},
    };

    endpoint.close(CLOSE_CODE_NORMAL, &[]);
    endpoint.wait_idle().await;
    Ok(())
}
