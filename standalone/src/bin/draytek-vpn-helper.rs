/// Privileged helper binary for DrayTek VPN network operations.
///
/// Invoked via pkexec to create/destroy TUN devices, configure routing, and manage DNS.
/// Designed to be minimal (std-only, no external deps) for security.
use std::net::Ipv4Addr;
use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: draytek-vpn-helper <setup|teardown|check> [options]");
        return ExitCode::from(1);
    }

    let result = match args[1].as_str() {
        "setup" => cmd_setup(&args[2..]),
        "teardown" => cmd_teardown(&args[2..]),
        "check" => cmd_check(),
        other => {
            eprintln!("Unknown subcommand: {other}");
            Err("Unknown subcommand".into())
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {e}");
            ExitCode::from(1)
        }
    }
}

// ── Argument parsing ──────────────────────────────────────────────────────────

struct SetupArgs {
    device: String,
    uid: u32,
    local_ip: Ipv4Addr,
    peer_ip: Ipv4Addr,
    mtu: u16,
    routes: Vec<String>,
    default_gw: Option<Ipv4Addr>,
    dns: Option<Ipv4Addr>,
}

struct TeardownArgs {
    device: String,
    restore_dns: bool,
}

fn parse_setup_args(args: &[String]) -> Result<SetupArgs, Box<dyn std::error::Error>> {
    let mut device = None;
    let mut uid = None;
    let mut local_ip = None;
    let mut peer_ip = None;
    let mut mtu = None;
    let mut routes = Vec::new();
    let mut default_gw = None;
    let mut dns = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--device" => {
                i += 1;
                device = Some(args.get(i).ok_or("--device requires a value")?.clone());
            }
            "--uid" => {
                i += 1;
                uid = Some(
                    args.get(i)
                        .ok_or("--uid requires a value")?
                        .parse::<u32>()?,
                );
            }
            "--local-ip" => {
                i += 1;
                local_ip = Some(
                    args.get(i)
                        .ok_or("--local-ip requires a value")?
                        .parse::<Ipv4Addr>()?,
                );
            }
            "--peer-ip" => {
                i += 1;
                peer_ip = Some(
                    args.get(i)
                        .ok_or("--peer-ip requires a value")?
                        .parse::<Ipv4Addr>()?,
                );
            }
            "--mtu" => {
                i += 1;
                mtu = Some(
                    args.get(i)
                        .ok_or("--mtu requires a value")?
                        .parse::<u16>()?,
                );
            }
            "--route" => {
                i += 1;
                routes.push(args.get(i).ok_or("--route requires a value")?.clone());
            }
            "--default-gw" => {
                i += 1;
                default_gw = Some(
                    args.get(i)
                        .ok_or("--default-gw requires a value")?
                        .parse::<Ipv4Addr>()?,
                );
            }
            "--dns" => {
                i += 1;
                dns = Some(
                    args.get(i)
                        .ok_or("--dns requires a value")?
                        .parse::<Ipv4Addr>()?,
                );
            }
            other => return Err(format!("Unknown option: {other}").into()),
        }
        i += 1;
    }

    Ok(SetupArgs {
        device: device.ok_or("--device is required")?,
        uid: uid.ok_or("--uid is required")?,
        local_ip: local_ip.ok_or("--local-ip is required")?,
        peer_ip: peer_ip.ok_or("--peer-ip is required")?,
        mtu: mtu.ok_or("--mtu is required")?,
        routes,
        default_gw,
        dns,
    })
}

fn parse_teardown_args(args: &[String]) -> Result<TeardownArgs, Box<dyn std::error::Error>> {
    let mut device = None;
    let mut restore_dns = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--device" => {
                i += 1;
                device = Some(args.get(i).ok_or("--device requires a value")?.clone());
            }
            "--restore-dns" => {
                restore_dns = true;
            }
            other => return Err(format!("Unknown option: {other}").into()),
        }
        i += 1;
    }

    Ok(TeardownArgs {
        device: device.ok_or("--device is required")?,
        restore_dns,
    })
}

// ── Validation ────────────────────────────────────────────────────────────────

fn validate_device_name(name: &str) -> Result<(), Box<dyn std::error::Error>> {
    if name.is_empty() || name.len() > 15 {
        return Err(format!("Device name must be 1-15 characters, got '{name}'").into());
    }
    if !name.starts_with(|c: char| c.is_ascii_alphabetic()) {
        return Err(format!("Device name must start with a letter: '{name}'").into());
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(format!("Device name must be alphanumeric: '{name}'").into());
    }
    Ok(())
}

