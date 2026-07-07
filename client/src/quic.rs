use ::anyhow::Error;
use quinn::{ClientConfig, Endpoint};
use rustls::pki_types::CertificateDer;

use std::{net::SocketAddr, sync::Arc};

/// Constructs a QUIC endpoint configured for use a client only.
///
/// ## Args
///
/// - server_certs: list of trusted certificates.
#[allow(unused)]
pub(crate) fn make_client_endpoint(
    bind_addr: SocketAddr,
    server_certs: &[&[u8]],
) -> Result<Endpoint, Error> {
    let mut client_cfg = configure_client(server_certs)?;
    client_cfg.transport_config(Arc::new(build_transport_config()));
    let mut endpoint = Endpoint::client(bind_addr)?;
    endpoint.set_default_client_config(client_cfg);
    Ok(endpoint)
}

/// Builds default quinn client config and trusts given certificates.
///
/// ## Args
///
/// - server_certs: a list of trusted certificates in DER format.
fn configure_client(server_certs: &[&[u8]]) -> Result<ClientConfig, Error> {
    let mut certs = rustls::RootCertStore::empty();
    for cert in server_certs {
        certs.add(CertificateDer::from(*cert))?;
    }

    Ok(ClientConfig::with_root_certificates(Arc::new(certs))?)
}

fn build_transport_config() -> quinn::TransportConfig {
    let mut transport = quinn::TransportConfig::default();

    transport.mtu_discovery_config(Some(quinn::MtuDiscoveryConfig::default()));
    transport.datagram_receive_buffer_size(Some(2 * 1024 * 1024));
    transport.datagram_send_buffer_size(2 * 1024 * 1024);
    transport.max_concurrent_uni_streams(0u32.into());
    transport.max_concurrent_bidi_streams(0u32.into());
    transport.congestion_controller_factory(Arc::new(quinn::congestion::BbrConfig::default()));

    transport
}

#[allow(unused)]
pub(crate) const ALPN_QUIC_HTTP: &[&[u8]] = &[b"hq-29"];
