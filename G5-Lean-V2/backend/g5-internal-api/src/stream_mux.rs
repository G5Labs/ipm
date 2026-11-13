//! Transport-independent stream multiplexing for host tunnels and dispatchers.
//!
//! Channel-tagged, length-delimited frames carry control RPC, workspace RPC,
//! provider traffic, Preview HTTP/WebSockets and PTY streams over one byte
//! transport. Per-stream credit flow control prevents a stalled consumer from
//! blocking another stream. The live-stream cap bounds peer resource use.
//!
//! Retired Commit and SessionStore tags remain reserved for mixed-version
//! framing; application routers reset them.
//!
//! ## Wire format
//!
//! Every frame is a fixed 10-byte header followed by `length` payload bytes:
//!
//! ```text
//! +---------------+------+---------+-----------+------------------+
//! | stream_id u32 | kind | channel | length u32| payload[length]  |
//! |    (BE)       |  u8  |   u8    |   (BE)    |                  |
//! +---------------+------+---------+-----------+------------------+
//! ```
//!
//! * `stream_id` — logical stream id. The initiator (the side that calls
//!   [`Mux::open`]) allocates ids from its parity class (host = even, runner =
//!   odd) so the two peers never collide on an id.
//! * `kind` — [`FrameKind`]: `Open`, `Data`, `Credit`, `Close`, `Reset`.
//! * `channel` — [`Channel`] kind, meaningful on `Open`.
//! * `length` — payload byte length (`Data`/`Open` payloads; `Credit` payload is
//!   a u32 BE window increment).
//!
//! ## Flow control (backpressure)
//!
//! Each stream's *send* side holds a [`Semaphore`] window of [`STREAM_WINDOW`]
//! byte-permits; a `Data` frame consumes permits equal to its payload length. The
//! *receive* side replenishes the peer's window with a `Credit` frame as bytes
//! are drained by the consumer. Because the peer is only ever permitted to send
//! up to the window and the receiver's inbound buffer is sized to the same
//! window, the single demux reader never blocks on a full per-stream buffer — so
//! a slow consumer of one stream cannot head-of-line-block the streaming `llm` SSE.
//!
//! ### Inbound buffer is byte-accounted, never item-counted (P5 review MED-1)
//!
//! The per-stream inbound buffer is bounded by **bytes**, mirroring the
//! send-side credit window, *not* by frame count. The demux reader charges every
//! inbound `Data` payload's byte length against a per-stream byte budget (a
//! [`Semaphore`] of [`STREAM_WINDOW`] permits, the receive-side mirror of the
//! send window) before enqueuing it, and the consuming [`MuxReader`] releases
//! those bytes back as it drains. Because a credit-respecting peer can never have
//! more than `STREAM_WINDOW` bytes outstanding, the budget always admits honest
//! traffic and the single demux reader **never blocks** on a per-stream buffer —
//! the hand-off uses an unbounded channel guarded by the byte budget and a
//! non-blocking `send`, so a slow consumer of one stream can never head-of-line-block
//! the streaming `llm` SSE regardless of frame size. A peer that over-sends past
//! the credit it was granted, or sends a **zero-length** `Data` frame (the
//! cheapest item-exhaustion vector — it costs no credit yet still occupies a
//! dispatch), is a protocol violation for that stream: the reader `Reset`s it and
//! drops the sink rather than buffer unboundedly or park the shared reader. This
//! closes the earlier item-count caveat where a flood of tiny/zero-length frames
//! could exhaust a fixed frame-depth buffer long before the byte window and stall
//! the demux loop for every other channel.
//!
//! ## Teardown / reconnect
//!
//! The mux lifetime is the VM lifetime. A VM crash is terminal for the task, so
//! there is **no mid-session reconnect** of the underlying stream: on underlying
//! EOF/error the reader task ends, every live stream observes EOF (`recv` →
//! `None`), and the host maps the stdio EOF onto task lifecycle exactly as the
//! local-pipe EOF does today. Teardown is a clean `Reset`-all + close. This is a
//! deliberate scope choice, documented in `SELFREVIEW.md`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use g5_contracts::error::G5Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{mpsc, Mutex, Semaphore};
use tokio::task::JoinHandle;

/// Per-stream flow-control window in bytes. The send side may have at most this
/// many unacknowledged `Data` payload bytes outstanding; the receive inbound
/// buffer is byte-accounted to the same window (see [`StreamSink::inbound_budget`])
/// so the demux reader never blocks.
pub const STREAM_WINDOW: usize = 256 * 1024;

/// Maximum `Data` payload carried by a single frame. Larger writes are chunked.
/// Keeping frames bounded bounds reader-side allocation and keeps the SSE stream
/// responsive (each chunk is dispatched as soon as it is read).
pub const MAX_FRAME_PAYLOAD: usize = 16 * 1024;

/// Maximum bytes [`MuxReader::read_line`] may buffer for a single, not-yet-newline
/// -terminated line before treating the stream as protocol-violating (P5 review
/// HIGH). Once an inbound chunk is folded into the reader's `leftover` line buffer
/// its receive-window budget has already been released and peer credit returned —
/// so the per-stream byte window no longer bounds it, and a hostile guest could
/// stream `MAX_FRAME_PAYLOAD` chunks that never contain `\n` to grow `leftover`
/// without limit (host OOM). Capping the partial line at one full receive window
/// bounds buffered memory to `MAX_LINE_BYTES + MAX_FRAME_PAYLOAD`; the cap is far
/// larger than any legitimate line-delimited `g5.claude.*` frame (the only traffic
/// that uses `read_line` — the LLM SSE stream is byte-relayed via `recv`, never
/// line-buffered), so honest traffic is never affected.
pub const MAX_LINE_BYTES: usize = STREAM_WINDOW;

/// Maximum number of concurrently-live logical streams over one multiplex. A
/// hostile peer (the no-identity guest) could otherwise open unbounded streams
/// to exhaust host memory or — once the accept backlog filled — stall the single
/// demux reader and head-of-line-block every channel. The cap is enforced
/// **symmetrically**: peer `Open`s past it are `Reset` without registering, and a
/// local [`Mux::open`]/[`MuxHandle::open`] past it fails cleanly without
/// registering (P5 review MED-2 — a flood of untrusted loopback connections in
/// the in-VM runner must not be able to register unbounded local streams). The
/// accept queue is sized to match so any stream within the cap can be handed off
/// without the reader ever awaiting a full queue.
pub const MAX_LIVE_STREAMS: usize = 256;

/// Logical traffic class carried over the multiplex.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    /// Worker-host control-plane JSON-RPC stream. The host opens exactly one
    /// control stream on each authenticated host tunnel.
    Ctl,
    /// Workspace dispatcher JSON-RPC, private to the authenticated facade.
    Rpc,
    /// Line-delimited `g5.claude.*` sidecar frames (the agent I/O).
    Stdio,
    /// Retired task-commit application channel. Wire tag 2 stays reserved for
    /// mixed-version compatibility; active routers reset this channel.
    Commit,
    /// Retired LLM relay channel. Keep its wire tag reserved for old peers;
    /// active routers reject it. Native provider clients now connect directly.
    Llm,
    /// Raw HTTP or WebSocket TCP connection to a service inside a workspace VM.
    /// The control plane opens one stream per browser connection and prefixes
    /// the inner stream with the target guest port.
    Http,
    /// Interactive workspace terminal protocol. The browser-facing API opens
    /// one stream per terminal and the dispatcher owns the PTY lifecycle.
    Pty,
    /// Retired session-store tunnel channel. Provider session files moved to
    /// `chat_session_files` (design v6 §8.4), which the provider runner
    /// harvests and restores itself. Wire tag 5 stays reserved for
    /// mixed-version compatibility; active routers reset this channel.
    SessionStore,
}

impl Channel {
    /// Map to the on-wire `channel` byte.
    #[must_use]
    pub fn to_wire(self) -> u8 {
        match self {
            Channel::Ctl => 7,
            Channel::Stdio => 1,
            Channel::Commit => 2,
            Channel::Llm => 3,
            Channel::SessionStore => 5,
            Channel::Rpc => 6,
            Channel::Http => 8,
            Channel::Pty => 9,
        }
    }

    fn from_wire(b: u8) -> Option<Channel> {
        match b {
            7 => Some(Channel::Ctl),
            1 => Some(Channel::Stdio),
            2 => Some(Channel::Commit),
            3 => Some(Channel::Llm),
            5 => Some(Channel::SessionStore),
            6 => Some(Channel::Rpc),
            8 => Some(Channel::Http),
            9 => Some(Channel::Pty),
            _ => None,
        }
    }
}

/// Length of the header that opens an [`Channel::Http`] stream.
pub const HTTP_PORT_HEADER_LEN: usize = 2;

/// The header that opens an [`Channel::Http`] stream: the guest-local TCP
/// port the stream connects to, big-endian. The control plane writes it
/// before any request bytes; the dispatcher reads it first.
#[must_use]
pub fn encode_http_port(port: u16) -> [u8; HTTP_PORT_HEADER_LEN] {
    port.to_be_bytes()
}