fn validate_mtu(mtu: u16) -> Result<(), Box<dyn std::error::Error>> {
    if !(576..=9000).contains(&mtu) {
        return Err(format!("MTU must be 576-9000, got {mtu}").into());
    }
    Ok(())
}

fn validate_cidr(cidr: &str) -> Result<(), Box<dyn std::error::Error>> {
    let parts: Vec<&str> = cidr.split('/').collect();
    if parts.len() != 2 {
        return Err(format!("Invalid CIDR format: {cidr}").into());
    }
    parts[0].parse::<Ipv4Addr>().map_err(|e| {
        format!("Invalid IP in CIDR '{cidr}': {e}")
    })?;
    let prefix: u8 = parts[1]
        .parse()
        .map_err(|e| format!("Invalid prefix in CIDR '{cidr}': {e}"))?;
    if prefix > 32 {
        return Err(format!("Prefix length must be 0-32, got {prefix}").into());
    }
    Ok(())
}

// ── Command execution ─────────────────────────────────────────────────────────

fn run_cmd(program: &str, args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("+ {program} {}", args.join(" "));
    let output = Command::new(program).args(args).output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "{program} {} failed (exit {}): {}",
            args.join(" "),
            output.status,
            stderr.trim()
        )
        .into());
    }
    Ok(())
}

// ── DNS helpers ──────────────────────────────────────────────────────────

/// Try to configure DNS via resolvectl (systemd-resolved).
/// Returns true on success, false if resolvectl is unavailable or fails.
fn try_resolvectl_dns_setup(device: &str, dns_ip: Ipv4Addr) -> bool {
    let dns_str = dns_ip.to_string();
    if run_cmd("resolvectl", &["dns", device, &dns_str]).is_err() {
        return false;
    }
    if run_cmd("resolvectl", &["domain", device, "~."]).is_err() {
        return false;
    }
    true
}

/// Configure DNS by writing directly to /etc/resolv.conf (requires root).
fn direct_dns_setup(dns_ip: Ipv4Addr) -> Result<(), Box<dyn std::error::Error>> {
    let resolv_path = "/etc/resolv.conf";
    let backup_path = "/run/draytek-vpn-resolv.bak";

    // Backup current resolv.conf
    if let Ok(current) = std::fs::read_to_string(resolv_path) {
        std::fs::write(backup_path, &current)
            .map_err(|e| format!("Failed to backup resolv.conf to {backup_path}: {e}"))?;
    }

    // Prepend our nameserver
    let existing = std::fs::read_to_string(resolv_path).unwrap_or_default();
    let new_content = format!("nameserver {dns_ip}\n{existing}");
    std::fs::write(resolv_path, new_content)
        .map_err(|e| format!("Failed to write {resolv_path}: {e}"))?;
    Ok(())
}

// ── Subcommands ───────────────────────────────────────────────────────────────

fn cmd_setup(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let setup = parse_setup_args(args)?;

    // Validate all inputs before executing anything
    validate_device_name(&setup.device)?;
    validate_mtu(setup.mtu)?;
    for route in &setup.routes {
        validate_cidr(route)?;
    }

    let uid_str = setup.uid.to_string();
    let local_ip_str = setup.local_ip.to_string();
    let peer_ip_str = setup.peer_ip.to_string();
    let mtu_str = setup.mtu.to_string();
    let addr_spec = format!("{local_ip_str} peer {peer_ip_str}");
    let _ = addr_spec; // used below via individual parts

    // 1. Create TUN device owned by user (remove stale device from prior session if present)
    if std::path::Path::new(&format!("/sys/class/net/{}", setup.device)).exists() {
        eprintln!("Note: removing stale {} device", setup.device);
        let _ = run_cmd("ip", &["tuntap", "del", "dev", &setup.device, "mode", "tun"]);
    }
    run_cmd(
        "ip",
        &["tuntap", "add", "dev", &setup.device, "mode", "tun", "user", &uid_str],
    )?;

    // 2. Configure IP address
    run_cmd(
        "ip",
        &["addr", "add", &local_ip_str, "peer", &peer_ip_str, "dev", &setup.device],
    )?;

    // 3. Set MTU and bring up
    run_cmd(
        "ip",
        &["link", "set", &setup.device, "mtu", &mtu_str, "up"],
    )?;

    // 4. Add routes
    for route in &setup.routes {
        run_cmd("ip", &["route", "add", route, "dev", &setup.device])?;
    }

    // 5. Default gateway
    if let Some(gw) = setup.default_gw {
        let gw_str = gw.to_string();
        run_cmd(
            "ip",
            &["route", "add", "default", "via", &gw_str, "dev", &setup.device],
        )?;
    }

    // 6. DNS configuration
    if let Some(dns_ip) = setup.dns {
        if try_resolvectl_dns_setup(&setup.device, dns_ip) {
            eprintln!("DNS configured via resolvectl for {}", setup.device);
        } else {
            eprintln!("resolvectl not available or failed, falling back to /etc/resolv.conf");
            match direct_dns_setup(dns_ip) {
                Ok(()) => eprintln!("DNS configured via /etc/resolv.conf: {dns_ip}"),
                Err(e) => eprintln!("Warning: DNS configuration failed: {e} — continuing without DNS"),
            }
        }
    }

    eprintln!("Setup complete for {}", setup.device);
    Ok(())
}

