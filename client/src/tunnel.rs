use anyhow::Result;
use bytes::Bytes;
use quinn::Connection;
use std::net::Ipv4Addr;
use std::sync::Arc;
use tracing::info;
use tuic_common::HandshakeMessage;
use tun_rs::DeviceBuilder;

use crate::config::VpnConfig;
use crate::route::setup_vpn_routes;

const MAX_HANDSHAKE_DATA: usize = 100;

pub async fn run_tunnel(connection: Connection, config: &VpnConfig) -> Result<()> {
    let mut recv = connection.accept_uni().await?;
    let handshake_bytes = recv.read_to_end(MAX_HANDSHAKE_DATA).await?;

    let handshake = HandshakeMessage::from(handshake_bytes);
    let addr: Ipv4Addr = handshake.addr;
    let prefix: u8 = handshake.prefix;

    info!("registring tun {addr}/{prefix}");
    let device_name = &config.tun.name;
    let server_address = &config.quic.server_address.ip().to_string();
    let mtu = config.tun.mtu;
    let device = Arc::new(
        DeviceBuilder::new()
            .name(device_name)
            .ipv4(addr, prefix, None)
            .mtu(mtu)
            .build_async()?,
    );

    let _guard = if config.tun.setup_routes {
        Some(setup_vpn_routes(server_address, device_name)?)
    } else {
        None
    };

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

    let result = tokio::select! {
        res = &mut device_recv_task => res.unwrap_or_else(|e| Err(e.into())),
        res = &mut connection_read_task => res.unwrap_or_else(|e| Err(e.into())),
    };
    device_recv_task.abort();
    connection_read_task.abort();

    result
}
