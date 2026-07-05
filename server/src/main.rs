use anyhow::Result;
use bytes::Bytes;
use dashmap::DashMap;
use quinn::Connection;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::str::FromStr;
use std::sync::Arc;
use tun_rs::DeviceBuilder;

mod quic;

use crate::quic::make_server_endpoint;

#[tokio::main]
async fn main() -> Result<()> {
    let connection_map: Arc<DashMap<String, Connection>> = Arc::new(DashMap::new());
    let device = Arc::new(
        DeviceBuilder::new()
            .name("utun12")
            .ipv4("10.0.0.2", 32, None)
            .build_async()?,
    );
    let port = 4433;
    println!("Server port: {}", port);
    let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::from_str("0.0.0.0").unwrap()), port);
    let (endpoint, _server_cert) = make_server_endpoint(server_addr)?;

    let device_recv = device.clone();
    let connection_map_recv = connection_map.clone();
    tokio::spawn(async move {
        loop {
            let mut read_buf: Vec<u8> = vec![0; 65536];
            let read = device_recv.clone().recv(&mut read_buf).await.unwrap();
            println!("[remote] read: {}", read);

            if let Some(connection) = connection_map_recv.get("connection") {
                connection
                    .send_datagram(Bytes::copy_from_slice(&read_buf[0..read]))
                    .unwrap();
                println!("[client] sent: {}", read);
            }
        }
    });

    while let Some(incoming_conn) = endpoint.accept().await {
        let device = device.clone();
        let connection_map = connection_map.clone();
        tokio::spawn(async move {
            let connection = incoming_conn.await.unwrap();
            let address = connection.remote_address();
            connection_map.insert("connection".to_string(), connection.clone());
            println!("Connection accepted, addr={}", address);
            loop {
                let read = connection.read_datagram().await.unwrap();
                println!("[client] read: {}", read.len());

                let sent = device.clone().send(&read).await.unwrap();
                println!("[remote] sent: {}", sent);
            }
        });
    }
    println!("Finished");
    Ok(())
}
