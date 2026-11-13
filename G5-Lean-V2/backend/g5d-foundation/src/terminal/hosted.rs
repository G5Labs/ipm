//! Hosted terminal sessions: a PTY the dispatcher keeps the master of and
//! hands the slave to a module, with no shell of its own
//! (`g5.preview.output.live-tail`).
//!
//! A hosted session retains the bytes a module's child writes to the slave in
//! one bounded ring, exactly as a browser shell retains its scrollback, and
//! serves them to the current viewer by byte offset with the same
//! `replay_from` semantics (`g5.terminal.reattach`). Unlike a shell, nothing
//! ever reads the PTY on a viewer's behalf: a reader thread drains the master
//! continuously so a module whose output nobody watches never blocks on it.
//! The session is *alive* while any slave holder exists; the master's EOF
//! marks it *ended but retained*, still attachable for replay until the
//! manager discards it.

use std::io::Read;
#[cfg(unix)]
use std::os::fd::{AsRawFd, OwnedFd};
#[cfg(unix)]
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use g5_contracts::error::G5Error;
use portable_pty::{NativePtySystem, PtySize, PtySystem};
use tokio::sync::{watch, Mutex};
use tokio_util::sync::CancellationToken;

use super::session::{duplicate_pty_fd, wait_for_pty_ready, ByteRingBuffer, ReplaySnapshot};

/// The offset the next byte will have and whether the writer side is done.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Progress {
    next_offset: u64,
    ended: bool,
}

/// A PTY master the dispatcher hosts for one module child.
pub struct HostedSession {
    name: String,
    capacity: usize,
    /// Kept for `resize`; dropping it closes the master.
    master: Mutex<Option<Box<dyn portable_pty::MasterPty + Send>>>,
    ring: Arc<StdMutex<ByteRingBuffer>>,
    progress: watch::Sender<Progress>,
    ended: Arc<AtomicBool>,
    /// Test hook: successful reads so far, to pin the read-then-`end` race.
    #[cfg(test)]
    reads: Arc<std::sync::atomic::AtomicUsize>,
    /// Wakes the reader thread's blocking poll so `end` can stop it.
    #[cfg(unix)]
    reader_wake: StdMutex<Option<UnixStream>>,
}

/// What `host_session` returns: the session and the slave descriptor the
/// caller passes to its child, then drops.
pub struct HostedPty {
    pub session: Arc<HostedSession>,
    #[cfg(unix)]
    pub slave: OwnedFd,
}

/// One viewer's position in a hosted session's output.
pub struct HostedCursor {
    ring: Arc<StdMutex<ByteRingBuffer>>,
    progress: watch::Receiver<Progress>,
    offset: u64,
}

/// Why a viewer's read ended.
#[derive(Debug, PartialEq, Eq)]
pub enum HostedReadEnd {
    /// The writer side closed and every retained byte up to the end was
    /// delivered to this cursor.
    Ended,
    /// The viewer fell further behind than the ring retains; it must reattach
    /// with its offset and accept the reset.
    Lagged,
    /// A newer viewer superseded this one.
    Superseded,
}

