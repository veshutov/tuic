use anyhow::{Context, Result};
use bytes::Bytes;
use common::await_shutdown;
use quinn::{Connection, VarInt};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tun_rs::DeviceBuilder;

mod config;
mod quic;

use crate::config::{TunConfig, VpnConfig};
use crate::quic::make_client_endpoint;

#[tokio::main]
async fn main() -> Result<()> {
    let config = VpnConfig::new("config")?;
    println!("{:#?}", config);

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
                        eprintln!("Connect failed: {e}, retrying in {backoff:?}");
                        sleep(backoff).await;
                        continue;
                    }
                };
                let connection = match connecting.await {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("Handshake failed: {e}, retrying in {backoff:?}");
                        sleep(backoff).await;
                        continue;
                    }
                };
                println!("Connected to server {}", connection.remote_address());
                match run_tunnel(connection, &config.tun).await {
                    Ok(_) => println!("Session ended, reconnecting..."),
                    Err(e) => eprintln!("Error running tunnel: {e}"),
                };
                sleep(backoff).await;
            }
        })
    };

    tokio::select! {
        _ = await_shutdown() => {
            main_task.abort();
        },
        _ = &mut main_task => println!("Main task died, exiting..."),
    }

    endpoint.close(VarInt::from_u32(0), &[]);
    endpoint.wait_idle().await;
    Ok(())
}

async fn run_tunnel(connection: Connection, config: &TunConfig) -> Result<()> {
    let data = connection.read_datagram().await?;
    let message: Vec<&str> = std::str::from_utf8(&data)?.split("/").collect();
    let ip = *message.get(0).context("tun address")?;
    let subnet_prefix: u8 = message.get(1).context("tun subnet")?.parse()?;
    println!("Registring tun: {ip}/{subnet_prefix}");
    let device = Arc::new(
        DeviceBuilder::new()
            .name(config.name.clone())
            .ipv4(ip, subnet_prefix, None)
            .mtu(config.mtu)
            .build_async()?,
    );

    let mut device_recv_task = {
        let device = device.clone();
        let connection = connection.clone();
        tokio::spawn(async move {
            let mut buf = vec![0u8; 1500];
            loop {
                let n = device.recv(&mut buf).await?;
                connection.send_datagram(Bytes::copy_from_slice(&buf[..n]))?;
            }
            #[allow(unreachable_code)]
            Ok::<(), anyhow::Error>(())
        })
    };

    let mut connection_read_task = {
        let connection = connection.clone();
        let device = device.clone();
        tokio::spawn(async move {
            loop {
                let data = connection.read_datagram().await?;
                device.send(&data).await?;
            }
            #[allow(unreachable_code)]
            Ok::<(), anyhow::Error>(())
        })
    };

    tokio::select! {
        res = &mut device_recv_task => {
            if let Err(e) = res {
                eprintln!("Tun receive task died: {e}")
            }
        },
        res = &mut connection_read_task => {
            if let Err(e) = res {
                eprintln!("Connection read task died: {e}")
            }
        },
    }
    device_recv_task.abort();
    connection_read_task.abort();

    Ok(())
}
