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
            .mtu(1350)
            .build_async()?,
    );
    let server_addr = SocketAddr::new(
        IpAddr::V4(Ipv4Addr::from_str(server_ip).unwrap()),
        4433,
    );

    let cert_bytes = std::fs::read("../server/server_cert.der")?;
    let server_cert = CertificateDer::from(cert_bytes);

    let endpoint = make_client_endpoint("0.0.0.0:0".parse().unwrap(), &[&server_cert])?;
    // connect to server
    let connection = endpoint
        .connect(server_addr, "localhost")
        .unwrap()
        .await
        .unwrap();
    println!("Connected to server {}", connection.remote_address());

    let connection_send = connection.clone();
    let device_send = device.clone();
    tokio::spawn(async move {
        let connection = connection_send.clone();
        let device = device_send.clone();
        let mut read_buf: Vec<u8> = vec![0; 65536];
        loop {
            let read = device.recv(&mut read_buf).await.unwrap();
            println!("[device] read: {}", read);
            connection
                .send_datagram(Bytes::copy_from_slice(&read_buf[0..read]))
                .unwrap();
            println!("[server] sent {}", read);
        }
    });

    loop {
        let read = connection.read_datagram().await.unwrap();
        println!("[server] read: {}", read.len());

        let sent = device.send(&read).await.unwrap();
        println!("[device] sent: {}", sent);
    }

    // Waiting for a stream will complete with an error when the server closes the connection
    let _ = connection.accept_uni().await;

    // Make sure the server has a chance to clean up
    endpoint.wait_idle().await;

    Ok(())
}
