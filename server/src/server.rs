use anyhow::{Context, Result, anyhow};
use bytes::Bytes;
use dashmap::DashMap;
use quinn::{Connection, Endpoint, Incoming};
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info};
use tuic_common::CLOSE_CODE_NORMAL;
use tun_rs::{AsyncDevice, DeviceBuilder};

use crate::config::VpnConfig;
use crate::ip::IpPool;
use crate::quic::make_server_endpoint;
use crate::route::{apply_vpn_nat, source_match, src_dst_ipv4};
use crate::session::Session;

#[derive(Clone)]
pub struct VpnServer(Arc<VpnServerState>);

pub struct VpnServerState {
    device: AsyncDevice,
    endpoint: Endpoint,
    ip_pool: IpPool,
    connections: DashMap<Ipv4Addr, Connection>,
    setup_nat: bool,
}

impl std::ops::Deref for VpnServer {
    type Target = VpnServerState;
    fn deref(&self) -> &VpnServerState {
        &self.0
    }
}

const MAX_CONSECUTIVE_ERRORS: u64 = 5;
const ERROR_BACKOFF_BASE_MS: u64 = 100;
const TUN_READ_BUF_SIZE: usize = 65536;

impl VpnServer {
    pub fn new(config: VpnConfig) -> Result<Self> {
        let tun_config = config.tun;

        let (addr, prefix) = tun_config
            .subnet
            .split_once('/')
            .context("subnet must be in CIDR form, e.g. 10.0.0.0/24")?;
        let addr: Ipv4Addr = addr.parse().context("invalid subnet address")?;
        let prefix: u8 = prefix.parse().context("invalid subnet prefix")?;
        let ip_pool = IpPool::new(addr, prefix);
        let device = DeviceBuilder::new()
            .name(&tun_config.name)
            .ipv4(ip_pool.server_ip(), ip_pool.subnet_prefix, None)
            .mtu(tun_config.mtu)
            .build_async()?;
        let endpoint = make_server_endpoint(&config.quic)?;
        let inner = VpnServerState {
            device,
            endpoint,
            ip_pool,
            connections: DashMap::new(),
            setup_nat: tun_config.setup_nat,
        };
        Ok(VpnServer(Arc::new(inner)))
    }

    pub async fn run(&self) -> Result<()> {
        let subnet = format!(
            "{}/{}",
            self.ip_pool.subnet_base, self.ip_pool.subnet_prefix
        );

        let _nat_guard = if self.setup_nat {
            Some(apply_vpn_nat(&subnet)?)
        } else {
            None
        };

        let mut device_worker = {
            let server = self.clone();
            tokio::spawn(async move { listen_device(server).await })
        };
        let mut connection_worker = {
            let server = self.clone();
            tokio::spawn(async move { accept_connections(server).await })
        };

        tokio::select! {
            join_result = &mut device_worker => {
                connection_worker.abort();
                join_result.context("Device worker failed")?
            }
            join_result = &mut connection_worker => {
                device_worker.abort();
                join_result.context("Connection worker failed")
            }
        }
    }

    pub async fn shutdown(&self) {
        self.endpoint.close(CLOSE_CODE_NORMAL, &[]);
        self.endpoint.wait_idle().await;
    }

    fn register(&self, connection: &Connection) -> Result<Session> {
        let ip = self
            .ip_pool
            .allocate()
            .ok_or_else(|| anyhow!("Ip pool exhausted"))?;
        self.connections.insert(ip, connection.clone());
        let session = Session::new(ip, connection.clone(), self.clone());
        self.send_welcome(connection, ip)?;
        Ok(session)
    }

    fn send_welcome(&self, connection: &Connection, ip: Ipv4Addr) -> Result<()> {
        let message = format!("{ip}/{}", self.ip_pool.subnet_prefix);
        connection.send_datagram(Bytes::from(message))?;
        Ok(())
    }

    pub fn unregister(&self, ip: Ipv4Addr) {
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
            .register(&connection)
            .context("registering connection")?;
        info!("Connection accepted, addr={remote_address}");
        self.handle_session(session).await
    }

    async fn handle_session(&self, session: Session) -> Result<()> {
        let mut consecutive_errors = 0;
        loop {
            let read = session.read().await?;
            if source_match(session.ip, &read) {
                match self.device.send(&read).await {
                    Ok(_sent) => {
                        consecutive_errors = 0;
                    }
                    Err(e) => {
                        consecutive_errors += 1;
                        if consecutive_errors > MAX_CONSECUTIVE_ERRORS {
                            return Err(e.into());
                        }
                        sleep(Duration::from_millis(
                            ERROR_BACKOFF_BASE_MS * consecutive_errors,
                        ))
                        .await;
                    }
                }
            }
        }
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
    info!("Endpoint accept connections loop exited");
}

async fn listen_device(server: VpnServer) -> Result<()> {
    let mut buf = vec![0u8; TUN_READ_BUF_SIZE];
    let mut consecutive_errors = 0;

    loop {
        match server.device.recv(&mut buf).await {
            Ok(nbytes) => {
                consecutive_errors = 0;
                server.route_to_client(&buf[..nbytes]);
            }
            Err(e) => {
                consecutive_errors += 1;
                if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                    return Err(e).context("tun device unrecoverable after retries");
                }
                sleep(Duration::from_millis(
                    ERROR_BACKOFF_BASE_MS * consecutive_errors,
                ))
                .await;
            }
        }
    }
}