/// The port named by an [`Channel::Http`] stream's first
/// [`HTTP_PORT_HEADER_LEN`] bytes, or `None` when fewer arrived.
#[must_use]
pub fn decode_http_port(header: &[u8]) -> Option<u16> {
    let bytes = header.get(..HTTP_PORT_HEADER_LEN)?;
    Some(u16::from_be_bytes([bytes[0], bytes[1]]))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FrameKind {
    Open,
    Data,
    Credit,
    Close,
    Reset,
}

impl FrameKind {
    fn to_wire(self) -> u8 {
        match self {
            FrameKind::Open => 1,
            FrameKind::Data => 2,
            FrameKind::Credit => 3,
            FrameKind::Close => 4,
            FrameKind::Reset => 5,
        }
    }

    fn from_wire(b: u8) -> Option<FrameKind> {
        match b {
            1 => Some(FrameKind::Open),
            2 => Some(FrameKind::Data),
            3 => Some(FrameKind::Credit),
            4 => Some(FrameKind::Close),
            5 => Some(FrameKind::Reset),
            _ => None,
        }
    }
}

/// A frame queued for the single writer task.
struct Frame {
    stream_id: u32,
    kind: FrameKind,
    channel: u8,
    payload: Vec<u8>,
}

impl Frame {
    /// A payload-less control frame (`Open`/`Close`/`Reset`/`Credit`-less) on the
    /// default channel byte. Used for the `Reset`/`Close` signalling paths.
    fn control(stream_id: u32, kind: FrameKind) -> Frame {
        Frame {
            stream_id,
            kind,
            channel: 0,
            payload: Vec::new(),
        }
    }

    fn header(&self) -> [u8; 10] {
        let mut h = [0u8; 10];
        h[0..4].copy_from_slice(&self.stream_id.to_be_bytes());
        h[4] = self.kind.to_wire();
        h[5] = self.channel;
        h[6..10].copy_from_slice(&(self.payload.len() as u32).to_be_bytes());
        h
    }
}

/// Receive-side handle the reader task keeps for each open stream. Tracks both
/// directions independently so a `Close` half-closes one direction while the
/// reverse keeps flowing (the entry is dropped only once both directions close).
struct StreamSink {
    /// Pushes inbound `Data` payloads to the owning [`MuxStream`]. `Some` while
    /// the peer→local (receive) direction is open; set to `None` on a peer
    /// `Close` so the local reader's `recv` returns `None` (EOF) **without**
    /// tearing the entry down — the reverse (local→peer) direction keeps its
    /// `send_window` so later `Credit` still lands (half-close). The channel is
    /// *unbounded*: flow control is enforced by `inbound_budget` (bytes), not by
    /// item count, so the demux reader never blocks on a full queue (MED-1).
    inbound: Option<mpsc::UnboundedSender<Vec<u8>>>,
    /// Receive-side byte budget mirroring [`STREAM_WINDOW`]: the demux reader
    /// charges every inbound `Data` payload's length here (non-blocking) before
    /// enqueuing, and the owning [`MuxReader`] releases the bytes back as it
    /// drains. A credit-respecting peer (≤ one window outstanding) always fits, so
    /// the reader never blocks; an over-send fails the charge and `Reset`s the
    /// stream rather than head-of-line-blocking every other channel (MED-1).
    inbound_budget: Arc<Semaphore>,
    /// Replenished by inbound `Credit` frames; governs how much the *peer* may
    /// send. Shared with the [`MuxStream`] send path via `Arc`. Capped at
    /// [`STREAM_WINDOW`] so a hostile peer cannot over-credit and defeat flow
    /// control (or overflow the semaphore).
    send_window: Arc<Semaphore>,
    /// Set once the local→peer (send) direction is closed — by our
    /// [`MuxWriter::close`] or a `Reset`. Shared with the [`MuxWriter`] so either
    /// closer can detect when both directions are done and drop the entry.
    send_closed: Arc<std::sync::atomic::AtomicBool>,
    /// `true` once the peer→local (receive) direction is closed (peer `Close`).
    recv_closed: Arc<AtomicBool>,
}

type Registry = Arc<Mutex<HashMap<u32, StreamSink>>>;

fn reset_stream_on_drop(
    stream_id: u32,
    outbound: mpsc::Sender<Frame>,
    registry: Registry,
    reset_sent: Arc<AtomicBool>,
) {
    if reset_sent.swap(true, Ordering::AcqRel) {
        return;
    }
    let reset = Frame::control(stream_id, FrameKind::Reset);
    if let Ok(runtime) = tokio::runtime::Handle::try_current() {
        runtime.spawn(async move {
            if let Some(sink) = registry.lock().await.remove(&stream_id) {
                sink.send_window.close();
            }
            let _ = outbound.send(reset).await;
        });
    } else {
        if let Ok(mut registered) = registry.try_lock() {
            if let Some(sink) = registered.remove(&stream_id) {
                sink.send_window.close();
            }
        }
        let _ = outbound.try_send(reset);
    }
}

/// Write half of a logical stream. Cheaply shareable (it holds only channel
/// senders + the window `Arc`), so the stdio transport can hand an `Arc` of it
/// to both the frame writer and the inbox→stdin injector, exactly like the
/// local-pipe `Arc<Mutex<ChildStdin>>`.
pub struct MuxWriter {
    stream_id: u32,
    outbound: mpsc::Sender<Frame>,
    /// The *peer's* permission to receive from us — acquired before sending.
    send_window: Arc<Semaphore>,
    /// Set once this send half is closed; shared with the registry's
    /// [`StreamSink::send_closed`] so the reader and the writer agree on when
    /// both directions have closed and the entry can be dropped.
    closed: Arc<std::sync::atomic::AtomicBool>,
    /// Registry handle so `close` can drop the entry once both directions are
    /// closed (a fully-closed stream must stop counting toward the live cap).
    registry: Registry,
    reset_sent: Arc<AtomicBool>,
}

impl MuxWriter {
    /// Send `data` to the peer, chunked to [`MAX_FRAME_PAYLOAD`] and bounded by
    /// the peer's flow-control window. Awaits window credit, so a slow peer
    /// naturally backpressures the caller (the relay pump stops reading its
    /// source socket, which backpressures the OS buffer).
    pub async fn send(&self, data: &[u8]) -> Result<(), G5Error> {
        if self.closed.load(Ordering::Relaxed) {
            return Err(G5Error::Internal {
                message: "mux stream send half is closed".to_string(),
            });
        }
        for chunk in data.chunks(MAX_FRAME_PAYLOAD) {
            // Acquire one permit per payload byte; forget them — they are
            // re-granted by the peer via `Credit` frames, not by drop.
            let permit = self
                .send_window
                .acquire_many(chunk.len() as u32)
                .await
                .map_err(|_| G5Error::Internal {
                    message: "mux stream window closed".to_string(),
                })?;
            permit.forget();
            self.outbound
                .send(Frame {
                    stream_id: self.stream_id,
                    kind: FrameKind::Data,
                    channel: 0,
                    payload: chunk.to_vec(),
                })
                .await
                .map_err(|_| G5Error::Internal {
                    message: "mux writer task gone".to_string(),
                })?;
        }
        Ok(())
    }

    /// Cancellation-safe bounded send for one small control frame. Flow
    /// credit and outbound capacity are both reserved before either is spent;
    /// after both reservations succeed, enqueue is synchronous. Therefore a
    /// timeout sends zero bytes and consumes zero credit, while success sends
    /// the whole single mux frame — never a partial control line or leaked
    /// transport credit on a surviving sticky runtime.
    pub async fn send_single_bounded(
        &self,
        data: &[u8],
        timeout: Duration,
    ) -> Result<bool, G5Error> {
        if data.is_empty() {
            return Err(G5Error::Internal {
                message: "bounded mux control frame must not be empty".to_string(),
            });
        }
        if data.len() > MAX_FRAME_PAYLOAD {
            return Err(G5Error::Internal {
                message: format!(
                    "bounded mux control frame exceeds single-frame cap: {} > {}",
                    data.len(),
                    MAX_FRAME_PAYLOAD
                ),
            });
        }
        if self.closed.load(Ordering::Relaxed) {
            return Err(G5Error::Internal {
                message: "mux stream send half is closed".to_string(),
            });
        }
        let deadline = tokio::time::Instant::now() + timeout;
        let flow_permit = match tokio::time::timeout_at(
            deadline,
            self.send_window.acquire_many(data.len() as u32),
        )
        .await
        {
            Ok(Ok(permit)) => permit,
            Ok(Err(_)) => {
                return Err(G5Error::Internal {
                    message: "mux stream window closed".to_string(),
                })
            }
            Err(_) => return Ok(false),
        };
        let outbound_permit = match tokio::time::timeout_at(deadline, self.outbound.reserve()).await
        {
            Ok(Ok(permit)) => permit,
            Ok(Err(_)) => {
                return Err(G5Error::Internal {
                    message: "mux writer task gone".to_string(),
                })
            }
            Err(_) => return Ok(false),
        };
        outbound_permit.send(Frame {
            stream_id: self.stream_id,
            kind: FrameKind::Data,
            channel: 0,
            payload: data.to_vec(),
        });
        flow_permit.forget();
        Ok(true)
    }

    /// Half-close the send direction: the peer's `recv` returns `None` once it
    /// drains buffered data. The reverse (peer→local) direction is untouched —
    /// we keep receiving until the peer closes too. Idempotent.
    pub async fn close(&self) {
        if self.closed.swap(true, Ordering::Relaxed) {
            return;
        }
        let _ = self
            .outbound
            .send(Frame::control(self.stream_id, FrameKind::Close))
            .await;
        // If the peer has already closed the reverse direction, both halves are
        // done: drop the registry entry so a fully-closed stream no longer
        // counts toward the live-stream cap. If the peer has not closed yet, the
        // reader's `Close` arm will perform the symmetric drop when it does.
        let mut reg = self.registry.lock().await;
        if reg
            .get(&self.stream_id)
            .is_some_and(|s| s.recv_closed.load(Ordering::Acquire))
        {
            reg.remove(&self.stream_id);
        }
    }
}

impl Drop for MuxWriter {
    fn drop(&mut self) {
        if !self.closed.load(Ordering::Acquire) {
            reset_stream_on_drop(
                self.stream_id,
                self.outbound.clone(),
                self.registry.clone(),
                self.reset_sent.clone(),
            );
        }
    }
}

/// Read half of a logical stream. Owned (single consumer), yielding inbound
/// chunks and returning flow-control credit as bytes are drained.
pub struct MuxReader {
    stream_id: u32,
    inbound: mpsc::UnboundedReceiver<Vec<u8>>,
    /// Receive-side byte budget shared with the demux reader's [`StreamSink`].
    /// Released (`add_permits`) as chunks are drained out of `inbound` so the
    /// reader's charge for future frames recovers in lockstep with consumption.
    inbound_budget: Arc<Semaphore>,
    /// Leftover bytes from a partially consumed inbound chunk (line buffering).
    leftover: Vec<u8>,
    outbound: mpsc::Sender<Frame>,
    /// Registry handle so [`read_line`](MuxReader::read_line) can `Reset` and
    /// tear down this stream (drop the sink, close the send window) when a peer
    /// floods bytes with no newline past [`MAX_LINE_BYTES`] — exactly the
    /// `reader_loop` `Reset` teardown, performed from the consumer side.
    registry: Registry,
    recv_closed: Arc<AtomicBool>,
    reset_sent: Arc<AtomicBool>,
    /// Bytes drained from `inbound` not yet acknowledged back to the peer.
    unacked: usize,
}

impl MuxReader {
    /// Receive the next inbound chunk, or `None` at peer EOF (`Close`/teardown).
    /// Releases the receive-side byte budget and replenishes the peer's send
    /// window for the bytes handed back.
    pub async fn recv(&mut self) -> Option<Vec<u8>> {
        // Cancellation safety (stream_mux D5, issue #1598 fix-forward): flush
        // credit owed for PRIOR consumption first, park inbound bytes in the
        // persistent `leftover` buffer before any further await, and take them
        // out only on the synchronous path straight to `return` — dropping
        // this future at any await point loses no bytes and no credit.
        self.flush_credit().await;
        if self.leftover.is_empty() {
            let chunk = self.inbound.recv().await?;
            self.release_inbound(chunk.len());
            self.leftover = chunk;
        }
        // Consumption is counted synchronously at the take; the grant is sent
        // by the NEXT reader-method entry (bounded by the half-window batching
        // this credit path already had).
        self.unacked += self.leftover.len();
        Some(std::mem::take(&mut self.leftover))
    }

    /// Release `n` bytes of receive-side buffer budget once a chunk leaves the
    /// inbound channel, so the demux reader's per-frame charge recovers. Never
    /// exceeds [`STREAM_WINDOW`]: only previously-charged bytes are returned.
    fn release_inbound(&self, n: usize) {
        if n > 0 {
            self.inbound_budget.add_permits(n);
        }
    }

    /// Send the accumulated consumed-byte credit back to the peer as a
    /// `Credit` frame. Batched at half the window to avoid a credit storm on
    /// tiny reads. Cancellation-safe with EXACTLY-ONCE grants (stream_mux D5):
    /// `unacked` is subtracted only AFTER the channel send returns — tokio's
    /// mpsc guarantees a cancelled `send` did NOT enqueue the frame, so a
    /// dropped flush retries the same grant later, and a completed send never
    /// double-grants. (The previous zero-before-send shape leaked the whole
    /// grant on cancellation: one lost half-window grant halves the peer's
    /// send window for good; two wedge the stream.)
    async fn flush_credit(&mut self) {
        if self.unacked >= STREAM_WINDOW / 2 {
            let grant = self.unacked as u32;
            let send_result = self
                .outbound
                .send(Frame {
                    stream_id: self.stream_id,
                    kind: FrameKind::Credit,
                    channel: 0,
                    payload: grant.to_be_bytes().to_vec(),
                })
                .await;
            // A closed outbound channel means the mux is torn down — the
            // un-sent grant is irrelevant; keep the accounting exact anyway.
            if send_result.is_ok() {
                self.unacked = 0;
            }
        }
    }

    /// Read the next newline-delimited line (newline stripped), buffering across
    /// inbound chunks. Returns `None` at EOF. Used by the stdio transport.
    ///
    /// Bounds the buffered partial line at [`MAX_LINE_BYTES`] (P5 review HIGH): a
    /// hostile guest could otherwise stream `MAX_FRAME_PAYLOAD` chunks that never
    /// contain `\n`, each releasing receive-window budget and earning fresh peer
    /// credit, and grow `leftover` without limit (host OOM) — the per-stream byte
    /// window cannot protect bytes that have already moved into `leftover`. When
    /// `leftover` exceeds the cap with no newline it is a protocol violation for
    /// this stream: [`reset_stream`](MuxReader::reset_stream) tears it down
    /// (`Reset` to the peer, drop the sink, close the send window) and `read_line`
    /// returns `None` (EOF), so buffered memory stays bounded by
    /// `MAX_LINE_BYTES + MAX_FRAME_PAYLOAD` regardless of newline behavior.
    pub async fn read_line(&mut self) -> Option<String> {
        self.read_line_with_limit(MAX_LINE_BYTES).await
    }

    /// Read a newline-delimited line with a caller-selected byte limit.
    ///
    /// Protocols with an explicit message limit may use a larger bound than
    /// [`MAX_LINE_BYTES`]. The same reset behavior still bounds memory to the
    /// selected limit plus one mux frame.
    pub async fn read_line_with_limit(&mut self, max_line_bytes: usize) -> Option<String> {
        // Flush credit owed for PRIOR consumption first (stream_mux D5,
        // CodeRabbit PR #1698): a cancelled flush leaves `unacked` pending
        // while `leftover` may already hold a complete line — the early-return
        // path below would otherwise hand that line back without ever
        // flushing, and the following call could block on inbound data while
        // the peer starves for the unsent credit.
        self.flush_credit().await;
        loop {
            if let Some(pos) = self.leftover.iter().position(|&b| b == b'\n') {
                if pos > max_line_bytes {
                    self.reset_stream().await;
                    return None;
                }
                let line: Vec<u8> = self.leftover.drain(..=pos).collect();
                // Strip trailing '\n' (and a paired '\r').
                let mut end = line.len() - 1;
                if end > 0 && line[end - 1] == b'\r' {
                    end -= 1;
                }
                return Some(String::from_utf8_lossy(&line[..end]).into_owned());
            }
            // No newline yet: refuse to buffer an unbounded partial line. The cap
            // is checked here (after the newline scan, before pulling more) so a
            // peer that keeps streaming newline-less chunks is `Reset` the moment
            // `leftover` crosses one full window rather than growing forever.
            if self.leftover.len() > max_line_bytes {
                self.reset_stream().await;
                return None;
            }
            match self.inbound.recv().await {
                Some(chunk) => {
                    // Cancellation safety (stream_mux D5, issue #1598
                    // fix-forward): bytes go into the persistent `leftover`
                    // buffer BEFORE the awaited credit flush — the pre-fix
                    // order (`ack().await` before the extend) dropped the
                    // chunk when a select loop cancelled this future
                    // mid-await, losing a buffered sidecar frame at the
                    // park-loop race. Consumption is counted synchronously;
                    // the flush is exactly-once under cancellation.
                    self.release_inbound(chunk.len());
                    self.leftover.extend_from_slice(&chunk);
                    self.unacked += chunk.len();
                    self.flush_credit().await;
                }
                None => {
                    if self.leftover.is_empty() {
                        return None;
                    }
                    let rest = std::mem::take(&mut self.leftover);
                    return Some(String::from_utf8_lossy(&rest).into_owned());
                }
            }
        }
    }

    /// Tear down this stream from the consumer side as a protocol violation:
    /// tell the peer to stop (`Reset`), drop the receive sink so any in-flight
    /// routing stops, and close the send window so a writer blocked in
    /// `acquire_many` errors out instead of leaking. Mirrors the `reader_loop`
    /// `Reset` arm; idempotent enough for the `read_line` cap path (the sink may
    /// already be gone on a torn-down mux). Clears `leftover` so the over-cap
    /// buffer is freed promptly.
    async fn reset_stream(&mut self) {
        self.reset_sent.store(true, Ordering::Release);
        let _ = self
            .outbound
            .send(Frame::control(self.stream_id, FrameKind::Reset))
            .await;
        if let Some(sink) = self.registry.lock().await.remove(&self.stream_id) {
            sink.send_window.close();
        }
        self.leftover.clear();
    }
}

impl Drop for MuxReader {
    fn drop(&mut self) {
        if !self.recv_closed.load(Ordering::Acquire) {
            reset_stream_on_drop(
                self.stream_id,
                self.outbound.clone(),
                self.registry.clone(),
                self.reset_sent.clone(),
            );
        }
    }
}

/// One logical bidirectional stream over the multiplex.
///
/// Exposes async byte semantics (`send`/`recv`/`close`) rather than
/// `AsyncRead`/`AsyncWrite`: the relays pump bytes between a stream and a real
/// socket/child pipe with two explicit loops (one per [`into_split`] half),
/// which keeps flow control (the async window acquire) out of a manual
/// `poll_write` and makes streaming behavior obvious.
///
/// [`into_split`]: MuxStream::into_split
pub struct MuxStream {
    channel: Channel,
    writer: MuxWriter,
    reader: MuxReader,
}

impl MuxStream {
    /// The logical traffic class this stream carries.
    pub fn channel(&self) -> Channel {
        self.channel
    }

    /// Send `data` to the peer (see [`MuxWriter::send`]).
    pub async fn send(&self, data: &[u8]) -> Result<(), G5Error> {
        self.writer.send(data).await
    }

    /// Receive the next inbound chunk (see [`MuxReader::recv`]).
    pub async fn recv(&mut self) -> Option<Vec<u8>> {
        self.reader.recv().await
    }

    /// Read the next newline-delimited line (see [`MuxReader::read_line`]).
    pub async fn read_line(&mut self) -> Option<String> {
        self.reader.read_line().await
    }

    /// Read one newline-delimited message with an explicit protocol limit.
    pub async fn read_line_with_limit(&mut self, max_line_bytes: usize) -> Option<String> {
        self.reader.read_line_with_limit(max_line_bytes).await
    }

    /// Half-close the send direction (see [`MuxWriter::close`]).
    pub async fn close(&mut self) {
        self.writer.close().await
    }

    /// Split into independently-owned write and read halves for the relay pumps
    /// and the stdio transport seam.
    pub fn into_split(self) -> (MuxWriter, MuxReader) {
        (self.writer, self.reader)
    }
}

/// A multiplex over one duplex byte stream. Splittable conceptually into a
/// reader task (owns the read half, demuxes) and a writer task (owns the write
/// half, serializes frames); the [`Mux`] handle opens new local streams and
/// accepts peer-opened ones.
pub struct Mux {
    outbound: mpsc::Sender<Frame>,
    registry: Registry,
    closed: Arc<AtomicBool>,
    accept_rx: mpsc::Receiver<MuxStream>,
    next_id: Arc<AtomicU32>,
    reader: JoinHandle<()>,
    writer: Option<JoinHandle<()>>,
}

/// Bounded drain window a dropped [`Mux`] grants its writer pump before the
/// abort backstop severs the connection (D7). Long enough to flush any queued
/// frames on a healthy carrier; short enough that even a leaked stream/handle
/// cannot strand the peer waiting for EOF.
const WRITER_DRAIN_GRACE: Duration = Duration::from_secs(5);

/// Dropping the `Mux` CLOSES the carrier for the peer (D7). This is
/// load-bearing, not hygiene: the pump tasks cannot end on their own once the
/// owning `Mux` is gone — `reader_loop` holds an outbound-sender clone, so
/// `writer_loop` never observes channel-close and the write half never emits
/// FIN — and a peer that waits for EOF waits forever. The one-shot
/// A short-lived guest used to mask this by killing the VM at teardown; the
/// workspace VM keeps serving, so this drop is the only signal that
/// returns the in-guest listener to idle after a host-side turn ends (live
/// audit 2026-08-23: pre-fix, the host held the attach TCP connection ESTAB
/// 25+ minutes after the drive exited and the preview became unattachable).
///
/// Shape: abort the READER immediately (dropping its outbound-sender clone and
/// the read half), then give the WRITER a bounded [`WRITER_DRAIN_GRACE`] to
/// flush queued frames and exit on its own once surviving stream writers /
/// handles drop — aborting it outright loses in-flight frames on clean
/// teardown (the retired session-store tunnel tests caught exactly that). The abort
/// backstop after the grace guarantees a bounded close even if a stream or
/// handle leaked. Outside a runtime (sync test teardown), abort directly —
/// there is nothing left to flush to.
impl Drop for Mux {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::Release);
        self.reader.abort();
        let Some(mut writer) = self.writer.take() else {
            return;
        };
        match tokio::runtime::Handle::try_current() {
            Ok(rt) => {
                let registry = self.registry.clone();
                rt.spawn(async move {
                    terminate_registry(&registry).await;
                    if tokio::time::timeout(WRITER_DRAIN_GRACE, &mut writer)
                        .await
                        .is_err()
                    {
                        writer.abort();
                    }
                });
            }
            Err(_) => writer.abort(),
        }
    }
}

