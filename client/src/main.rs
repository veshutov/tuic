use anyhow::{Result, anyhow};
use bytes::Bytes;
use rustls::pki_types::CertificateDer;
use std::env;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::str::FromStr;
use std::sync::Arc;
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
    let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::from_str(server_ip)?), 443);
    let endpoint = make_client_endpoint("0.0.0.0:0".parse()?)?;

    loop {
        match endpoint.connect(server_addr, common::SERVER_NAME) {
            Ok(connecting) => {
                match connecting.await {
                    Ok(connection) => {
                        println!("Connected to server {}", connection.remote_address());

                        let connection_recv = connection.clone();
                        let device_recv = device.clone();

                        let recv_worker = tokio::spawn(async move {
                            let connection = connection_recv.clone();
                            let device = device_recv.clone();
                            let mut read_buf: Vec<u8> = vec![0; 65536];
                            loop {
                                let read = device.recv(&mut read_buf).await.unwrap();
                                // println!("[device] read: {}", read);
                                connection
                                    .send_datagram(Bytes::copy_from_slice(&read_buf[0..read]))
                                    .unwrap();
                                // println!("[server] sent {}", read);
                            }
                        });

                        let connection_send = connection.clone();
                        let device_send = device.clone();

                        let send_worker = tokio::spawn(async move {
                            loop {
                                let read = connection_send.read_datagram().await.unwrap();
                                // println!("[server] read: {}", read.len());

                                let _sent = device_send.send(&read).await.unwrap();
                                // println!("[device] sent: {}", sent);
                            }
                        });

                        let _ = tokio::join!(recv_worker, send_worker);
                    }
                    Err(e) => {
                        println!("Could not connect to endpoint: {e}");
                        break;
                    }
                }
            }
            Err(e) => {
                println!("Could not connect to endpoint: {e}");
                break;
            }
        }
    }

    println!("Shutting down");
    endpoint.wait_idle().await;
    Ok(())
}
