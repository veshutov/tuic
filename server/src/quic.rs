use anyhow::Error;
use quinn::{Endpoint, ServerConfig};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};

use std::fs;
use std::path::Path;
use std::{net::SocketAddr, sync::Arc};

pub fn make_server_endpoint(bind_addr: SocketAddr) -> Result<Endpoint, Error> {
    let server_config = configure_server()?;
    let endpoint = Endpoint::server(server_config, bind_addr)?;
    Ok(endpoint)
}

fn configure_server() -> Result<ServerConfig, Error> {
    let (cert_der, key_der) = load_or_generate_cert();
    let mut server_config = ServerConfig::with_single_cert(vec![cert_der.clone()], key_der)?;
    server_config.transport_config(Arc::new(common::build_transport_config()));

    Ok(server_config)
}

fn load_or_generate_cert() -> (CertificateDer<'static>, PrivateKeyDer<'static>) {
    let cert_path = "server_cert.der";
    let key_path = "server_key.der";

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
