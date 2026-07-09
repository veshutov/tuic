use anyhow::Error;
use quinn::congestion::BbrConfig;
use quinn::{Endpoint, MtuDiscoveryConfig, ServerConfig, TransportConfig};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};

use std::fs;
use std::path::Path;
use std::sync::Arc;

use crate::config::QuicConfig;

pub fn make_server_endpoint(config: &QuicConfig) -> Result<Endpoint, Error> {
    let server_config = configure_server(config)?;
    let endpoint = Endpoint::server(server_config, config.endpoint_address)?;
    Ok(endpoint)
}

fn configure_server(config: &QuicConfig) -> Result<ServerConfig, Error> {
    let (cert_der, key_der) = load_or_generate_cert(config);
    let mut server_config = ServerConfig::with_single_cert(vec![cert_der.clone()], key_der)?;
    server_config.transport_config(Arc::new(build_transport_config(config)));

    Ok(server_config)
}

fn load_or_generate_cert(
    quic_config: &QuicConfig,
) -> (CertificateDer<'static>, PrivateKeyDer<'static>) {
    let cert_path = &quic_config.server_cert;
    let key_path = &quic_config.server_key;

    if Path::new(cert_path).exists() && Path::new(key_path).exists() {
        println!("Loading existing cert + key");
        let cert_bytes = fs::read(cert_path).unwrap();
        let key_bytes = fs::read(key_path).unwrap();

        let cert = CertificateDer::from(cert_bytes);
        let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_bytes));

        (cert, key)
    } else {
        println!("Generating new cert + key");
        let cert = rcgen::generate_simple_self_signed(vec![common::SERVER_NAME.into()]).unwrap();

        let cert_der = cert.cert.der().to_vec();
        let key_der = cert.signing_key.serialize_der();

        fs::write(cert_path, &cert_der).unwrap();
        fs::write(key_path, &key_der).unwrap();

        let cert = CertificateDer::from(cert_der);
        let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_der));

        (cert, key)
    }
}

pub fn build_transport_config(quic_config: &QuicConfig) -> TransportConfig {
    let mut transport = TransportConfig::default();

    transport.mtu_discovery_config(Some(MtuDiscoveryConfig::default()));
    transport.datagram_receive_buffer_size(Some(quic_config.receive_buffer_size_kb * 1024));
    transport.datagram_send_buffer_size(quic_config.send_buffer_size_kb * 1024);
    transport.max_concurrent_uni_streams(0u32.into());
    transport.max_concurrent_bidi_streams(0u32.into());
    transport.congestion_controller_factory(Arc::new(BbrConfig::default()));

    transport
}
