# tuic

Simple TUN-QUIC-TUN VPN

Server can generate self-signed TLS certificate on startup or use existing ones ('server_cert' and 'server_key' options in config), clients can load the certificate ('server_cert' option in config) or use native system trust store ('load_native_certs' option in config).

Needs sudo to create TUN device.

Client:
  - connects to the server via QUIC protocol
  - provides its credentials
  - receives IP address from the server
  - creates a TUN interface with the assigned IP address
  - creates routes that forward all traffic via TUN interface, excluding traffic to the server (automatic only for Mac OS, see [route.rs](client/src/route.rs) and 'setup_routes' option in config)

Server:
  - creates TUN interface with specified IP address range (e.g. 10.0.0.0/24)
  - creates NAT rule that forwards all traffic from the TUN interface via main interface with masquerading (automatic only for Linux, see [route.rs](server/src/route.rs) and 'setup_nat' option in config)  
  - listens for incoming QUIC connections
  - checks clients credentials
  - assigns IP addresses to clients from the same range (e.g. 10.0.0.0/24)
  - forwards all client traffic via the TUN interface

  Configuration file path can be provided via command line argument, if not present, it will try to load it from './tuic.toml'

  Example configs:
  - [client](client/tuic.toml)
  - [server](server/tuic.toml)
