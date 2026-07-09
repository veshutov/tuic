use anyhow::{Result, anyhow};
use std::process::Command;
use tracing::error;

pub struct RoutesGuard {
    tun: String,
    server_ip: String,
}

impl Drop for RoutesGuard {
    fn drop(&mut self) {
        if let Err(e) = cleanup_vpn_routes(&self.server_ip, &self.tun) {
            error!("Failed to cleanup routes: {e}");
        }
    }
}

pub fn setup_vpn_routes(server_ip: &str, tun: &str) -> Result<RoutesGuard> {
    let output = Command::new("sh")
        .arg("-c")
        .arg("route -n get default | awk '/gateway:/{print $2}'")
        .output()?;

    if !output.status.success() {
        return Err(anyhow!("failed to get default gateway"));
    }

    let default_gw = str::from_utf8(&output.stdout)?.trim();

    run_route(&["add", "-host", server_ip, default_gw])?;
    run_route(&["add", "-net", "0.0.0.0/1", "-interface", tun])?;
    run_route(&["add", "-net", "128.0.0.0/1", "-interface", tun])?;
    Ok(RoutesGuard {
        tun: tun.to_owned(),
        server_ip: server_ip.to_owned(),
    })
}

fn cleanup_vpn_routes(server_ip: &str, tun: &str) -> Result<()> {
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