impl Mux {
    /// Build a mux over a duplex byte stream. `initiator` selects the stream-id
    /// parity (host = `true` → even ids, runner = `false` → odd ids) so the two
    /// peers never collide on a locally allocated id.
    pub fn new<S>(stream: S, initiator: bool) -> Mux
    where
        S: AsyncRead + AsyncWrite + Send + 'static,
    {
        let (read_half, write_half) = tokio::io::split(stream);
        let (outbound_tx, outbound_rx) = mpsc::channel::<Frame>(256);
        // Sized to the live-stream cap so the demux reader can hand off any
        // accepted stream without ever awaiting a full queue (HoL-free).
        let (accept_tx, accept_rx) = mpsc::channel::<MuxStream>(MAX_LIVE_STREAMS);
        let registry: Registry = Arc::new(Mutex::new(HashMap::new()));
        let closed = Arc::new(AtomicBool::new(false));
        let next_id = Arc::new(AtomicU32::new(if initiator { 2 } else { 1 }));

        let writer = tokio::spawn(writer_loop(
            write_half,
            outbound_rx,
            registry.clone(),
            closed.clone(),
        ));
        let reader = tokio::spawn(reader_loop(
            read_half,
            registry.clone(),
            outbound_tx.clone(),
            accept_tx,
            initiator,
            closed.clone(),
        ));

        Mux {
            outbound: outbound_tx,
            registry,
            closed,
            accept_rx,
            next_id,
            reader,
            writer: Some(writer),
        }
    }

    /// Open a new outbound stream on `channel`. Allocates a local stream id and
    /// emits an `Open` frame; returns the [`MuxStream`] for I/O.
    ///
    /// Enforces [`MAX_LIVE_STREAMS`] *before* registering (MED-2): an untrusted
    /// guest opening unbounded loopback connections in the in-VM runner must not
    /// be able to register unbounded local streams. Over the cap, this fails
    /// cleanly without registering or emitting an `Open`, and the caller closes
    /// the rejected connection.
    pub async fn open(&self, channel: Channel) -> Result<MuxStream, G5Error> {
        if self.closed.load(Ordering::Acquire) {
            return Err(mux_closed_error());
        }
        let stream_id = self.next_id.fetch_add(2, Ordering::Relaxed);
        let stream = self
            .register_stream(stream_id, channel)
            .await
            .ok_or_else(|| G5Error::Internal {
                message: "mux live-stream cap reached".to_string(),
            })?;
        if self.closed.load(Ordering::Acquire) {
            remove_registered_stream(&self.registry, stream_id).await;
            return Err(mux_closed_error());
        }
        if self
            .outbound
            .send(Frame {
                stream_id,
                kind: FrameKind::Open,
                channel: channel.to_wire(),
                payload: Vec::new(),
            })
            .await
            .is_err()
        {
            self.closed.store(true, Ordering::Release);
            remove_registered_stream(&self.registry, stream_id).await;
            return Err(mux_closed_error());
        }
        Ok(stream)
    }

    /// Accept the next stream the peer opened, or `None` once the mux is torn
    /// down (underlying stream EOF/error).
    pub async fn accept(&mut self) -> Option<MuxStream> {
        self.accept_rx.recv().await
    }

    /// A cheaply-cloneable handle that can [`open`](MuxHandle::open) new streams.
    /// `accept` stays on the owning [`Mux`]; the handle lets the in-VM listener
    /// tasks open `Llm` streams concurrently while the runner's main
    /// loop accepts the host-opened `Stdio` stream.
    pub fn handle(&self) -> MuxHandle {
        MuxHandle {
            outbound: self.outbound.clone(),
            registry: self.registry.clone(),
            closed: self.closed.clone(),
            next_id: self.next_id.clone(),
        }
    }

