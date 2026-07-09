use anyhow::{Result, anyhow};
use default_net::get_default_interface;
use std::io::Write;
use std::process::{Command, Stdio};
use tracing::{error, info};

pub struct NatGuard;

impl Drop for NatGuard {
    fn drop(&mut self) {
        if let Err(e) = teardown_vpn_nat() {
            error!("Failed to teardown VPN NAT: {e}");
        }
    }
}

pub fn apply_vpn_nat(vpn_net: &str) -> Result<NatGuard> {
    info!("Applying NAT for {vpn_net}");
    let main_nic = main_nic()?;
    let ruleset = format!(
        r#"
table ip vpn_nat
flush table ip vpn_nat

table ip vpn_nat {{
    chain postrouting {{
        type nat hook postrouting priority 100;
        ip saddr {vpn_net} oifname "{main_nic}" masquerade
    }}
}}
"#
    );

    let mut child = Command::new("sudo")
        .arg("nft")
        .arg("-f")
        .arg("-") // read ruleset from stdin
        .stdin(Stdio::piped())
        .spawn()?;

    child
        .stdin
        .take()
        .expect("stdin was piped")
        .write_all(ruleset.as_bytes())?;

    let status = child.wait()?;
    if !status.success() {
        return Err(anyhow!("nft exited with status {status}"));
    }
    Ok(NatGuard)
}

fn teardown_vpn_nat() -> Result<()> {
    let status = Command::new("sudo")
        .args(["nft", "delete", "table", "ip", "vpn_nat"])
        .status()?;

    if !status.success() {
        // Not fatal on shutdown — table may already be gone.
        eprintln!("warning: `nft delete table ip vpn_nat` exited with {status}");
    }
    Ok(())
}

fn main_nic() -> Result<String> {
    let iface = get_default_interface().map_err(|e| anyhow::anyhow!(e))?;
    Ok(iface.name)
}
