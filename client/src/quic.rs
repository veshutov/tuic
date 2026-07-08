use ::anyhow::Error;
use quinn::{ClientConfig, Endpoint};
use rustls::pki_types::CertificateDer;

use std::{net::SocketAddr, sync::Arc};

#[allow(unused)]
pub fn make_client_endpoint(bind_addr: SocketAddr) -> Result<Endpoint, Error> {
    let mut client_cfg = configure_client()?;
    client_cfg.transport_config(Arc::new(common::build_transport_config()));
    let mut endpoint = Endpoint::client(bind_addr)?;
    endpoint.set_default_client_config(client_cfg);
    Ok(endpoint)
}

fn configure_client() -> Result<ClientConfig, Error> {
    let cert_bytes = std::fs::read("server_cert.der")?;
    let server_cert = CertificateDer::from(cert_bytes);
    let mut certs = rustls::RootCertStore::empty();
    certs.add(server_cert)?;
    Ok(ClientConfig::with_root_certificates(Arc::new(certs))?)
}
