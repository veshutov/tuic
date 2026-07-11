use anyhow::Error;
use quinn::{ClientConfig, Endpoint, MtuDiscoveryConfig, TransportConfig, congestion::BbrConfig};
use rustls::pki_types::CertificateDer;

use std::sync::Arc;

use crate::config::QuicConfig;

pub fn make_client_endpoint(config: &QuicConfig) -> Result<Endpoint, Error> {
    let mut client_cfg = configure_client(config)?;
    client_cfg.transport_config(Arc::new(build_transport_config(config)));
    let mut endpoint = Endpoint::client(config.endpoint_address)?;
    endpoint.set_default_client_config(client_cfg);
    Ok(endpoint)
}

fn configure_client(config: &QuicConfig) -> Result<ClientConfig, Error> {
    let cert_bytes = std::fs::read(&config.server_cert)?;
    let server_cert = CertificateDer::from(cert_bytes);
    let mut certs = rustls::RootCertStore::empty();
    certs.add(server_cert)?;
    Ok(ClientConfig::with_root_certificates(Arc::new(certs))?)
}

fn build_transport_config(quic_config: &QuicConfig) -> TransportConfig {
    let mut transport = TransportConfig::default();

    transport.enable_segmentation_offload(true);
    transport.mtu_discovery_config(Some(MtuDiscoveryConfig::default()));
    transport.datagram_receive_buffer_size(Some(quic_config.receive_buffer_size_kb * 1024));
    transport.datagram_send_buffer_size(quic_config.send_buffer_size_kb * 1024);
    transport.max_concurrent_uni_streams(0u32.into());
    transport.max_concurrent_bidi_streams(10u32.into());
    transport.congestion_controller_factory(Arc::new(BbrConfig::default()));

    transport
}
