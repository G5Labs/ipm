//! The door's content-addressed store: files under `<root>/<kind>/...`, a
//! byte bound that in-flight transfers reserve against, least-recently-used
//! eviction keyed on modification time (a read touches the entry), read
//! leases so an entry a response is still streaming is neither evicted nor
//! dropped from the accounting, and per-key single flight so N concurrent
//! misses fetch once.
//!
//! Every operation that changes the disk and the accounting together (a
//! publication, an eviction walk) runs on a blocking thread that owns the
//! maintenance lock and finishes whether or not the request that started it
//! is still there, so a cancelled request can never leave a file counted
//! wrongly or a lock released early.

use std::{
    collections::HashMap,
    ffi::CString,
    io,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, SystemTime},
};

use tokio::sync::{OwnedMutexGuard, OwnedSemaphorePermit, Semaphore};

use super::path_under;

/// Distinct fills (temporaries mid-transfer) the store admits at once; a
/// further miss waits for a slot rather than opening another temporary.
pub const MAX_CONCURRENT_FILLS: usize = 16;
/// Reservations are taken ahead in steps of up to this size so a large
/// transfer does not re-check the bound on every chunk.
const RESERVE_STEP: u64 = 8 * 1024 * 1024;
/// A temporary older than this with no owner left is a leftover (a host
/// crash mid-transfer); eviction walks remove them.
const STALE_TEMP_AGE: Duration = Duration::from_secs(3600);
/// Free space the store's filesystem must keep after a fill: the byte bound
/// is the door's share of the disk, but when something else (VM scratch,
/// image templates) has grown into that share, the door stops filling before
/// it touches the host's start reserve.
pub const DEFAULT_MIN_FREE_BYTES: u64 = 4 * 1024 * 1024 * 1024;

type Flights = Arc<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>>;
type Leases = Arc<Mutex<HashMap<String, usize>>>;

pub struct Store {
    root: PathBuf,
    max_bytes: u64,
    min_free_bytes: u64,
    /// Bytes of committed entries.
    used: AtomicU64,
    /// Bytes reserved by temporaries mid-transfer (reporting only).
    inflight: AtomicU64,
    /// The admission counter: committed plus reserved bytes in ONE number,
    /// so a reservation turning into committed bytes never shows an
    /// in-between state that would admit a fill past the bound.
    charged: AtomicU64,
    flights: Flights,
    fills: Arc<Semaphore>,
    /// Entries a reader is streaming; eviction skips them and they stay
    /// accounted, so slow readers cannot pin unlinked blocks past the bound.
    leases: Leases,
    /// Serializes the free-space check with the charge it admits.
    admission: Mutex<()>,
    /// Serializes publications with eviction walks. Owned by the blocking
    /// task that performs either, so it is held through the operation and
    /// its accounting even when the awaiting request is cancelled.
    maintenance: Arc<tokio::sync::Mutex<()>>,
}

/// A committed entry plus the read lease that keeps it on disk and in the
/// accounting until the lease is dropped (after the response body ends).
#[derive(Debug)]
pub struct Entry {
    pub path: PathBuf,
    pub len: u64,
    pub lease: Lease,
}

#[derive(Debug)]
pub struct Lease {
    key: String,
    leases: Leases,
}

impl Drop for Lease {
    fn drop(&mut self) {
        if let Ok(mut map) = self.leases.lock() {
            if let Some(count) = map.get_mut(&self.key) {
                *count -= 1;
                if *count == 0 {
                    map.remove(&self.key);
                }
            }
        }
    }
}

/// One key's single-flight lock. Dropping it releases the key and drops the
/// map entry when nobody else holds or waits for it, so the map stays
/// bounded.
pub struct Flight {
    guard: Option<OwnedMutexGuard<()>>,
    // Declared after `guard`, so it is dropped after the guard is released.
    _membership: FlightMembership,
}

impl Drop for Flight {
    fn drop(&mut self) {
        drop(self.guard.take());
    }
}

