//! Per-execution event journal and the incarnation disposition table.
//!
//! Two independent stores, deliberately separated (design v9 §7.5):
//!
//! * The **journal** under `<state>/journal/<exec_id>` is an append-only file of
//!   events carrying a monotonic per-execution `seq`. Every event is durable
//!   here *before* it is streamed on `rpc`, so a client that disconnects
//!   mid-run can replay a complete ordered prefix. `events.ack` prunes an
//!   acknowledged prefix, and a disk budget prunes oldest-acknowledged-first.
//!   The acknowledgement watermark of every execution is kept in
//!   `<state>/journal/.ack/<exec_id>` so that a fully pruned journal can never
//!   hand out a sequence number twice, and a terminal event closes the journal
//!   for good: nothing is appended after the outcome.
//! * The **disposition table** under `<state>/dispositions/<exec_id>` records
//!   `cancelled` or `already_finished` for the life of the incarnation. It is
//!   never pruned with the journal, so a late `modules.invoke` of a cancelled
//!   identity still answers `cancelled` without spawning, and a finished
//!   identity still answers `already_finished` with its outcome digest long
//!   after its events were acknowledged away.
//!
//! One mutex serialises every append, acknowledgement, prune and disposition
//! write, so a prune can never drop an event appended concurrently and the
//! first disposition recorded for an identity is the one that stays.

use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::modules::ExecId;

/// Total bytes of journal the dispatcher retains before pruning acknowledged
/// prefixes oldest-first. Unacknowledged events are never dropped for budget.
pub const DEFAULT_JOURNAL_BUDGET_BYTES: u64 = 64 * 1024 * 1024;
/// A single event is bounded well below the budget so one execution cannot
/// monopolise it; module output limits are enforced separately by the runner.
pub const MAX_EVENT_BYTES: usize = 1024 * 1024;
/// Replay page bounds (`g5.workspace.module-execution`): one reply line must
/// stay under the 4 MiB the host relay and the control plane read, and a
/// caller may ask for less. A page always carries at least one event.
pub const DEFAULT_REPLAY_LIMIT: u64 = 1024;
pub const MAX_REPLAY_LIMIT: u64 = 4096;
pub const DEFAULT_REPLAY_MAX_BYTES: u64 = 2 * 1024 * 1024;
pub const MIN_REPLAY_MAX_BYTES: u64 = 64 * 1024;
pub const MAX_REPLAY_MAX_BYTES: u64 = 3 * 1024 * 1024;
/// An acknowledged prefix is reclaimed from disk only once it is worth a
/// rewrite: at least this many bytes and at least a quarter of the retained
/// file. Smaller prefixes stay until a later acknowledgement, the budget
/// sweep, or until nothing unacknowledged remains (the file is removed), so a
/// consumer acknowledging in batches does not rewrite the same suffix after
/// every batch.
const COMPACTION_MIN_BYTES: u64 = 1024 * 1024;

const ACK_DIR: &str = ".ack";

#[derive(Debug, thiserror::Error)]
pub enum JournalError {
    #[error("journal io: {0}")]
    Io(String),
    #[error("event exceeds {MAX_EVENT_BYTES} bytes")]
    EventTooLarge,
    #[error("execution journal is closed by its terminal event")]
    Closed,
    #[error("execution journal is fenced after a failed append; restart repairs it")]
    Fenced,
}

fn io(error: std::io::Error) -> JournalError {
    JournalError::Io(error.to_string())
}

/// One journaled event. `seq` is dense and monotonic per execution starting at
/// 1; `terminal` marks the outcome, which is always the last event.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct JournalEvent {
    pub seq: u64,
    pub kind: String,
    pub payload: Value,
    #[serde(default)]
    pub terminal: bool,
}

/// One bounded page of unacknowledged events, oldest first.
#[derive(Clone, Debug, PartialEq)]
pub struct ReplayPage {
    pub events: Vec<JournalEvent>,
    /// Unacknowledged events remain beyond this page.
    pub has_more: bool,
    /// The sequence to request next: one past the last event returned, or
    /// the first unacknowledged sequence at or after the request when the
    /// page is empty.
    pub next_seq: u64,
}

/// Where one retained record starts in the execution's file. Records are
/// contiguous and in sequence order, so a record ends where the next starts
/// (or at the accounted file length).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct IndexEntry {
    seq: u64,
    offset: u64,
}

/// The durable disposition of an execution identity for this incarnation.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "disposition", rename_all = "snake_case")]
pub enum Disposition {
    /// A cancellation was recorded. This may precede any process, and may even
    /// precede the first `modules.invoke` of the identity. When the run
    /// nevertheless settled, its exact outcome is retained alongside without
    /// changing the decision.
    Cancelled {
        reason: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        settled: Option<Value>,
    },
    /// The execution reached a terminal outcome. The digest binds the exact
    /// outcome payload so a replayed invoke cannot be answered with a
    /// different result than the one already reported.
    AlreadyFinished {
        outcome: Value,
        outcome_digest: String,
    },
}

impl Disposition {
    pub fn to_value(&self) -> Value {
        match self {
            Self::Cancelled { reason, settled } => match settled {
                Some(outcome) => {
                    json!({"disposition":"cancelled","reason":reason,"outcome":outcome})
                }
                None => json!({"disposition":"cancelled","reason":reason}),
            },
            Self::AlreadyFinished {
                outcome,
                outcome_digest,
            } => json!({
                "disposition":"already_finished",
                "outcome":outcome,
                "outcome_digest":outcome_digest,
            }),
        }
    }
}

pub fn digest_of(value: &Value) -> String {
    // Serialised with sorted keys by serde_json's BTreeMap-backed maps, so the
    // digest is stable for an unchanged logical outcome.
    let bytes = serde_json::to_vec(value).unwrap_or_default();
    format!("{:x}", Sha256::digest(bytes))
}

/// Durable per-execution watermark, rewritten on every acknowledgement and
/// whenever the journal closes, and read back before any retained events.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Watermark {
    acked_through: u64,
    next_seq: u64,
    #[serde(default)]
    terminal: bool,
}

struct ExecutionState {
    /// Next sequence number to assign.
    next_seq: u64,
    /// Highest contiguous sequence acknowledged by the control plane.
    acked_through: u64,
    /// Bytes currently retained on disk for this execution.
    bytes: u64,
    /// Offsets of the retained records, in sequence order, so a replay reads
    /// only the page it serves instead of parsing the whole file.
    index: Vec<IndexEntry>,
    /// The terminal event was appended; the journal accepts nothing more.
    closed: bool,
    /// A failed append could not restore the file boundary; nothing is
    /// appended until a restart repairs the file.
    fenced: bool,
}

impl ExecutionState {
    /// Position in the index of the first record the control plane has not
    /// acknowledged; the index length when every retained record is.
    fn first_unacked(&self) -> usize {
        let acked = self.acked_through;
        self.index.partition_point(|entry| entry.seq <= acked)
    }

    /// Bytes of acknowledged records still on disk: the retained file starts
    /// with them, so they end where the first unacknowledged record starts.
    fn acked_bytes(&self) -> u64 {
        self.index
            .get(self.first_unacked())
            .map(|entry| entry.offset)
            .unwrap_or(self.bytes)
    }

    fn record_end(&self, position: usize) -> u64 {
        self.index
            .get(position + 1)
            .map(|entry| entry.offset)
            .unwrap_or(self.bytes)
    }

