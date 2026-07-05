use anyhow::Result;
use etherparse::{Ipv4HeaderSlice, PacketBuilder};
use rustls::pki_types::CertificateDer;
use smoltcp::socket::tcp::{Socket, SocketBuffer};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{self, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpStream, UdpSocket};
use tokio::time::sleep;
use tun_rs::AsyncDevice;
use tun_rs::DeviceBuilder;

mod quic;

use crate::quic::make_client_endpoint;

#[tokio::main]
async fn main() -> Result<()> {
    let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 5000);

    let cert_bytes = std::fs::read("../server/server_cert.der")?;
    let server_cert = CertificateDer::from(cert_bytes);

    let endpoint = make_client_endpoint("0.0.0.0:0".parse().unwrap(), &[&server_cert])?;
    // connect to server
    let connection = endpoint
        .connect(server_addr, "localhost")
        .unwrap()
        .await
        .unwrap();
    println!("[client] connected: addr={}", connection.remote_address());

    let mut reader = BufReader::new(io::stdin());

    loop {
        let mut line = String::new();

        reader.read_line(&mut line).await?;
        connection.send_datagram(line.clone().into()).unwrap();

        let d = connection.read_datagram().await?;
        println!("From server: {:?}", d);

        if line.len() == 2 {
            break;
        }
    }

    // Waiting for a stream will complete with an error when the server closes the connection
    let _ = connection.accept_uni().await;

    // Make sure the server has a chance to clean up
    endpoint.wait_idle().await;

    Ok(())

    // let dev = Arc::new(
    //     DeviceBuilder::new()
    //         .name("utun11")
    //         .ipv4("10.0.0.1", 24, None)
    //         .build_async()?,
    // );
    // println!("Async TUN device ready!");

    // let mut real_stream = TcpStream::connect(("158.160.156.225", 443)).await?;

    // let mut read_buf: Vec<u8> = vec![0; 65536];
    // let mut write_buf: Vec<u8> = vec![0; 65536];
    // loop {
    //     let dev = dev.clone();
    //     let len = dev.recv(&mut read_buf).await?;
    //     println!("read to local {}", len);
    //     // TODO resend leftovers
    //     let _sent_len = real_stream.write(&read_buf[..len]).await?;
    //     println!("sent to remote {}", _sent_len);

    //     if let Ok(len) = real_stream.try_read(&mut write_buf) {
    //         println!("read from remote {}", len);
    //         if len > 0 {
    //             // TODO resend leftovers
    //             let _sent_len = dev.send(&write_buf[0..len]).await?;
    //             println!("sent to local {}", _sent_len);
    //         }
    //     }

    //     // let packet = buf[..len].to_vec();
    //     // println!("Received packet: {} bytes", len);
    //     // handle_packet(dev, packet).await;
    // }
}

async fn handle_packet(dev: Arc<AsyncDevice>, buf: Vec<u8>) {
    let ip_header = Ipv4HeaderSlice::from_slice(&buf).unwrap();
    let src_ip = ip_header.source_addr();
    let dst_ip = ip_header.destination_addr();

    if ip_header.protocol() != etherparse::IpNumber::TCP {
        return;
    }

    let ip_header_len = ip_header.slice().len();
    let payload_start = ip_header_len;
    if let Ok(str) = String::from_utf8(buf) {
        println!("buffer: {:?}", str);
    }
    // let header = &buf[payload_start..payload_start + 8];
    // let src_port = u16::from_be_bytes([header[0], header[1]]);
    // let dst_port = u16::from_be_bytes([header[2], header[3]]);
    // let payload = &buf[payload_start + 8..];

    // // Create a real socket and forward
    // let sock = UdpSocket::bind("0.0.0.0:0").await?;
    // sock.send_to(payload, (dst_ip, dst_port)).await?;

    // // Wait for reply
    // let mut resp_buf = vec![0u8; 65536];
    // let (len, _from) = sock.recv_from(&mut resp_buf).await?;

    // // Rebuild IP+UDP packet (swap src/dst!)
    // let builder = PacketBuilder::ipv4(dst_ip.octets(), src_ip.octets(), 64).udp(dst_port, src_port);

    // let mut out_buf = Vec::<u8>::new();
    // builder.write(&mut out_buf, &resp_buf[..len]).unwrap();

    // dev.send(&out_buf).await?;
    // Ok(())
}