/// A participant's reference to a key's single-flight entry, taken before
/// waiting for the lock: when the last participant (holder or waiter, even
/// one cancelled while waiting) lets go, the map entry goes with it.
struct FlightMembership {
    mutex: Arc<tokio::sync::Mutex<()>>,
    key: String,
    flights: Flights,
}

impl Drop for FlightMembership {
    fn drop(&mut self) {
        if let Ok(mut map) = self.flights.lock() {
            // The map and this membership hold one reference each; any other
            // participant holds a further one, so `<= 2` means we are last.
            if Arc::strong_count(&self.mutex) <= 2 {
                map.remove(&self.key);
            }
        }
    }
}

/// A temporary file inside the store (same filesystem as the final path, so
/// commit is a rename), created on the spot so it always exists while this
/// handle does, whose bytes are reserved against the bound as they are
/// written. Dropping it uncommitted removes the file and releases the
/// reservation, including when the request that opened it is cancelled.
pub struct Temp {
    path: PathBuf,
    reserved: u64,
    written: u64,
    committed: bool,
    store: Arc<Store>,
    _fill: OwnedSemaphorePermit,
}

impl Temp {
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The temporary opened for writing. It is never (re)created here: a
    /// request cancelled while the open is queued has already removed the
    /// file, and the late open then fails instead of leaving a leftover.
    pub async fn open(&self) -> io::Result<tokio::fs::File> {
        tokio::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&self.path)
            .await
    }

    /// Account `n` more bytes about to be written. Fails when the object
    /// would exceed the per-object bound or the store cannot make room for it
    /// even after evicting committed entries.
    pub async fn account(&mut self, n: u64) -> io::Result<()> {
        self.written = self.written.saturating_add(n);
        let max_object = self.store.max_object_bytes();
        if self.written > max_object {
            return Err(io::Error::new(
                io::ErrorKind::StorageFull,
                format!("object exceeds the cache door's per-object bound of {max_object} bytes"),
            ));
        }
        if self.written > self.reserved {
            let needed = self.written - self.reserved;
            let headroom = max_object.saturating_sub(self.reserved);
            let step = needed
                .max(RESERVE_STEP.min(max_object / 16))
                .min(headroom)
                .max(needed);
            // Take the reservation before anything awaits, so a request
            // cancelled while room is being made still releases it through
            // `Drop`.
            if !self.store.admit(step) {
                return Err(io::Error::new(
                    io::ErrorKind::StorageFull,
                    "host disk is at the cache door's free-space floor",
                ));
            }
            self.reserved += step;
            if self.store.over_bound() {
                self.store.make_room().await;
                if self.store.over_bound() {
                    self.reserved -= step;
                    self.store.release(step);
                    return Err(io::Error::new(
                        io::ErrorKind::StorageFull,
                        "cache door store cannot make room for the transfer",
                    ));
                }
            }
        }
        Ok(())
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        self.store.release(self.reserved);
        if !self.committed {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

impl Store {
    pub async fn open(root: &Path, max_bytes: u64) -> io::Result<Arc<Self>> {
        Self::open_with(root, max_bytes, DEFAULT_MIN_FREE_BYTES).await
    }

    /// Open a store bounded to `max_bytes` that also refuses fills once the
    /// filesystem's free space would drop under `min_free_bytes`.
    pub async fn open_with(
        root: &Path,
        max_bytes: u64,
        min_free_bytes: u64,
    ) -> io::Result<Arc<Self>> {
        tokio::fs::create_dir_all(root).await?;
        // Leftovers of fetches interrupted by a previous host crash.
        let mut leftovers = tokio::fs::read_dir(root).await?;
        while let Some(entry) = leftovers.next_entry().await? {
            if entry.file_name().to_string_lossy().starts_with(".tmp-") {
                let _ = tokio::fs::remove_file(entry.path()).await;
            }
        }
        let used = scan_bytes(root.to_path_buf()).await?;
        Ok(Arc::new(Self {
            root: root.to_path_buf(),
            max_bytes,
            min_free_bytes,
            used: AtomicU64::new(used),
            inflight: AtomicU64::new(0),
            charged: AtomicU64::new(used),
            flights: Arc::new(Mutex::new(HashMap::new())),
            fills: Arc::new(Semaphore::new(MAX_CONCURRENT_FILLS)),
            leases: Arc::new(Mutex::new(HashMap::new())),
            admission: Mutex::new(()),
            maintenance: Arc::new(tokio::sync::Mutex::new(())),
        }))
    }

    pub fn used_bytes(&self) -> u64 {
        self.used.load(Ordering::Relaxed)
    }

    pub fn inflight_bytes(&self) -> u64 {
        self.inflight.load(Ordering::Relaxed)
    }

    /// Committed plus reserved bytes, the number admission is judged on.
    pub fn charged_bytes(&self) -> u64 {
        self.charged.load(Ordering::Relaxed)
    }

    /// Keys with a live single-flight entry (bounded by holders and waiters).
    pub fn flight_count(&self) -> usize {
        self.flights.lock().map(|m| m.len()).unwrap_or(0)
    }

    /// Keys a reader currently holds a lease on.
    pub fn lease_count(&self) -> usize {
        self.leases.lock().map(|m| m.len()).unwrap_or(0)
    }

    /// No single object may take more than half the store, so one fill can
    /// never flush everything else.
    pub fn max_object_bytes(&self) -> u64 {
        self.max_bytes / 2
    }

    /// Charge `n` bytes to the admission counter if the filesystem keeps
    /// the free-space floor once every outstanding reservation (bytes other
    /// fills may still write) and these `n` are on disk. Check and charge
    /// are one critical section, so concurrent fills cannot all pass the
    /// check on the same free space. An unreadable filesystem statistic
    /// does not block fills.
    fn admit(&self, n: u64) -> bool {
        let _admission = self.admission.lock().expect("store admission");
        let allowed = match free_bytes(&self.root) {
            Ok(free) => {
                free.saturating_sub(self.inflight_bytes()).saturating_sub(n) >= self.min_free_bytes
            }
            Err(_) => true,
        };
        if allowed {
            self.charge(n);
        }
        allowed
    }

    fn lease(&self, rel: &str) -> Lease {
        if let Ok(mut map) = self.leases.lock() {
            *map.entry(rel.to_string()).or_insert(0) += 1;
        }
        Lease {
            key: rel.to_string(),
            leases: self.leases.clone(),
        }
    }

    /// The entry for `rel` if present, leased to the caller. A hit refreshes
    /// the entry's modification time, which is the eviction order.
    pub async fn get(&self, rel: &str) -> Option<Entry> {
        let path = path_under(&self.root, rel)?;
        // Lease first, then look: an eviction racing this read sees the
        // lease and leaves the file alone.
        let lease = self.lease(rel);
        let metadata = tokio::fs::metadata(&path).await.ok()?;
        if !metadata.is_file() {
            return None;
        }
        let touch = path.clone();
        let _ = tokio::task::spawn_blocking(move || {
            std::fs::File::options()
                .write(true)
                .open(&touch)
                .and_then(|file| file.set_modified(SystemTime::now()))
        })
        .await;
        Some(Entry {
            path,
            len: metadata.len(),
            lease,
        })
    }

    /// Serialize fills of one key. Callers re-check `get` after acquiring.
    pub async fn lock(&self, rel: &str) -> Flight {
        let mutex = {
            let mut map = self.flights.lock().expect("store flight map");
            map.entry(rel.to_string())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                .clone()
        };
        // Membership first: a waiter cancelled before it acquires the lock
        // still drops the entry if it was the last participant.
        let membership = FlightMembership {
            mutex: mutex.clone(),
            key: rel.to_string(),
            flights: self.flights.clone(),
        };
        let guard = mutex.lock_owned().await;
        Flight {
            guard: Some(guard),
            _membership: membership,
        }
    }

    /// Open a temporary for a fill, waiting for a fill slot. The file is
    /// created here, synchronously (one `creat` on the local disk), so no
    /// cancellation can separate the handle from its file.
    pub async fn temp(self: &Arc<Self>) -> io::Result<Temp> {
        let fill = self
            .fills
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| io::Error::other("cache door fill slots closed"))?;
        let path = self.root.join(format!(".tmp-{}", uuid::Uuid::now_v7()));
        std::fs::File::create(&path)?;
        Ok(Temp {
            path,
            reserved: 0,
            written: 0,
            committed: false,
            store: self.clone(),
            _fill: fill,
        })
    }

    /// Move a fully written temporary to its final place, account for it
    /// (net of any entry it replaces), evict past the bound, and lease the
    /// new entry to the caller.
    pub async fn commit(self: &Arc<Self>, rel: &str, temp: Temp) -> io::Result<Entry> {
        self.publish(rel, temp, None).await
    }

    /// Write `bytes` into a fresh temporary and commit it, the write itself
    /// included in the publication so a cancelled request cannot leave a
    /// temporary behind.
    pub async fn put_bytes(self: &Arc<Self>, rel: &str, bytes: &[u8]) -> io::Result<Entry> {
        let mut temp = self.temp().await?;
        temp.account(bytes.len() as u64).await?;
        self.publish(rel, temp, Some(bytes.to_vec())).await
    }

    /// The publication (optional write, rename, accounting) runs on a
    /// blocking thread that owns the maintenance lock and the reservation,
    /// to completion: a request cancelled mid-commit leaves the file either
    /// counted and present or absent and uncounted, never in between, and
    /// no eviction walk can interleave with it.
    async fn publish(
        self: &Arc<Self>,
        rel: &str,
        mut temp: Temp,
        bytes: Option<Vec<u8>>,
    ) -> io::Result<Entry> {
        let path = path_under(&self.root, rel)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "unsafe store key"))?;
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let maintenance = self.maintenance.clone().lock_owned().await;
        // The publication task owns the reservation and the lock from here;
        // `temp`'s drop has nothing left to release or remove.
        let reserved = std::mem::take(&mut temp.reserved);
        temp.committed = true;
        let store = self.clone();
        let from = temp.path.clone();
        let to = path.clone();
        let key = rel.to_string();
        let (len, lease) = tokio::task::spawn_blocking(move || -> io::Result<(u64, Lease)> {
            let _maintenance = maintenance;
            let outcome = (|| {
                if let Some(bytes) = &bytes {
                    std::fs::write(&from, bytes)?;
                }
                let len = std::fs::metadata(&from)?.len();
                let replaced = match std::fs::metadata(&to) {
                    Ok(meta) => meta.len(),
                    Err(error) if error.kind() == io::ErrorKind::NotFound => 0,
                    Err(error) => return Err(error),
                };
                std::fs::rename(&from, &to)?;
                Ok((len, replaced))
            })();
            match outcome {
                Ok((len, replaced)) => {
                    // The reservation becomes committed bytes in one step of
                    // the admission counter.
                    let _ = store
                        .charged
                        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |c| {
                            Some(
                                c.saturating_add(len)
                                    .saturating_sub(reserved)
                                    .saturating_sub(replaced),
                            )
                        });
                    let _ =
                        store
                            .inflight
                            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
                                Some(v.saturating_sub(reserved))
                            });
                    let _ = store
                        .used
                        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |u| {
                            Some(u.saturating_add(len).saturating_sub(replaced))
                        });
                    // Leased before the lock is released, so no eviction walk
                    // can take the entry away from the fill that made it.
                    Ok((len, store.lease(&key)))
                }
                Err(error) => {
                    store.release(reserved);
                    let _ = std::fs::remove_file(&from);
                    Err(error)
                }
            }
        })
        .await
        .map_err(io::Error::other)??;
        if self.used_bytes() > self.max_bytes {
            let maintenance = self.maintenance.clone().lock_owned().await;
            self.evict_until(self.max_bytes / 10 * 9, maintenance).await;
        }
        Ok(Entry { path, len, lease })
    }

    fn over_bound(&self) -> bool {
        self.charged_bytes() > self.max_bytes
    }

    fn charge(&self, n: u64) {
        self.charged.fetch_add(n, Ordering::Relaxed);
        self.inflight.fetch_add(n, Ordering::Relaxed);
    }

    /// Evict committed entries until the reservations taken so far fit.
    async fn make_room(self: &Arc<Self>) {
        let maintenance = self.maintenance.clone().lock_owned().await;
        self.evict_until(
            self.max_bytes.saturating_sub(self.inflight_bytes()),
            maintenance,
        )
        .await;
    }

    fn release(&self, n: u64) {
        let _ = self
            .charged
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
                Some(v.saturating_sub(n))
            });
        let _ = self
            .inflight
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
                Some(v.saturating_sub(n))
            });
    }

    /// Remove least recently used, unleased, committed entries until the
    /// committed total is at most `target`, then resynchronize the byte
    /// count with the disk. The walk, the removals and the accounting run on
    /// one blocking thread that owns the maintenance lock, so no publication
    /// interleaves and a cancelled caller changes nothing.
    async fn evict_until(self: &Arc<Self>, target: u64, maintenance: OwnedMutexGuard<()>) {
        let store = self.clone();
        let _ = tokio::task::spawn_blocking(move || {
            let _maintenance = maintenance;
            let root = &store.root;
            sweep_stale_temps(root);
            let mut files = Vec::new();
            collect_files(root, &mut files);
            files.sort_by_key(|(mtime, _, _)| *mtime);
            let mut remaining: u64 = files.iter().map(|(_, len, _)| *len).sum();
            let mut freed = 0u64;
            for (_, len, path) in files {
                if remaining <= target {
                    break;
                }
                // The lease check and the unlink happen under the lease map's
                // lock, so a reader cannot take a lease on a file that is
                // about to go: it either leases first (and the file stays) or
                // finds the file gone.
                let Ok(map) = store.leases.lock() else {
                    continue;
                };
                let leased = path
                    .strip_prefix(root)
                    .ok()
                    .and_then(|rel| rel.to_str())
                    .is_none_or(|rel| map.contains_key(rel));
                if !leased && std::fs::remove_file(&path).is_ok() {
                    freed += len;
                    remaining = remaining.saturating_sub(len);
                }
                drop(map);
            }
            // Resynchronize the committed count with the disk and move the
            // admission counter by the same delta, whatever reservations
            // came and went meanwhile.
            let before = store.used.swap(remaining, Ordering::Relaxed);
            let _ = store
                .charged
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |c| {
                    Some(c.saturating_add(remaining).saturating_sub(before))
                });
            if freed > 0 {
                tracing::info!(
                    freed_bytes = freed,
                    remaining_bytes = remaining,
                    max_bytes = store.max_bytes,
                    "cache door evicted"
                );
            }
        })
        .await;
    }
}