    /// An acknowledged prefix is reclaimed when nothing unacknowledged
    /// remains (a removal, never a rewrite) or when it is large enough to
    /// be worth rewriting the rest of the file.
    fn compaction_due(&self) -> bool {
        let acked = self.acked_bytes();
        acked > 0
            && (self.first_unacked() >= self.index.len()
                || acked >= COMPACTION_MIN_BYTES.max(self.bytes / 4))
    }
}

struct Inner {
    executions: BTreeMap<String, ExecutionState>,
    total_bytes: u64,
}

/// The journal plus disposition store for one incarnation.
pub struct Journal {
    journal_dir: PathBuf,
    ack_dir: PathBuf,
    disposition_dir: PathBuf,
    budget_bytes: u64,
    inner: Mutex<Inner>,
    /// Executions whose journal was still open when this journal was
    /// opened: supervision was lost before their terminal event.
    interrupted: Vec<(ExecId, Option<Disposition>)>,
}

impl Journal {
    pub fn open(state_dir: &Path, budget_bytes: u64) -> Result<Self, JournalError> {
        let journal_dir = state_dir.join("journal");
        let ack_dir = journal_dir.join(ACK_DIR);
        let disposition_dir = state_dir.join("dispositions");
        fs::create_dir_all(&ack_dir).map_err(io)?;
        fs::create_dir_all(&disposition_dir).map_err(io)?;
        // Journal, watermarks and dispositions are dispatcher-private.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for dir in [&journal_dir, &ack_dir, &disposition_dir] {
                fs::set_permissions(dir, fs::Permissions::from_mode(0o700)).map_err(io)?;
            }
        }
        let mut journal = Self {
            journal_dir,
            ack_dir,
            disposition_dir,
            budget_bytes,
            inner: Mutex::new(Inner {
                executions: BTreeMap::new(),
                total_bytes: 0,
            }),
            interrupted: Vec::new(),
        };
        journal.interrupted = journal.recover()?;
        Ok(journal)
    }

    /// Rebuild in-memory sequence state from the retained events and the
    /// acknowledgement watermarks, repair torn tails, reconcile the journal
    /// with the disposition table, and report executions whose journal is
    /// still open: `(exec_id, recorded disposition)` — `None` means
    /// undecided, `Some(Cancelled)` means cancellation was recorded but the
    /// run never produced its terminal event.
    fn recover(&self) -> Result<Vec<(ExecId, Option<Disposition>)>, JournalError> {
        let mut inner = self.inner.lock().unwrap();
        let mut keys = BTreeMap::new();
        for dir in [&self.journal_dir, &self.ack_dir] {
            let Ok(entries) = fs::read_dir(dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let Ok(name) = entry.file_name().into_string() else {
                    continue;
                };
                if entry.metadata().map(|m| m.is_file()).unwrap_or(false) {
                    keys.insert(name, ());
                }
            }
        }
        let mut interrupted = Vec::new();
        for key in keys.into_keys() {
            let Some(exec_id) = ExecId::from_file_name(&key) else {
                continue;
            };
            let path = self.journal_dir.join(&key);
            // A torn trailing record from an interrupted append is cut away
            // before anything is appended after it.
            let (records, torn) = read_records_repairing(&path)?;
            let mut index: Vec<IndexEntry> = records
                .iter()
                .map(|record| IndexEntry {
                    seq: record.event.seq,
                    offset: record.offset,
                })
                .collect();
            let events: Vec<JournalEvent> =
                records.into_iter().map(|record| record.event).collect();
            if torn {
                let mut body = Vec::new();
                index.clear();
                for event in &events {
                    index.push(IndexEntry {
                        seq: event.seq,
                        offset: body.len() as u64,
                    });
                    body.extend_from_slice(&serde_json::to_vec(event).unwrap_or_default());
                    body.push(b'\n');
                }
                if events.is_empty() {
                    let _ = fs::remove_file(&path);
                    sync_dir(&self.journal_dir)?;
                } else {
                    atomic_write(&path, &body)?;
                }
            }
            let bytes = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            let watermark = self.read_watermark(&exec_id)?;
            let highest = events.last().map(|event| event.seq).unwrap_or(0);
            let lowest = events.first().map(|event| event.seq).unwrap_or(0);
            let terminal_event = events.iter().find(|event| event.terminal).cloned();
            // A closing watermark whose terminal event is neither retained
            // nor acknowledged is the trace of an interrupted terminal
            // commit: the journal is treated as open so the terminal event is
            // re-emitted from the retained settlement.
            let terminal_unacknowledged = watermark.terminal
                && terminal_event.is_none()
                && watermark.acked_through + 1 < watermark.next_seq;
            let closed =
                (watermark.terminal && !terminal_unacknowledged) || terminal_event.is_some();
            inner.total_bytes += bytes;
            inner.executions.insert(
                key,
                ExecutionState {
                    // A terminal that never became durable was never
                    // streamed, so it keeps the sequence its closing
                    // watermark assigned — even when everything before it
                    // was pruned; otherwise the watermark's high-water mark
                    // stands.
                    next_seq: if terminal_unacknowledged {
                        (watermark.next_seq - 1)
                            .max(highest + 1)
                            .max(watermark.acked_through + 1)
                    } else {
                        (highest + 1).max(watermark.next_seq)
                    }
                    .max(1),
                    // Everything below the retained prefix was pruned, which
                    // only happens after acknowledgement; an acknowledged
                    // prefix may also still be on disk, awaiting compaction.
                    acked_through: lowest.saturating_sub(1).max(watermark.acked_through),
                    bytes,
                    index,
                    closed,
                    fenced: false,
                },
            );
            let disposition = self.read_disposition(&exec_id)?;
            if closed {
                // The outcome was journaled but the disposition write was
                // lost: reconstruct it, so the identity answers
                // `already_finished` instead of failing against a closed
                // journal.
                if disposition.is_none() {
                    let outcome = terminal_event.map(|event| event.payload).unwrap_or_else(|| {
                        json!({"status":"settled","detail":{"reason":"outcome_acknowledged_before_restart"}})
                    });
                    let reconstructed = Disposition::AlreadyFinished {
                        outcome_digest: digest_of(&outcome),
                        outcome,
                    };
                    let body = serde_json::to_vec(&reconstructed)
                        .map_err(|e| JournalError::Io(e.to_string()))?;
                    atomic_write(&self.disposition_path(&exec_id), &body)?;
                }
            } else {
                match disposition {
                    None => interrupted.push((exec_id, None)),
                    Some(Disposition::Cancelled { .. }) => interrupted.push((exec_id, disposition)),
                    // Finished per the table but the terminal event was
                    // never journaled: close the journal with that outcome.
                    Some(Disposition::AlreadyFinished { .. }) => {
                        interrupted.push((exec_id, disposition))
                    }
                }
            }
        }
        Ok(interrupted)
    }

    /// Executions whose journal was still open when this journal was
    /// opened, with the disposition recorded for them (if any).
    pub fn interrupted(&self) -> &[(ExecId, Option<Disposition>)] {
        &self.interrupted
    }

    fn journal_path(&self, exec_id: &ExecId) -> PathBuf {
        self.journal_dir.join(exec_id.file_name())
    }

    fn ack_path(&self, exec_id: &ExecId) -> PathBuf {
        self.ack_dir.join(exec_id.file_name())
    }

    fn disposition_path(&self, exec_id: &ExecId) -> PathBuf {
        self.disposition_dir.join(exec_id.file_name())
    }

    fn read_watermark(&self, exec_id: &ExecId) -> Result<Watermark, JournalError> {
        match fs::read(self.ack_path(exec_id)) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes).unwrap_or_default()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Watermark::default()),
            Err(error) => Err(io(error)),
        }
    }

    fn write_watermark(&self, exec_id: &ExecId, watermark: Watermark) -> Result<(), JournalError> {
        atomic_write(
            &self.ack_path(exec_id),
            &serde_json::to_vec(&watermark).map_err(|e| JournalError::Io(e.to_string()))?,
        )
    }

    /// Append an event and return it with its assigned sequence number. The
    /// caller streams the returned event only after this call succeeds. A
    /// terminal event closes the journal; later appends are refused.
    pub fn append(
        &self,
        exec_id: &ExecId,
        kind: &str,
        payload: Value,
        terminal: bool,
    ) -> Result<JournalEvent, JournalError> {
        let mut inner = self.inner.lock().unwrap();
        let key = exec_id.file_name();
        let state = inner
            .executions
            .entry(key.clone())
            .or_insert(ExecutionState {
                next_seq: 1,
                acked_through: 0,
                bytes: 0,
                index: Vec::new(),
                closed: false,
                fenced: false,
            });
        if state.closed {
            return Err(JournalError::Closed);
        }
        if state.fenced {
            return Err(JournalError::Fenced);
        }
        let event = JournalEvent {
            seq: state.next_seq,
            kind: kind.to_owned(),
            payload,
            terminal,
        };
        let mut line = serde_json::to_vec(&event).map_err(|e| JournalError::Io(e.to_string()))?;
        if line.len() > MAX_EVENT_BYTES {
            return Err(JournalError::EventTooLarge);
        }
        line.push(b'\n');
        let path = self.journal_path(exec_id);
        let created = !path.exists();
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(io)?;
        // The file must end exactly where the last successful append left
        // it. A torn prefix from an earlier failed write is cut away first;
        // a file that is shorter than accounted for cannot be trusted. A file
        // that vanished underneath the journal starts over: its retained
        // prefix is gone, but the sequence continues from memory.
        let previous_bytes = state.bytes;
        let restarted = created && previous_bytes > 0;
        if restarted {
            state.bytes = 0;
            state.index.clear();
        }
        let on_disk = file.metadata().map_err(io)?.len();
        if on_disk != state.bytes {
            if on_disk > state.bytes && file.set_len(state.bytes).is_ok() {
                // restored
            } else {
                state.fenced = true;
                return Err(JournalError::Fenced);
            }
        }
        let written = file
            .write_all(&line)
            // Durable before the event is streamed: a reader that replays
            // after a disconnect must never be missing an event a live client
            // already saw.
            .and_then(|_| file.sync_data());
        if let Err(error) = written {
            // Whatever reached the disk is cut back to the boundary, so the
            // next append (with the same sequence) starts clean; if even that
            // fails the journal is fenced until a restart repairs it.
            if file.set_len(state.bytes).is_err() {
                state.fenced = true;
            }
            return Err(io(error));
        }
        // The directory entry and, for a terminal event, the closing
        // watermark are part of the same commit: if either fails, the event
        // is cut back (or the journal fenced) so no sequence number can be
        // observed twice.
        let committed = (|| -> Result<(), JournalError> {
            if created {
                sync_dir(&self.journal_dir)?;
            }
            if terminal {
                self.write_watermark(
                    exec_id,
                    Watermark {
                        acked_through: state.acked_through,
                        next_seq: state.next_seq + 1,
                        terminal: true,
                    },
                )?;
            }
            Ok(())
        })();
        if let Err(error) = committed {
            // The event is cut back; a closing watermark that may already
            // have been renamed into place is reverted too, and an
            // indeterminate result fences the journal.
            if file.set_len(state.bytes).is_err() {
                state.fenced = true;
            }
            if terminal
                && self
                    .write_watermark(
                        exec_id,
                        Watermark {
                            acked_through: state.acked_through,
                            next_seq: state.next_seq,
                            terminal: false,
                        },
                    )
                    .is_err()
            {
                state.fenced = true;
            }
            return Err(error);
        }
        state.index.push(IndexEntry {
            seq: event.seq,
            offset: state.bytes,
        });
        state.next_seq += 1;
        state.bytes += line.len() as u64;
        if terminal {
            state.closed = true;
        }
        if restarted {
            inner.total_bytes = inner.total_bytes.saturating_sub(previous_bytes);
        }
        inner.total_bytes += line.len() as u64;
        self.enforce_budget_locked(&mut inner)?;
        Ok(event)
    }

    /// Whether the terminal event of this execution has been journaled.
    pub fn is_closed(&self, exec_id: &ExecId) -> bool {
        self.inner
            .lock()
            .unwrap()
            .executions
            .get(&exec_id.file_name())
            .is_some_and(|state| state.closed)
    }

    /// One bounded page of unacknowledged events with `seq >= from_seq`,
    /// read from the index: at most `limit` events (clamped to
    /// `1..=MAX_REPLAY_LIMIT`) whose serialized lines total at most
    /// `max_bytes` (clamped to `MIN..=MAX_REPLAY_MAX_BYTES`), except that a
    /// page always carries the first event even when that event alone is
    /// larger. Acknowledged records still on disk are never served again.
    pub fn replay_page(
        &self,
        exec_id: &ExecId,
        from_seq: u64,
        limit: u64,
        max_bytes: u64,
    ) -> Result<ReplayPage, JournalError> {
        let inner = self.inner.lock().unwrap();
        let empty = |next_seq: u64| ReplayPage {
            events: Vec::new(),
            has_more: false,
            next_seq,
        };
        let Some(state) = inner.executions.get(&exec_id.file_name()) else {
            return Ok(empty(from_seq));
        };
        let from = from_seq.max(state.acked_through.saturating_add(1));
        let start = state.index.partition_point(|entry| entry.seq < from);
        if start >= state.index.len() {
            return Ok(empty(from));
        }
        let limit = usize::try_from(limit.clamp(1, MAX_REPLAY_LIMIT)).unwrap_or(1);
        let max_bytes = max_bytes.clamp(MIN_REPLAY_MAX_BYTES, MAX_REPLAY_MAX_BYTES);
        let mut end = start;
        let mut bytes = 0_u64;
        while end < state.index.len() && end - start < limit {
            let record = state.record_end(end) - state.index[end].offset;
            if end > start && bytes + record > max_bytes {
                break;
            }
            bytes += record;
            end += 1;
        }
        let start_offset = state.index[start].offset;
        let end_offset = state.record_end(end - 1);
        let mut file = fs::File::open(self.journal_path(exec_id)).map_err(io)?;
        file.seek(SeekFrom::Start(start_offset)).map_err(io)?;
        let mut body = vec![0_u8; usize::try_from(end_offset - start_offset).unwrap_or(0)];
        file.read_exact(&mut body).map_err(io)?;
        let mut events = Vec::with_capacity(end - start);
        for line in body.split(|byte| *byte == b'\n') {
            if line.is_empty() {
                continue;
            }
            events.push(
                serde_json::from_slice::<JournalEvent>(line)
                    .map_err(|_| JournalError::Io("journal index is out of step".into()))?,
            );
        }
        if events.len() != end - start
            || events.first().map(|event| event.seq) != Some(state.index[start].seq)
        {
            return Err(JournalError::Io("journal index is out of step".into()));
        }
        let next_seq = events.last().map(|event| event.seq + 1).unwrap_or(from);
        Ok(ReplayPage {
            events,
            has_more: end < state.index.len(),
            next_seq,
        })
    }

    /// Every unacknowledged event with `seq >= from_seq`, in order, by
    /// reading pages until none remain. In-process callers only; the RPC
    /// surface serves one bounded page per request.
    pub fn replay(
        &self,
        exec_id: &ExecId,
        from_seq: u64,
    ) -> Result<Vec<JournalEvent>, JournalError> {
        let mut events = Vec::new();
        let mut from = from_seq;
        loop {
            let page = self.replay_page(exec_id, from, MAX_REPLAY_LIMIT, MAX_REPLAY_MAX_BYTES)?;
            events.extend(page.events);
            if !page.has_more {
                return Ok(events);
            }
            from = page.next_seq;
        }
    }

    /// Every record still on disk for the execution, acknowledged or not,
    /// for in-process evidence recovery that must not miss a fact the
    /// control plane already consumed but compaction has not yet reclaimed.
    pub fn replay_retained(&self, exec_id: &ExecId) -> Result<Vec<JournalEvent>, JournalError> {
        let _inner = self.inner.lock().unwrap();
        read_events(&self.journal_path(exec_id))
    }

    /// Acknowledge a contiguous prefix and prune it from disk. The whole
    /// transaction runs under the journal lock, so an append cannot slip in
    /// between reading and rewriting the retained file.
    pub fn ack(&self, exec_id: &ExecId, up_to_seq: u64) -> Result<u64, JournalError> {
        let mut inner = self.inner.lock().unwrap();
        let key = exec_id.file_name();
        let Some(state) = inner.executions.get_mut(&key) else {
            return Ok(0);
        };
        if up_to_seq <= state.acked_through {
            return Ok(state.acked_through);
        }
        // Only events that exist can be acknowledged, and the acknowledgement
        // is published in memory only once it is durable: pruning depends on
        // nothing that could be lost.
        let acked = up_to_seq.min(state.next_seq.saturating_sub(1));
        self.write_watermark(
            exec_id,
            Watermark {
                acked_through: acked,
                next_seq: state.next_seq,
                terminal: state.closed,
            },
        )?;
        state.acked_through = acked;
        // The acknowledgement is durable and in effect; the disk is reclaimed
        // only when the prefix is worth a rewrite or nothing else remains.
        if state.compaction_due() {
            self.prune_locked(&mut inner, exec_id)?;
        }
        Ok(acked)
    }

    /// Drop the acknowledged prefix from the retained file: the unacknowledged
    /// suffix is copied byte for byte from its indexed offset, so nothing is
    /// re-parsed and an append waiting on the lock cannot interleave.
    fn prune_locked(&self, inner: &mut Inner, exec_id: &ExecId) -> Result<(), JournalError> {
        let key = exec_id.file_name();
        let Some(state) = inner.executions.get_mut(&key) else {
            return Ok(());
        };
        if state.fenced {
            // The file boundary is untrusted until a restart repairs it.
            return Ok(());
        }
        let path = self.journal_path(exec_id);
        let first_unacked = state.first_unacked();
        let cut = state.acked_bytes();
        if cut == 0 {
            return Ok(());
        }
        let previous = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let mut body = Vec::new();
        if first_unacked < state.index.len() {
            let mut file = fs::File::open(&path).map_err(io)?;
            file.seek(SeekFrom::Start(cut)).map_err(io)?;
            body.resize(usize::try_from(state.bytes - cut).unwrap_or(0), 0);
            file.read_exact(&mut body).map_err(io)?;
        }
        if body.is_empty() {
            match fs::remove_file(&path) {
                Ok(()) => sync_dir(&self.journal_dir)?,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(io(error)),
            }
        } else {
            atomic_write(&path, &body)?;
        }
        state.index.drain(..first_unacked);
        for entry in &mut state.index {
            entry.offset -= cut;
        }
        state.bytes = body.len() as u64;
        inner.total_bytes = inner.total_bytes.saturating_sub(previous);
        inner.total_bytes += body.len() as u64;
        Ok(())
    }

    /// Prune acknowledged prefixes oldest-first until the budget is met.
    /// Unacknowledged events are retained even over budget: losing them would
    /// silently break replay, which is worse than exceeding a soft bound.
    fn enforce_budget_locked(&self, inner: &mut Inner) -> Result<(), JournalError> {
        if inner.total_bytes <= self.budget_bytes {
            return Ok(());
        }
        let candidates: Vec<String> = inner
            .executions
            .iter()
            .filter(|(_, state)| state.acked_bytes() > 0)
            .map(|(key, _)| key.clone())
            .collect();
        for key in candidates {
            let Some(exec_id) = ExecId::from_file_name(&key) else {
                continue;
            };
            self.prune_locked(inner, &exec_id)?;
            if inner.total_bytes <= self.budget_bytes {
                break;
            }
        }
        Ok(())
    }

    /// Executions whose terminal outcome has not been fully acknowledged.
    pub fn pending_outcomes(&self) -> Vec<Value> {
        let inner = self.inner.lock().unwrap();
        let mut pending = Vec::new();
        for key in inner.executions.keys() {
            let Some(exec_id) = ExecId::from_file_name(key) else {
                continue;
            };
            let Ok(events) = read_events(&self.journal_path(&exec_id)) else {
                continue;
            };
            if let Some(event) = events.iter().find(|event| event.terminal) {
                pending.push(json!({
                    "exec_id": exec_id.as_str(),
                    "seq": event.seq,
                    "outcome": event.payload,
                }));
            }
        }
        pending
    }

    /// Record a disposition. The first recorded disposition for an identity
    /// wins: a cancellation already reported to the control plane cannot be
    /// overwritten by a late completion, and vice versa. The check and the
    /// write are one transaction under the journal lock.
    pub fn record_disposition(
        &self,
        exec_id: &ExecId,
        disposition: &Disposition,
    ) -> Result<Disposition, JournalError> {
        let _inner = self.inner.lock().unwrap();
        if let Some(existing) = self.read_disposition(exec_id)? {
            return Ok(existing);
        }
        let body = serde_json::to_vec(disposition).map_err(|e| JournalError::Io(e.to_string()))?;
        atomic_write(&self.disposition_path(exec_id), &body)?;
        Ok(disposition.clone())
    }

    /// Record the exact settlement outcome of an execution. A fresh identity
    /// becomes `already_finished`; an identity that was cancelled keeps its
    /// cancellation decision and retains the outcome alongside it; a
    /// finished identity keeps its first outcome.
    pub fn record_settlement(
        &self,
        exec_id: &ExecId,
        outcome: &Value,
    ) -> Result<Disposition, JournalError> {
        let _inner = self.inner.lock().unwrap();
        let updated = match self.read_disposition(exec_id)? {
            None => Disposition::AlreadyFinished {
                outcome_digest: digest_of(outcome),
                outcome: outcome.clone(),
            },
            Some(Disposition::Cancelled {
                reason,
                settled: None,
            }) => Disposition::Cancelled {
                reason,
                settled: Some(outcome.clone()),
            },
            Some(existing) => return Ok(existing),
        };
        let body = serde_json::to_vec(&updated).map_err(|e| JournalError::Io(e.to_string()))?;
        atomic_write(&self.disposition_path(exec_id), &body)?;
        Ok(updated)
    }

    pub fn disposition(&self, exec_id: &ExecId) -> Result<Option<Disposition>, JournalError> {
        let _inner = self.inner.lock().unwrap();
        self.read_disposition(exec_id)
    }

    fn read_disposition(&self, exec_id: &ExecId) -> Result<Option<Disposition>, JournalError> {
        match fs::read(self.disposition_path(exec_id)) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes).ok()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(io(error)),
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn retained_bytes(&self) -> u64 {
        self.inner.lock().unwrap().total_bytes
    }
}

