use anyhow::{Context, Result, anyhow};
use bytes::Bytes;
use dashmap::DashMap;
use quinn::{Connection, Endpoint, Incoming};
use rand::{Rng, rng};
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use tracing::{error, info};
use tuic_common::{
    CLOSE_CODE_NORMAL, ClientHello, MAX_HANDSHAKE_DATA, NONCE_SIZE, ServerHello,
};
use tun_rs::{AsyncDevice, DeviceBuilder};

use crate::config::VpnConfig;
use crate::ip::IpPool;
use crate::quic::make_server_endpoint;
use crate::route::{apply_vpn_nat, source_match, src_dst_ipv4};
use crate::session::Session;

#[derive(Clone)]
pub struct VpnServer(Arc<VpnServerState>);

pub struct VpnServerState {
    config: VpnConfig,
    device: AsyncDevice,
    endpoint: Endpoint,
    ip_pool: IpPool,
    connections: DashMap<Ipv4Addr, Connection>,
    cancellation_token: CancellationToken,
    task_tracker: TaskTracker,
}

impl std::ops::Deref for VpnServer {
    type Target = VpnServerState;
    fn deref(&self) -> &VpnServerState {
        &self.0
    }
}

const MAX_CONSECUTIVE_ERRORS: u64 = 5;
const ERROR_BACKOFF_BASE_MS: u64 = 100;
const READ_BUF_SIZE: usize = 2048;
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

impl VpnServer {
    pub fn new(config: VpnConfig) -> Result<Self> {
        let tun_config = &config.tun;

        let (addr, prefix) = tun_config
            .subnet
            .split_once('/')
            .context("subnet must be in CIDR form, e.g. 10.0.0.0/24")?;
        let addr: Ipv4Addr = addr.parse().context("invalid subnet address")?;
        let prefix: u8 = prefix.parse().context("invalid subnet prefix")?;
        let ip_pool = IpPool::new(addr, prefix);
        let mut device_builder = DeviceBuilder::new();
        if let Some(name) = &tun_config.name {
            device_builder = device_builder.name(name)
        }
        let device = device_builder
            .ipv4(ip_pool.server_ip(), ip_pool.subnet_prefix, None)
            .mtu(tun_config.mtu)
            .build_async()?;
        let endpoint = make_server_endpoint(&config.quic)?;
        let inner = VpnServerState {
            config,
            device,
            endpoint,
            ip_pool,
            connections: DashMap::new(),
            cancellation_token: CancellationToken::new(),
            task_tracker: TaskTracker::new(),
        };
        Ok(VpnServer(Arc::new(inner)))
    }

    pub async fn run(&self) -> Result<()> {
        let subnet = format!(
            "{}/{}",
            self.ip_pool.subnet_base, self.ip_pool.subnet_prefix
        );

        let _nat_guard = if self.config.tun.setup_nat {
            Some(apply_vpn_nat(&subnet)?)
        } else {
            None
        };

        let device_worker = {
            let server = self.clone();
            self.task_tracker
                .spawn(async move { listen_device(server).await })
        };
        let connection_worker = {
            let server = self.clone();
            self.task_tracker
                .spawn(async move { accept_connections(server).await })
        };

        tokio::select! {
            r = device_worker => r.unwrap_or_else(|e| Err(e.into())),
            r = connection_worker => r.map_err(|e| anyhow::anyhow!(e)),
        }
    }

    pub async fn shutdown(&self) {
        self.cancellation_token.cancel();
        self.endpoint.close(CLOSE_CODE_NORMAL, &[]);
        self.endpoint.wait_idle().await;
        self.task_tracker.close();
        if tokio::time::timeout(SHUTDOWN_TIMEOUT, self.task_tracker.wait())
            .await
            .is_err()
        {
            error!("timed out waiting for server tasks to finish")
        };
    }

    async fn register(&self, connection: &Connection) -> Result<Session> {
        let ip = self
            .ip_pool
            .allocate()
            .ok_or_else(|| anyhow!("ip pool exhausted"))?;
        self.connections.insert(ip, connection.clone());
        let session = Session::new(ip, connection.clone(), self.clone());
        self.handshake(connection, ip).await?;
        Ok(session)
    }

    async fn handshake(&self, connection: &Connection, ip: Ipv4Addr) -> Result<()> {
        let (mut send, mut recv) = connection.open_bi().await?;

        info!("sending nonce");
        let mut nonce = [0u8; NONCE_SIZE];
        rng().fill_bytes(&mut nonce);
        send.write_all(&nonce).await?;

        let client_hello_bytes = recv.read_to_end(MAX_HANDSHAKE_DATA).await?;
        let client_hello = ClientHello::from_vec(client_hello_bytes)?;
        info!("received client hello, user={}", client_hello.user);

        let secret = self
            .config
            .users
            .get(&client_hello.user)
            .context("unknown client")?;
        if !client_hello.verify(secret, &nonce) {
            return Err(anyhow!("authentication failed, user={}", client_hello.user));
        }

        info!(
            "authentication succeeded, assigning ip '{}' to user '{}'",
            ip, client_hello.user
        );
        let message: Vec<u8> = ServerHello {
            addr: ip,
            prefix: self.ip_pool.subnet_prefix,
        }
        .into();

        send.write_all(&message).await?;
        send.finish()?;

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
        info!("registring connection, addr={remote_address}");

        let session = self
            .register(&connection)
            .await
            .context("registering connection")?;
        info!("connection registered, addr={remote_address}");

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
        server.task_tracker.spawn(async move {
            if let Err(e) = vpn_server.handle_incoming(incoming).await {
                error!("connection error: {e:#}");
            }
        });
    }
    info!("accept_connections: shutting down");
}

async fn listen_device(server: VpnServer) -> Result<()> {
    let mut buf = vec![0u8; READ_BUF_SIZE];
    let mut consecutive_errors = 0;

    loop {
        tokio::select! {
            _ = server.cancellation_token.cancelled() => {
                info!("listen_device: shutting down");
                return Ok(());
            }
            result = server.device.recv(&mut buf) => {
                match result {
                    Ok(nbytes) => {
                        consecutive_errors = 0;
                        server.route_to_client(&buf[..nbytes]);
                    }
                    Err(e) => {
                        consecutive_errors += 1;
                        if consecutive_errors > MAX_CONSECUTIVE_ERRORS {
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
    }
}
