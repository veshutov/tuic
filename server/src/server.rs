use anyhow::{Context, Result, anyhow};
use bytes::Bytes;
use dashmap::DashMap;
use etherparse::{NetSlice, SlicedPacket};
use quinn::{Connection, Endpoint, Incoming, ReadDatagram, VarInt};
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::Duration;
use tun_rs::{AsyncDevice, DeviceBuilder};

use crate::config::VpnServerConfig;
use crate::ip::IpPool;
use crate::quic::make_server_endpoint;

#[derive(Clone)]
pub struct VpnServer {
    device: Arc<AsyncDevice>,
    endpoint: Endpoint,
    ip_pool: Arc<IpPool>,
    connections: Arc<DashMap<Ipv4Addr, Connection>>,
    assigned_ips: Arc<DashMap<usize, Ipv4Addr>>,
}

impl VpnServer {
    pub fn new(config: VpnServerConfig) -> Result<Self> {
        let ip_pool = Arc::new(IpPool::new(config.subnet_addr, config.subnet_prefix));
        let device = Arc::new(
            DeviceBuilder::new()
                .name(&config.device_name)
                .ipv4(ip_pool.server_ip(), ip_pool.subnet_prefix, None)
                .mtu(config.mtu)
                .build_async()?,
        );
        let endpoint = make_server_endpoint(config.listen_addr)?;

        Ok(VpnServer {
            device,
            endpoint,
            ip_pool,
            connections: Arc::new(DashMap::new()),
            assigned_ips: Arc::new(DashMap::new()),
        })
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
            _ = device_worker => {println!("device_w")}
            _ = connection_worker => {println!("conn_w")}
        }

        Ok(())
    }

    pub async fn shutdown(&self) {
        self.endpoint.close(VarInt::from_u32(0), &[0]);
        self.endpoint.wait_idle().await;
    }

    fn register(&self, connection: Connection) -> Result<Session> {
        let id = connection.stable_id();
        let ip = self
            .ip_pool
            .allocate()
            .ok_or_else(|| anyhow!("Ip pool exhausted"))?;
        self.connections.insert(ip, connection.clone());
        self.assigned_ips.insert(id, ip);
        let session = Session {
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

    fn unregister(&self, connection: &Connection) {
        let id = connection.stable_id();
        if let Some((_, ip)) = self.assigned_ips.remove(&id) {
            self.ip_pool.release(ip);
            self.connections.remove(&ip);
        }
    }

    fn route_to_client(&self, packet: &[u8]) {
        let Some(dst_ip) = destination_ipv4(packet) else {
            return;
        };
        let Some(connection) = self.connections.get(&dst_ip) else {
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

    async fn handle_incoming(&self, incoming: Incoming) -> Result<()> {
        let connection = incoming.await.context("accepting connection")?;
        let remote_address = connection.remote_address();

        let session = self
            .register(connection.clone())
            .context("registering connection")?;
        println!("Connection accepted, addr={remote_address}");
        self.handle_session(session).await
    }

    async fn handle_session(&self, session: Session) -> Result<()> {
        loop {
            let read = session.read().await?;
            let _sent = self.device.send(&read).await?;
        }
    }
}

struct Session {
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
        self.server.unregister(&self.connection);
    }
}

async fn accept_connections(server: VpnServer) {
    while let Some(incoming) = server.endpoint.accept().await {
        let vpn_server = server.clone();
        tokio::spawn(async move {
            if let Err(e) = vpn_server.handle_incoming(incoming).await {
                eprintln!("Connection error: {e}");
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
