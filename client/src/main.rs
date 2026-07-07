use anyhow::{Result, anyhow};
use bytes::Bytes;
use quinn::Connection;
use std::env;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tun_rs::DeviceBuilder;

mod quic;

use crate::quic::make_client_endpoint;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        return Err(anyhow!("Invalid arguments, provide server ip"));
    }

    let server_ip = &args[1];
    let device = Arc::new(
        DeviceBuilder::new()
            .name("utun11")
            .ipv4("10.0.0.1", 24, None)
            .mtu(1150)
            .build_async()?,
    );
    let server_addr = SocketAddr::new(
        IpAddr::V4(Ipv4Addr::from_str(server_ip)?),
        common::SERVER_PORT,
    );
    let endpoint = make_client_endpoint("0.0.0.0:0".parse()?)?;
    let backoff = Duration::from_millis(500);

    loop {
        let connecting = match endpoint.connect(server_addr, common::SERVER_NAME) {
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
        run_tunnel(connection, device.clone()).await;
        println!("Session ended, reconnecting...");
    }
}

async fn run_tunnel(connection: Connection, device: Arc<tun_rs::AsyncDevice>) {
    let mut buf = vec![0u8; 1500];

    loop {
        tokio::select! {
            recv_result = device.recv(&mut buf) => {
                let n = match recv_result {
                    Ok(n) => n,
                    Err(e) => { eprintln!("Tun recv error: {e}"); return; }
                };
                if let Err(e) = connection.send_datagram(Bytes::copy_from_slice(&buf[..n])) {
                    eprintln!("Send datagram error: {e}");
                    return;
                }
            }
            dgram_result = connection.read_datagram() => {
                let data = match dgram_result {
                    Ok(d) => d,
                    Err(e) => { eprintln!("Read datagram error: {e}"); return; }
                };
                if let Err(e) = device.send(&data).await {
                    eprintln!("Tun send error: {e}");
                    return;
                }
            }
        }
    }
}
