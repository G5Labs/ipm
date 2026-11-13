//! Host disk capacity evidence shared by Start admission and preparation
//! helpers. VM disks stay sparse: these checks admit or classify, and never
//! reserve capacity for later writes.
use std::{
    fs::OpenOptions,
    io,
    path::{Path, PathBuf},
};

pub use g5_contracts::error::HostDiskPressure as DiskPressure;

pub fn free_bytes(path: &Path) -> io::Result<u64> {
    use std::os::unix::ffi::OsStrExt;
    let path = std::ffi::CString::new(path.as_os_str().as_bytes())?;
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: valid NUL-terminated path and writable output.
    if unsafe { libc::statvfs(path.as_ptr(), stats.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let stats = unsafe { stats.assume_init() };
    Ok(available_bytes(stats.f_bavail, stats.f_frsize))
}

// Darwin uses different widths for these statvfs fields. Convert before
// multiplying, and saturate even when a filesystem reports extreme values.
fn available_bytes(blocks: impl Into<u64>, block_size: impl Into<u64>) -> u64 {
    blocks.into().saturating_mul(block_size.into())
}

pub fn pressure(path: &Path, phase: &str, required_bytes: u64) -> DiskPressure {
    DiskPressure {
        phase: phase.into(),
        available_bytes: free_bytes(path).unwrap_or(0),
        required_bytes,
    }
}

/// Start admission: the host must have at least the VM's recorded V available.
/// No multiple of V or fixed headroom is added, and nothing is reserved.
pub fn admit_private(available_bytes: u64, vm_mib: u32) -> Result<(), DiskPressure> {
    let required_bytes = u64::from(vm_mib) << 20;
    if available_bytes >= required_bytes {
        Ok(())
    } else {
        Err(DiskPressure {
            phase: "disk_admission".into(),
            available_bytes,
            required_bytes,
        })
    }
}

/// The host's VM storage root: image templates and per-VM roots.
pub fn storage_root() -> PathBuf {
    std::env::var_os("G5D_ROOTFS_STATE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| "/var/lib/g5d/rootfs".into())
}

/// Only a local storage errno establishes capacity pressure. Registry text,
/// image validation and Git failures cannot manufacture this classification.
pub fn classify_io(error: &(dyn std::error::Error + 'static), phase: &str) -> Option<DiskPressure> {
    let mut next = Some(error);
    while let Some(error) = next {
        if let Some(pressure) = error.downcast_ref::<DiskPressure>() {
            return Some(pressure.clone());
        }
        if error
            .downcast_ref::<io::Error>()
            .is_some_and(|e| matches!(e.raw_os_error(), Some(libc::ENOSPC | libc::EDQUOT)))
        {
            return Some(pressure(&storage_root(), phase, 4096));
        }
        next = error.source();
    }
    None
}

/// External formatters do not expose errno. Probe the destination filesystem
/// after a failed write to distinguish host pressure from a full/invalid guest
/// image. Never classify arbitrary formatter or image text as capacity.
pub fn write_pressure(path: &Path, phase: &str) -> Option<DiskPressure> {
    use std::os::unix::fs::OpenOptionsExt;
    let path = if path.is_dir() {
        path.to_owned()
    } else {
        path.parent()?.to_owned()
    };
    let probe = path.join(format!(".g5-capacity-probe-{}", uuid::Uuid::new_v4()));
    let result = (|| -> io::Result<()> {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&probe)?;
        #[cfg(target_os = "linux")]
        {
            use std::os::fd::AsRawFd;
            if unsafe { libc::fallocate(file.as_raw_fd(), 0, 0, 4096) } != 0 {
                return Err(io::Error::last_os_error());
            }
        }
        drop(file);
        Ok(())
    })();
    let _ = std::fs::remove_file(&probe);
    result.err().and_then(|error| classify_io(&error, phase))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn statvfs_widths_and_overflow() {
        assert_eq!(available_bytes(3u32, 4096u64), 12288);
        assert_eq!(available_bytes(3u64, 4096u32), 12288);
        assert_eq!(available_bytes(u64::MAX, 4096u32), u64::MAX);
        assert_eq!(available_bytes(0u64, u32::MAX), 0);
        assert!(free_bytes(Path::new("/")).is_ok());
    }
    #[test]
    fn capacity_diagnostics_are_bounded_and_distinct() {
        let failure = DiskPressure {
            phase: "cold_image".into(),
            available_bytes: 168363999232,
            required_bytes: 85899345920,
        };
        assert_eq!(
            DiskPressure::parse(&failure.reason()),
            Some(failure.clone())
        );
        assert_eq!(
            classify_io(
                &g5_contracts::G5Error::HostDiskPressure(failure.clone()),
                "fallback"
            ),
            Some(failure.clone())
        );
        assert_eq!(
            DiskPressure::from_console_tail(&format!("[stderr] {}", failure.reason())),
            Some(failure.clone())
        );
        for prefix in [
            "[stdout] ",
            "[stdout] populate: ",
            "[stdout] [stderr] ",
            "[stderr] invalid image: ",
            "helper failed: ",
        ] {
            let text = format!("{prefix}{}", failure.reason());
            assert_eq!(DiskPressure::parse(&text), None);
            assert_eq!(DiskPressure::from_console_tail(&text), None);
        }
        for message in [
            "invalid image",
            "authorization failed",
            "source corrupt",
            "No space left on device",
            "host_disk_capacity:{}",
        ] {
            assert_eq!(DiskPressure::parse(message), None);
        }
    }

    #[test]
    fn admission_uses_exactly_recorded_v_including_the_qa_incident() {
        for mib in [1024, 32768, 81920, 131072] {
            let v = u64::from(mib) << 20;
            assert!(admit_private(v - 1, mib).is_err());
            assert!(admit_private(v, mib).is_ok());
            assert!(admit_private(v + 1, mib).is_ok());
        }
        assert!(admit_private(168363999232, 81920).is_ok());
        assert!(classify_io(&io::Error::from_raw_os_error(libc::ENOSPC), "oci_download").is_some());
        assert!(classify_io(
            &io::Error::from_raw_os_error(libc::EDQUOT),
            "image_population"
        )
        .is_some());
        assert!(classify_io(
            &io::Error::other("No space left on device"),
            "image_population"
        )
        .is_none());
        assert!(classify_io(
            &io::Error::from_raw_os_error(libc::EACCES),
            "image_population"
        )
        .is_none());
    }
}
