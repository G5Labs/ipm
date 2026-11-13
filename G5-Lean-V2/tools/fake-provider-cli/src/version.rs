//! The provider CLI pins, embedded from the single-source pin files so the
//! fakes always report the version the real images install.

/// Contents of `scripts/claude-code-cli-version`.
pub const CLAUDE_CODE_PIN: &str = include_str!("../../../scripts/claude-code-cli-version");
/// Contents of `scripts/codex-cli-version`.
pub const CODEX_PIN: &str = include_str!("../../../scripts/codex-cli-version");

/// The pinned Claude Code version, e.g. `2.1.270`.
pub fn claude_version() -> &'static str {
    CLAUDE_CODE_PIN.trim()
}

/// The pinned Codex CLI version, e.g. `0.153.3`.
pub fn codex_version() -> &'static str {
    CODEX_PIN.trim()
}

/// What `claude --version` prints.
pub fn claude_version_line() -> String {
    format!("{} (Claude Code)", claude_version())
}

/// What `codex --version` prints.
pub fn codex_version_line() -> String {
    format!("codex-cli {}", codex_version())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pins_are_semver_like() {
        for pin in [claude_version(), codex_version()] {
            let parts: Vec<&str> = pin.split('.').collect();
            assert_eq!(parts.len(), 3, "pin {pin} is not major.minor.patch");
            for part in parts {
                part.parse::<u32>()
                    .unwrap_or_else(|_| panic!("pin {pin} has a non-numeric component"));
            }
        }
    }

    #[test]
    fn version_lines_match_the_real_cli_formats() {
        assert!(claude_version_line().ends_with(" (Claude Code)"));
        assert!(codex_version_line().starts_with("codex-cli "));
    }
}
