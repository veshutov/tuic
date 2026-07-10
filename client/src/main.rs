use anyhow::Result;
use quinn::VarInt;
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

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let config_path = std::env::args().nth(1).unwrap_or_else(|| "tuic".into());
    info!("loading config from path: {config_path}");
    let config = VpnConfig::from_file(&config_path)?;
    info!("{:#?}", config);

    let server_address = config.quic.server_address;
    let server_name = config.quic.server_name.clone();
    let endpoint = make_client_endpoint(&config.quic)?;
    let backoff = Duration::from_millis(config.quic.reconnect_interval_ms);

    let mut main_task = {
        let endpoint = endpoint.clone();
        tokio::spawn(async move {
            loop {
                let connecting = match endpoint.connect(server_address, &server_name) {
                    Ok(c) => c,
                    Err(e) => {
                        error!("connect failed: {e}, retrying in {backoff:?}");
                        sleep(backoff).await;
                        continue;
                    }
                };
                let connection = match connecting.await {
                    Ok(c) => c,
                    Err(e) => {
                        error!("handshake failed: {e}, retrying in {backoff:?}");
                        sleep(backoff).await;
                        continue;
                    }
                };
                info!("connected to server {}", connection.remote_address());
                match run_tunnel(connection.clone(), &config).await {
                    Ok(_) => error!("session ended, reconnecting..."),
                    Err(e) => error!("error running tunnel: {e}"),
                };
                connection.close(CLOSE_CODE_NORMAL, &[]);
                sleep(backoff).await;
            }
        })
    };

    tokio::select! {
        _ = await_shutdown() => {
            main_task.abort();
        },
        _ = &mut main_task => error!("main task died, exiting..."),
    }

    endpoint.close(VarInt::from_u32(0), &[]);
    endpoint.wait_idle().await;
    Ok(())
}