/// Free bytes available to this process on `path`'s filesystem.
fn free_bytes(path: &Path) -> io::Result<u64> {
    let c_path = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "nul byte in path"))?;
    // SAFETY: `stat` is a plain C struct the call fills in; `c_path` lives
    // for the call.
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c_path.as_ptr(), &mut stat) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok((stat.f_bavail as u64).saturating_mul(stat.f_frsize as u64))
}

/// Remove `.tmp-*` files nobody can own any more (older than
/// [`STALE_TEMP_AGE`]); a live transfer refreshes its temporary's
/// modification time with every chunk.
fn sweep_stale_temps(root: &Path) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        if !entry.file_name().to_string_lossy().starts_with(".tmp-") {
            continue;
        }
        let stale = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|m| SystemTime::now().duration_since(m).ok())
            .is_some_and(|age| age > STALE_TEMP_AGE);
        if stale {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

fn collect_files(dir: &Path, out: &mut Vec<(SystemTime, u64, PathBuf)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if metadata.is_dir() {
            collect_files(&path, out);
        } else if metadata.is_file() {
            if path
                .file_name()
                .map(|n| n.to_string_lossy().starts_with(".tmp-"))
                .unwrap_or(false)
            {
                continue;
            }
            let mtime = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            out.push((mtime, metadata.len(), path));
        }
    }
}

async fn scan_bytes(root: PathBuf) -> io::Result<u64> {
    tokio::task::spawn_blocking(move || {
        let mut files = Vec::new();
        collect_files(&root, &mut files);
        files.iter().map(|(_, len, _)| *len).sum()
    })
    .await
    .map_err(io::Error::other)
}
