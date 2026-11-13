//! Port Scanning — Detects newly-listening TCP ports inside the worker environment.
//!
//! Periodically scans `/proc/net/tcp{,6}` (Linux) or equivalent mechanisms to detect
//! new ports. When a new port is detected, emits a `port_open` event to g5-api
//! for port forwarding registration. Identifies the listening process and generates
//! human-readable labels.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::Serialize;
use tracing::{debug, info, warn};

use g5_contracts::error::G5Error;

// ─── Configuration ───────────────────────────────────────────────────────────

/// Configuration for the port scanner.
#[derive(Debug, Clone)]
pub struct PortScanConfig {
    /// Scan interval.
    pub interval: Duration,
    /// Ports to exclude from detection (e.g., worker's own port).
    pub exclude_ports: Vec<u16>,
    /// Whether to attempt process identification.
    pub identify_processes: bool,
}

// ─── Events ──────────────────────────────────────────────────────────────────

/// A port event (open or close).
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum PortEvent {
    #[serde(rename = "port_open")]
    Open {
        port: u16,
        pid: Option<u32>,
        process_name: Option<String>,
        label: Option<String>,
    },
    #[serde(rename = "port_close")]
    Close { port: u16 },
}

/// Information about a listening port.
#[derive(Debug, Clone)]
pub struct ListeningPort {
    pub port: u16,
    pub pid: Option<u32>,
    pub process_name: Option<String>,
    pub label: Option<String>,
    pub detected_at: DateTime<Utc>,
}

// ─── Port Scanner ────────────────────────────────────────────────────────────

/// Scans for newly-listening TCP ports.
pub struct PortScanner {
    config: PortScanConfig,
    stopped: Arc<AtomicBool>,
    known_ports: Arc<tokio::sync::Mutex<HashSet<u16>>>,
    listening_ports: Arc<tokio::sync::Mutex<Vec<ListeningPort>>>,
}

impl PortScanner {
    /// Create a new port scanner.
    #[must_use]
    pub fn new(config: PortScanConfig) -> Self {
        Self {
            config,
            stopped: Arc::new(AtomicBool::new(false)),
            known_ports: Arc::new(tokio::sync::Mutex::new(HashSet::new())),
            listening_ports: Arc::new(tokio::sync::Mutex::new(Vec::new())),
        }
    }

    /// Start the scanning loop. Emits events via the provided channel.
    /// Should be spawned as a background task.
    pub async fn run(&self, event_tx: tokio::sync::mpsc::Sender<PortEvent>) -> Result<(), G5Error> {
        info!(
            interval_ms = self.config.interval.as_millis(),
            "Starting port scanner"
        );

        // Initialize with current listening ports
        let initial_ports = self.scan_listening_ports().await?;
        {
            let mut known = self.known_ports.lock().await;
            for port in &initial_ports {
                known.insert(*port);
            }
        }
        debug!(
            count = initial_ports.len(),
            "Initialized with existing ports"
        );

        let mut interval = tokio::time::interval(self.config.interval);

        loop {
            interval.tick().await;

            if self.stopped.load(Ordering::Relaxed) {
                info!("Port scanner stopped");
                return Ok(());
            }

            match self.scan_once_internal().await {
                Ok(events) => {
                    for event in events {
                        if event_tx.send(event).await.is_err() {
                            warn!("Port event channel closed, stopping scanner");
                            return Ok(());
                        }
                    }
                }
                Err(e) => {
                    warn!(error = %e, "Port scan failed, will retry");
                }
            }
        }
    }

