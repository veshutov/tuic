use std::net::Ipv4Addr;

use bytes::Bytes;
use quinn::{Connection, ConnectionError};
use tuic_common::CLOSE_CODE_NORMAL;

use crate::server::VpnServer;

pub struct Session {
    pub ip: Ipv4Addr,
    connection: Connection,
    server: VpnServer,
}

impl Session {
    pub fn new(ip: Ipv4Addr, connection: Connection, server: VpnServer) -> Self {
        Self {
            ip,
            connection,
            server,
        }
    }

    pub async fn read(&self) -> Result<Bytes, ConnectionError> {
        self.connection.read_datagram().await
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.server.unregister(self.ip);
        self.connection.close(CLOSE_CODE_NORMAL, &[]);
    }
}
