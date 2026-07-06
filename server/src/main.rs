use anyhow::Result;
use bytes::Bytes;
use dashmap::DashMap;
use quinn::{Connection, Incoming};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::str::FromStr;
use std::sync::Arc;
use tun_rs::{AsyncDevice, DeviceBuilder};

mod quic;

use crate::quic::make_server_endpoint;

#[tokio::main]
async fn main() -> Result<()> {
    let connection_map: Arc<DashMap<String, Connection>> = Arc::new(DashMap::new());
    let device = Arc::new(
        DeviceBuilder::new()
            .name("utun12")
            .ipv4("10.0.0.2", 24, None)
            .mtu(1100)
            .build_async()?,
    );
    let port = 4433;
    println!("Server port: {port}");
    let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::from_str("0.0.0.0")?), port);
    let (endpoint, _server_cert) = make_server_endpoint(server_addr)?;

    let device_recv = device.clone();
    let connection_map_recv = connection_map.clone();
    let device_worker = tokio::spawn(async move {
        if let Err(e) = handle_device(device_recv, connection_map_recv).await {
            println!("Error while listening device: {e}")
        }
    });

    let device_send = device.clone();
    let connection_map_send = connection_map.clone();
    let endpoint_send = endpoint.clone();
    let connection_worker = tokio::spawn(async move {
        while let Some(incoming_conn) = endpoint_send.accept().await {
            let device = device_send.clone();
            let connection_map = connection_map_send.clone();
            tokio::spawn(async move {
                if let Err(e) = handle_connection(device, connection_map, incoming_conn).await {
                    println!("Connection error: {e}");
                }
            });
        }
    });

    match tokio::signal::ctrl_c().await {
        Ok(()) => println!("Shutting down..."),
        Err(err) => println!("Unable to listen for shutdown signal: {err}"),
    }

    connection_worker.abort();
    device_worker.abort();
    let _ = tokio::join!(device_worker, connection_worker);

    endpoint.wait_idle().await;
    Ok(())
}

async fn handle_connection(
    device: Arc<AsyncDevice>,
    connection_map: Arc<DashMap<String, Connection>>,
    inc_connection: Incoming,
) -> Result<()> {
    let connection = inc_connection.await?;
    connection_map.insert("connection".to_string(), connection.clone());
    let address = connection.remote_address();
    println!("Connection accepted, addr={address}");
    loop {
        let read = connection.read_datagram().await?;
        // println!("[client] read: {}", read.len());

        let _sent = device.clone().send(&read).await?;
        // println!("[remote] sent: {}", sent);
    }
}

async fn handle_device(
    device: Arc<AsyncDevice>,
    connection_map: Arc<DashMap<String, Connection>>,
) -> Result<()> {
    let mut read_buf: Vec<u8> = vec![0; 65536];
    loop {
        let read = device.recv(&mut read_buf).await?;
        // println!("[remote] read: {}", read);

        if let Some(connection) = connection_map.get("connection") {
            if let Err(e) = connection.send_datagram(Bytes::copy_from_slice(&read_buf[0..read])) {
                println!(
                    "Error while sending data to {}: {}",
                    connection.remote_address(),
                    e
                )
            };
            // println!("[client] sent: {}", read);
        }
    }
}
