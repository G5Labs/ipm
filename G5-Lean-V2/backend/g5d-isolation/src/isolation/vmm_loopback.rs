//! Deterministic host-loopback identity for one native `vm-exec` VMM process.
//!
use std::net::Ipv4Addr;

/// First two bits of the reserved second-octet range (`127.128.0.0/10`).
const VMM_LOOPBACK_SECOND_OCTET_BASE: u8 = 128;
/// A /10 leaves 22 address bits for the native host PID.
const VMM_LOOPBACK_MAX_PID: u32 = 0x3f_ffff;

/// Why a native PID cannot be represented by the reserved VMM loopback range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum VmmLoopbackAddrError {
    /// PID zero is not a live userspace process identity.
    #[error("vm-exec child has no live native PID")]
    MissingPid,
    /// The host PID space exceeds the checked 22-bit address contract.
    #[error("vm-exec PID {0} exceeds the 22-bit VMM loopback range")]
    PidOutOfRange(u32),
}

/// Map one live native `vm-exec` PID bijectively into `127.128.0.0/10`.
///
/// A leaked or parked VMM retains its native PID, so it retains this address
/// across supervisor restarts. The OS closes the process's sockets before that
/// PID can be reused.
pub fn vmm_loopback_addr_for_pid(pid: u32) -> Result<Ipv4Addr, VmmLoopbackAddrError> {
    if pid == 0 {
        return Err(VmmLoopbackAddrError::MissingPid);
    }
    if pid > VMM_LOOPBACK_MAX_PID {
        return Err(VmmLoopbackAddrError::PidOutOfRange(pid));
    }

    Ok(Ipv4Addr::new(
        127,
        VMM_LOOPBACK_SECOND_OCTET_BASE | ((pid >> 16) as u8 & 0x3f),
        (pid >> 8) as u8,
        pid as u8,
    ))
}

/// Resolve the host address owning this VMM's passt ingress listeners.
///
#[cfg(any(feature = "cloud-hypervisor", test))]
pub(crate) fn host_vmm_port_map_addr_for_pid(pid: u32) -> Result<Ipv4Addr, VmmLoopbackAddrError> {
    vmm_loopback_addr_for_pid(pid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pid_mapping_matches_reserved_range() {
        assert_eq!(
            vmm_loopback_addr_for_pid(1).unwrap(),
            Ipv4Addr::new(127, 128, 0, 1)
        );
        assert_eq!(
            vmm_loopback_addr_for_pid(0x01_0203).unwrap(),
            Ipv4Addr::new(127, 129, 2, 3)
        );
        assert_eq!(
            vmm_loopback_addr_for_pid(0x3f_ffff).unwrap(),
            Ipv4Addr::new(127, 191, 255, 255)
        );
    }

    #[test]
    fn pid_mapping_is_bijective_and_fail_closed_at_boundaries() {
        assert_eq!(
            vmm_loopback_addr_for_pid(0),
            Err(VmmLoopbackAddrError::MissingPid)
        );
        assert_eq!(
            vmm_loopback_addr_for_pid(0x40_0000),
            Err(VmmLoopbackAddrError::PidOutOfRange(0x40_0000))
        );
        assert_ne!(
            vmm_loopback_addr_for_pid(42).unwrap(),
            vmm_loopback_addr_for_pid(43).unwrap()
        );
    }

    #[test]
    fn host_port_map_address_matches_platform_transport() {
        let pid = 0x01_0203;
        assert_eq!(
            host_vmm_port_map_addr_for_pid(pid).unwrap(),
            Ipv4Addr::new(127, 129, 2, 3)
        );

        assert_eq!(
            host_vmm_port_map_addr_for_pid(0),
            Err(VmmLoopbackAddrError::MissingPid)
        );
        assert_eq!(
            host_vmm_port_map_addr_for_pid(0x40_0000),
            Err(VmmLoopbackAddrError::PidOutOfRange(0x40_0000))
        );
    }
}
