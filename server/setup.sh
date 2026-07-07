#!/bin/bash
set -e

sudo nft -f - <<'EOF'
table ip vpn_nat
flush table ip vpn_nat

define WAN_IF   = "eth0"
define VPN_NET  = 10.0.0.0/24

table ip vpn_nat {
    chain postrouting {
        type nat hook postrouting priority 100;
        ip saddr $VPN_NET oifname $WAN_IF masquerade
    }
}
EOF

sudo ./tun-server
