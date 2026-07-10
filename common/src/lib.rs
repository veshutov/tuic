use std::net::Ipv4Addr;

use tokio::signal;
use tracing::info;

use quinn::VarInt;

pub const CLOSE_CODE_NORMAL: VarInt = VarInt::from_u32(0);

pub struct HandshakeMessage {
    pub addr: Ipv4Addr,
    pub prefix: u8,
}

impl Into<Vec<u8>> for HandshakeMessage {
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

impl From<Vec<u8>> for HandshakeMessage {
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
