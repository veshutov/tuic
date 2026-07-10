# tuic

Simple tun-quic-tun vpn

Client:
  - connects to the server via QUIC protocol
  - receives assigned IP address from the server
  - creates a tun interface with the assigned IP address
  - creates route that forwards all traffic to the tun interface, excluding traffic to the server (only for Mac OS, see [route.rs](client/route.rs))

Server:
  - creates tun interface with specified IP address range (e.g. 10.0.0.0/24)
  - creates NAT rule that forwards all traffic from the tun interface via main interface with masquerading (only for Linux, see [route.rs](server/route.rs))  
  - listens for incoming QUIC connections
  - assigns IP addresses to clients from the same range
  - forwards all client traffic via the tun interface
