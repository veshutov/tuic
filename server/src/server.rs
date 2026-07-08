use anyhow::{Result, anyhow};
use dashmap::DashMap;
use quinn::{Connection, Endpoint, VarInt};
use std::net::Ipv4Addr;
use std::sync::Arc;
use tun_rs::AsyncDevice;

use crate::ip::IpPool;

#[derive(Clone)]
pub struct VpnServer {
    pub device: Arc<AsyncDevice>,
    pub endpoint: Endpoint,
    pub ip_pool: Arc<IpPool>,
    pub connections: Arc<DashMap<Ipv4Addr, Connection>>,
    pub assigned_ips: Arc<DashMap<usize, Ipv4Addr>>,
}

impl VpnServer {
    pub fn new(device: Arc<AsyncDevice>, endpoint: Endpoint, ip_pool: Arc<IpPool>) -> Self {
        VpnServer {
            device,
            endpoint,
            ip_pool,
            connections: Arc::new(DashMap::new()),
            assigned_ips: Arc::new(DashMap::new()),
        }
    }

    pub fn register(&self, connection: Connection) -> Result<Ipv4Addr> {
        let id = connection.stable_id();
        let ip = self
            .ip_pool
            .allocate()
            .ok_or_else(|| anyhow!("No vacant ip in pool"))?;
        self.connections.insert(ip, connection);
        self.assigned_ips.insert(id, ip);
        Ok(ip)
    }

    pub fn unregister(&self, connection: Connection) {
        let id = connection.stable_id();
        if let Some(ip) = self.assigned_ips.get(&id) {
            self.ip_pool.release(*ip);
            self.connections.remove(&ip);
        }
        self.assigned_ips.remove(&id);
    }

    pub async fn shutdown(&self) {
        self.endpoint.close(VarInt::from_u32(0), &[0]);
        self.endpoint.wait_idle().await;
    }
}