    /// Stop the scanner.
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Relaxed);
    }

    /// Get the current set of detected listening ports.
    pub fn listening_ports(&self) -> Vec<ListeningPort> {
        // Use try_lock to avoid blocking; return empty if locked
        match self.listening_ports.try_lock() {
            Ok(ports) => ports.clone(),
            Err(e) => {
                tracing::debug!(error = %e, "Cannot acquire port scan lock — returning empty");
                Vec::new()
            }
        }
    }

    /// Perform a single scan (for testing or on-demand use).
    pub async fn scan_once(&self) -> Result<Vec<PortEvent>, G5Error> {
        self.scan_once_internal().await
    }

    // ─── Internal ────────────────────────────────────────────────────────────

    async fn scan_once_internal(&self) -> Result<Vec<PortEvent>, G5Error> {
        let current_ports = self.scan_listening_ports().await?;
        let current_set: HashSet<u16> = current_ports.into_iter().collect();

        let mut events = Vec::new();
        let mut known = self.known_ports.lock().await;
        let mut listening = self.listening_ports.lock().await;

        // Detect new ports
        for &port in &current_set {
            if !known.contains(&port) && !self.config.exclude_ports.contains(&port) {
                let (pid, process_name) = if self.config.identify_processes {
                    self.identify_process(port).await
                } else {
                    (None, None)
                };

                let label = process_name.as_ref().map(|name| generate_label(name, port));

                info!(
                    port = port,
                    pid = ?pid,
                    process = ?process_name,
                    label = ?label,
                    "New listening port detected"
                );

                events.push(PortEvent::Open {
                    port,
                    pid,
                    process_name: process_name.clone(),
                    label: label.clone(),
                });

                listening.push(ListeningPort {
                    port,
                    pid,
                    process_name,
                    label,
                    detected_at: Utc::now(),
                });

                known.insert(port);
            }
        }

        // Detect closed ports
        let closed: Vec<u16> = known
            .iter()
            .copied()
            .filter(|p| !current_set.contains(p) && !self.config.exclude_ports.contains(p))
            .collect();

        for port in closed {
            info!(port = port, "Port closed");
            events.push(PortEvent::Close { port });
            known.remove(&port);
            listening.retain(|lp| lp.port != port);
        }

        Ok(events)
    }

    /// Scan for currently listening TCP ports.
    async fn scan_listening_ports(&self) -> Result<Vec<u16>, G5Error> {
        // On Linux, parse /proc/net/tcp
        #[cfg(target_os = "linux")]
        {
            self.scan_proc_net_tcp().await
        }

        // On macOS (local workers), use a different approach
        #[cfg(target_os = "macos")]
        {
            self.scan_macos_ports().await
        }

        // Fallback for other platforms
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            Ok(Vec::new())
        }
    }

    #[cfg(target_os = "linux")]
    async fn scan_proc_net_tcp(&self) -> Result<Vec<u16>, G5Error> {
        let mut ports = HashSet::new();
        for path in ["/proc/net/tcp", "/proc/net/tcp6"] {
            match tokio::fs::read_to_string(path).await {
                Ok(content) => ports.extend(parse_proc_tcp_ports(&content)),
                // Kernels without IPv6 have no tcp6 table.
                Err(error)
                    if path.ends_with("tcp6") && error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(G5Error::Internal {
                        message: format!("Failed to read {path}: {error}"),
                    })
                }
            }
        }
        Ok(ports.into_iter().collect())
    }

    #[cfg(target_os = "macos")]
    async fn scan_macos_ports(&self) -> Result<Vec<u16>, G5Error> {
        // Machine-readable `n` fields contain the address. Human output ends
        // each row with `(LISTEN)`, not the address, and is unsuitable here.
        let output = tokio::process::Command::new("lsof")
            .args(["-iTCP", "-P", "-n", "-sTCP:LISTEN", "-Fn"])
            .output()
            .await
            .map_err(|e| G5Error::Internal {
                message: format!("Failed to run lsof: {}", e),
            })?;
        // lsof uses exit 1 for an empty selection. Other failures must not
        // manufacture a closed-port observation from an incomplete scan.
        if !output.status.success() && output.status.code() != Some(1) {
            return Err(G5Error::Internal {
                message: "lsof port scan failed".into(),
            });
        }
        Ok(parse_lsof_ports(&String::from_utf8_lossy(&output.stdout)))
    }

    /// Identify the process listening on a given port.
    async fn identify_process(&self, port: u16) -> (Option<u32>, Option<String>) {
        #[cfg(target_os = "linux")]
        {
            // Read /proc/net/tcp to find the inode, then scan /proc/*/fd to find the PID
            // For simplicity, use ss or lsof as fallback
            if let Ok(output) = tokio::process::Command::new("ss")
                .args(["-tlnp", &format!("sport = :{}", port)])
                .output()
                .await
            {
                let stdout = String::from_utf8_lossy(&output.stdout);
                // Parse ss output for PID and process name
                for line in stdout.lines() {
                    if let Some(users_start) = line.find("users:((") {
                        let rest = &line[users_start + 9..];
                        if let Some(end) = rest.find('"') {
                            let process_name = &rest[1..end];
                            // Find pid=NNNN
                            if let Some(pid_start) = rest.find("pid=") {
                                let pid_str = &rest[pid_start + 4..];
                                if let Some(end) = pid_str.find(',') {
                                    if let Ok(pid) = pid_str[..end].parse::<u32>() {
                                        return (Some(pid), Some(process_name.to_string()));
                                    }
                                }
                            }
                            return (None, Some(process_name.to_string()));
                        }
                    }
                }
            }
            (None, None)
        }

        #[cfg(not(target_os = "linux"))]
        {
            // On macOS, lsof can identify the process
            if let Ok(output) = tokio::process::Command::new("lsof")
                .args(["-i", &format!(":{}", port), "-P", "-n"])
                .output()
                .await
            {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines().skip(1) {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        let name = parts[0].to_string();
                        let pid = parts[1].parse::<u32>().ok();
                        return (pid, Some(name));
                    }
                }
            }
            (None, None)
        }
    }
}