/// Open the slave of the PTY whose master is `master_fd`, close-on-exec and
/// without making it this process's controlling terminal.
#[cfg(unix)]
fn open_slave(master_fd: std::os::fd::RawFd) -> Result<OwnedFd, G5Error> {
    use std::os::fd::FromRawFd as _;
    let mut name = [0 as libc::c_char; 256];
    // SAFETY: `name` is a writable buffer of the stated length and the master
    // descriptor is live.
    if unsafe { libc::ptsname_r(master_fd, name.as_mut_ptr(), name.len()) } != 0 {
        return Err(G5Error::Internal {
            message: format!(
                "Failed to name hosted PTY slave: {}",
                std::io::Error::last_os_error()
            ),
        });
    }
    // SAFETY: ptsname_r NUL-terminated the buffer on success.
    let fd = unsafe {
        libc::open(
            name.as_ptr(),
            libc::O_RDWR | libc::O_NOCTTY | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(G5Error::Internal {
            message: format!(
                "Failed to open hosted PTY slave: {}",
                std::io::Error::last_os_error()
            ),
        });
    }
    // SAFETY: a successful open returns a descriptor this process owns.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

impl HostedSession {
    /// Open a PTY pair, keep the master, start draining it, and return the
    /// slave for the caller's child. `capacity` bounds the retained output.
    pub fn open(name: &str, capacity: usize, cols: u16, rows: u16) -> Result<HostedPty, G5Error> {
        let pty_system = NativePtySystem::default();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| G5Error::Internal {
                message: format!("Failed to create hosted PTY pair: {error}"),
            })?;
        let master = pair.master;
        #[cfg(unix)]
        let slave = {
            let master_fd = master.as_raw_fd().ok_or_else(|| G5Error::Internal {
                message: "hosted PTY master has no file descriptor".to_string(),
            })?;
            // portable-pty does not expose the slave descriptor; open our own
            // slave from the master's device name (already granted and
            // unlocked by `openpty`) before portable-pty's handle is dropped,
            // so the master never sees a hangup between the two holders.
            open_slave(master_fd)?
        };
        drop(pair.slave);
        let reader = master
            .try_clone_reader()
            .map_err(|error| G5Error::Internal {
                message: format!("Failed to clone hosted PTY reader: {error}"),
            })?;
        #[cfg(unix)]
        let (reader_poll_fd, wake_rx, wake_tx) = {
            let fd = master.as_raw_fd().ok_or_else(|| G5Error::Internal {
                message: "hosted PTY master has no file descriptor".to_string(),
            })?;
            // SAFETY: fd is the live PTY master supplied by portable-pty;
            // fcntl reads and updates only its file status flags.
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
            if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
            {
                return Err(G5Error::Internal {
                    message: format!(
                        "Failed to make hosted PTY nonblocking: {}",
                        std::io::Error::last_os_error()
                    ),
                });
            }
            let (wake_rx, wake_tx) = UnixStream::pair().map_err(|error| G5Error::Internal {
                message: format!("Failed to create hosted PTY wake pipe: {error}"),
            })?;
            (duplicate_pty_fd(fd)?, wake_rx, wake_tx)
        };

        let ring = Arc::new(StdMutex::new(ByteRingBuffer::new(capacity)));
        let (progress, _) = watch::channel(Progress {
            next_offset: 0,
            ended: false,
        });
        let ended = Arc::new(AtomicBool::new(false));
        #[cfg(test)]
        let reads = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        #[cfg(test)]
        let thread_reads = Arc::clone(&reads);
        let thread_ring = Arc::clone(&ring);
        let thread_progress = progress.clone();
        let thread_ended = Arc::clone(&ended);
        let thread_name = format!("pty-host-{}", name.chars().take(24).collect::<String>());
        std::thread::Builder::new()
            .name(thread_name)
            .spawn(move || {
                let mut reader = reader;
                let mut buf = [0u8; 8192];
                let finish = |ring: &StdMutex<ByteRingBuffer>| {
                    thread_ended.store(true, Ordering::Release);
                    let next_offset = ring.lock().unwrap().next_offset();
                    let _ = thread_progress.send_replace(Progress {
                        next_offset,
                        ended: true,
                    });
                };
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) => break finish(&thread_ring),
                        Ok(n) => {
                            #[cfg(test)]
                            thread_reads.fetch_add(1, Ordering::Release);
                            let mut ring = thread_ring.lock().unwrap();
                            // `end` ran, perhaps while the writer never
                            // paused: stop retaining and release the master
                            // now rather than at the next quiet moment.
                            // Checked and published under the ring lock,
                            // which `end` takes before it publishes, so a
                            // read racing `end` neither retains bytes nor
                            // overwrites the ended progress.
                            if thread_ended.load(Ordering::Acquire) {
                                drop(ring);
                                break finish(&thread_ring);
                            }
                            ring.push(&buf[..n]);
                            let next_offset = ring.next_offset();
                            let _ = thread_progress.send_replace(Progress {
                                next_offset,
                                ended: false,
                            });
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            #[cfg(unix)]
                            match wait_for_pty_ready(
                                reader_poll_fd.as_raw_fd(),
                                wake_rx.as_raw_fd(),
                                libc::POLLIN,
                            ) {
                                Ok(true) => continue,
                                // Woken by `end`: the manager discarded the
                                // session; nothing more is retained.
                                Ok(false) => break finish(&thread_ring),
                                Err(_) => break finish(&thread_ring),
                            }
                            #[cfg(not(unix))]
                            {
                                std::thread::sleep(std::time::Duration::from_millis(5));
                                continue;
                            }
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                        // EIO: the last slave holder closed. Any other error
                        // also ends the writer side; the retained bytes stay.
                        Err(_) => break finish(&thread_ring),
                    }
                }
            })
            .map_err(|error| G5Error::Internal {
                message: format!("Failed to spawn hosted PTY reader thread: {error}"),
            })?;

        let session = Arc::new(Self {
            name: name.to_string(),
            capacity,
            master: Mutex::new(Some(master)),
            ring,
            progress,
            ended,
            #[cfg(test)]
            reads,
            #[cfg(unix)]
            reader_wake: StdMutex::new(Some(wake_tx)),
        });
        Ok(HostedPty {
            session,
            #[cfg(unix)]
            slave,
        })
    }

    /// The session's identity (`preview:<task id>` for a preview).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The retained-output bound.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// False once the master read EOF (every slave holder closed) or the
    /// manager ended the session; the output stays attachable for replay.
    pub fn is_alive(&self) -> bool {
        !self.ended.load(Ordering::Acquire)
    }

    /// Bytes written so far (the offset the next byte will have).
    pub fn next_offset(&self) -> u64 {
        self.progress.borrow().next_offset
    }

    /// Test hook: hold the ring so the reader can read but not retain, which
    /// pins the race between a successful read and `end`.
    #[cfg(test)]
    pub(super) fn hold_ring(&self) -> std::sync::MutexGuard<'_, ByteRingBuffer> {
        self.ring.lock().unwrap()
    }

    /// Test hook: how many reads the reader thread has completed.
    #[cfg(test)]
    pub(super) fn reads(&self) -> usize {
        self.reads.load(Ordering::Acquire)
    }

    /// Resize the PTY; the child sees `SIGWINCH`. A no-op once ended.
    pub async fn resize(&self, cols: u16, rows: u16) -> Result<(), G5Error> {
        let master = self.master.lock().await;
        let Some(master) = master.as_ref() else {
            return Ok(());
        };
        master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| G5Error::Internal {
                message: format!("Failed to resize hosted PTY: {error}"),
            })
    }

    /// Snapshot the retained output from `requested_offset` (or the retained
    /// tail when it is unavailable) and a cursor that continues from the
    /// snapshot's end. Taken under the ring lock, so nothing is missed or
    /// duplicated between the snapshot and the cursor.
    pub fn replay_from(&self, requested_offset: Option<u64>) -> (ReplaySnapshot, HostedCursor) {
        let ring = self.ring.lock().unwrap();
        let snapshot = ring.replay_from(requested_offset);
        let offset = snapshot.offset + snapshot.data.len() as u64;
        drop(ring);
        (
            snapshot,
            HostedCursor {
                ring: Arc::clone(&self.ring),
                progress: self.progress.subscribe(),
                offset,
            },
        )
    }

    /// Stop retaining: wake the reader, close the master (the child's writes
    /// fail with `EIO`) and mark the session ended. Retained bytes stay
    /// readable until the session is dropped.
    pub async fn end(&self) {
        #[cfg(unix)]
        if let Some(wake) = self.reader_wake.lock().unwrap().take() {
            let _ = wake.shutdown(std::net::Shutdown::Write);
        }
        self.master.lock().await.take();
        if !self.ended.swap(true, Ordering::AcqRel) {
            let next_offset = self.ring.lock().unwrap().next_offset();
            let _ = self.progress.send_replace(Progress {
                next_offset,
                ended: true,
            });
        }
    }
}

