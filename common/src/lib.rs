use anyhow::Result;
use std::net::Ipv4Addr;

use sha2::{Digest, Sha256};
use tokio::signal;
use tracing::info;

use quinn::VarInt;

pub const NONCE_SIZE: usize = 16;
pub const MAX_HANDSHAKE_DATA: usize = 512;
pub const CLOSE_CODE_NORMAL: VarInt = VarInt::from_u32(0);

pub struct ClientHello {
    pub user: String,
    pub session_secret: Vec<u8>,
}

impl ClientHello {
    pub fn from_secret(user: String, secret: &str, nonce: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(secret.as_bytes());
        hasher.update(nonce);
        let session_secret = hasher.finalize().to_vec();
        Self {
            user: user,
            session_secret,
        }
    }

    pub fn verify(&self, secret: &str, nonce: &[u8]) -> bool {
        let mut hasher = Sha256::new();
        hasher.update(secret.as_bytes());
        hasher.update(nonce);
        let expected_session_secret = hasher.finalize().to_vec();
        self.session_secret == expected_session_secret
    }
}

impl Into<Vec<u8>> for ClientHello {
    fn into(self) -> Vec<u8> {
        let mut result = Vec::new();
        result.extend(self.user.as_bytes());
        result.extend(":".as_bytes());
        result.extend(self.session_secret);
        result
    }
}

impl ClientHello {
    pub fn from_vec(value: Vec<u8>) -> Result<Self> {
        let target_byte = b':';
        let pos = value
            .iter()
            .position(|&b| b == target_byte)
            .expect("invalid client helo");
        let user = &value[..pos];
        let session_secret = &value[pos + 1..];
        Ok(Self {
            user: str::from_utf8(user)?.to_owned(),
            session_secret: session_secret.to_vec(),
        })
    }
}

pub struct ServerHello {
    pub addr: Ipv4Addr,
    pub prefix: u8,
}

impl Into<Vec<u8>> for ServerHello {
    fn into(self) -> Vec<u8> {
        vec![
            self.addr.octets()[0],
            self.addr.octets()[1],
            self.addr.octets()[2],
            self.addr.octets()[3],
            self.prefix,
        ]
    }
}

impl From<Vec<u8>> for ServerHello {
    fn from(value: Vec<u8>) -> Self {
        Self {
            addr: Ipv4Addr::new(value[0], value[1], value[2], value[3]),
            prefix: value[4],
        }
    }
}

pub async fn await_shutdown() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("received SIGINT, initiating shutdown...");
        }
        _ = terminate => {
            info!("received SIGTERM, initiating shutdown...");
        }
    }
}
