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
    let mut device_recv_task = {
        let connection = connection.clone();
        let device = device.clone();
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
                println!("Connection read task died: {e}")
            }
        },
    }
    device_recv_task.abort();
    connection_read_task.abort();
}
