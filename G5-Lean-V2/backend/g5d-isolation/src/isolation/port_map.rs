//! Host-port allocation for workspace VM passt port forwarding.
//!
//! `g5d vm-exec` takes explicit `host_port:guest_port` mappings via the
//! `-p`/`--port-map` flag (see `cli::VmExecArgs` and `cloud_hypervisor::vm_exec_main`),
//! but the workspace host has to choose the host port. The allocation contract
//! fixes the host-port source as the IANA ephemeral range 49152-65535:
//! `PortAllocator` is the primitive that hands out unique, leak-free host
//! ports from that range so N concurrent microVMs can each bind the same guest
//! port (e.g. `:8080`) and still be independently reachable on the host.
//!
//! The allocator is **process-local**: it prevents two microVMs on the *same*
//! workspace host from colliding on a host port. Disambiguating ports across
//! hosts is the control plane's job (it keys reverse-tunnel slots by
//! `(host_id, host_port)` — see the ontology D34 slot-lifecycle contract).
//!
//! Ports are handed out behind RAII: [`PortAllocator::allocate`] returns a
//! [`PortLease`] guard, and dropping the guard returns the port to the pool.
//! A microVM-boot path holds the lease for the VM's lifetime, so the host port
//! is freed exactly when the VM is torn down, with no explicit free to forget.

use std::collections::BTreeSet;
use std::fmt;
use std::net::{Ipv4Addr, SocketAddrV4};
use std::sync::{Arc, Mutex};

use g5_contracts::error::G5Error;

/// First port of the IANA ephemeral / dynamic range (RFC 6335). Host ports for
/// Passt mappings are drawn from `EPHEMERAL_START..=EPHEMERAL_END` so they don't
/// collide with well-known or registered services on the worker host.
pub const EPHEMERAL_START: u16 = 49152;

/// Last port of the IANA ephemeral / dynamic range (also the largest `u16`).
pub const EPHEMERAL_END: u16 = 65535;

/// Errors returned by [`PortAllocator`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortAllocError {
    /// Every port in the allocator's range is currently leased.
    Exhausted {
        /// Inclusive lower bound of the exhausted range.
        start: u16,
        /// Inclusive upper bound of the exhausted range.
        end: u16,
    },
    /// `with_range` was called with an invalid range. Either `start > end`, or
    /// the requested range escapes the IANA ephemeral pool
    /// (`EPHEMERAL_START..=EPHEMERAL_END`). The allocator's bound guarantee is
    /// that *every* port it ever hands out lives in the ephemeral pool; ranges
    /// like `1..=2` (privileged) would silently break that.
    InvalidRange {
        /// The requested (invalid) lower bound.
        start: u16,
        /// The requested (invalid) upper bound.
        end: u16,
    },
}

impl fmt::Display for PortAllocError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PortAllocError::Exhausted { start, end } => write!(
                f,
                "no free host port in ephemeral range {start}-{end} (all leased)"
            ),
            PortAllocError::InvalidRange { start, end } => write!(
                f,
                "invalid host-port range {start}-{end}: must satisfy \
                 {EPHEMERAL_START} <= start <= end <= {EPHEMERAL_END}"
            ),
        }
    }
}

impl std::error::Error for PortAllocError {}

impl From<PortAllocError> for G5Error {
    fn from(e: PortAllocError) -> Self {
        G5Error::Internal {
            message: format!("host-port allocation failed: {e}"),
        }
    }
}

/// Interior allocation state, shared between a [`PortAllocator`] and every
/// [`PortLease`] it has handed out so that dropping a lease can return the port
/// to the pool.
#[derive(Debug)]
struct Inner {
    start: u16,
    end: u16,
    allocated: BTreeSet<u16>,
}

/// Allocates unique host ports from the IANA ephemeral range for passt port
/// forwarding.
///
/// Cheap to clone — clones share the same underlying allocation state, so a
/// single logical pool can be shared across concurrent VM-boot paths. Cloning
/// does **not** create a second independent pool.
#[derive(Debug, Clone)]
pub struct PortAllocator {
    inner: Arc<Mutex<Inner>>,
}