fn read_events(path: &Path) -> Result<Vec<JournalEvent>, JournalError> {
    read_records_repairing(path)
        .map(|(records, _)| records.into_iter().map(|record| record.event).collect())
}

/// A parsed record with where it starts in the file.
struct Record {
    event: JournalEvent,
    offset: u64,
}

/// Parse the retained records in file order with their offsets; the flag
/// reports a torn or malformed record (an interrupted append) or records out
/// of sequence order, which the caller repairs by rewriting the sorted
/// records before appending — the offset index relies on that order.
fn read_records_repairing(path: &Path) -> Result<(Vec<Record>, bool), JournalError> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok((Vec::new(), false))
        }
        Err(error) => return Err(io(error)),
    };
    let mut records: Vec<Record> = Vec::new();
    let mut torn = !bytes.is_empty() && !bytes.ends_with(b"\n");
    let mut offset = 0_u64;
    for line in bytes.split(|byte| *byte == b'\n') {
        let start = offset;
        offset += line.len() as u64 + 1;
        if line.is_empty() {
            continue;
        }
        // A torn record from an interrupted append is dropped rather than
        // failing the whole replay; its event was never acknowledged.
        match serde_json::from_slice::<JournalEvent>(line) {
            Ok(event) => {
                if records
                    .last()
                    .is_some_and(|previous| previous.event.seq >= event.seq)
                {
                    torn = true;
                }
                records.push(Record {
                    event,
                    offset: start,
                });
            }
            Err(_) => torn = true,
        }
    }
    records.sort_by_key(|record| record.event.seq);
    Ok((records, torn))
}

