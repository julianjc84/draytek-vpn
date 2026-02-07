/// Linux TUN device management.
///
/// Opens an existing TUN device (created by the privileged helper) using raw
/// ioctls, then wraps the fd in a tun_rs::AsyncDevice for async I/O.
use anyhow::{Context, Result};
use std::ffi::CString;
use std::os::unix::io::RawFd;
use tracing::info;

// ioctl request code for TUNSETIFF
const TUNSETIFF: libc::c_ulong = 0x400454ca;

/// Open an existing TUN device by name.
///
/// The device must already be created (with user ownership) by the privileged helper.
/// This performs only the open + TUNSETIFF attach — no IP/MTU/flags configuration,
/// so no privileges are needed.
pub fn open_tun(name: &str) -> Result<tun_rs::AsyncDevice> {
    info!("Opening TUN device {name}");

    let fd = unsafe {
        libc::open(
            c"/dev/net/tun".as_ptr(),
            libc::O_RDWR | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error())
            .context("Failed to open /dev/net/tun");
    }

    // Set up ifreq with device name and IFF_TUN | IFF_NO_PI
    let c_name = CString::new(name)
        .context("Invalid TUN device name")?;
    if c_name.as_bytes_with_nul().len() > libc::IFNAMSIZ {
        unsafe { libc::close(fd); }
        anyhow::bail!("TUN device name too long: {name}");
    }

    let result = attach_tun(fd, &c_name);
    if let Err(e) = result {
        unsafe { libc::close(fd); }
        return Err(e);
    }

    let device = unsafe { tun_rs::AsyncDevice::from_fd(fd) }
        .context("Failed to create AsyncDevice from TUN fd")?;

    info!("TUN device {name} opened successfully");
    Ok(device)
}

/// Attach to an existing TUN device via TUNSETIFF ioctl.
fn attach_tun(fd: RawFd, name: &CString) -> Result<()> {
    unsafe {
        let mut req: libc::ifreq = std::mem::zeroed();

        // Copy device name
        std::ptr::copy_nonoverlapping(
            name.as_ptr() as *const libc::c_char,
            req.ifr_name.as_mut_ptr(),
            name.as_bytes_with_nul().len(),
        );

        // IFF_TUN | IFF_NO_PI
        req.ifr_ifru.ifru_flags =
            (libc::IFF_TUN | libc::IFF_NO_PI) as libc::c_short;

        let ret = libc::ioctl(fd, TUNSETIFF as _, &mut req as *mut _);
        if ret < 0 {
            return Err(std::io::Error::last_os_error())
                .context("TUNSETIFF ioctl failed — is the device created?");
        }
    }
    Ok(())
}
