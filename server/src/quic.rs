use anyhow::Error;
use quinn::{Endpoint, ServerConfig};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};

use std::fs;
use std::path::Path;
use std::{net::SocketAddr, sync::Arc};

/// Constructs a QUIC endpoint configured to listen for incoming connections on a certain address
/// and port.
///
/// ## Returns
///
/// - a stream of incoming QUIC connections
/// - server certificate serialized into DER format
#[allow(unused)]
pub(crate) fn make_server_endpoint(
    bind_addr: SocketAddr,
) -> Result<(Endpoint, CertificateDer<'static>), Error> {
    let (server_config, server_cert) = configure_server()?;
    let endpoint = Endpoint::server(server_config, bind_addr)?;
    Ok((endpoint, server_cert))
}

/// Returns default server configuration along with its certificate.
fn configure_server() -> Result<(ServerConfig, CertificateDer<'static>), Error> {
    let (cert_der, key_der) = load_or_generate_cert();
    let mut server_config = ServerConfig::with_single_cert(vec![cert_der.clone()], key_der)?;
    let transport_config = Arc::get_mut(&mut server_config.transport).unwrap();
    transport_config.max_concurrent_uni_streams(0_u8.into());

    Ok((server_config, cert_der))
}

fn load_or_generate_cert() -> (CertificateDer<'static>, PrivateKeyDer<'static>) {
    let cert_path = "server_cert.der";
    let key_path = "server_key.der";

    if Path::new(cert_path).exists() && Path::new(key_path).exists() {
        println!("Loading existing cert + key");
        // Load existing cert + key from disk
        let cert_bytes = fs::read(cert_path).unwrap();
        let key_bytes = fs::read(key_path).unwrap();

        let cert = CertificateDer::from(cert_bytes);
        let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_bytes));

        (cert, key)
    } else {
        println!("Generating new cert + key");
        // Generate fresh cert + key, then persist
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();

        let cert_der = cert.cert.der().to_vec();
        let key_der = cert.signing_key.serialize_der();

        fs::write(cert_path, &cert_der).unwrap();
        fs::write(key_path, &key_der).unwrap();

        let cert = CertificateDer::from(cert_der);
        let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_der));

        (cert, key)
    }
}

#[allow(unused)]
pub(crate) const ALPN_QUIC_HTTP: &[&[u8]] = &[b"hq-29"];