impl PortAllocator {
    /// The address scope passt uses for its host-side port-map listener.
    ///
    /// Probing only loopback is insufficient on platforms that allow a
    /// loopback-specific bind beside an existing wildcard listener. Probe the
    /// wildcard address so a successful result matches passt's later bind.
    fn host_listener_probe_addr(port: u16) -> SocketAddrV4 {
        SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port)
    }

    /// Construct an allocator spanning the full IANA ephemeral range
    /// (`EPHEMERAL_START..=EPHEMERAL_END`, i.e. 49152..=65535).
    pub fn new() -> Self {
        // The full ephemeral range trivially satisfies the with_range invariant
        // (EPHEMERAL_START <= EPHEMERAL_START <= EPHEMERAL_END <= EPHEMERAL_END).
        // Surface the impossible-path as unreachable!() rather than expect()
        // so a future bounds change in with_range can't silently regress this
        // into a runtime panic that swallows the diagnostic.
        match Self::with_range(EPHEMERAL_START, EPHEMERAL_END) {
            Ok(a) => a,
            Err(_) => unreachable!(
                "PortAllocator::new(): EPHEMERAL_START..=EPHEMERAL_END is always a valid range"
            ),
        }
    }

    /// Construct an allocator over an explicit inclusive `[start, end]` range
    /// inside the IANA ephemeral pool.
    ///
    /// Used by tests and by callers that want to partition the ephemeral pool
    /// into disjoint sub-ranges. Returns [`PortAllocError::InvalidRange`] if
    /// the bounds are not `EPHEMERAL_START <= start <= end <= EPHEMERAL_END`.
    /// In particular: privileged ports (<1024), registered ports
    /// (1024..=49151), or anything above the `u16` range cannot be requested
    /// — the allocator's core invariant is that every port it ever hands out
    /// lives in the IANA dynamic/private pool.
    pub fn with_range(start: u16, end: u16) -> Result<Self, PortAllocError> {
        // `end > EPHEMERAL_END` is structurally impossible because
        // `EPHEMERAL_END == u16::MAX`; clippy rightly flags it as a tautology.
        // The `start > end` and `start < EPHEMERAL_START` checks are sufficient
        // to enforce the contract (the `end_unreachable` test pins this).
        if start > end || start < EPHEMERAL_START {
            return Err(PortAllocError::InvalidRange { start, end });
        }
        Ok(Self {
            inner: Arc::new(Mutex::new(Inner {
                start,
                end,
                allocated: BTreeSet::new(),
            })),
        })
    }

    /// Allocate the lowest free host port in range, returning an RAII lease.
    ///
    /// The port is marked allocated until the returned [`PortLease`] is
    /// dropped. Returns [`PortAllocError::Exhausted`] when every port in range
    /// is currently leased.
    pub fn allocate(&self) -> Result<PortLease, PortAllocError> {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let (start, end) = (inner.start, inner.end);
        for port in start..=end {
            if !inner.allocated.contains(&port) {
                inner.allocated.insert(port);
                return Ok(PortLease {
                    port,
                    inner: Some(Arc::clone(&self.inner)),
                });
            }
        }
        Err(PortAllocError::Exhausted { start, end })
    }

    /// Return a host port to the free pool by number.
    ///
    /// **Restart-time reconciliation primitive — not a runtime free.** This is
    /// the path a freshly-started worker calls to clear ports for VMs that
    /// exited during its prior life: the worker reads live VM port maps from
    /// the control plane, then calls `release()` for any port whose VM is
    /// gone. Calling `release(P)` while a live [`PortLease`] still holds `P`
    /// is a footgun: the lease will keep the VM alive, but the allocator
    /// will believe `P` is free and may hand it out to a second lease — two
    /// leases will then believe they own the same port. Normal teardown
    /// MUST rely on dropping the [`PortLease`].
    ///
    /// Releasing a port that is not currently allocated — or releasing the
    /// same port twice — is a harmless no-op and never panics or corrupts the
    /// pool.
    pub fn release(&self, port: u16) {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        inner.allocated.remove(&port);
    }

    /// Lease an ephemeral host port that is also confirmed **OS-bindable**.
    ///
    /// [`allocate`](Self::allocate) hands out a unique in-range *number*, not a
    /// guaranteed-bindable port — a leased number can still be held by an
    /// unrelated process (experience `port_allocator_number_vs_os_bindable_port`).
    /// This probes each candidate with a real `TcpListener::bind` and **releases
    /// the probe immediately**: callers must leave it free for passt to bind
    /// the host-side forwarding listener. The lease prevents other VM launches
    /// in this supervisor from selecting the same port.
    /// Rejected candidates are **held** for the duration so `allocate()` advances
    /// past them, then released on return. Tries up to `max_attempts`, then fails
    /// closed. The returned [`PortLease`] is held for the resource's lifetime
    /// (D32). Shared by the agent carrier and the preview host-port lease.
    pub async fn lease_bindable(&self, max_attempts: usize) -> Result<PortLease, String> {
        // Held only for this call so a failed candidate's number is not re-offered
        // by the next `allocate()`; dropped on return.
        let mut rejected: Vec<PortLease> = Vec::new();
        let mut last_err: Option<String> = None;
        for _ in 0..max_attempts {
            let lease = self
                .allocate()
                .map_err(|e| format!("host port allocation failed: {e}"))?;
            let port = lease.port();
            match tokio::net::TcpListener::bind(Self::host_listener_probe_addr(port)).await {
                Ok(probe) => {
                    drop(probe);
                    return Ok(lease);
                }
                Err(e) => {
                    rejected.push(lease);
                    last_err = Some(format!("probe-bind :{port} failed: {e}"));
                }
            }
        }
        Err(format!(
            "could not bind any of {max_attempts} leased host ports (last error: {})",
            last_err.unwrap_or_else(|| "none".to_string())
        ))
    }

    /// Number of ports currently leased. Primarily for tests and observability.
    pub fn allocated_count(&self) -> usize {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .allocated
            .len()
    }
}

