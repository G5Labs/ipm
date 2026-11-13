//! Runtime detection of available isolation backends.
//!
//! Probes the current platform at startup to determine which isolation
//! backends are available. Checks for KVM support (Linux),
//! and basic process capabilities (git in PATH).
//!
//! Detection runs once at startup; results are cached for the lifetime
//! of the process. A `g5d doctor` command re-runs detection and reports
//! results.

use std::path::{Path, PathBuf};

use super::{BackendType, IsolationLevel};

// ---------------------------------------------------------------------------
// DetectionResult
// ---------------------------------------------------------------------------

/// Result of runtime backend detection.
#[derive(Debug, Clone)]
pub struct DetectionResult {
    /// Available backends in priority order (strongest first).
    pub available_backends: Vec<AvailableBackend>,
    /// Current platform information.
    pub platform: Platform,
    /// Recommended backend based on platform and availability.
    pub recommended: BackendType,
}

/// An available isolation backend.
#[derive(Debug, Clone)]
pub struct AvailableBackend {
    /// Backend type.
    pub backend_type: BackendType,
    /// Isolation level provided.
    pub isolation_level: IsolationLevel,
    /// Human-readable details (e.g., "Cloud Hypervisor with KVM on Linux x86_64").
    pub details: String,
}

/// Platform information.
#[derive(Debug, Clone)]
pub struct Platform {
    /// Operating system: "linux", "macos", "windows".
    pub os: String,
    /// CPU architecture: "x86_64", "aarch64".
    pub arch: String,
    /// Whether KVM is available (Linux only).
    pub kvm_available: bool,
    /// Whether WSL2 is available (Windows only).
    pub wsl2_available: bool,
}

// ---------------------------------------------------------------------------
// Detection functions
// ---------------------------------------------------------------------------

/// Detect available isolation backends on the current platform.
///
/// This function probes the system for:
/// - Cloud Hypervisor platform artifacts + KVM (Linux)
/// - WSL2 (Windows, parked; currently always unavailable)
/// - git binary (always, for process backend)
///
/// Results are returned in priority order (strongest isolation first).
pub async fn detect_backends() -> DetectionResult {
    let mut available = Vec::new();
    let platform = detect_platform().await;

    #[cfg(feature = "cloud-hypervisor")]
    if super::cloud_hypervisor::available() {
        available.push(AvailableBackend {
            backend_type: BackendType::CloudHypervisor,
            isolation_level: IsolationLevel::Vm,
            details: format!("Cloud Hypervisor with KVM on Linux {}", platform.arch),
        });
    }

    // 3. Process backend (always available if git is present)
    if find_binary("git").is_some() {
        tracing::debug!("ProcessBackend available: git found in PATH");
        available.push(AvailableBackend {
            backend_type: BackendType::Process,
            isolation_level: IsolationLevel::Process,
            details: "Process isolation with git worktrees".to_string(),
        });
    } else {
        tracing::warn!("git binary not found in PATH — ProcessBackend unavailable");
    }

    // 4. Determine recommendation (first available = strongest)
    let recommended = available
        .first()
        .map(|b| b.backend_type)
        .unwrap_or(BackendType::Process);

    tracing::info!(
        available_count = available.len(),
        recommended = ?recommended,
        os = %platform.os,
        arch = %platform.arch,
        "Backend detection complete"
    );

    DetectionResult {
        available_backends: available,
        platform,
        recommended,
    }
}

/// Check if a specific binary is available in PATH.
///
/// Returns the full path to the binary if found, or `None` if not in PATH.
#[must_use]
pub fn find_binary(name: &str) -> Option<PathBuf> {
    // Use the `which` approach: search PATH directories for the binary.
    // We implement this manually to avoid an external dependency.
    if let Ok(path_var) = std::env::var("PATH") {
        let separator = if cfg!(windows) { ';' } else { ':' };
        for dir in path_var.split(separator) {
            let candidate = Path::new(dir).join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
            // On Windows, also check with common extensions
            #[cfg(windows)]
            {
                for ext in &["exe", "cmd", "bat"] {
                    let with_ext = Path::new(dir).join(format!("{}.{}", name, ext));
                    if with_ext.is_file() {
                        return Some(with_ext);
                    }
                }
            }
        }
    }
    None
}

/// Check KVM availability on Linux.
///
/// Returns `true` if `/dev/kvm` exists and is readable.
#[must_use]
pub fn check_kvm() -> bool {
    #[cfg(target_os = "linux")]
    {
        let kvm_path = Path::new("/dev/kvm");
        if kvm_path.exists() {
            // Check if readable by attempting to open
            match std::fs::File::open(kvm_path) {
                Ok(_) => {
                    tracing::debug!("/dev/kvm exists and is readable");
                    true
                }
                Err(e) => {
                    tracing::debug!("/dev/kvm exists but not readable: {}", e);
                    false
                }
            }
        } else {
            tracing::debug!("/dev/kvm does not exist");
            false
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Detect the current platform.
async fn detect_platform() -> Platform {
    let os = std::env::consts::OS.to_string();
    let arch = std::env::consts::ARCH.to_string();

    let kvm_available = check_kvm();
    // Historical diagnostic field; there is no supported Windows VM backend.
    let wsl2_available = false;

    Platform {
        os,
        arch,
        kvm_available,
        wsl2_available,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_binary_git() {
        // git should be available in most development environments
        let result = find_binary("git");
        // We don't assert it's Some because CI may not have git,
        // but if it is, it should be a valid path
        if let Some(path) = result {
            assert!(path.is_file());
        }
    }

    #[test]
    fn test_find_binary_nonexistent() {
        let result = find_binary("this_binary_definitely_does_not_exist_xyz123");
        assert!(result.is_none());
    }

    #[test]
    fn test_check_kvm_returns_bool() {
        // Just ensure it doesn't panic
        let _ = check_kvm();
    }

    #[tokio::test]
    async fn test_detect_backends_runs() {
        // Integration test: ensure detection runs without panic
        let result = detect_backends().await;
        // Platform should always be populated
        assert!(!result.platform.os.is_empty());
        assert!(!result.platform.arch.is_empty());
    }

    #[test]
    fn test_platform_debug() {
        let platform = Platform {
            os: "linux".to_string(),
            arch: "x86_64".to_string(),
            kvm_available: false,
            wsl2_available: false,
        };
        let debug = format!("{:?}", platform);
        assert!(debug.contains("linux"));
    }

    #[test]
    fn test_detection_result_debug() {
        let result = DetectionResult {
            available_backends: vec![AvailableBackend {
                backend_type: BackendType::Process,
                isolation_level: IsolationLevel::Process,
                details: "test".to_string(),
            }],
            platform: Platform {
                os: "linux".to_string(),
                arch: "x86_64".to_string(),
                kvm_available: false,
                wsl2_available: false,
            },
            recommended: BackendType::Process,
        };
        let debug = format!("{:?}", result);
        assert!(debug.contains("Process"));
    }
}