impl Drop for HostedSession {
    fn drop(&mut self) {
        #[cfg(unix)]
        if let Some(wake) = self.reader_wake.lock().unwrap().take() {
            let _ = wake.shutdown(std::net::Shutdown::Write);
        }
    }
}

impl HostedCursor {
    /// The offset of the next byte this cursor will deliver.
    pub fn offset(&self) -> u64 {
        self.offset
    }

    /// The next retained bytes after this cursor, waiting for the writer when
    /// none are pending. `Err(Ended)` once the writer closed and everything
    /// up to the end was delivered; `Err(Lagged)` when the ring no longer
    /// holds the cursor's position.
    pub async fn next(&mut self, cancel: &CancellationToken) -> Result<Vec<u8>, HostedReadEnd> {
        loop {
            if cancel.is_cancelled() {
                return Err(HostedReadEnd::Superseded);
            }
            let progress = *self.progress.borrow_and_update();
            if progress.next_offset > self.offset {
                let bytes = self.ring.lock().unwrap().read_from(self.offset);
                return match bytes {
                    Some(bytes) if !bytes.is_empty() => {
                        self.offset += bytes.len() as u64;
                        Ok(bytes)
                    }
                    // The writer advanced past the snapshot we just read;
                    // loop to pick it up.
                    Some(_) => continue,
                    None => Err(HostedReadEnd::Lagged),
                };
            }
            if progress.ended {
                return Err(HostedReadEnd::Ended);
            }
            tokio::select! {
                biased;
                _ = cancel.cancelled() => return Err(HostedReadEnd::Superseded),
                changed = self.progress.changed() => {
                    if changed.is_err() {
                        // The session was dropped: the manager discarded it.
                        return Err(HostedReadEnd::Ended);
                    }
                }
            }
        }
    }
}