impl Default for PortAllocator {
    fn default() -> Self {
        Self::new()
    }
}

/// RAII guard for an allocated host port.
///
/// Holds the leased port and a handle back to the allocator's shared state.
/// Dropping the lease returns the port to the free pool, so a microVM-boot
/// path that holds the lease for the VM's lifetime frees the host port exactly
/// when the VM is torn down. The leased port is the `host_port` half of a
/// `-p host_port:guest_port` passt mapping.
#[derive(Debug)]
pub struct PortLease {
    port: u16,
    /// `None` after `release()` has consumed the lease — `Drop` then becomes a
    /// no-op. Holding the `Arc` here instead of unconditionally cloning it on
    /// `release` keeps a single `Arc` strong-count per active lease (no leak).
    inner: Option<Arc<Mutex<Inner>>>,
}

impl PortLease {
    /// The leased host port.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Explicitly release the port without waiting for `Drop`.
    ///
    /// Equivalent to dropping the lease (the underlying allocator state mutates
    /// once), but lets the caller free the port at a specific point in code
    /// without scope tricks. After `release`, `Drop` runs but is a no-op.
    pub fn release(mut self) {
        if let Some(inner) = self.inner.take() {
            let mut guard = inner
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            guard.allocated.remove(&self.port);
        }
    }
}

