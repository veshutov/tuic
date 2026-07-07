#!/bin/bash
set -e

if [[ "$EUID" -ne 0 ]]; then
  echo "Please run with sudo: sudo $0"
  exit 1
fi

VPN_SERVER_IP="$1"
TUN_IF="utun11"
VPN_BINARY="../target/release/tun-client"
ORIG_GW=$(route -n get default | awk '/gateway:/{print $2}')
VPN_PID=""

cleanup() {
  trap - EXIT INT TERM

  echo "Cleaning up routes..."
  route delete -host "$VPN_SERVER_IP" >/dev/null 2>&1 || true
  route delete -net 0.0.0.0/1   -interface "$TUN_IF" >/dev/null 2>&1 || true
  route delete -net 128.0.0.0/1 -interface "$TUN_IF" >/dev/null 2>&1 || true

  if [[ -n "$VPN_PID" ]]; then
    kill "$VPN_PID" >/dev/null 2>&1 || true
  fi
  echo "Done"
}
trap cleanup EXIT INT TERM

echo "Starting VPN client..."
"$VPN_BINARY" "$VPN_SERVER_IP" &
VPN_PID=$!

echo "Waiting for $TUN_IF to come up..."
until ifconfig "$TUN_IF" >/dev/null 2>&1; do
  sleep 0.2
done
sleep 0.5

route add -host "$VPN_SERVER_IP" "$ORIG_GW"
route add -net 0.0.0.0/1   -interface "$TUN_IF"
route add -net 128.0.0.0/1 -interface "$TUN_IF"

echo "VPN is active"
wait "$VPN_PID"
