use anyhow::Result;
use bytes::BytesMut;
use quinn::Connection;
use std::net::Ipv4Addr;
use tracing::info;
use tuic_common::{ClientHello, MAX_HANDSHAKE_DATA, ServerHello};
use tun_rs::DeviceBuilder;

use crate::config::VpnConfig;
use crate::route::setup_vpn_routes;

const READ_BUF_SIZE: usize = 2048;

pub async fn run_tunnel(connection: &Connection, config: &VpnConfig) -> Result<()> {
    let handshake = handshake(&connection, config).await?;

    let addr: Ipv4Addr = handshake.addr;
    let prefix: u8 = handshake.prefix;

    let server_address = &config.quic.server_address.ip().to_string();
    let mtu = config.tun.mtu;

    let mut device_builder = DeviceBuilder::new();
    if let Some(name) = &config.tun.name {
        device_builder = device_builder.name(name);
    }
    let device = device_builder
        .ipv4(addr, prefix, None)
        .mtu(mtu)
        .build_async()?;
    let device_name = &device.name()?;
    info!("registred {device_name} {addr}/{prefix}");
    let _guard = if config.tun.setup_routes {
        Some(setup_vpn_routes(server_address, device_name)?)
    } else {
        None
    };

    let device_recv_task = async {
        let mut buf = BytesMut::with_capacity(READ_BUF_SIZE);
        loop {
            buf.reserve(READ_BUF_SIZE);
            unsafe {
                buf.set_len(READ_BUF_SIZE);
            }

            let n = device.recv(&mut buf).await?;
            let packet = buf.split_to(n).freeze();
            connection.send_datagram(packet)?;
        }
        #[allow(unreachable_code)]
        Ok::<(), anyhow::Error>(())
    };

    let connection_read_task = async {
        if let Some(max_datagram_size) = connection.max_datagram_size() {
            info!("connection max_datagram_size = {max_datagram_size}");
        }
        loop {
            let data = connection.read_datagram().await?;
            device.send(&data).await?;
        }
        #[allow(unreachable_code)]
        Ok::<(), anyhow::Error>(())
    };

    tokio::select! {
        res = device_recv_task => res,
        res = connection_read_task => res,
    }
}

async fn handshake(
    connection: &Connection,
    config: &VpnConfig,
) -> Result<ServerHello, anyhow::Error> {
    let (mut send, mut recv) = connection.open_bi().await?;
    let client_hello_bytes: Vec<u8> = ClientHello {
        user: config.user.name.clone(),
        secret: config.user.secret.clone(),
    }
    .into();
    info!("sending client hello");
    send.write_all(&client_hello_bytes).await?;
    send.finish()?;

    let server_hello_bytes = recv.read_to_end(MAX_HANDSHAKE_DATA).await?;
    let server_hello = ServerHello::from(server_hello_bytes);
    info!("authentication succeeded");
    Ok(server_hello)
}