impl Drop for PortLease {
    fn drop(&mut self) {
        // Return the port to the pool unless explicit release() already did.
        // Recover from a poisoned lock rather than skip the release — losing
        // a host port to a poisoned mutex would be a slow leak. Idempotent:
        // if the port was already released by number (PortAllocator::release),
        // this remove is a harmless no-op.
        if let Some(ref inner) = self.inner {
            let mut guard = inner
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            guard.allocated.remove(&self.port);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bindability_probe_matches_passt_wildcard_listener_scope() {
        assert_eq!(
            PortAllocator::host_listener_probe_addr(EPHEMERAL_START),
            SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, EPHEMERAL_START)
        );
    }

    #[tokio::test]
    async fn lease_bindable_skips_port_occupied_at_wildcard_scope() {
        let (occupied, occupied_port) = (EPHEMERAL_START..EPHEMERAL_END)
            .find_map(|port| {
                let occupied = std::net::TcpListener::bind((Ipv4Addr::UNSPECIFIED, port)).ok()?;
                let next = std::net::TcpListener::bind((Ipv4Addr::UNSPECIFIED, port + 1)).ok()?;
                drop(next);
                Some((occupied, port))
            })
            .expect("find two consecutive bindable ephemeral ports");
        let allocator = PortAllocator::with_range(occupied_port, occupied_port + 1).unwrap();

        let lease = allocator
            .lease_bindable(2)
            .await
            .expect("skip wildcard-occupied candidate");

        assert_eq!(lease.port(), occupied_port + 1);
        drop(occupied);
    }

    #[test]
    fn new_spans_full_ephemeral_range() {
        let alloc = PortAllocator::new();
        let lease = alloc.allocate().expect("allocate from full range");
        // Lowest-first: the very first allocation is the bottom of the range,
        // which also proves the port is within [EPHEMERAL_START, EPHEMERAL_END].
        assert_eq!(lease.port(), EPHEMERAL_START);
    }

    #[test]
    fn allocate_is_lowest_first_and_in_bounds() {
        let alloc = PortAllocator::with_range(49152, 49155).unwrap();
        let a = alloc.allocate().unwrap();
        let b = alloc.allocate().unwrap();
        let c = alloc.allocate().unwrap();
        assert_eq!(a.port(), 49152);
        assert_eq!(b.port(), 49153);
        assert_eq!(c.port(), 49154);
        for lease in [&a, &b, &c] {
            assert!(lease.port() >= 49152 && lease.port() <= 49155);
        }
    }

    #[test]
    fn exhaustion_returns_error() {
        let alloc = PortAllocator::with_range(49152, 49153).unwrap();
        let _a = alloc.allocate().unwrap();
        let _b = alloc.allocate().unwrap();
        assert_eq!(alloc.allocated_count(), 2);
        let err = alloc.allocate().unwrap_err();
        assert_eq!(
            err,
            PortAllocError::Exhausted {
                start: 49152,
                end: 49153
            }
        );
    }

    #[test]
    fn invalid_range_rejected_when_start_gt_end() {
        let err = PortAllocator::with_range(50000, 49999).unwrap_err();
        assert_eq!(
            err,
            PortAllocError::InvalidRange {
                start: 50000,
                end: 49999
            }
        );
    }

    #[test]
    fn with_range_rejects_below_ephemeral_low() {
        // Privileged-port range — must be rejected: the allocator's core
        // invariant is "every port lives in the IANA ephemeral pool".
        let err = PortAllocator::with_range(1, 10).unwrap_err();
        assert_eq!(err, PortAllocError::InvalidRange { start: 1, end: 10 });

        // Bounds-exact: the largest "below ephemeral" range still rejects.
        let err = PortAllocator::with_range(EPHEMERAL_START - 1, EPHEMERAL_END).unwrap_err();
        assert_eq!(
            err,
            PortAllocError::InvalidRange {
                start: EPHEMERAL_START - 1,
                end: EPHEMERAL_END,
            }
        );
    }

    #[test]
    fn with_range_rejects_start_inside_but_end_unreachable() {
        // `end > EPHEMERAL_END` is impossible with `u16` (max == EPHEMERAL_END),
        // so the only way to exceed the upper bound is for `start > end` — which
        // is already covered by `invalid_range_rejected_when_start_gt_end`.
        // This test exists to pin the contract: the IANA upper bound is
        // exactly `u16::MAX`. If `EPHEMERAL_END` ever changes, this assertion
        // protects the doc + test claims.
        assert_eq!(EPHEMERAL_END, u16::MAX);
    }

    #[test]
    fn single_port_range_allocates_then_exhausts() {
        let alloc = PortAllocator::with_range(49160, 49160).unwrap();
        let lease = alloc.allocate().unwrap();
        assert_eq!(lease.port(), 49160);
        assert!(alloc.allocate().is_err());
    }

    #[test]
    fn raii_release_on_drop_returns_port_to_pool() {
        let alloc = PortAllocator::with_range(49152, 49152).unwrap();
        {
            let lease = alloc.allocate().unwrap();
            assert_eq!(lease.port(), 49152);
            assert_eq!(alloc.allocated_count(), 1);
            // exhausted while the lease is held
            assert!(alloc.allocate().is_err());
        } // lease dropped here
        assert_eq!(alloc.allocated_count(), 0);
        // port is reusable after the lease drops
        let lease2 = alloc.allocate().unwrap();
        assert_eq!(lease2.port(), 49152);
    }

    #[test]
    fn explicit_release_returns_port_to_pool() {
        let alloc = PortAllocator::with_range(49152, 49152).unwrap();
        let lease = alloc.allocate().unwrap();
        assert_eq!(alloc.allocated_count(), 1);
        lease.release(); // consumes the lease without waiting for Drop
        assert_eq!(alloc.allocated_count(), 0);
        let lease2 = alloc.allocate().unwrap();
        assert_eq!(lease2.port(), 49152);
    }

    #[test]
    fn explicit_release_does_not_leak_arc_strong_count() {
        // Regression: prior implementation used Arc::clone + mem::forget(self),
        // permanently leaking one Arc strong-count per release. With the
        // Option<Arc<…>> design, a release() round-trip must net zero new
        // strong-counts on the pool's Arc.
        let alloc = PortAllocator::with_range(49152, 49161).unwrap();
        let before = Arc::strong_count(&alloc.inner);

        // 10 allocate + release cycles. After each cycle the only live Arc
        // strong-count should be the one held by `alloc.inner` itself; if
        // release leaked an Arc, the count would grow by 10.
        for _ in 0..10 {
            let lease = alloc.allocate().unwrap();
            lease.release();
        }

        let after = Arc::strong_count(&alloc.inner);
        assert_eq!(
            after, before,
            "PortLease::release leaked an Arc: strong_count grew from {before} to {after}"
        );
        assert_eq!(alloc.allocated_count(), 0);
    }

    #[test]
    fn double_free_is_a_noop() {
        let alloc = PortAllocator::with_range(49152, 49152).unwrap();
        let lease = alloc.allocate().unwrap();
        let port = lease.port();
        drop(lease); // first release (RAII)
        assert_eq!(alloc.allocated_count(), 0);
        // Second release of the same port by number must be a harmless no-op,
        // not an underflow or a spurious "free" of a now-reallocated port.
        alloc.release(port);
        assert_eq!(alloc.allocated_count(), 0);
        // The port is cleanly reusable exactly once.
        let lease2 = alloc.allocate().unwrap();
        assert_eq!(lease2.port(), port);
        assert_eq!(alloc.allocated_count(), 1);
    }

    #[test]
    fn release_by_number_frees_held_port() {
        let alloc = PortAllocator::with_range(49152, 49153).unwrap();
        let lease = alloc.allocate().unwrap();
        let port = lease.port();
        std::mem::forget(lease); // simulate a lost lease (no drop)
        assert_eq!(alloc.allocated_count(), 1);
        alloc.release(port); // crash-recovery reconciliation path
        assert_eq!(alloc.allocated_count(), 0);
    }

    #[test]
    fn cloned_allocator_shares_one_pool() {
        let alloc = PortAllocator::with_range(49152, 49153).unwrap();
        let clone = alloc.clone();
        let a = alloc.allocate().unwrap();
        let b = clone.allocate().unwrap();
        // Distinct ports from the shared pool — no double-allocation across clones.
        assert_ne!(a.port(), b.port());
        assert_eq!(alloc.allocated_count(), 2);
        // Pool is exhausted as seen from either handle.
        assert!(clone.allocate().is_err());
    }

    #[test]
    fn error_converts_to_g5_internal() {
        let g5: G5Error = PortAllocError::Exhausted {
            start: 49152,
            end: 49152,
        }
        .into();
        match g5 {
            G5Error::Internal { message } => {
                assert!(message.contains("host-port allocation failed"));
            }
            other => panic!("expected G5Error::Internal, got {other:?}"),
        }
    }
}
