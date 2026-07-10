use anyhow::{Context, Result, anyhow};
use default_net::get_default_interface;
use std::process::Command;
use tracing::{error, info};

pub struct RoutesGuard {
    tun: String,
    server_ip: String,
}

impl Drop for RoutesGuard {
    fn drop(&mut self) {
        if let Err(e) = cleanup_vpn_routes(&self.server_ip, &self.tun) {
            error!("failed to cleanup routes: {e}");
        }
    }
}

pub fn setup_vpn_routes(server_ip: &str, tun: &str) -> Result<RoutesGuard> {
    info!("setting up routes for {server_ip}");
    let default_gw = main_gw_address()?;

    run_route(&["add", "-host", server_ip, &default_gw])?;
    run_route(&["add", "-net", "0.0.0.0/1", "-interface", tun])?;
    run_route(&["add", "-net", "128.0.0.0/1", "-interface", tun])?;

    Ok(RoutesGuard {
        tun: tun.to_owned(),
        server_ip: server_ip.to_owned(),
    })
}

fn cleanup_vpn_routes(server_ip: &str, tun: &str) -> Result<()> {
    info!("cleaning up routes");
    run_route(&["delete", "-host", server_ip])?;
    run_route(&["delete", "-net", "0.0.0.0/1", "-interface", tun])?;
    run_route(&["delete", "-net", "128.0.0.0/1", "-interface", tun])?;
    Ok(())
}

fn run_route(args: &[&str]) -> Result<()> {
    let status = Command::new("route").args(args).status()?;
    if !status.success() {
        return Err(anyhow!(
            "`route {}` failed with status {status}",
            args.join(" ")
        ));
    }
    Ok(())
}

fn main_gw_address() -> Result<String> {
    let iface = get_default_interface().map_err(|e| anyhow::anyhow!(e))?;
    let gw = iface.gateway.context("Could not detect GW")?;
    Ok(gw.ip_addr.to_string())
}
