use anyhow::{Context, Result, anyhow};
use bytes::Bytes;
use dashmap::DashMap;
use etherparse::{NetSlice, SlicedPacket};
use quinn::{Connection, Endpoint, Incoming, ReadDatagram, VarInt};
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info};
use tun_rs::{AsyncDevice, DeviceBuilder};

use crate::config::VpnConfig;
use crate::ip::IpPool;
use crate::quic::make_server_endpoint;

#[derive(Clone)]
pub struct VpnServer(Arc<Inner>);

pub struct Inner {
    device: AsyncDevice,
    endpoint: Endpoint,
    ip_pool: IpPool,
    connections: DashMap<Ipv4Addr, Connection>,
}

impl std::ops::Deref for VpnServer {
    type Target = Inner;
    fn deref(&self) -> &Inner {
        &self.0
    }
}

impl VpnServer {
    pub fn new(config: VpnConfig) -> Result<Self> {
        let tun_config = config.tun;

        let (addr, prefix) = tun_config.subnet.split_once('/')
            .context("subnet must be in CIDR form, e.g. 10.0.0.0/24")?;
        let addr: Ipv4Addr = addr.parse()?;
        let prefix: u8 = prefix.parse()?;
        let ip_pool = IpPool::new(addr, prefix);
        let device = DeviceBuilder::new()
            .name(&tun_config.name)
            .ipv4(ip_pool.server_ip(), ip_pool.subnet_prefix, None)
            .mtu(tun_config.mtu)
            .build_async()?;
        let endpoint = make_server_endpoint(&config.quic)?;
        let inner = Inner {
            device,
            endpoint,
            ip_pool,
            connections: DashMap::new(),
        };
        Ok(VpnServer(Arc::new(inner)))
    }

    pub async fn run(&self) -> Result<()> {
        let device_worker = {
            let server = self.clone();
            tokio::spawn(async move { listen_device(server).await })
        };
        let connection_worker = {
            let server = self.clone();
            tokio::spawn(async move { accept_connections(server).await })
        };

        tokio::select! {
            _ = device_worker => {}
            _ = connection_worker => {}
        }

        Ok(())
    }

    pub async fn shutdown(&self) {
        self.endpoint.close(VarInt::from_u32(0), &[]);
        self.endpoint.wait_idle().await;
    }

    fn register(&self, connection: Connection) -> Result<Session> {
        let ip = self
            .ip_pool
            .allocate()
            .ok_or_else(|| anyhow!("Ip pool exhausted"))?;
        self.connections.insert(ip, connection.clone());
        let session = Session {
            ip,
            connection: connection.clone(),
            server: self.clone(),
        };
        self.send_welcome(connection, ip)?;
        Ok(session)
    }

    fn send_welcome(&self, connection: Connection, ip: Ipv4Addr) -> Result<()> {
        let message = format!("{ip}/{}", self.ip_pool.subnet_prefix);
        connection.send_datagram(Bytes::from(message))?;
        Ok(())
    }

    fn unregister(&self, ip: Ipv4Addr) {
        self.connections.remove(&ip);
        self.ip_pool.release(ip);
    }

    fn route_to_client(&self, packet: &[u8]) {
        let Some((_, dst_ip)) = src_dst_ipv4(packet) else {
            return;
        };
        let Some(connection) = self.connections.get(&dst_ip) else {
            return;
        };

        if let Err(e) = connection.send_datagram(Bytes::copy_from_slice(packet)) {
            error!(
                "Error while sending data to {}: {}",
                connection.remote_address(),
                e
            )
        };
    }

    async fn handle_incoming(&self, incoming: Incoming) -> Result<()> {
        let connection = incoming.await.context("accepting connection")?;
        let remote_address = connection.remote_address();

        let session = self
            .register(connection.clone())
            .context("registering connection")?;
        info!("Connection accepted, addr={remote_address}");
        self.handle_session(session).await
    }

    async fn handle_session(&self, session: Session) -> Result<()> {
        loop {
            let read = session.read().await?;
            if source_match(session.ip, &read) {
                let _sent = self.device.send(&read).await?;
            } else {
                error!("Invalid packet source")
            }
        }
    }
}

struct Session {
    ip: Ipv4Addr,
    connection: Connection,
    server: VpnServer,
}

impl Session {
    pub fn read(&self) -> ReadDatagram<'_> {
        self.connection.read_datagram()
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.server.unregister(self.ip);
    }
}

async fn accept_connections(server: VpnServer) {
    while let Some(incoming) = server.endpoint.accept().await {
        let vpn_server = server.clone();
        tokio::spawn(async move {
            if let Err(e) = vpn_server.handle_incoming(incoming).await {
                error!("Connection error: {e}");
            }
        });
    }
}

async fn listen_device(server: VpnServer) -> Result<()> {
    let mut buf = vec![0u8; 65536];
    let mut consecutive_errors = 0;

    loop {
        match server.device.recv(&mut buf).await {
            Ok(nbytes) => {
                consecutive_errors = 0;
                server.route_to_client(&buf[..nbytes]);
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

fn source_match(session_ip: Ipv4Addr, packet: &[u8]) -> bool {
    if let Some((src_ip, _)) = src_dst_ipv4(packet) {
        src_ip == session_ip
    } else {
        false
    }
}

fn src_dst_ipv4(packet: &[u8]) -> Option<(Ipv4Addr, Ipv4Addr)> {
    let sliced = match SlicedPacket::from_ip(packet) {
        Ok(sliced) => sliced,
        Err(e) => {
            error!("Failed to parse packet: {e:?}");
            return None;
        }
    };

    match sliced.net {
        Some(NetSlice::Ipv4(v4)) => {
            Some((v4.header().source_addr(), v4.header().destination_addr()))
        }
        _ => None,
    }
}