/// Make a directory entry change durable (creation, rename, removal).
fn sync_dir(dir: &Path) -> Result<(), JournalError> {
    #[cfg(unix)]
    {
        fs::File::open(dir).and_then(|d| d.sync_all()).map_err(io)
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
        Ok(())
    }
}

fn atomic_write(path: &Path, body: &[u8]) -> Result<(), JournalError> {
    let parent = path.parent().unwrap_or(Path::new("."));
    let temporary = parent.join(format!(
        ".{}-{}",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("entry"),
        uuid::Uuid::new_v4()
    ));
    let mut file = fs::File::create(&temporary).map_err(io)?;
    file.write_all(body).map_err(io)?;
    file.sync_data().map_err(io)?;
    drop(file);
    fs::rename(&temporary, path).map_err(io)?;
    sync_dir(parent)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn exec(vm: uuid::Uuid) -> ExecId {
        ExecId::parse(&format!("task.{}.1.{vm}", uuid::Uuid::new_v4())).unwrap()
    }

    #[test]
    fn concurrent_appends_and_acks_never_lose_unacknowledged_events() {
        let root = tempfile::tempdir().unwrap();
        let journal = Arc::new(Journal::open(root.path(), DEFAULT_JOURNAL_BUDGET_BYTES).unwrap());
        let id = exec(uuid::Uuid::new_v4());
        let writer = {
            let journal = journal.clone();
            let id = id.clone();
            std::thread::spawn(move || {
                // Large enough that acknowledgements cross the compaction
                // threshold repeatedly while appends continue.
                let filler = "x".repeat(16 * 1024);
                for index in 0..400u64 {
                    journal
                        .append(&id, "tick", json!({"index":index,"filler":filler}), false)
                        .unwrap();
                }
            })
        };
        let acker = {
            let journal = journal.clone();
            let id = id.clone();
            std::thread::spawn(move || {
                let mut acked = 0;
                while acked < 300 {
                    let highest = journal
                        .replay(&id, 1)
                        .unwrap()
                        .last()
                        .map(|e| e.seq)
                        .unwrap_or(0);
                    if highest > acked {
                        acked = journal.ack(&id, highest.min(300)).unwrap();
                    }
                    std::thread::yield_now();
                }
            })
        };
        writer.join().unwrap();
        acker.join().unwrap();
        let retained = journal.replay(&id, 1).unwrap();
        let seqs: Vec<u64> = retained.iter().map(|e| e.seq).collect();
        assert_eq!(
            seqs,
            (301..=400).collect::<Vec<_>>(),
            "retained set must be exactly the unacknowledged suffix"
        );
        assert!(
            retained
                .iter()
                .all(|e| e.payload["index"].as_u64() == Some(e.seq - 1)),
            "every page read through the index parses the record it names"
        );
        // Compaction happened at least once (300 x 16 KiB far exceeds the
        // threshold), and the physical file holds no acknowledged record.
        let path = root.path().join("journal").join(id.file_name());
        let physical = read_events(&path).unwrap();
        assert!(physical.first().is_some_and(|e| e.seq > 1));
        assert_eq!(
            physical.len(),
            retained.len() + (301 - physical[0].seq) as usize
        );
    }

    fn big(bytes: usize) -> Value {
        json!({"filler": "y".repeat(bytes)})
    }

    #[test]
    fn replay_pages_bound_events_and_bytes_and_report_the_remainder() {
        let root = tempfile::tempdir().unwrap();
        let journal = Journal::open(root.path(), DEFAULT_JOURNAL_BUDGET_BYTES).unwrap();
        let id = exec(uuid::Uuid::new_v4());
        for index in 0..10u64 {
            journal
                .append(&id, "tick", json!({"index":index}), false)
                .unwrap();
        }
        let page = journal
            .replay_page(&id, 1, 3, DEFAULT_REPLAY_MAX_BYTES)
            .unwrap();
        assert_eq!(
            page.events.iter().map(|e| e.seq).collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert!(page.has_more);
        assert_eq!(page.next_seq, 4);
        let rest = journal
            .replay_page(&id, page.next_seq, 100, DEFAULT_REPLAY_MAX_BYTES)
            .unwrap();
        assert_eq!(
            rest.events.iter().map(|e| e.seq).collect::<Vec<_>>(),
            (4..=10).collect::<Vec<_>>()
        );
        assert!(!rest.has_more);
        assert_eq!(rest.next_seq, 11);
        let beyond = journal
            .replay_page(&id, 11, 100, DEFAULT_REPLAY_MAX_BYTES)
            .unwrap();
        assert!(beyond.events.is_empty());
        assert_eq!(beyond.next_seq, 11);
        assert!(!beyond.has_more);
        // Acknowledged records are never served again, even when they are
        // still on disk and the caller asks from the beginning.
        assert_eq!(journal.ack(&id, 4).unwrap(), 4);
        let after_ack = journal
            .replay_page(&id, 1, 100, DEFAULT_REPLAY_MAX_BYTES)
            .unwrap();
        assert_eq!(after_ack.events[0].seq, 5);
        let none = journal
            .replay_page(&id, 1, 2, DEFAULT_REPLAY_MAX_BYTES)
            .unwrap();
        assert_eq!(
            none.events.iter().map(|e| e.seq).collect::<Vec<_>>(),
            [5, 6]
        );
        assert_eq!(
            journal
                .replay_page(&id, 1, 1, DEFAULT_REPLAY_MAX_BYTES)
                .unwrap()
                .next_seq,
            6
        );
        let empty_from_acked = journal.replay_page(&id, 2, 100, DEFAULT_REPLAY_MAX_BYTES);
        assert_eq!(empty_from_acked.unwrap().events[0].seq, 5);

        // Byte bound: 100 KiB records, a 256 KiB page carries two of them,
        // and a record larger than the page is still served alone.
        let wide = exec(uuid::Uuid::new_v4());
        for _ in 0..5 {
            journal
                .append(&wide, "blob", big(100 * 1024), false)
                .unwrap();
        }
        journal
            .append(&wide, "blob", big(900 * 1024), false)
            .unwrap();
        let first = journal.replay_page(&wide, 1, 100, 256 * 1024).unwrap();
        assert_eq!(
            first.events.iter().map(|e| e.seq).collect::<Vec<_>>(),
            [1, 2]
        );
        assert!(first.has_more);
        let second = journal
            .replay_page(&wide, first.next_seq, 100, 256 * 1024)
            .unwrap();
        assert_eq!(
            second.events.iter().map(|e| e.seq).collect::<Vec<_>>(),
            [3, 4]
        );
        let third = journal
            .replay_page(&wide, second.next_seq, 100, 256 * 1024)
            .unwrap();
        assert_eq!(third.events.iter().map(|e| e.seq).collect::<Vec<_>>(), [5]);
        assert!(
            third.has_more,
            "the large record did not fit beside the fifth"
        );
        let alone = journal
            .replay_page(&wide, third.next_seq, 100, 64 * 1024)
            .unwrap();
        assert_eq!(alone.events.len(), 1);
        assert_eq!(alone.events[0].seq, 6);
        assert!(alone.events[0].payload["filler"].as_str().unwrap().len() == 900 * 1024);
        assert!(!alone.has_more);
        // Clamps: a zero limit serves one event; an oversized limit is bounded.
        assert_eq!(journal.replay_page(&wide, 1, 0, 0).unwrap().events.len(), 1);
    }

    #[test]
    fn a_backlog_larger_than_the_reply_line_is_served_in_pages() {
        let root = tempfile::tempdir().unwrap();
        let journal = Journal::open(root.path(), DEFAULT_JOURNAL_BUDGET_BYTES).unwrap();
        let id = exec(uuid::Uuid::new_v4());
        for _ in 0..6 {
            journal.append(&id, "blob", big(900 * 1024), false).unwrap();
        }
        assert!(journal.retained_bytes() > 4 * 1024 * 1024);
        let mut pages = Vec::new();
        let mut from = 1;
        loop {
            let page = journal
                .replay_page(&id, from, DEFAULT_REPLAY_LIMIT, DEFAULT_REPLAY_MAX_BYTES)
                .unwrap();
            let line_bytes: usize = page
                .events
                .iter()
                .map(|e| serde_json::to_vec(e).unwrap().len() + 1)
                .sum();
            assert!(line_bytes <= DEFAULT_REPLAY_MAX_BYTES as usize);
            pages.push(page.events.iter().map(|e| e.seq).collect::<Vec<_>>());
            if !page.has_more {
                break;
            }
            from = page.next_seq;
        }
        assert_eq!(pages, vec![vec![1, 2], vec![3, 4], vec![5, 6]]);
        assert_eq!(
            journal.replay(&id, 1).unwrap().len(),
            6,
            "the in-process reader walks every page"
        );
    }

    #[test]
    fn acknowledgement_below_the_threshold_keeps_the_file_and_hides_the_prefix() {
        let root = tempfile::tempdir().unwrap();
        let id = exec(uuid::Uuid::new_v4());
        let path = root.path().join("journal").join(id.file_name());
        {
            let journal = Journal::open(root.path(), DEFAULT_JOURNAL_BUDGET_BYTES).unwrap();
            for index in 0..10u64 {
                journal
                    .append(&id, "tick", json!({"index":index}), false)
                    .unwrap();
            }
            let before = fs::metadata(&path).unwrap().modified().unwrap();
            let bytes_before = fs::metadata(&path).unwrap().len();
            assert_eq!(journal.ack(&id, 4).unwrap(), 4);
            assert_eq!(
                fs::metadata(&path).unwrap().len(),
                bytes_before,
                "a small acknowledged prefix is not rewritten"
            );
            assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), before);
            assert_eq!(
                journal
                    .replay(&id, 1)
                    .unwrap()
                    .iter()
                    .map(|e| e.seq)
                    .collect::<Vec<_>>(),
                (5..=10).collect::<Vec<_>>()
            );
            assert_eq!(journal.replay_retained(&id).unwrap().len(), 10);
        }
        // Reopen with the acknowledged prefix still on disk: the watermark
        // stands, nothing acknowledged is served, and no sequence repeats.
        let journal = Journal::open(root.path(), DEFAULT_JOURNAL_BUDGET_BYTES).unwrap();
        assert_eq!(
            journal
                .replay(&id, 1)
                .unwrap()
                .iter()
                .map(|e| e.seq)
                .collect::<Vec<_>>(),
            (5..=10).collect::<Vec<_>>()
        );
        assert_eq!(
            journal
                .append(&id, "tick", json!({"index":10}), false)
                .unwrap()
                .seq,
            11
        );
        assert_eq!(
            journal.ack(&id, 3).unwrap(),
            4,
            "a lower ack keeps the watermark"
        );
        assert_eq!(journal.ack(&id, 11).unwrap(), 11);
        assert!(
            !path.exists(),
            "a fully acknowledged file is removed, never rewritten"
        );
        drop(journal);
        let journal = Journal::open(root.path(), DEFAULT_JOURNAL_BUDGET_BYTES).unwrap();
        assert_eq!(
            journal
                .append(&id, "tick", json!({"index":11}), false)
                .unwrap()
                .seq,
            12
        );
    }

    #[test]
    fn a_large_acknowledged_prefix_is_compacted_and_the_index_survives_reopen() {
        let root = tempfile::tempdir().unwrap();
        let id = exec(uuid::Uuid::new_v4());
        let path = root.path().join("journal").join(id.file_name());
        let journal = Journal::open(root.path(), DEFAULT_JOURNAL_BUDGET_BYTES).unwrap();
        for _ in 0..3 {
            journal.append(&id, "blob", big(600 * 1024), false).unwrap();
        }
        let before = fs::metadata(&path).unwrap().len();
        assert_eq!(journal.ack(&id, 2).unwrap(), 2);
        let after = fs::metadata(&path).unwrap().len();
        assert!(
            after < before / 2,
            "1.2 MiB acknowledged exceeds the threshold"
        );
        assert_eq!(journal.retained_bytes(), after);
        let page = journal
            .replay_page(&id, 1, DEFAULT_REPLAY_LIMIT, DEFAULT_REPLAY_MAX_BYTES)
            .unwrap();
        assert_eq!(page.events.iter().map(|e| e.seq).collect::<Vec<_>>(), [3]);
        assert_eq!(
            page.events[0].payload["filler"].as_str().unwrap().len(),
            600 * 1024,
            "the rewritten file is read through shifted offsets"
        );
        assert_eq!(
            journal.append(&id, "tick", json!({}), false).unwrap().seq,
            4
        );
        drop(journal);
        let journal = Journal::open(root.path(), DEFAULT_JOURNAL_BUDGET_BYTES).unwrap();
        let page = journal
            .replay_page(&id, 3, DEFAULT_REPLAY_LIMIT, DEFAULT_REPLAY_MAX_BYTES)
            .unwrap();
        assert_eq!(
            page.events.iter().map(|e| e.seq).collect::<Vec<_>>(),
            [3, 4]
        );
        assert_eq!(page.events[0].kind, "blob");
        assert_eq!(page.events[1].kind, "tick");
        assert_eq!(
            journal.append(&id, "tick", json!({}), false).unwrap().seq,
            5
        );
        assert_eq!(
            journal
                .replay_page(&id, 5, 1, DEFAULT_REPLAY_MAX_BYTES)
                .unwrap()
                .events[0]
                .seq,
            5
        );
    }

    #[test]
    fn the_budget_sweep_reclaims_acknowledged_prefixes_below_the_threshold() {
        let root = tempfile::tempdir().unwrap();
        // A budget small enough that a 300 KiB journal exceeds it while the
        // acknowledged prefix stays under the 1 MiB compaction threshold.
        let journal = Journal::open(root.path(), 200 * 1024).unwrap();
        let id = exec(uuid::Uuid::new_v4());
        for _ in 0..10 {
            journal.append(&id, "blob", big(30 * 1024), false).unwrap();
        }
        let full = journal.retained_bytes();
        assert!(full > 300 * 1024);
        assert_eq!(journal.ack(&id, 5).unwrap(), 5);
        assert_eq!(
            journal.retained_bytes(),
            full,
            "the acknowledgement alone leaves the small prefix on disk"
        );
        journal.append(&id, "blob", big(30 * 1024), false).unwrap();
        assert!(
            journal.retained_bytes() < full,
            "the next append's budget sweep reclaimed the acknowledged prefix"
        );
        assert_eq!(
            journal
                .replay(&id, 1)
                .unwrap()
                .iter()
                .map(|e| e.seq)
                .collect::<Vec<_>>(),
            (6..=11).collect::<Vec<_>>()
        );
        let path = root.path().join("journal").join(id.file_name());
        assert_eq!(read_events(&path).unwrap()[0].seq, 6);
    }

    #[test]
    fn watermark_survives_full_prune_and_terminal_event_closes_journal() {
        let root = tempfile::tempdir().unwrap();
        let vm = uuid::Uuid::new_v4();
        let id = exec(vm);
        {
            let journal = Journal::open(root.path(), DEFAULT_JOURNAL_BUDGET_BYTES).unwrap();
            journal.append(&id, "a", json!({}), false).unwrap();
            journal.append(&id, "b", json!({}), false).unwrap();
            assert_eq!(journal.ack(&id, 2).unwrap(), 2);
            assert!(!root.path().join("journal").join(id.file_name()).exists());
        }
        let journal = Journal::open(root.path(), DEFAULT_JOURNAL_BUDGET_BYTES).unwrap();
        assert_eq!(journal.interrupted(), &[(id.clone(), None)]);
        let event = journal.append(&id, "c", json!({}), true).unwrap();
        assert_eq!(
            event.seq, 3,
            "a fully pruned journal must not reuse sequence numbers"
        );
        assert!(matches!(
            journal.append(&id, "d", json!({}), false),
            Err(JournalError::Closed)
        ));
        let reopened = Journal::open(root.path(), DEFAULT_JOURNAL_BUDGET_BYTES).unwrap();
        assert!(reopened.interrupted().is_empty());
        assert!(reopened.is_closed(&id));
        assert_eq!(reopened.pending_outcomes()[0]["seq"], 3);
    }

    #[test]
    fn first_disposition_wins_under_contention() {
        let root = tempfile::tempdir().unwrap();
        let journal = Arc::new(Journal::open(root.path(), DEFAULT_JOURNAL_BUDGET_BYTES).unwrap());
        let id = exec(uuid::Uuid::new_v4());
        let writers: Vec<_> = (0..8)
            .map(|index| {
                let journal = journal.clone();
                let id = id.clone();
                std::thread::spawn(move || {
                    journal
                        .record_disposition(
                            &id,
                            &Disposition::Cancelled {
                                reason: format!("writer-{index}"),
                                settled: None,
                            },
                        )
                        .unwrap()
                })
            })
            .collect();
        let results: Vec<Disposition> = writers.into_iter().map(|w| w.join().unwrap()).collect();
        assert!(results.iter().all(|d| *d == results[0]));
        assert_eq!(
            journal.disposition(&id).unwrap().as_ref(),
            Some(&results[0])
        );
    }

    #[test]
    fn append_restores_the_boundary_after_a_torn_write_in_the_same_process() {
        let root = tempfile::tempdir().unwrap();
        let journal = Journal::open(root.path(), DEFAULT_JOURNAL_BUDGET_BYTES).unwrap();
        let id = exec(uuid::Uuid::new_v4());
        journal.append(&id, "a", json!({}), false).unwrap();
        journal.append(&id, "b", json!({}), false).unwrap();
        // Simulate a write that failed part-way: bytes past the accounted
        // boundary with no newline.
        let path = root.path().join("journal").join(id.file_name());
        let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(b"{\"seq\":3,\"ki").unwrap();
        drop(file);
        let event = journal.append(&id, "c", json!({}), false).unwrap();
        assert_eq!(event.seq, 3);
        let replay = journal.replay(&id, 1).unwrap();
        assert_eq!(replay.iter().map(|e| e.seq).collect::<Vec<_>>(), [1, 2, 3]);
        let lines = fs::read_to_string(&path).unwrap();
        assert_eq!(
            lines.lines().count(),
            3,
            "the torn prefix was cut away, not joined"
        );
    }

    #[test]
    fn acknowledgement_is_published_only_once_its_watermark_is_durable() {
        let root = tempfile::tempdir().unwrap();
        let journal = Journal::open(root.path(), DEFAULT_JOURNAL_BUDGET_BYTES).unwrap();
        let id = exec(uuid::Uuid::new_v4());
        journal.append(&id, "a", json!({}), false).unwrap();
        journal.append(&id, "b", json!({}), false).unwrap();
        // The watermark directory becomes a plain file: the write must fail
        // and the acknowledgement must not take effect in memory.
        let ack_dir = root.path().join("journal").join(ACK_DIR);
        fs::remove_dir_all(&ack_dir).unwrap();
        fs::write(&ack_dir, b"not a directory").unwrap();
        assert!(journal.ack(&id, 2).is_err());
        assert!(
            journal.ack(&id, 2).is_err(),
            "an identical retry does not fake success"
        );
        assert_eq!(
            journal.replay(&id, 1).unwrap().len(),
            2,
            "nothing was pruned"
        );
        fs::remove_file(&ack_dir).unwrap();
        fs::create_dir_all(&ack_dir).unwrap();
        assert_eq!(journal.ack(&id, 2).unwrap(), 2);
        assert!(journal.replay(&id, 1).unwrap().is_empty());
        let reopened = Journal::open(root.path(), DEFAULT_JOURNAL_BUDGET_BYTES).unwrap();
        assert_eq!(reopened.append(&id, "c", json!({}), false).unwrap().seq, 3);
    }

    #[test]
    fn a_cancelled_identity_retains_its_exact_settlement_outcome() {
        let root = tempfile::tempdir().unwrap();
        let id = exec(uuid::Uuid::new_v4());
        let outcome = json!({"status":"succeeded","result":{"finished":"anyway"}});
        {
            let journal = Journal::open(root.path(), DEFAULT_JOURNAL_BUDGET_BYTES).unwrap();
            journal
                .record_disposition(
                    &id,
                    &Disposition::Cancelled {
                        reason: "stop".into(),
                        settled: None,
                    },
                )
                .unwrap();
            let settled = journal.record_settlement(&id, &outcome).unwrap();
            assert!(
                matches!(settled, Disposition::Cancelled { ref reason, settled: Some(ref o) } if reason == "stop" && *o == outcome)
            );
            // The decision is immutable; a second settlement changes nothing.
            let again = journal
                .record_settlement(&id, &json!({"status":"other"}))
                .unwrap();
            assert_eq!(again, settled);
        }
        let reopened = Journal::open(root.path(), DEFAULT_JOURNAL_BUDGET_BYTES).unwrap();
        assert!(matches!(
            reopened.disposition(&id).unwrap(),
            Some(Disposition::Cancelled { settled: Some(o), .. }) if o == outcome
        ));
    }

    #[test]
    fn a_closing_watermark_without_its_terminal_event_is_reopened_for_settlement() {
        let root = tempfile::tempdir().unwrap();
        let id = exec(uuid::Uuid::new_v4());
        let outcome = json!({"status":"succeeded"});
        {
            let journal = Journal::open(root.path(), DEFAULT_JOURNAL_BUDGET_BYTES).unwrap();
            journal.append(&id, "a", json!({}), false).unwrap();
            journal.record_settlement(&id, &outcome).unwrap();
            journal
                .append(&id, "outcome", outcome.clone(), true)
                .unwrap();
        }
        // A crash after the watermark was renamed but before the terminal
        // event became durable: only the first event remains.
        let path = root.path().join("journal").join(id.file_name());
        let first_line = fs::read_to_string(&path)
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .to_owned()
            + "\n";
        fs::write(&path, first_line).unwrap();
        let journal = Journal::open(root.path(), DEFAULT_JOURNAL_BUDGET_BYTES).unwrap();
        assert!(!journal.is_closed(&id));
        let interrupted = journal.interrupted();
        assert!(
            matches!(interrupted, [(exec, Some(Disposition::AlreadyFinished { outcome: o, .. }))] if *exec == id && *o == outcome)
        );
        let reissued = journal
            .append(&id, "outcome", outcome.clone(), true)
            .unwrap();
        assert_eq!(reissued.seq, 2, "the lost terminal keeps its sequence");
        assert_eq!(journal.pending_outcomes().len(), 1);
    }

    #[test]
    fn a_lost_terminal_after_a_fully_pruned_prefix_keeps_its_sequence() {
        let root = tempfile::tempdir().unwrap();
        let id = exec(uuid::Uuid::new_v4());
        let outcome = json!({"status":"succeeded"});
        let path = root.path().join("journal").join(id.file_name());
        {
            let journal = Journal::open(root.path(), DEFAULT_JOURNAL_BUDGET_BYTES).unwrap();
            journal.append(&id, "a", json!({}), false).unwrap();
            journal.append(&id, "b", json!({}), false).unwrap();
            assert_eq!(journal.ack(&id, 2).unwrap(), 2);
            assert!(!path.exists(), "a fully acknowledged prefix is pruned away");
            journal.record_settlement(&id, &outcome).unwrap();
            let terminal = journal
                .append(&id, "outcome", outcome.clone(), true)
                .unwrap();
            assert_eq!(terminal.seq, 3);
        }
        // The crash lost the terminal event after its closing watermark was
        // renamed: nothing of the journal remains but the watermark.
        fs::remove_file(&path).unwrap();
        let journal = Journal::open(root.path(), DEFAULT_JOURNAL_BUDGET_BYTES).unwrap();
        assert!(!journal.is_closed(&id));
        assert!(
            matches!(journal.interrupted(), [(exec, Some(Disposition::AlreadyFinished { .. }))] if *exec == id)
        );
        let reissued = journal
            .append(&id, "outcome", outcome.clone(), true)
            .unwrap();
        assert_eq!(
            reissued.seq, 3,
            "the lost terminal keeps its sequence past the pruned prefix"
        );
        let replayed = journal.replay(&id, 3).unwrap();
        assert_eq!(replayed.len(), 1);
        assert!(replayed[0].terminal);
        assert_eq!(journal.pending_outcomes().len(), 1);
        assert_eq!(journal.ack(&id, 3).unwrap(), 3);
        assert!(journal.pending_outcomes().is_empty());
        drop(journal);
        let journal = Journal::open(root.path(), DEFAULT_JOURNAL_BUDGET_BYTES).unwrap();
        assert!(journal.is_closed(&id));
        assert!(journal.interrupted().is_empty());
        assert!(journal.pending_outcomes().is_empty());
    }

    #[test]
    fn recovery_repairs_torn_tails_and_reconciles_dispositions() {
        let root = tempfile::tempdir().unwrap();
        let vm = uuid::Uuid::new_v4();
        let torn = exec(vm);
        let finished_without_disposition = exec(vm);
        let cancelled_open = exec(vm);
        {
            let journal = Journal::open(root.path(), DEFAULT_JOURNAL_BUDGET_BYTES).unwrap();
            journal.append(&torn, "a", json!({}), false).unwrap();
            journal.append(&torn, "b", json!({}), false).unwrap();
            journal
                .append(&finished_without_disposition, "a", json!({}), false)
                .unwrap();
            journal
                .append(
                    &finished_without_disposition,
                    "outcome",
                    json!({"status":"succeeded"}),
                    true,
                )
                .unwrap();
            journal
                .append(&cancelled_open, "a", json!({}), false)
                .unwrap();
            journal
                .record_disposition(
                    &cancelled_open,
                    &Disposition::Cancelled {
                        reason: "stop".into(),
                        settled: None,
                    },
                )
                .unwrap();
        }
        // A crash mid-append leaves a partial trailing record.
        let path = root.path().join("journal").join(torn.file_name());
        let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(b"{\"seq\":3,\"kind\":\"c\",\"pay").unwrap();
        drop(file);
        // The finished execution never had its disposition written (a crash
        // between the terminal append and the table write).
        assert!(!root
            .path()
            .join("dispositions")
            .join(finished_without_disposition.file_name())
            .exists());

        let journal = Journal::open(root.path(), DEFAULT_JOURNAL_BUDGET_BYTES).unwrap();
        let replay = journal.replay(&torn, 1).unwrap();
        assert_eq!(replay.iter().map(|e| e.seq).collect::<Vec<_>>(), [1, 2]);
        let next = journal.append(&torn, "c", json!({}), true).unwrap();
        assert_eq!(next.seq, 3);
        assert_eq!(
            journal.replay(&torn, 1).unwrap().len(),
            3,
            "the repaired file parses cleanly"
        );
        assert!(matches!(
            journal.disposition(&finished_without_disposition).unwrap(),
            Some(Disposition::AlreadyFinished { outcome, .. }) if outcome["status"] == "succeeded"
        ));
        let interrupted = journal.interrupted();
        assert!(interrupted.iter().any(|(id, d)| *id == torn && d.is_none()));
        assert!(interrupted.iter().any(
            |(id, d)| *id == cancelled_open && matches!(d, Some(Disposition::Cancelled { .. }))
        ));
        assert!(!interrupted
            .iter()
            .any(|(id, _)| *id == finished_without_disposition));
    }
}
