use std::sync::Arc;

use quinn::{TransportConfig, congestion};

pub const SERVER_NAME: &str = "localhost";

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
