use quinn::{TransportConfig, congestion};
use std::sync::Arc;
use tokio::signal;

pub const SERVER_NAME: &str = "localhost";
pub const SERVER_PORT: u16 = 443;

pub fn build_transport_config() -> TransportConfig {
    let mut transport = TransportConfig::default();

    transport.mtu_discovery_config(Some(quinn::MtuDiscoveryConfig::default()));
    transport.datagram_receive_buffer_size(Some(2 * 1024 * 1024));
    transport.datagram_send_buffer_size(2 * 1024 * 1024);
    transport.max_concurrent_uni_streams(0u32.into());
    transport.max_concurrent_bidi_streams(0u32.into());
    transport.congestion_controller_factory(Arc::new(congestion::BbrConfig::default()));

    transport
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
            println!("Received SIGINT, initiating shutdown...");
        }
        _ = terminate => {
            println!("Received SIGTERM, initiating shutdown...");
        }
    }
}