fn cmd_check() -> Result<(), Box<dyn std::error::Error>> {
    let status = std::fs::read_to_string("/proc/self/status")
        .map_err(|e| format!("Failed to read /proc/self/status: {e}"))?;

    let cap_eff = status
        .lines()
        .find(|line| line.starts_with("CapEff:"))
        .ok_or("CapEff line not found in /proc/self/status")?;

    let hex_str = cap_eff
        .split_whitespace()
        .nth(1)
        .ok_or("Failed to parse CapEff value")?;

    let caps = u64::from_str_radix(hex_str.trim_start_matches("0x"), 16)
        .map_err(|e| format!("Failed to parse CapEff hex '{hex_str}': {e}"))?;

    const CAP_NET_ADMIN: u64 = 1 << 12;
    if caps & CAP_NET_ADMIN != 0 {
        eprintln!("capability check: CAP_NET_ADMIN is present in effective set (CapEff={hex_str})");
        Ok(())
    } else {
        eprintln!("capability check: CAP_NET_ADMIN is NOT present in effective set (CapEff={hex_str})");
        Err("CAP_NET_ADMIN not present".into())
    }
}

fn cmd_teardown(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let teardown = parse_teardown_args(args)?;

    validate_device_name(&teardown.device)?;

    // 1. Flush routes (best-effort)
    if let Err(e) = run_cmd("ip", &["route", "flush", "dev", &teardown.device]) {
        eprintln!("Warning: failed to flush routes: {e}");
    }

    // 2. Bring down interface (best-effort)
    if let Err(e) = run_cmd("ip", &["link", "set", &teardown.device, "down"]) {
        eprintln!("Warning: failed to bring down {}: {e}", teardown.device);
    }

    // 3. Delete TUN device
    if let Err(e) = run_cmd(
        "ip",
        &["tuntap", "del", "dev", &teardown.device, "mode", "tun"],
    ) {
        eprintln!("Warning: failed to delete {}: {e}", teardown.device);
    }

    // 4. Restore DNS (try both methods — safe no-ops if nothing to do)
    if teardown.restore_dns {
        // Try resolvectl revert (no-op if resolvectl wasn't used or device is gone)
        match Command::new("resolvectl").args(["revert", &teardown.device]).output() {
            Ok(output) if output.status.success() => {
                eprintln!("+ Reverted DNS via resolvectl for {}", teardown.device);
            }
            _ => {
                // resolvectl not available or device already gone — that's fine
            }
        }

        // Restore /etc/resolv.conf backup if it exists (covers direct-write case)
        let backup_path = "/run/draytek-vpn-resolv.bak";
        let resolv_path = "/etc/resolv.conf";
        if std::path::Path::new(backup_path).exists() {
            match std::fs::read_to_string(backup_path) {
                Ok(original) => {
                    if let Err(e) = std::fs::write(resolv_path, original) {
                        eprintln!("Warning: failed to restore {resolv_path}: {e}");
                    } else {
                        eprintln!("+ Restored {resolv_path} from backup");
                        let _ = std::fs::remove_file(backup_path);
                    }
                }
                Err(e) => {
                    eprintln!("Warning: failed to read DNS backup {backup_path}: {e}");
                }
            }
        }
    }

    eprintln!("Teardown complete for {}", teardown.device);
    Ok(())
}