    /// Register inbound routing + send-window for a stream id and build the
    /// owning [`MuxStream`]. Shared by [`Mux::open`] and the reader's peer-open
    /// path so both directions are symmetric. Returns `None` if the live-stream
    /// cap is reached (see [`register_stream_inner`]).
    async fn register_stream(&self, stream_id: u32, channel: Channel) -> Option<MuxStream> {
        register_stream_inner(&self.registry, &self.outbound, stream_id, channel).await
    }
}

/// A cheaply-cloneable opener for an existing [`Mux`]. Holds only channel
/// senders + shared registry/id state, so cloning is free.
#[derive(Clone)]
pub struct MuxHandle {
    outbound: mpsc::Sender<Frame>,
    registry: Registry,
    closed: Arc<AtomicBool>,
    next_id: Arc<AtomicU32>,
}

impl MuxHandle {
    /// Open a new outbound stream on `channel` (see [`Mux::open`]). Enforces
    /// [`MAX_LIVE_STREAMS`] before registering (MED-2); over the cap it fails
    /// cleanly without registering or emitting an `Open`.
    pub async fn open(&self, channel: Channel) -> Result<MuxStream, G5Error> {
        if self.closed.load(Ordering::Acquire) {
            return Err(mux_closed_error());
        }
        let stream_id = self.next_id.fetch_add(2, Ordering::Relaxed);
        let stream = register_stream_inner(&self.registry, &self.outbound, stream_id, channel)
            .await
            .ok_or_else(|| G5Error::Internal {
                message: "mux live-stream cap reached".to_string(),
            })?;
        if self.closed.load(Ordering::Acquire) {
            remove_registered_stream(&self.registry, stream_id).await;
            return Err(mux_closed_error());
        }
        if self
            .outbound
            .send(Frame {
                stream_id,
                kind: FrameKind::Open,
                channel: channel.to_wire(),
                payload: Vec::new(),
            })
            .await
            .is_err()
        {
            self.closed.store(true, Ordering::Release);
            remove_registered_stream(&self.registry, stream_id).await;
            return Err(mux_closed_error());
        }
        Ok(stream)
    }
}

fn mux_closed_error() -> G5Error {
    G5Error::Internal {
        message: "mux carrier is closed".to_string(),
    }
}

async fn remove_registered_stream(registry: &Registry, stream_id: u32) {
    if let Some(sink) = registry.lock().await.remove(&stream_id) {
        sink.send_window.close();
    }
}

async fn terminate_registry(registry: &Registry) {
    let mut reg = registry.lock().await;
    for (_id, sink) in reg.drain() {
        sink.recv_closed.store(true, Ordering::Release);
        sink.send_closed.store(true, Ordering::Release);
        sink.send_window.close();
    }
}

