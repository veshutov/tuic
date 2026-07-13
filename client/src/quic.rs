use anyhow::{Context, Error};
use quinn::{ClientConfig, Endpoint, MtuDiscoveryConfig, TransportConfig, congestion::BbrConfig};
use rustls::pki_types::CertificateDer;
use tracing::info;

use std::{fs::read, sync::Arc};

use crate::config::QuicConfig;

pub fn make_client_endpoint(config: &QuicConfig) -> Result<Endpoint, Error> {
    let mut client_cfg = configure_client(config)?;
    client_cfg.transport_config(Arc::new(build_transport_config(config)));
    let mut endpoint = Endpoint::client(config.endpoint_address)?;
    endpoint.set_default_client_config(client_cfg);
    Ok(endpoint)
}

fn configure_client(config: &QuicConfig) -> Result<ClientConfig, Error> {
    let mut certs_store = rustls::RootCertStore::empty();

    if config.load_native_certs {
        let load_result = rustls_native_certs::load_native_certs();
        let errors = load_result.errors;
        if !errors.is_empty() {
            return Err(anyhow::anyhow!(
                "failed to load native certificates {:?}",
                errors
            ));
        }
        let natinve_certs = load_result.certs;
        let count = natinve_certs.len();
        certs_store.add_parsable_certificates(natinve_certs);
        info!("loaded {} native certs", count)
    }

    if let Some(server_cert) = &config.server_cert {
        let cert_bytes = read(server_cert).context("failed to load server certificates")?;
        certs_store
            .add(CertificateDer::from(cert_bytes))
            .context("failed to add server certificate to the store")?;
        info!("loaded server cert from {server_cert}")
    }

    Ok(ClientConfig::with_root_certificates(Arc::new(certs_store))?)
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
