/// TUN device creation for the NM plugin.
///
/// Running as root (spawned by NM), so we can create the TUN device directly
/// without pkexec or capability checks.
use anyhow::{Context, Result};
use std::ffi::CString;
use std::net::Ipv4Addr;
use std::os::unix::io::RawFd;
use tracing::{info, warn};

// ioctl request code for TUNSETIFF
const TUNSETIFF: libc::c_ulong = 0x400454ca;

/// Create a TUN device, configure its IP and MTU, and bring it up.
///
/// Returns the async TUN device for read/write. Since we're running as root
/// (NM spawns VPN plugins as root), no privilege elevation is needed.
pub fn create_tun(
    name: &str,
    local_ip: Ipv4Addr,
    peer_ip: Ipv4Addr,
    mtu: u16,
) -> Result<tun_rs::AsyncDevice> {
    info!("Creating TUN device {name}");

    // Create the TUN device using ip commands (we're root)
    let status = std::process::Command::new("ip")
        .args(["tuntap", "add", "dev", name, "mode", "tun"])
        .status()
        .context("Failed to run ip tuntap add")?;
    if !status.success() {
        anyhow::bail!("ip tuntap add failed with {status}");
    }

    // Configure IP address
    let status = std::process::Command::new("ip")
        .args([
            "addr",
            "add",
            &format!("{local_ip}"),
            "peer",
            &format!("{peer_ip}"),
            "dev",
            name,
        ])
        .status()
        .context("Failed to configure IP address")?;
    if !status.success() {
        delete_tun(name);
        anyhow::bail!("ip addr add failed with {status}");
    }

    // Set MTU and bring up
    let status = std::process::Command::new("ip")
        .args(["link", "set", name, "mtu", &mtu.to_string(), "up"])
        .status()
        .context("Failed to set MTU and bring up")?;
    if !status.success() {
        delete_tun(name);
        anyhow::bail!("ip link set failed with {status}");
    }

    // Open the TUN device
    let fd = unsafe { libc::open(c"/dev/net/tun".as_ptr(), libc::O_RDWR | libc::O_CLOEXEC) };
    if fd < 0 {
        delete_tun(name);
        return Err(std::io::Error::last_os_error()).context("Failed to open /dev/net/tun");
    }

    let c_name = CString::new(name).context("Invalid TUN device name")?;
    if let Err(e) = attach_tun(fd, &c_name) {
        unsafe {
            libc::close(fd);
        }
        delete_tun(name);
        return Err(e);
    }

    let device = unsafe { tun_rs::AsyncDevice::from_fd(fd) }
        .context("Failed to create AsyncDevice from TUN fd")?;

    info!("TUN device {name} created and configured");
    Ok(device)
}

/// Delete the TUN device.
pub fn delete_tun(name: &str) {
    info!("Deleting TUN device {name}");
    let result = std::process::Command::new("ip")
        .args(["link", "delete", name])
        .status();
    match result {
        Ok(status) if status.success() => info!("TUN device {name} deleted"),
        Ok(status) => warn!("ip link delete {name} exited with {status}"),
        Err(e) => warn!("Failed to delete TUN device {name}: {e}"),
    }
}

/// Attach to a TUN device via TUNSETIFF ioctl.
fn attach_tun(fd: RawFd, name: &CString) -> Result<()> {
    unsafe {
        let mut req: libc::ifreq = std::mem::zeroed();

        std::ptr::copy_nonoverlapping(
            name.as_ptr() as *const libc::c_char,
            req.ifr_name.as_mut_ptr(),
            name.as_bytes_with_nul().len(),
        );

        req.ifr_ifru.ifru_flags = (libc::IFF_TUN | libc::IFF_NO_PI) as libc::c_short;

        let ret = libc::ioctl(fd, TUNSETIFF as _, &mut req as *mut _);
        if ret < 0 {
            return Err(std::io::Error::last_os_error()).context("TUNSETIFF ioctl failed");
        }
    }
    Ok(())
}