#[cfg(any(target_os = "linux", test))]
fn parse_proc_tcp_ports(content: &str) -> Vec<u16> {
    content
        .lines()
        .skip(1)
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            fields.next()?;
            let local = fields.next()?;
            fields.next()?;
            if fields.next()? != "0A" {
                return None;
            }
            u16::from_str_radix(local.rsplit_once(':')?.1, 16).ok()
        })
        .collect()
}

#[cfg(any(target_os = "macos", test))]
fn parse_lsof_ports(content: &str) -> Vec<u16> {
    content
        .lines()
        .filter_map(|line| line.strip_prefix('n')?.rsplit_once(':')?.1.parse().ok())
        .collect()
}

/// Generate a human-readable label for a detected port based on process name.
fn generate_label(process_name: &str, port: u16) -> String {
    let lower = process_name.to_lowercase();
    if lower.contains("node") || lower.contains("npm") {
        match port {
            3000 => "Node.js dev server".to_string(),
            5173..=5175 => "Vite dev server".to_string(),
            3001..=3999 => format!("Node.js server (port {})", port),
            _ => format!("Node.js process (port {})", port),
        }
    } else if lower.contains("python") || lower.contains("uvicorn") || lower.contains("gunicorn") {
        match port {
            8000 => "Python server (uvicorn/Django)".to_string(),
            5000 => "Flask dev server".to_string(),
            _ => format!("Python server (port {})", port),
        }
    } else if lower.contains("cargo") || lower.contains("rustc") {
        format!("Rust dev server (port {})", port)
    } else if lower.contains("java") || lower.contains("gradle") || lower.contains("maven") {
        match port {
            8080 => "Java server (port 8080)".to_string(),
            _ => format!("Java process (port {})", port),
        }
    } else if lower.contains("go") {
        format!("Go server (port {})", port)
    } else if lower.contains("ruby") || lower.contains("rails") || lower.contains("puma") {
        match port {
            3000 => "Rails dev server".to_string(),
            _ => format!("Ruby server (port {})", port),
        }
    } else if lower.contains("postgres") || lower.contains("mysql") || lower.contains("redis") {
        format!("Database ({}, port {})", process_name, port)
    } else {
        format!("{} (port {})", process_name, port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ipv4_ipv6_listeners_and_ignore_other_tcp_states() {
        let ipv4 = "  sl  local_address rem_address st
  0: 0100007F:0BB8 00000000:0000 0A
  1: 0100007F:1F90 00000000:0000 01
invalid
";
        let ipv6 = "  sl  local_address remote_address st
  0: 00000000000000000000000001000000:1445 00000000000000000000000000000000:0000 0A
";
        assert_eq!(parse_proc_tcp_ports(ipv4), vec![3000]);
        assert_eq!(parse_proc_tcp_ports(ipv6), vec![5189]);
        assert_eq!(
            parse_lsof_ports(
                "p123
n*:3000
p456
n[::1]:5189
n127.0.0.1:8000
ninvalid
"
            ),
            vec![3000, 5189, 8000]
        );
    }

    /// Lowest port the kernel hands to `:0` binds. Parallel test processes
    /// start long-lived servers on `:0`, so a released ephemeral number can be
    /// re-listened for minutes and hide the close this test waits for.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn ephemeral_range_start() -> u16 {
        #[cfg(target_os = "linux")]
        {
            std::fs::read_to_string("/proc/sys/net/ipv4/ip_local_port_range")
                .ok()
                .and_then(|range| range.split_whitespace().next()?.parse().ok())
                .unwrap_or(32_768)
        }
        #[cfg(target_os = "macos")]
        {
            std::process::Command::new("sysctl")
                .args(["-n", "net.inet.ip.portrange.first"])
                .output()
                .ok()
                .and_then(|output| String::from_utf8(output.stdout).ok()?.trim().parse().ok())
                .unwrap_or(49_152)
        }
    }

    /// Bind a loopback listener on a free port below the ephemeral range,
    /// falling back to `:0` when no port below it is free.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    async fn bind_outside_ephemeral_range(
        address: &str,
    ) -> std::io::Result<tokio::net::TcpListener> {
        const LOW: u16 = 20_000;
        let high = ephemeral_range_start().min(30_000);
        if high <= LOW {
            return tokio::net::TcpListener::bind(address).await;
        }
        let ip: std::net::IpAddr = address
            .trim_end_matches(":0")
            .trim_matches(|c| c == '[' || c == ']')
            .parse()
            .expect("loopback address");
        let span = u32::from(high - LOW);
        // Spread concurrent runs of this test across the range.
        let start = std::process::id() % span;
        for offset in 0..span {
            let port = LOW + u16::try_from((start + offset) % span).expect("span fits u16");
            match tokio::net::TcpListener::bind((ip, port)).await {
                Ok(listener) => return Ok(listener),
                Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {}
                Err(error) => return Err(error),
            }
        }
        tokio::net::TcpListener::bind(address).await
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[tokio::test]
    async fn native_ipv4_and_ipv6_listeners_emit_open_and_close() {
        for (address, ipv6) in [("127.0.0.1:0", false), ("[::1]:0", true)] {
            let listener = match bind_outside_ephemeral_range(address).await {
                Ok(listener) => listener,
                Err(error)
                    if ipv6
                        && matches!(
                            error.raw_os_error(),
                            Some(libc::EADDRNOTAVAIL) | Some(libc::EAFNOSUPPORT)
                        ) =>
                {
                    eprintln!("IPv6 loopback unavailable: {error}; IPv4 remains verified");
                    continue;
                }
                Err(error) => panic!("native listener {address}: {error}"),
            };
            let port = listener.local_addr().unwrap().port();
            let scanner = PortScanner::new(PortScanConfig {
                interval: Duration::from_millis(10),
                exclude_ports: vec![],
                identify_processes: false,
            });
            assert!(
                scanner.scan_once().await.unwrap().iter().any(
                    |event| matches!(event, PortEvent::Open {port: seen, ..} if *seen == port)
                ),
                "open {address}"
            );
            assert!(scanner
                .listening_ports()
                .iter()
                .any(|seen| seen.port == port));
            assert!(
                !scanner.scan_once().await.unwrap().iter().any(
                    |event| matches!(event, PortEvent::Open {port: seen, ..} if *seen == port)
                ),
                "no duplicate open"
            );
            drop(listener);
            // The scan is namespace-wide and keyed on the port number alone,
            // so a listener another test process binds on the released number
            // masks the close. The port sits below the ephemeral range, where
            // `:0` binds never land; the bounded rescan absorbs the rare
            // fixed-port collision.
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            loop {
                let closed =
                    scanner.scan_once().await.unwrap().iter().any(
                        |event| matches!(event, PortEvent::Close {port: seen} if *seen == port),
                    );
                if closed {
                    break;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "close {address}: port {port} still listed as listening after its listener was dropped"
                );
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        }
    }
}
