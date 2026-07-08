use anyhow::{Context, Result};
use bytes::Bytes;
use common::await_shutdown;
use etherparse::{NetSlice, SlicedPacket};
use quinn::{Connection, Incoming};
use std::net::{Ipv4Addr, SocketAddr};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tun_rs::DeviceBuilder;

mod ip;
mod quic;
mod server;

use crate::ip::IpPool;
use crate::quic::make_server_endpoint;
use crate::server::VpnServer;

#[tokio::main]
async fn main() -> Result<()> {
    let vpn_server = build_vpn_server()?;

    let device_worker = tokio::spawn(listen_device(vpn_server.clone()));
    let connection_worker = tokio::spawn(accept_connections(vpn_server.clone()));

    tokio::select! {
        _ = await_shutdown() => {},
        _ = connection_worker => println!("Connection worker died, exiting..."),
        _ = device_worker => println!("Device worker died, exiting..."),
    }

    vpn_server.shutdown().await;
    Ok(())
}

fn build_vpn_server() -> Result<VpnServer> {
    let subnet_addr = Ipv4Addr::from_str("10.0.0.0")?;
    let subnet_prefix = 24;
    let ip_pool = Arc::new(IpPool::new(subnet_addr, subnet_prefix));

    let device_name = "utun12";
    let device = Arc::new(
        DeviceBuilder::new()
            .name(device_name)
            .ipv4(ip_pool.server_ip(), ip_pool.subnet_prefix, None)
            .mtu(common::MTU)
            .build_async()?,
    );

    let server_addr = SocketAddr::from((Ipv4Addr::UNSPECIFIED, common::SERVER_PORT));
    let endpoint = make_server_endpoint(server_addr)?;

    Ok(VpnServer::new(device, endpoint, ip_pool))
}

async fn accept_connections(vpn_server: VpnServer) {
    while let Some(incoming) = vpn_server.endpoint.accept().await {
        let vpn_server = vpn_server.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_incoming(&vpn_server, incoming).await {
                eprintln!("Connection error: {e}");
            }
        });
    }
}

async fn handle_incoming(vpn_server: &VpnServer, incoming: Incoming) -> Result<()> {
    let connection = incoming.await.context("accepting connection")?;
    let remote_address = connection.remote_address();

    let ip = vpn_server
        .register(connection.clone())
        .context("registering connection")?;
    println!("Connection accepted, client addr={remote_address}, assigned ip={ip}");

    let result = handle_connection(vpn_server, connection.clone(), ip).await;
    vpn_server.unregister(connection);
    result
}

async fn handle_connection(
    vpn_server: &VpnServer,
    connection: Connection,
    ip: Ipv4Addr,
) -> Result<()> {
    let message = format!("{ip}/{}", vpn_server.ip_pool.subnet_prefix);
    connection.send_datagram(Bytes::from(message))?;
    loop {
        let read = connection.read_datagram().await?;
        let _sent = vpn_server.device.send(&read).await?;
    }
}

async fn listen_device(vpn_server: VpnServer) -> Result<()> {
    let mut buf = vec![0u8; 65536];
    let mut consecutive_errors = 0;

    loop {
        match vpn_server.device.recv(&mut buf).await {
            Ok(nbytes) => {
                consecutive_errors = 0;
                route_packet(&vpn_server, &buf[..nbytes]);
            }
            Err(e) => {
                consecutive_errors += 1;
                if consecutive_errors >= 5 {
                    return Err(e).context("tun device unrecoverable after retries");
                }
                tokio::time::sleep(Duration::from_millis(100 * consecutive_errors)).await;
            }
        }
    }
}

fn route_packet(vpn_server: &VpnServer, packet: &[u8]) {
    let Some(dst_ip) = destination_ipv4(packet) else {
        return;
    };
    let Some(connection) = vpn_server.connections.get(&dst_ip) else {
        return;
    };

    if let Err(e) = connection.send_datagram(Bytes::copy_from_slice(packet)) {
        eprintln!(
            "Error while sending data to {}: {}",
            connection.remote_address(),
            e
        )
    };
}

fn destination_ipv4(packet: &[u8]) -> Option<Ipv4Addr> {
    let sliced = match SlicedPacket::from_ip(packet) {
        Ok(sliced) => sliced,
        Err(e) => {
            eprintln!("Failed to parse packet: {e:?}");
            return None;
        }
    };

    match sliced.net {
        Some(NetSlice::Ipv4(v4)) => Some(v4.header().destination_addr()),
        _ => None,
    }
}