/// Shared by [`Mux::open`], [`MuxHandle::open`], and [`reader_loop`]'s
/// peer-`Open` handling. The [`MAX_LIVE_STREAMS`] cap **and** the no-overwrite
/// (duplicate-id) check are evaluated **atomically** with the registry insert
/// (single lock hold) so concurrent local opens — e.g. the in-VM runner spawning
/// a stream per untrusted loopback connection — can never race past the cap, and
/// a hostile peer re-`Open`ing a live id can never replace its sink. Returns
/// `None` (no registration, no side effects) when the cap is already reached or
/// the id is already live; the caller maps that to a clean failure (`open`
/// errors; the reader `Reset`s the peer's `Open`).
async fn register_stream_inner(
    registry: &Registry,
    outbound: &mpsc::Sender<Frame>,
    stream_id: u32,
    channel: Channel,
) -> Option<MuxStream> {
    let (inbound_tx, inbound_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    let send_window = Arc::new(Semaphore::new(STREAM_WINDOW));
    let inbound_budget = Arc::new(Semaphore::new(STREAM_WINDOW));
    let send_closed = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let recv_closed = Arc::new(AtomicBool::new(false));
    let reset_sent = Arc::new(AtomicBool::new(false));
    {
        let mut reg = registry.lock().await;
        if reg.len() >= MAX_LIVE_STREAMS {
            return None;
        }
        // Never overwrite a live sink. For local opens the id is freshly
        // allocated from a monotonic per-parity counter so this can never fire;
        // for a peer `Open` (reader_loop) it rejects a duplicate id atomically
        // with the insert, so a hostile re-`Open` cannot replace the existing
        // sink and misroute its data/credits. The caller maps `None` to a `Reset`.
        if reg.contains_key(&stream_id) {
            return None;
        }
        reg.insert(
            stream_id,
            StreamSink {
                inbound: Some(inbound_tx),
                inbound_budget: inbound_budget.clone(),
                send_window: send_window.clone(),
                send_closed: send_closed.clone(),
                recv_closed: recv_closed.clone(),
            },
        );
    }
    Some(MuxStream {
        channel,
        writer: MuxWriter {
            stream_id,
            outbound: outbound.clone(),
            send_window,
            closed: send_closed,
            registry: registry.clone(),
            reset_sent: reset_sent.clone(),
        },
        reader: MuxReader {
            stream_id,
            inbound: inbound_rx,
            inbound_budget,
            leftover: Vec::new(),
            outbound: outbound.clone(),
            registry: registry.clone(),
            recv_closed,
            reset_sent,
            unacked: 0,
        },
    })
}

/// The single writer task: serializes queued frames onto the write half.
async fn writer_loop<W>(
    mut write_half: W,
    mut outbound_rx: mpsc::Receiver<Frame>,
    registry: Registry,
    closed: Arc<AtomicBool>,
) where
    W: AsyncWrite + Unpin + Send + 'static,
{
    while let Some(frame) = outbound_rx.recv().await {
        let header = frame.header();
        if write_half.write_all(&header).await.is_err() {
            break;
        }
        if !frame.payload.is_empty() && write_half.write_all(&frame.payload).await.is_err() {
            break;
        }
        if write_half.flush().await.is_err() {
            break;
        }
    }
    closed.store(true, Ordering::Release);
    terminate_registry(&registry).await;
}

/// The single reader task: parses frames and dispatches to per-stream sinks.
async fn reader_loop<R>(
    mut read_half: R,
    registry: Registry,
    outbound: mpsc::Sender<Frame>,
    accept_tx: mpsc::Sender<MuxStream>,
    initiator: bool,
    closed: Arc<AtomicBool>,
) where
    R: AsyncRead + Unpin + Send + 'static,
{
    // A peer must only `Open` ids in *its* parity class — the opposite of the
    // class we allocate locally (`Mux::new`: initiator/host = even, runner =
    // odd). So the expected `stream_id % 2` for a peer `Open` equals
    // `u32::from(initiator)`: when we are the initiator (local even) the peer is
    // odd (==1); when we are not (local odd) the peer is even (==0). Any peer
    // `Open` outside this class is a protocol violation (see the `Open` arm).
    let peer_open_parity = u32::from(initiator);
    let mut header = [0u8; 10];
    loop {
        if read_half.read_exact(&mut header).await.is_err() {
            break; // EOF or error → mux teardown.
        }
        let stream_id = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
        let Some(kind) = FrameKind::from_wire(header[4]) else {
            break; // Protocol violation — fail closed.
        };
        let channel_byte = header[5];
        let length = u32::from_be_bytes([header[6], header[7], header[8], header[9]]) as usize;

        // Validate the advertised payload length BEFORE allocating. A hostile
        // peer could otherwise send a header claiming a multi-GiB payload and
        // force `vec![0u8; length]` to OOM the host/runner from the single demux
        // reader. Every legitimate frame's payload is at most MAX_FRAME_PAYLOAD
        // (Data is chunked to it; Credit is 4 bytes; Open/Close/Reset carry
        // none), so anything larger is a framing violation. We cannot trust the
        // stream's alignment after an oversized header (we don't know where the
        // next frame begins), so fail the whole connection closed rather than
        // attempt to resync — exactly as for an unknown frame kind above.
        if length > MAX_FRAME_PAYLOAD {
            break;
        }

        let mut payload = vec![0u8; length];
        if length > 0 && read_half.read_exact(&mut payload).await.is_err() {
            break;
        }

        match kind {
            FrameKind::Open => {
                let Some(channel) = Channel::from_wire(channel_byte) else {
                    continue; // Unknown channel — ignore the stream.
                };
                // Validate the peer-supplied id BEFORE touching the registry
                // (untrusted input). A well-behaved peer only opens ids in its
                // parity class and never id 0; `register_stream_inner` then
                // rejects a duplicate atomically under the registry lock. Each
                // rejection sends a `Reset` for the offending id and leaves any
                // pre-existing stream on a colliding id untouched — a hostile
                // peer must not be able to overwrite a live sink (its own peer
                // stream or, via wrong parity, one of our local streams) and
                // misroute its data/credits. Zero and wrong-parity are cheap to
                // reject up front (no lock); duplicate is checked inside
                // `register_stream_inner` so the check is atomic with the insert.
                if stream_id == 0 || stream_id % 2 != peer_open_parity {
                    let _ = outbound
                        .send(Frame::control(stream_id, FrameKind::Reset))
                        .await;
                    continue;
                }
                // Cap concurrent live streams + reject duplicates: a hostile peer
                // must not be able to open unbounded streams (host memory), stall
                // the shared reader, or re-`Open` a live id. `register_stream_inner`
                // enforces the cap and the no-overwrite invariant atomically and
                // returns `None` on either — reject with a `Reset` and do not
                // register. This branch never blocks the demux reader.
                let Some(stream) =
                    register_stream_inner(&registry, &outbound, stream_id, channel).await
                else {
                    let _ = outbound
                        .send(Frame::control(stream_id, FrameKind::Reset))
                        .await;
                    continue;
                };
                // Non-blocking hand-off: never `.await` on a full accept queue —
                // that is exactly the head-of-line block this hardening removes
                // (a slow `accept` consumer must not stall every other channel).
                // The queue is sized to the live cap, so within-cap streams
                // always fit; the `Full` arm is a defensive backstop.
                match accept_tx.try_send(stream) {
                    Ok(()) => {}
                    Err(mpsc::error::TrySendError::Full(_)) => {
                        // Accept backlog saturated: treat as over-capacity.
                        // Unregister + `Reset` rather than block the reader.
                        if let Some(sink) = registry.lock().await.remove(&stream_id) {
                            sink.send_window.close();
                        }
                        let _ = outbound
                            .send(Frame::control(stream_id, FrameKind::Reset))
                            .await;
                    }
                    Err(mpsc::error::TrySendError::Closed(_)) => break, // Mux dropped.
                }
            }
            FrameKind::Data => {
                // Route to the receive sink, but only while the receive direction
                // is open (`Some`). A `Data` on an unknown stream, or after the
                // peer's `Close` (recv half-closed), is dropped — and so a flood of
                // frames on an already-torn-down stream costs nothing and emits no
                // further `Reset`s.
                let routed = {
                    let reg = registry.lock().await;
                    reg.get(&stream_id).and_then(|s| {
                        s.inbound
                            .as_ref()
                            .map(|tx| (tx.clone(), s.inbound_budget.clone()))
                    })
                };
                let Some((tx, budget)) = routed else {
                    continue;
                };
                // Decide whether this frame is a protocol violation for the stream
                // (MED-1):
                //  * A zero-length `Data` is meaningless and the cheapest
                //    head-of-line-exhaustion vector — it consumes no flow-control
                //    credit yet still occupies a demux dispatch, so a flood could
                //    exhaust a frame-counted buffer long before the byte window.
                //  * Charge every other frame against the inbound byte budget,
                //    mirroring the send-side credit window. A credit-respecting
                //    peer can never have more than `STREAM_WINDOW` bytes
                //    outstanding, so the charge always succeeds for honest traffic
                //    and the non-blocking enqueue never parks the shared reader —
                //    a slow consumer on one stream cannot head-of-line-block any
                //    other. A failed charge means the peer over-sent past the
                //    credit it was granted. (`length <= MAX_FRAME_PAYLOAD <=
                //    STREAM_WINDOW`, so the `u32` cast and the window cap hold.)
                let violation = if length == 0 {
                    true
                } else {
                    match budget.try_acquire_many(length as u32) {
                        Ok(permit) => {
                            // Held by `inbound_budget` until the consumer drains the
                            // chunk and `MuxReader::release_inbound` returns it.
                            permit.forget();
                            // Non-blocking enqueue: `Err` only if the consumer is
                            // gone (the read half was dropped).
                            tx.send(payload).is_err()
                        }
                        Err(_) => true,
                    }
                };
                if violation {
                    // Zero-length / over-send / consumer-gone — tell the peer to
                    // stop, then drop the sink and close its send window (a removed
                    // sink can never receive `Credit`, so a sender blocked in
                    // `acquire_many` would otherwise leak its task). A `Reset`
                    // tears the stream down, so its credit no longer matters and
                    // the peer's view cannot desync.
                    let _ = outbound
                        .send(Frame::control(stream_id, FrameKind::Reset))
                        .await;
                    if let Some(sink) = registry.lock().await.remove(&stream_id) {
                        sink.send_window.close();
                    }
                }
            }
            FrameKind::Credit => {
                if payload.len() == 4 {
                    let grant = u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]])
                        as usize;
                    let reg = registry.lock().await;
                    if let Some(sink) = reg.get(&stream_id) {
                        // Cap the grant so total available permits never exceed
                        // one full window: a hostile peer must not be able to
                        // over-credit us into sending past `STREAM_WINDOW` bytes
                        // outstanding (defeating flow control) or overflow the
                        // semaphore. An honest peer never hits this — it only
                        // ever credits bytes it has actually drained, so
                        // available + in-flight == STREAM_WINDOW at most.
                        let headroom =
                            STREAM_WINDOW.saturating_sub(sink.send_window.available_permits());
                        let grant = grant.min(headroom);
                        if grant > 0 {
                            sink.send_window.add_permits(grant);
                        }
                    }
                }
            }
            FrameKind::Close => {
                // Peer half-closed its *send* direction: signal EOF to our
                // reader by dropping the inbound sender, but keep the entry (and
                // its `send_window`) so the reverse (local→peer) direction keeps
                // flowing and future `Credit` still lands. Drop the entry only
                // once our send direction is also closed (both directions done).
                let mut reg = registry.lock().await;
                if let Some(sink) = reg.get_mut(&stream_id) {
                    sink.inbound = None;
                    sink.recv_closed.store(true, Ordering::Release);
                    if sink.send_closed.load(Ordering::Relaxed) {
                        reg.remove(&stream_id);
                    }
                }
            }
            FrameKind::Reset => {
                // Abortive teardown of this stream: drop the sink so `recv`
                // sees EOF *and* close the send window so a sender blocked in
                // `acquire_many` (awaiting credit that will never arrive)
                // unblocks with an error instead of leaking its task and
                // keeping the writer alive.
                if let Some(sink) = registry.lock().await.remove(&stream_id) {
                    sink.recv_closed.store(true, Ordering::Release);
                    sink.send_window.close();
                }
            }
        }
    }

    // Teardown (underlying EOF/error): drop every sink so all live streams
    // observe EOF, and close every send window so any sender blocked awaiting
    // credit unblocks with an error rather than hanging the writer task forever.
    closed.store(true, Ordering::Release);
    terminate_registry(&registry).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use std::time::Duration;
    use tokio::time::{sleep, timeout};

    /// The HTTP stream header round-trips every port, keeps the wire's
    /// big-endian bytes, and names no port before both bytes arrive.
    #[test]
    fn http_port_header_round_trips() {
        for port in [0, 1, 80, 3000, 8080, u16::MAX] {
            assert_eq!(decode_http_port(&encode_http_port(port)), Some(port));
        }
        assert_eq!(encode_http_port(3000), [0x0b, 0xb8]);
        assert_eq!(
            decode_http_port(&[0x0b, 0xb8, b'G', b'E', b'T']),
            Some(3000)
        );
        assert_eq!(decode_http_port(&[0x0b]), None);
    }

    /// Build a connected host/runner mux pair over an in-process duplex pipe.
    fn mux_pair() -> (Mux, Mux) {
        let (a, b) = tokio::io::duplex(64 * 1024);
        (Mux::new(a, true), Mux::new(b, false))
    }

    /// D7: dropping a `Mux` must CLOSE the carrier for the peer. The pump
    /// tasks cannot end on their own once the owning `Mux` is gone
    /// (`reader_loop` holds an outbound-sender clone, so `writer_loop` never
    /// observes channel-close and the write half never FINs), and a peer that
    /// waits for EOF — the preview chat-attach guest listener, whose VM
    /// deliberately outlives the host drive — waits forever (live audit
    /// 2026-08-23: host fd ESTAB 25+ minutes after the drive exited).
    #[tokio::test]
    async fn dropping_the_mux_closes_the_carrier_for_the_peer() {
        let (host, mut guest) = mux_pair();
        drop(host);
        let observed = timeout(Duration::from_secs(2), guest.accept()).await;
        let closed = matches!(observed, Ok(None));
        assert!(
            closed,
            "peer must observe carrier teardown (accept -> None) once the \
             other side's Mux drops; got timeout={} open-stream={}",
            observed.is_err(),
            matches!(observed, Ok(Some(_)))
        );
    }

    #[tokio::test]
    async fn opener_rejects_new_streams_after_carrier_teardown() {
        let (local_io, peer_io) = tokio::io::duplex(64 * 1024);
        let mut mux = Mux::new(local_io, true);
        let opener = mux.handle();
        drop(peer_io);
        assert!(timeout(Duration::from_secs(2), mux.accept())
            .await
            .expect("carrier teardown must be observed")
            .is_none());

        let error = match opener.open(Channel::Rpc).await {
            Ok(_) => panic!("a torn-down carrier cannot accept a new stream"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("mux carrier is closed"));
    }

    #[tokio::test]
    async fn cancelling_an_open_blocked_on_outbound_capacity_releases_its_slot() {
        let registry: Registry = Arc::new(Mutex::new(HashMap::new()));
        let (outbound, mut outbound_rx) = mpsc::channel::<Frame>(1);
        outbound
            .send(Frame::control(999, FrameKind::Reset))
            .await
            .expect("fill outbound channel");
        let opener = MuxHandle {
            outbound: outbound.clone(),
            registry: registry.clone(),
            closed: Arc::new(AtomicBool::new(false)),
            next_id: Arc::new(AtomicU32::new(2)),
        };

        let pending_open = tokio::spawn(async move { opener.open(Channel::Rpc).await });
        timeout(Duration::from_secs(2), async {
            loop {
                if registry.lock().await.len() == 1 {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("open registered before blocking on outbound capacity");

        pending_open.abort();
        let _ = pending_open.await;
        timeout(Duration::from_secs(2), async {
            loop {
                if registry.lock().await.is_empty() {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("cancelled open must release its registered slot");

        let filler = outbound_rx.recv().await.expect("filler frame");
        assert_eq!(filler.stream_id, 999);
        let reset = timeout(Duration::from_secs(2), outbound_rx.recv())
            .await
            .expect("cancelled open must emit reset")
            .expect("reset frame");
        assert_eq!(reset.stream_id, 2);
        assert_eq!(reset.kind, FrameKind::Reset);
    }

    #[tokio::test]
    async fn bounded_mux_timeout_spends_no_credit_and_enqueues_no_frame() {
        let (outbound, mut outbound_rx) = mpsc::channel(1);
        let send_window = Arc::new(Semaphore::new(0));
        let writer = MuxWriter {
            stream_id: 41,
            outbound,
            send_window: Arc::clone(&send_window),
            closed: Arc::new(AtomicBool::new(false)),
            registry: Arc::new(Mutex::new(HashMap::new())),
            reset_sent: Arc::new(AtomicBool::new(false)),
        };
        let payload = b"{\"action\":\"wake_recovery_authorize\"}\n";

        assert!(!writer
            .send_single_bounded(payload, Duration::from_millis(5))
            .await
            .expect("bounded timeout"));
        assert!(outbound_rx.try_recv().is_err());
        assert_eq!(send_window.available_permits(), 0);

        send_window.add_permits(payload.len());
        assert!(writer
            .send_single_bounded(payload, Duration::from_secs(1))
            .await
            .expect("bounded send"));
        let frame = outbound_rx.recv().await.expect("one whole frame");
        assert_eq!(frame.stream_id, 41);
        assert_eq!(frame.kind, FrameKind::Data);
        assert_eq!(frame.payload, payload);
        assert_eq!(send_window.available_permits(), 0);
    }

    #[tokio::test]
    async fn bounded_mux_rejects_empty_payload_without_enqueuing_a_frame() {
        let (outbound, mut outbound_rx) = mpsc::channel(1);
        let send_window = Arc::new(Semaphore::new(16));
        let writer = MuxWriter {
            stream_id: 42,
            outbound,
            send_window: Arc::clone(&send_window),
            closed: Arc::new(AtomicBool::new(false)),
            registry: Arc::new(Mutex::new(HashMap::new())),
            reset_sent: Arc::new(AtomicBool::new(false)),
        };

        let error = writer
            .send_single_bounded(&[], Duration::from_secs(1))
            .await
            .expect_err("empty bounded frame must fail");
        assert!(error.to_string().contains("must not be empty"));
        assert!(outbound_rx.try_recv().is_err());
        assert_eq!(send_window.available_permits(), 16);
    }

    /// Serialize one raw wire frame (10-byte header + payload), for tests that
    /// inject frames a `MuxWriter` cannot produce (zero-length `Data`, opens past
    /// the local cap) directly onto the peer's write half.
    fn raw_frame(id: u32, kind: FrameKind, channel: u8, payload: &[u8]) -> Vec<u8> {
        let mut v = Vec::with_capacity(10 + payload.len());
        v.extend_from_slice(&id.to_be_bytes());
        v.push(kind.to_wire());
        v.push(channel);
        v.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        v.extend_from_slice(payload);
        v
    }

    /// Read one raw wire frame off `r`, or `None` at EOF. Mirrors the reader's
    /// own framing so tests can observe the frames the mux emits back to a peer
    /// (e.g. `Reset`s for rejected streams).
    async fn read_frame<R: AsyncRead + Unpin>(r: &mut R) -> Option<(u32, FrameKind, u8, Vec<u8>)> {
        let mut h = [0u8; 10];
        r.read_exact(&mut h).await.ok()?;
        let id = u32::from_be_bytes([h[0], h[1], h[2], h[3]]);
        let kind = FrameKind::from_wire(h[4])?;
        let channel = h[5];
        let len = u32::from_be_bytes([h[6], h[7], h[8], h[9]]) as usize;
        let mut payload = vec![0u8; len];
        if len > 0 {
            r.read_exact(&mut payload).await.ok()?;
        }
        Some((id, kind, channel, payload))
    }

    /// Cancellation-safety guard (stream_mux D5, issue #1598 fix-forward):
    /// drive a multi-grant transfer (several times the half-window credit
    /// threshold) through a select loop that repeatedly DROPS the in-flight
    /// `read_line` future — the sticky park loop's exact shape. Bytes must
    /// arrive complete and in order (the pre-fix order lost the chunk received
    /// in a cancelled poll), and the stream must not wedge (a leaked credit
    /// grant would exhaust the sender's window partway through).
    #[tokio::test]
    async fn read_line_survives_repeated_cancellation_without_byte_or_credit_loss() {
        let (host, mut runner) = mux_pair();
        let stdio = host.open(Channel::Stdio).await.expect("open");

        // 6 * half-window of payload => at least 3 credit grants must land or
        // the sender's window (256 KiB) wedges before the writer finishes.
        let lines: usize = 96;
        let line_len: usize = 8 * 1024;
        let writer = tokio::spawn(async move {
            for i in 0..lines {
                let mut line = vec![b'a' + (i % 26) as u8; line_len];
                line.push(b'\n');
                stdio.send(&line).await.expect("send");
            }
        });

        let mut accepted = timeout(Duration::from_secs(5), runner.accept())
            .await
            .expect("accept timely")
            .expect("a stream");
        let mut received = Vec::new();
        while received.len() < lines {
            // Race the read against an immediate-ish tick so the read future
            // is dropped mid-flight over and over (cancellation pressure).
            tokio::select! {
                line = accepted.read_line() => {
                    match line {
                        Some(line) => received.push(line),
                        None => break,
                    }
                }
                _ = sleep(Duration::from_micros(50)) => {}
            }
        }
        writer.await.expect("writer");
        assert_eq!(received.len(), lines, "no line lost under cancellation");
        for (i, line) in received.iter().enumerate() {
            assert_eq!(line.len(), line_len, "line {i} truncated");
            assert!(
                line.bytes().all(|b| b == b'a' + (i % 26) as u8),
                "line {i} corrupted"
            );
        }
    }

    /// Entry-flush regression (stream_mux D5, CodeRabbit PR #1698): pending
    /// credit left by a cancelled flush must be sent at the NEXT `read_line`
    /// entry, even when `leftover` already holds a complete line that the
    /// early-return path serves without touching the inbound channel.
    #[tokio::test]
    async fn read_line_flushes_pending_credit_before_returning_buffered_line() {
        let (host, mut runner) = mux_pair();
        let stdio = host.open(Channel::Stdio).await.expect("open");
        stdio.send(b"seed\n").await.expect("send");
        let accepted = timeout(Duration::from_secs(2), runner.accept())
            .await
            .expect("accept timely")
            .expect("a stream");
        let (_w, mut r) = accepted.into_split();
        assert_eq!(r.read_line().await.as_deref(), Some("seed"));
        // Simulate the cancelled-flush state: over-threshold pending credit
        // with a complete buffered line (test lives in-module, so private
        // fields are reachable).
        r.unacked = STREAM_WINDOW / 2;
        r.leftover = b"buffered\n".to_vec();
        let line = r.read_line().await;
        assert_eq!(line.as_deref(), Some("buffered"));
        assert!(
            r.unacked < STREAM_WINDOW / 2,
            "entry flush must drain the pending grant before serving the buffered line (got {})",
            r.unacked
        );
    }

    /// Same guard for `recv` (stream_mux D5): the leftover-routed shape must
    /// deliver every byte exactly once under repeated cancellation, across
    /// enough volume to require credit replenishment.
    #[tokio::test]
    async fn recv_survives_repeated_cancellation_without_byte_or_credit_loss() {
        let (host, mut runner) = mux_pair();
        let stdio = host.open(Channel::Stdio).await.expect("open");

        let chunks: usize = 96;
        let chunk_len: usize = 8 * 1024;
        let writer = tokio::spawn(async move {
            let mut stdio = stdio;
            for i in 0..chunks {
                let chunk = vec![b'a' + (i % 26) as u8; chunk_len];
                stdio.send(&chunk).await.expect("send");
            }
            stdio.close().await;
        });

        let accepted = timeout(Duration::from_secs(5), runner.accept())
            .await
            .expect("accept timely")
            .expect("a stream");
        let (_w, mut r) = accepted.into_split();
        let mut received: Vec<u8> = Vec::new();
        loop {
            tokio::select! {
                chunk = r.recv() => {
                    match chunk {
                        Some(chunk) => received.extend_from_slice(&chunk),
                        None => break,
                    }
                }
                _ = sleep(Duration::from_micros(50)) => {}
            }
            if received.len() >= chunks * chunk_len {
                break;
            }
        }
        writer.await.expect("writer");
        assert_eq!(
            received.len(),
            chunks * chunk_len,
            "no byte lost under cancellation"
        );
        for (i, window) in received.chunks(chunk_len).enumerate() {
            assert!(
                window.iter().all(|&b| b == b'a' + (i % 26) as u8),
                "chunk {i} corrupted"
            );
        }
    }

    #[tokio::test]
    async fn frame_round_trips_host_to_runner() {
        let (host, mut runner) = mux_pair();
        let stdio = host.open(Channel::Stdio).await.expect("open");
        stdio.send(b"hello frame\n").await.expect("send");

        let mut accepted = timeout(Duration::from_secs(2), runner.accept())
            .await
            .expect("accept timely")
            .expect("a stream");
        assert_eq!(accepted.channel(), Channel::Stdio);
        let line = accepted.read_line().await.expect("line");
        assert_eq!(line, "hello frame");
    }

    #[tokio::test]
    async fn three_channels_demux_independently() {
        let (host, mut runner) = mux_pair();
        let s_stdio = host.open(Channel::Stdio).await.unwrap();
        let s_commit = host.open(Channel::Commit).await.unwrap();
        let s_llm = host.open(Channel::Llm).await.unwrap();
        s_stdio.send(b"STDIO").await.unwrap();
        s_commit.send(b"COMMIT").await.unwrap();
        s_llm.send(b"LLM").await.unwrap();

        // Collect the three accepted streams keyed by their channel.
        let mut by_channel: HashMap<u8, MuxStream> = HashMap::new();
        for _ in 0..3 {
            let s = timeout(Duration::from_secs(2), runner.accept())
                .await
                .unwrap()
                .unwrap();
            by_channel.insert(s.channel().to_wire(), s);
        }
        let got = |c: Channel, by: &mut HashMap<u8, MuxStream>| {
            let key = c.to_wire();
            by.remove(&key).expect("channel present")
        };
        let mut stdio = got(Channel::Stdio, &mut by_channel);
        let mut commit = got(Channel::Commit, &mut by_channel);
        let mut llm = got(Channel::Llm, &mut by_channel);
        assert_eq!(stdio.recv().await.unwrap(), b"STDIO");
        assert_eq!(commit.recv().await.unwrap(), b"COMMIT");
        assert_eq!(llm.recv().await.unwrap(), b"LLM");
    }

    /// SSE proof: chunks must arrive incrementally, *before* the sender closes —
    /// the mux must not buffer the whole response until EOF.
    #[tokio::test]
    async fn llm_stream_delivers_chunks_incrementally() {
        let (host, mut runner) = mux_pair();
        let sender = host.open(Channel::Llm).await.unwrap();
        let mut recvd: Option<MuxStream> = None;

        for i in 0..5u8 {
            sender
                .send(format!("event: {i}\n").as_bytes())
                .await
                .unwrap();
            if recvd.is_none() {
                recvd = Some(runner.accept().await.unwrap());
            }
            let stream = recvd.as_mut().unwrap();
            // Each chunk is observable without the sender having closed.
            let chunk = timeout(Duration::from_secs(2), stream.recv())
                .await
                .expect("chunk arrives before close")
                .expect("some bytes");
            assert_eq!(chunk, format!("event: {i}\n").into_bytes());
        }
    }

    /// Backpressure proof: a sender cannot push more than one window ahead of a
    /// non-draining receiver. The over-window `send` must not complete until the
    /// receiver drains and credit flows back.
    #[tokio::test]
    async fn send_blocks_until_receiver_drains_window() {
        let (host, mut runner) = mux_pair();
        let sender = host.open(Channel::Commit).await.unwrap();
        let mut receiver = runner.accept().await.unwrap();

        // One full window fits without draining; a second window's worth must
        // block until the receiver consumes and credit returns.
        let payload = vec![7u8; STREAM_WINDOW + MAX_FRAME_PAYLOAD];
        let send_task = tokio::spawn(async move {
            sender.send(&payload).await.unwrap();
            sender
        });

        // Give the sender a chance to push the first window and stall.
        sleep(Duration::from_millis(50)).await;
        assert!(!send_task.is_finished(), "send must stall past one window");

        // Drain everything; credit flows back and `send` completes.
        let mut total = 0usize;
        while total < STREAM_WINDOW + MAX_FRAME_PAYLOAD {
            let chunk = timeout(Duration::from_secs(2), receiver.recv())
                .await
                .unwrap()
                .unwrap();
            total += chunk.len();
        }
        timeout(Duration::from_secs(2), send_task)
            .await
            .expect("send completes after drain")
            .expect("join");
        assert_eq!(total, STREAM_WINDOW + MAX_FRAME_PAYLOAD);
    }

    #[tokio::test]
    async fn close_surfaces_eof_to_peer() {
        let (host, mut runner) = mux_pair();
        let mut sender = host.open(Channel::Stdio).await.unwrap();
        sender.send(b"last\n").await.unwrap();
        sender.close().await;

        let mut r = runner.accept().await.unwrap();
        assert_eq!(r.read_line().await.unwrap(), "last");
        assert!(r.read_line().await.is_none(), "EOF after close");
    }

    /// Leak regression: a sender blocked in `acquire_many` (window exhausted,
    /// no credit returning) must unblock with an **error** when the peer dies
    /// and the underlying stream EOFs — otherwise the pump task leaks and keeps
    /// the writer alive forever. Proves the teardown closes the send window.
    #[tokio::test]
    async fn blocked_sender_errors_on_peer_eof() {
        let (a, b) = tokio::io::duplex(64 * 1024);
        let host = Mux::new(a, true);
        let sender = host.open(Channel::Commit).await.unwrap();

        // One window + one frame: the over-window chunk blocks on `acquire_many`
        // because nothing on the peer side ever drains / credits.
        let payload = vec![9u8; STREAM_WINDOW + MAX_FRAME_PAYLOAD];
        let send_task = tokio::spawn(async move { sender.send(&payload).await });

        sleep(Duration::from_millis(50)).await;
        assert!(
            !send_task.is_finished(),
            "send must still be blocked past one window"
        );

        // Peer dies: dropping `b` EOFs the host reader, which tears the mux down.
        drop(b);

        let result = timeout(Duration::from_secs(2), send_task)
            .await
            .expect("blocked send must unblock on teardown, not hang")
            .expect("join");
        assert!(
            result.is_err(),
            "blocked send must error out when the window closes on EOF"
        );
    }

    /// A sender blocked on a single stream must unblock with an error when that
    /// stream is `Reset` (here triggered by the receiver dropping its read half,
    /// so the peer reader sends `Reset` back), even while the mux stays up.
    #[tokio::test]
    async fn blocked_sender_errors_on_stream_reset() {
        let (host, mut runner) = mux_pair();
        let sender = host.open(Channel::Commit).await.unwrap();

        // Accept then immediately drop the receiver: the runner's reader will
        // fail to deliver inbound Data and send a `Reset` back to the host.
        let recv = runner.accept().await.unwrap();
        drop(recv);

        let payload = vec![3u8; STREAM_WINDOW + MAX_FRAME_PAYLOAD];
        let send_task = tokio::spawn(async move { sender.send(&payload).await });

        let result = timeout(Duration::from_secs(2), send_task)
            .await
            .expect("blocked send must unblock on Reset, not hang")
            .expect("join");
        assert!(
            result.is_err(),
            "blocked send must error out when the stream is Reset"
        );
    }

    /// B1 (HIGH): a frame header advertising a payload larger than
    /// [`MAX_FRAME_PAYLOAD`] must be rejected *before* any allocation — a hostile
    /// peer must not be able to make the demux reader `vec![0u8; 4 GiB]` and OOM
    /// the host. The reader fails the connection closed; `accept` then observes
    /// the torn-down mux promptly (a pre-fix reader would hang allocating).
    #[tokio::test]
    async fn oversized_frame_length_is_rejected_without_allocating() {
        let (a, mut b) = tokio::io::duplex(64 * 1024);
        let mut host = Mux::new(a, true);

        // Forge a header claiming a ~4 GiB payload (length = u32::MAX), far past
        // MAX_FRAME_PAYLOAD. stream_id uses the runner (odd) parity.
        let mut header = [0u8; 10];
        header[0..4].copy_from_slice(&1u32.to_be_bytes());
        header[4] = FrameKind::Data.to_wire();
        header[5] = 0;
        header[6..10].copy_from_slice(&u32::MAX.to_be_bytes());
        b.write_all(&header).await.unwrap();
        b.flush().await.unwrap();

        let accepted = timeout(Duration::from_secs(2), host.accept())
            .await
            .expect("reader must tear down promptly, not hang allocating a huge frame");
        assert!(
            accepted.is_none(),
            "mux must be torn down (fail closed) after an oversized frame length"
        );
    }

    /// B2 (HIGH): half-close. After one direction closes (`Close`), the reverse
    /// direction must keep flowing — including across multiple credit cycles
    /// (> one window), which requires the closed-side peer to keep honoring
    /// `Credit` for the still-open reverse stream. A pre-fix `Close` that removed
    /// the whole registry entry would drop reverse `Credit` and hang the response.
    #[tokio::test]
    async fn half_close_keeps_reverse_direction_flowing() {
        let (host, mut runner) = mux_pair();
        let mut s_host = host.open(Channel::Commit).await.unwrap();
        let s_runner = runner.accept().await.unwrap();

        // Host half-closes its send direction (e.g. finished sending the request
        // body) while the runner is about to stream a large response back.
        s_host.close().await;

        // Reverse (runner→host) must keep flowing past several windows.
        let total = STREAM_WINDOW + 3 * MAX_FRAME_PAYLOAD;
        let payload = vec![42u8; total];
        let send_task = tokio::spawn(async move {
            s_runner.send(&payload).await.unwrap();
            s_runner
        });

        let mut got = 0usize;
        while got < total {
            let chunk = timeout(Duration::from_secs(2), s_host.recv())
                .await
                .expect("reverse direction must keep flowing after half-close")
                .expect("bytes");
            got += chunk.len();
        }
        assert_eq!(got, total);
        timeout(Duration::from_secs(2), send_task)
            .await
            .expect("reverse send completes — credit kept flowing post half-close")
            .expect("join");
    }

    /// B4a (MED): peer `Open`s beyond [`MAX_LIVE_STREAMS`] must be `Reset` and
    /// must NOT head-of-line-block the shared demux reader — an already-accepted
    /// stream keeps flowing while a flood of un-accepted opens arrives. Driven by
    /// raw frame injection so the flood is independent of the *local* open cap
    /// (MED-2) which would otherwise stop us opening past the cap ourselves.
    #[tokio::test]
    async fn over_cap_opens_are_reset_without_blocking_live_streams() {
        let (a, b) = tokio::io::duplex(1024 * 1024);
        let mut host = Mux::new(a, true); // host accepts; we inject as the peer
        let (mut b_rd, mut b_wr) = tokio::io::split(b);

        // Peer (odd parity) opens one legit Stdio stream we will keep live.
        b_wr.write_all(&raw_frame(
            1,
            FrameKind::Open,
            Channel::Stdio.to_wire(),
            &[],
        ))
        .await
        .unwrap();
        // Flood the host with peer opens past the cap, distinct odd ids 3,5,7,…
        let flood = MAX_LIVE_STREAMS + 8;
        for k in 0..flood {
            let id = 3 + (k as u32) * 2;
            b_wr.write_all(&raw_frame(
                id,
                FrameKind::Open,
                Channel::Commit.to_wire(),
                &[],
            ))
            .await
            .unwrap();
        }
        // Legit data on the live stream AFTER the flood: it must still be
        // delivered, proving the demux reader was never head-of-line-blocked by
        // the flood (a pre-fix reader that blocked on a per-stream queue, or on a
        // full accept queue, would never reach this frame).
        b_wr.write_all(&raw_frame(1, FrameKind::Data, 0, b"still alive\n"))
            .await
            .unwrap();
        b_wr.flush().await.unwrap();

        // The live stream is accepted and keeps flowing despite the flood.
        let mut live = timeout(Duration::from_secs(2), host.accept())
            .await
            .expect("accept timely")
            .expect("live stream");
        assert_eq!(live.channel(), Channel::Stdio);
        let line = timeout(Duration::from_secs(2), live.read_line())
            .await
            .expect("live stream must not be HoL-blocked by the open flood")
            .expect("line");
        assert_eq!(line, "still alive");

        // The opens past the cap must each have been `Reset` without registering.
        // The host registers up to the cap (1 Stdio + (cap-1) of the flood) and
        // `Reset`s the remainder; count the `Reset` frames it emits back.
        let total_opens = 1 + flood;
        let expected_resets = total_opens - MAX_LIVE_STREAMS;
        let mut resets = 0usize;
        while resets < expected_resets {
            let (_id, kind, _ch, _p) = timeout(Duration::from_secs(2), read_frame(&mut b_rd))
                .await
                .expect("reader emits reset frames promptly")
                .expect("frame, not EOF");
            if kind == FrameKind::Reset {
                resets += 1;
            }
        }
        assert_eq!(
            resets, expected_resets,
            "every peer Open past MAX_LIVE_STREAMS must be Reset without registering"
        );
    }

    /// MED (P5 review): a hostile peer re-`Open`ing an already-live PEER stream id
    /// must be `Reset` without overwriting the existing sink — the original stream
    /// keeps routing its data. Driven by raw frame injection because a well-behaved
    /// `Mux` never reuses a live id.
    #[tokio::test]
    async fn duplicate_peer_open_is_reset_and_original_stream_survives() {
        let (a, b) = tokio::io::duplex(1024 * 1024);
        let mut host = Mux::new(a, true); // host accepts; we inject as the runner peer (odd ids)
        let (mut b_rd, mut b_wr) = tokio::io::split(b);

        // Peer opens an odd-parity Stdio stream and sends a line on it.
        b_wr.write_all(&raw_frame(
            1,
            FrameKind::Open,
            Channel::Stdio.to_wire(),
            &[],
        ))
        .await
        .unwrap();
        b_wr.write_all(&raw_frame(1, FrameKind::Data, 0, b"first\n"))
            .await
            .unwrap();
        // A hostile duplicate `Open` on the SAME live id must not overwrite the
        // sink (which would misroute its data/credits).
        b_wr.write_all(&raw_frame(
            1,
            FrameKind::Open,
            Channel::Commit.to_wire(),
            &[],
        ))
        .await
        .unwrap();
        // More data on the ORIGINAL stream after the duplicate: it must still
        // arrive, proving the sink was left intact.
        b_wr.write_all(&raw_frame(1, FrameKind::Data, 0, b"second\n"))
            .await
            .unwrap();
        b_wr.flush().await.unwrap();

        let mut live = timeout(Duration::from_secs(2), host.accept())
            .await
            .expect("accept timely")
            .expect("live stream");
        assert_eq!(live.channel(), Channel::Stdio);
        let l1 = timeout(Duration::from_secs(2), live.read_line())
            .await
            .expect("first line timely")
            .expect("line");
        assert_eq!(l1, "first");
        let l2 = timeout(Duration::from_secs(2), live.read_line())
            .await
            .expect("original stream must survive the duplicate Open")
            .expect("line");
        assert_eq!(l2, "second");

        // The duplicate (and only the duplicate) must be `Reset` back on its id.
        let mut saw_reset = false;
        while !saw_reset {
            let (id, kind, _ch, _p) = timeout(Duration::from_secs(2), read_frame(&mut b_rd))
                .await
                .expect("reset emitted promptly")
                .expect("frame, not EOF");
            if kind == FrameKind::Reset {
                assert_eq!(id, 1, "Reset must target the duplicate id");
                saw_reset = true;
            }
        }
    }

    /// MED (P5 review): a peer `Open` carrying the *local* (wrong) parity class —
    /// here colliding with one of the host's even, locally-allocated ids — must be
    /// `Reset` without disturbing the live local stream. Proves parity enforcement
    /// stops a peer from hijacking an id we own.
    #[tokio::test]
    async fn wrong_parity_peer_open_is_reset_and_local_stream_survives() {
        let (a, b) = tokio::io::duplex(1024 * 1024);
        let host = Mux::new(a, true); // initiator → local even ids
        let (mut b_rd, mut b_wr) = tokio::io::split(b);

        // Host opens a LOCAL stream: initiator parity → even id 2.
        let mut s = host.open(Channel::Commit).await.unwrap();

        // A hostile peer forges an `Open` on the host's even (local-parity) id —
        // wrong parity for a peer `Open`. It must be `Reset`, sink left intact.
        b_wr.write_all(&raw_frame(
            2,
            FrameKind::Open,
            Channel::Stdio.to_wire(),
            &[],
        ))
        .await
        .unwrap();
        // Peer sends Data on id 2: it must still route to the host's LOCAL stream,
        // proving the wrong-parity Open did not overwrite the sink.
        b_wr.write_all(&raw_frame(2, FrameKind::Data, 0, b"kept\n"))
            .await
            .unwrap();
        b_wr.flush().await.unwrap();

        let line = timeout(Duration::from_secs(2), s.read_line())
            .await
            .expect("local stream must survive a wrong-parity peer Open")
            .expect("line");
        assert_eq!(line, "kept");

        let mut saw_reset = false;
        while !saw_reset {
            let (id, kind, _ch, _p) = timeout(Duration::from_secs(2), read_frame(&mut b_rd))
                .await
                .expect("reset emitted promptly")
                .expect("frame, not EOF");
            if kind == FrameKind::Reset {
                assert_eq!(id, 2, "Reset must target the wrong-parity id");
                saw_reset = true;
            }
        }
    }

    /// MED (P5 review): stream id 0 is invalid; a peer `Open` on it must be
    /// `Reset` and never accepted, and the demux reader must keep running (a later
    /// legitimate stream is still accepted).
    #[tokio::test]
    async fn peer_open_with_zero_stream_id_is_reset() {
        let (a, b) = tokio::io::duplex(1024 * 1024);
        let mut host = Mux::new(a, true);
        let (mut b_rd, mut b_wr) = tokio::io::split(b);

        // id 0 is reserved/invalid: a peer `Open` on it must be `Reset`.
        b_wr.write_all(&raw_frame(
            0,
            FrameKind::Open,
            Channel::Stdio.to_wire(),
            &[],
        ))
        .await
        .unwrap();
        // A legit odd-parity open afterward proves the reader survived the zero id.
        b_wr.write_all(&raw_frame(
            1,
            FrameKind::Open,
            Channel::Stdio.to_wire(),
            &[],
        ))
        .await
        .unwrap();
        b_wr.flush().await.unwrap();

        let mut saw_zero_reset = false;
        while !saw_zero_reset {
            let (id, kind, _ch, _p) = timeout(Duration::from_secs(2), read_frame(&mut b_rd))
                .await
                .expect("reset emitted promptly")
                .expect("frame, not EOF");
            if kind == FrameKind::Reset && id == 0 {
                saw_zero_reset = true;
            }
        }
        // The later legit stream is still accepted: the zero-id Open did not tear
        // the mux down.
        let accepted = timeout(Duration::from_secs(2), host.accept())
            .await
            .expect("accept timely")
            .expect("legit stream after zero-id Open");
        assert_eq!(accepted.channel(), Channel::Stdio);
    }

    /// B4b (MED): a peer cannot over-credit a stream beyond [`STREAM_WINDOW`].
    /// A forged `Credit` of `u32::MAX` is clamped to zero headroom (the stream
    /// already has a full window), so a two-window send still stalls after the
    /// first window — the window never balloons.
    #[tokio::test]
    async fn over_credit_grant_is_capped_to_stream_window() {
        let (a, b) = tokio::io::duplex(64 * 1024);
        let (mut b_rd, mut b_wr) = tokio::io::split(b);
        let host = Mux::new(a, true);
        let s = host.open(Channel::Commit).await.unwrap();

        // Drain everything the host writes (its Open + Data frames) so the
        // writer/pipe never backpressures — this isolates the flow-control
        // *window* as the only thing that can stall a send. Detached and aborted
        // at the end (the host outlives this fn, so `b_rd` never EOFs on its own).
        let drain = tokio::spawn(async move {
            let mut buf = vec![0u8; 64 * 1024];
            loop {
                match b_rd.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
            }
        });

        // Forge an enormous Credit for the host's stream (even id = 2).
        let mut cframe = Vec::new();
        cframe.extend_from_slice(&2u32.to_be_bytes());
        cframe.push(FrameKind::Credit.to_wire());
        cframe.push(0);
        cframe.extend_from_slice(&4u32.to_be_bytes());
        cframe.extend_from_slice(&u32::MAX.to_be_bytes());
        b_wr.write_all(&cframe).await.unwrap();
        b_wr.flush().await.unwrap();
        sleep(Duration::from_millis(50)).await; // let the host process the credit

        // The window must be capped at STREAM_WINDOW: a two-window send must
        // stall after the first window (the over-credit was clamped to 0). If the
        // grant had not been capped, both windows would acquire and the send
        // would finish.
        let payload = vec![5u8; 2 * STREAM_WINDOW];
        let send_task = tokio::spawn(async move { s.send(&payload).await });
        sleep(Duration::from_millis(100)).await;
        assert!(
            !send_task.is_finished(),
            "over-credit must be capped to one window; a two-window send must stall"
        );

        send_task.abort();
        drop(b_wr);
        drain.abort();
    }

    /// MED-1: a flood of zero-length `Data` frames on one stream must NOT
    /// head-of-line-block the shared demux reader — a different live stream keeps
    /// delivering — and the flooded stream must be `Reset` (zero-length `Data` is
    /// rejected as a protocol violation). A pre-fix reader awaiting a full
    /// item-counted per-stream queue would park on the flood and never deliver
    /// the second stream's frame.
    #[tokio::test]
    async fn zero_length_data_flood_resets_stream_without_blocking_others() {
        let (a, b) = tokio::io::duplex(1024 * 1024);
        let mut host = Mux::new(a, true); // host demuxes; we inject as the peer
        let (mut b_rd, mut b_wr) = tokio::io::split(b);

        // Peer opens stream A (id 1, Commit — the flood target) and stream B
        // (id 3, Stdio — the bystander that must keep flowing).
        b_wr.write_all(&raw_frame(
            1,
            FrameKind::Open,
            Channel::Commit.to_wire(),
            &[],
        ))
        .await
        .unwrap();
        b_wr.write_all(&raw_frame(
            3,
            FrameKind::Open,
            Channel::Stdio.to_wire(),
            &[],
        ))
        .await
        .unwrap();
        // Flood A with zero-length `Data` frames — the cheapest item-exhaustion
        // vector. Far more than any plausible item-counted buffer depth.
        for _ in 0..512 {
            b_wr.write_all(&raw_frame(1, FrameKind::Data, 0, &[]))
                .await
                .unwrap();
        }
        // Legit data on B *after* the flood: must still be delivered promptly.
        b_wr.write_all(&raw_frame(3, FrameKind::Data, 0, b"B is alive\n"))
            .await
            .unwrap();
        b_wr.flush().await.unwrap();

        // Accept both streams (order: A then B).
        let mut s_a = None;
        let mut s_b = None;
        for _ in 0..2 {
            let s = timeout(Duration::from_secs(2), host.accept())
                .await
                .expect("accept timely")
                .expect("stream");
            match s.channel() {
                Channel::Commit => s_a = Some(s),
                Channel::Stdio => s_b = Some(s),
                other => panic!("unexpected channel {other:?}"),
            }
        }
        let mut s_a = s_a.expect("stream A accepted");
        let mut s_b = s_b.expect("stream B accepted");

        // Stream B delivers despite the flood on A — the reader was never parked.
        let line = timeout(Duration::from_secs(2), s_b.read_line())
            .await
            .expect("bystander stream must not be HoL-blocked by the zero-length flood")
            .expect("line");
        assert_eq!(line, "B is alive");

        // Stream A was rejected: the zero-length `Data` `Reset` it, so its inbound
        // sender is dropped and `recv` observes EOF.
        let a_eof = timeout(Duration::from_secs(2), s_a.recv())
            .await
            .expect("stream A recv resolves promptly");
        assert!(
            a_eof.is_none(),
            "a zero-length Data frame must Reset its stream (recv sees EOF)"
        );

        // And the host emitted a `Reset` for stream A (id 1) back to the peer.
        let mut saw_reset_for_a = false;
        for _ in 0..4 {
            let Some((id, kind, _ch, _p)) = timeout(Duration::from_secs(2), read_frame(&mut b_rd))
                .await
                .expect("reader emits frames promptly")
            else {
                break;
            };
            if kind == FrameKind::Reset && id == 1 {
                saw_reset_for_a = true;
                break;
            }
        }
        assert!(
            saw_reset_for_a,
            "host must Reset the stream that sent a zero-length Data frame"
        );
    }

    /// MED-2: local [`Mux::open`] past [`MAX_LIVE_STREAMS`] must fail cleanly and
    /// must not register beyond the cap (the in-VM runner spawns a stream per
    /// untrusted loopback connection; unbounded local opens would exhaust the
    /// host). Opens up to the cap succeed; the next opens error without growing
    /// the registry.
    #[tokio::test]
    async fn local_open_past_cap_fails_cleanly_without_registering() {
        let (a, b) = tokio::io::duplex(1024 * 1024);
        let host = Mux::new(a, true);

        // Pure drain on the peer side so the writer never backpressures and no
        // frames flow back to disturb the host's registry. Aborted at the end
        // (the host outlives this fn, so `b` never EOFs on its own).
        let drain = tokio::spawn(async move {
            let mut b = b;
            let mut buf = vec![0u8; 64 * 1024];
            loop {
                match b.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
            }
        });

        // Opens up to the cap all succeed; hold them so they stay registered.
        let mut streams = Vec::with_capacity(MAX_LIVE_STREAMS);
        for _ in 0..MAX_LIVE_STREAMS {
            streams.push(
                host.open(Channel::Commit)
                    .await
                    .expect("opens up to the cap succeed"),
            );
        }

        // The next open must fail cleanly — not register, not block, not panic.
        assert!(
            host.open(Channel::Commit).await.is_err(),
            "local open past MAX_LIVE_STREAMS must fail cleanly"
        );
        // Still capped: a second over-cap open also fails (the registry never grew
        // past the cap on the rejected open).
        assert!(
            host.open(Channel::Stdio).await.is_err(),
            "registry must not have grown past the cap on a rejected open"
        );
        assert_eq!(streams.len(), MAX_LIVE_STREAMS);

        drain.abort();
    }

    /// P5 review HIGH: a guest that floods `MAX_FRAME_PAYLOAD` chunks with NO
    /// newline must not be able to grow the reader's `leftover` buffer without
    /// limit (host OOM). `read_line` caps the partial line at [`MAX_LINE_BYTES`];
    /// once exceeded with no `\n` the stream is `Reset` (peer told to stop, sink
    /// dropped, send window closed) and `read_line` returns `None` (EOF).
    ///
    /// Driven through the real [`register_stream_inner`] registration path with
    /// the inbound sink fed directly, so the cap is exercised in isolation from
    /// the demux byte budget (no producer/consumer scheduling race over which
    /// guard fires first): every chunk read_line pulls returns its receive-window
    /// credit, so only the `leftover` cap can bound a newline-less stream.
    #[tokio::test]
    async fn read_line_caps_newline_less_flood_and_resets_stream() {
        let registry: Registry = Arc::new(Mutex::new(HashMap::new()));
        let (outbound_tx, mut outbound_rx) = mpsc::channel::<Frame>(256);
        let mut stream = register_stream_inner(&registry, &outbound_tx, 7, Channel::Stdio)
            .await
            .expect("register");

        // The inbound sender the demux would push decoded Data payloads onto.
        let inbound_tx = registry
            .lock()
            .await
            .get(&7)
            .and_then(|s| s.inbound.clone())
            .expect("sink registered");

        // Feed strictly more than one full window of newline-less, full-size
        // chunks so `leftover` must cross MAX_LINE_BYTES.
        let chunk = vec![b'x'; MAX_FRAME_PAYLOAD];
        let frames_to_exceed = MAX_LINE_BYTES / MAX_FRAME_PAYLOAD + 2;
        for _ in 0..frames_to_exceed {
            inbound_tx.send(chunk.clone()).expect("feed inbound");
        }

        // `read_line` must terminate with EOF (None) rather than buffer forever.
        let line = timeout(Duration::from_secs(2), stream.read_line())
            .await
            .expect("read_line must return promptly once the cap is hit, not buffer unbounded");
        assert!(
            line.is_none(),
            "a newline-less flood past MAX_LINE_BYTES must Reset the stream (read_line → None)"
        );

        // The over-cap buffer was freed and can never have exceeded the cap by
        // more than one frame's worth.
        assert!(
            stream.reader.leftover.len() <= MAX_LINE_BYTES + MAX_FRAME_PAYLOAD,
            "leftover must stay bounded by the cap + one frame, was {}",
            stream.reader.leftover.len()
        );

        // The cap path `Reset` the stream: a Reset frame was emitted for id 7 and
        // its sink was dropped from the registry (so it no longer counts live).
        let mut saw_reset = false;
        while let Ok(Some(frame)) = timeout(Duration::from_millis(200), outbound_rx.recv()).await {
            if frame.kind == FrameKind::Reset && frame.stream_id == 7 {
                saw_reset = true;
            }
        }
        assert!(
            saw_reset,
            "read_line cap must emit a Reset for the flooded stream"
        );
        assert!(
            registry.lock().await.get(&7).is_none(),
            "read_line cap must drop the flooded stream's sink"
        );
    }

    #[tokio::test]
    async fn read_line_with_limit_accepts_protocol_bound_above_default() {
        let registry: Registry = Arc::new(Mutex::new(HashMap::new()));
        let (outbound_tx, _outbound_rx) = mpsc::channel::<Frame>(256);
        let mut stream = register_stream_inner(&registry, &outbound_tx, 7, Channel::Ctl)
            .await
            .expect("register");
        let inbound_tx = registry
            .lock()
            .await
            .get(&7)
            .and_then(|sink| sink.inbound.clone())
            .expect("sink registered");
        let line_bytes = MAX_LINE_BYTES + MAX_FRAME_PAYLOAD;
        for _ in 0..line_bytes / MAX_FRAME_PAYLOAD {
            inbound_tx
                .send(vec![b'x'; MAX_FRAME_PAYLOAD])
                .expect("feed inbound");
        }
        inbound_tx.send(vec![b'\n']).expect("feed newline");

        let line = timeout(
            Duration::from_secs(2),
            stream.read_line_with_limit(line_bytes),
        )
        .await
        .expect("line timely")
        .expect("line");
        assert_eq!(line.len(), line_bytes);
    }

    /// Companion to the cap test: a normal newline-terminated line (well under
    /// [`MAX_LINE_BYTES`]) that spans multiple inbound chunks must still read
    /// back correctly — the cap must not break the legitimate stdio path.
    #[tokio::test]
    async fn read_line_reads_normal_line_across_chunks() {
        let (host, mut runner) = mux_pair();
        let sender = host.open(Channel::Stdio).await.unwrap();

        // Send a line in two sends with no newline until the end.
        sender.send(b"hello ").await.unwrap();
        sender.send(b"multiplexed world\n").await.unwrap();

        let mut r = timeout(Duration::from_secs(2), runner.accept())
            .await
            .expect("accept timely")
            .expect("stream");
        let line = timeout(Duration::from_secs(2), r.read_line())
            .await
            .expect("line timely")
            .expect("line");
        assert_eq!(line, "hello multiplexed world");
    }

    /// The on-wire `channel` byte must round-trip for every variant, and each
    /// variant must map to a distinct byte — the wire tag is the only thing the
    /// peer reads to route a stream, so a collision would silently misroute
    /// traffic. Retired tags 2, 4 and 5 remain reserved on the wire.
    #[test]
    fn channel_wire_byte_round_trips_for_every_variant() {
        let all = [
            (Channel::Stdio, 1u8),
            (Channel::Commit, 2),
            (Channel::Llm, 3),
            (Channel::SessionStore, 5),
            (Channel::Rpc, 6),
            (Channel::Ctl, 7),
            (Channel::Http, 8),
            (Channel::Pty, 9),
        ];
        let mut seen = std::collections::HashSet::new();
        for (channel, byte) in all {
            assert_eq!(channel.to_wire(), byte, "{channel:?} wire byte");
            assert_eq!(
                Channel::from_wire(byte),
                Some(channel),
                "{byte} must decode back to {channel:?}"
            );
            assert!(seen.insert(byte), "wire byte {byte} used by two channels");
        }
        // An unknown byte (one past the last variant) must not decode.
        assert_eq!(Channel::from_wire(10), None);
    }
}
