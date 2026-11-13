//! Provider session files (design v6 §8.4).
//!
//! The vocabulary shared by the `chat_session_files` internal routes and the
//! provider runner that harvests and restores the files: the provider tag, the
//! relative-path rules, the per-file size cap, and the zstd body codec both
//! ends of the wire use. A relative path is always relative to the provider
//! home the CLI writes into (`CLAUDE_CONFIG_DIR` for Claude, `CODEX_HOME` for
//! Codex), so it starts with that provider's session directory.

use std::io::Read;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Largest session file stored, in decompressed bytes (the cap the Codex
/// artifact mirror used).
pub const CHAT_SESSION_FILE_MAX_BYTES: u64 = 128 * 1024 * 1024;

/// Longest relative path accepted, in bytes.
pub const CHAT_SESSION_FILE_MAX_PATH_BYTES: usize = 1024;

/// Longest provider session id accepted, in bytes.
pub const CHAT_SESSION_FILE_MAX_PROVIDER_SESSION_ID_BYTES: usize = 256;

/// Content type of a zstd-compressed session file body: the `PUT` request
/// body and the `content` response body.
pub const SESSION_FILE_CONTENT_TYPE: &str = "application/zstd";

/// Response headers of `GET .../session-files/content`.
pub const SESSION_FILE_PROVIDER_HEADER: &str = "x-g5-session-file-provider";
pub const SESSION_FILE_SHA256_HEADER: &str = "x-g5-session-file-sha256";
pub const SESSION_FILE_SIZE_HEADER: &str = "x-g5-session-file-size";

const SESSION_FILE_ZSTD_LEVEL: i32 = 3;

/// The provider whose CLI wrote a session file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionFileProvider {
    ClaudeCode,
    Codex,
}

impl SessionFileProvider {
    pub fn as_str(self) -> &'static str {
        match self {
            SessionFileProvider::ClaudeCode => "claude_code",
            SessionFileProvider::Codex => "codex",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "claude_code" => Some(SessionFileProvider::ClaudeCode),
            "codex" => Some(SessionFileProvider::Codex),
            _ => None,
        }
    }

    /// The directory under the provider home that holds every session file
    /// of this provider: `projects/` under `CLAUDE_CONFIG_DIR`, `sessions/`
    /// under `CODEX_HOME`.
    pub fn root_dir(self) -> &'static str {
        match self {
            SessionFileProvider::ClaudeCode => "projects",
            SessionFileProvider::Codex => "sessions",
        }
    }
}

impl std::fmt::Display for SessionFileProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A session file rejected at the boundary.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SessionFileError {
    #[error(
        "provider session id must be non-empty, without surrounding whitespace, and at most \
         {CHAT_SESSION_FILE_MAX_PROVIDER_SESSION_ID_BYTES} bytes"
    )]
    InvalidProviderSessionId,
    #[error("session file path is not a safe relative path under `{root}/`")]
    InvalidPath { root: &'static str },
    #[error("sha256 must be 64 lowercase hex characters")]
    InvalidSha256,
    #[error("session file exceeds {CHAT_SESSION_FILE_MAX_BYTES} bytes")]
    TooLarge,
    #[error("session file body is not a valid zstd stream: {0}")]
    InvalidBody(String),
    #[error("session file bytes do not match the declared sha256 or size")]
    DigestMismatch,
}

pub fn validate_provider_session_id(provider_session_id: &str) -> Result<(), SessionFileError> {
    if provider_session_id.is_empty()
        || provider_session_id.trim() != provider_session_id
        || provider_session_id.len() > CHAT_SESSION_FILE_MAX_PROVIDER_SESSION_ID_BYTES
        || provider_session_id.contains(['/', '\\', '\0'])
    {
        return Err(SessionFileError::InvalidProviderSessionId);
    }
    Ok(())
}

/// A relative path is `<root dir>/<segment>/...`: no absolute path, no empty,
/// `.` or `..` segment, no backslash or NUL, and at most
/// [`CHAT_SESSION_FILE_MAX_PATH_BYTES`] bytes. Joining it onto the provider
/// home can therefore never leave the provider's session directory.
pub fn validate_session_file_path(
    provider: SessionFileProvider,
    relative_path: &str,
) -> Result<(), SessionFileError> {
    let invalid = SessionFileError::InvalidPath {
        root: provider.root_dir(),
    };
    if relative_path.is_empty()
        || relative_path.len() > CHAT_SESSION_FILE_MAX_PATH_BYTES
        || relative_path.contains(['\\', '\0'])
    {
        return Err(invalid);
    }
    let mut segments = relative_path.split('/');
    if segments.next() != Some(provider.root_dir()) {
        return Err(invalid);
    }
    let mut rest = 0usize;
    for segment in segments {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err(invalid);
        }
        rest += 1;
    }
    if rest == 0 {
        return Err(invalid);
    }
    Ok(())
}

pub fn validate_sha256_hex(value: &str) -> Result<(), SessionFileError> {
    if value.len() == 64
        && value
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    {
        Ok(())
    } else {
        Err(SessionFileError::InvalidSha256)
    }
}

/// Lowercase hex sha256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Compress a session file for the wire.
pub fn encode_session_file_body(bytes: &[u8]) -> Result<Vec<u8>, SessionFileError> {
    zstd::stream::encode_all(bytes, SESSION_FILE_ZSTD_LEVEL)
        .map_err(|error| SessionFileError::InvalidBody(error.to_string()))
}

/// Decompress a session file body, refusing to produce more than
/// [`CHAT_SESSION_FILE_MAX_BYTES`] (a small body can expand without bound).
pub fn decode_session_file_body(body: &[u8]) -> Result<Vec<u8>, SessionFileError> {
    let decoder = zstd::stream::read::Decoder::with_buffer(body)
        .map_err(|error| SessionFileError::InvalidBody(error.to_string()))?;
    let mut bytes = Vec::new();
    decoder
        .take(CHAT_SESSION_FILE_MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| SessionFileError::InvalidBody(error.to_string()))?;
    if bytes.len() as u64 > CHAT_SESSION_FILE_MAX_BYTES {
        return Err(SessionFileError::TooLarge);
    }
    Ok(bytes)
}

/// One stored session file as the internal client returns it: bytes already
/// decompressed and checked against the digest and size the API reported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatSessionFileContent {
    pub provider: SessionFileProvider,
    pub sha256: String,
    pub bytes: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The file the `transcript` frame in [`PREVIOUS_CODEC_FRAMES`] encodes.
    const PREVIOUS_CODEC_TRANSCRIPT: &[u8] = b"{\"type\":\"user\",\"uuid\":\"0b7c3c1e-5b1a-4f7e-9d2c-1a2b3c4d5e6f\",\"cwd\":\"/work/code\",\"message\":{\"role\":\"user\",\"content\":\"Resume where we left off.\"}}\n{\"type\":\"assistant\",\"uuid\":\"7f6e5d4c-3b2a-4190-8f7e-6d5c4b3a2f1e\",\"parentUuid\":\"0b7c3c1e-5b1a-4f7e-9d2c-1a2b3c4d5e6f\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"Picking up the zstd upgrade.\"}]}}\n{\"type\":\"summary\",\"summary\":\"zstd advisories\",\"leafUuid\":\"7f6e5d4c-3b2a-4190-8f7e-6d5c4b3a2f1e\"}\n";

    /// Bodies `encode_session_file_body` produced before the zstd 0.14 upgrade
    /// (zstd 0.13.3, zstd-safe 7.2.4, zstd-sys 2.0.16, libzstd 1.5.7), as hex:
    /// what a one-version-back worker still uploads and what the API served
    /// before the upgrade. The `transcript` frame encodes
    /// [`PREVIOUS_CODEC_TRANSCRIPT`], `empty` an empty file, `repeat` 1000
    /// copies of `{"type":"user"}\n` and `x_1mib` one MiB of `x`.
    const PREVIOUS_CODEC_FRAMES: [(&str, &str); 4] = [
        (
            "transcript",
            "28b52ffd0058f5070036903422308958076029e14bc51b4007d3cd6ec66126760d45300a7b035eb7\
             1994aba0280c42290029002c00907a668e893370798a932cd25e0215ed4d5cda220ed38e3ce5350e\
             c2cef0a9d5ccf2b3daf9d3833a218618a545e31debd4ba3c835a87eabc809d69741a49cf21492738\
             7fb1731e32f3a1f40e0d2951073b07a607a7040334badcc9cc4762b28ab33c0626da9b08d0100a64\
             699b384a7b0b649e880d8d96ce67ce2d549503852aee1d669e7fa4341f03b876cc0fb91054674165\
             c3e1cac55417a413619d41d754d7227a1aef09b063010e00f2f6c0a2c93555c32ded646bf8538d12\
             59b99f1531300e74e601db34533206e529a6c9d719210a",
        ),
        ("empty", "28b52ffd2000010000"),
        (
            "repeat",
            "28b52ffd0058c50000807b2274797065223a2275736572227d0a0100dafcf07309",
        ),
        (
            "x_1mib",
            "28b52ffd00585400001078780100fbff39c00202001078020010780200107802001078020010780200\
             107802001078010000",
        ),
    ];

    fn previous_codec_file(name: &str) -> Vec<u8> {
        match name {
            "transcript" => PREVIOUS_CODEC_TRANSCRIPT.to_vec(),
            "empty" => Vec::new(),
            "repeat" => b"{\"type\":\"user\"}\n".repeat(1000),
            "x_1mib" => vec![b'x'; 1 << 20],
            _ => unreachable!("unknown fixture {name}"),
        }
    }

    fn unhex(hex: &str) -> Vec<u8> {
        let digits: Vec<u8> = hex.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
        digits
            .chunks(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect()
    }

    #[test]
    fn provider_tags_round_trip() {
        for provider in [SessionFileProvider::ClaudeCode, SessionFileProvider::Codex] {
            assert_eq!(
                SessionFileProvider::parse(provider.as_str()),
                Some(provider)
            );
            assert_eq!(
                serde_json::to_value(provider).unwrap(),
                serde_json::json!(provider.as_str())
            );
        }
        assert_eq!(SessionFileProvider::parse("naive_react"), None);
    }

    #[test]
    fn paths_must_stay_under_the_provider_session_directory() {
        let claude = SessionFileProvider::ClaudeCode;
        let codex = SessionFileProvider::Codex;
        for ok in [
            "projects/-work/0b7c.jsonl",
            "projects/-work/0b7c/subagents/agent-1.meta.json",
        ] {
            assert_eq!(validate_session_file_path(claude, ok), Ok(()), "{ok}");
        }
        assert_eq!(
            validate_session_file_path(codex, "sessions/2026/09/15/rollout-x-id.jsonl"),
            Ok(())
        );
        for bad in [
            "",
            "projects",
            "projects/",
            "/projects/-work/a.jsonl",
            "projects//a.jsonl",
            "projects/./a.jsonl",
            "projects/../a.jsonl",
            "projects/-work/..",
            "projects\\-work\\a.jsonl",
            "sessions/2026/a.jsonl",
            "projects/a\0.jsonl",
        ] {
            assert!(
                validate_session_file_path(claude, bad).is_err(),
                "{bad:?} must be rejected for Claude"
            );
        }
        assert!(validate_session_file_path(codex, "projects/-work/a.jsonl").is_err());
        let long = format!("sessions/{}", "a".repeat(CHAT_SESSION_FILE_MAX_PATH_BYTES));
        assert!(validate_session_file_path(codex, &long).is_err());
    }

    #[test]
    fn provider_session_ids_and_digests_are_checked() {
        assert_eq!(validate_provider_session_id("0b7c-1"), Ok(()));
        for bad in ["", " id", "id ", "a/b", "a\\b"] {
            assert!(validate_provider_session_id(bad).is_err(), "{bad:?}");
        }
        assert!(validate_provider_session_id(
            &"a".repeat(CHAT_SESSION_FILE_MAX_PROVIDER_SESSION_ID_BYTES + 1)
        )
        .is_err());
        let digest = sha256_hex(b"hello");
        assert_eq!(
            digest,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
        assert_eq!(validate_sha256_hex(&digest), Ok(()));
        assert!(validate_sha256_hex(&digest.to_uppercase()).is_err());
        assert!(validate_sha256_hex("abc").is_err());
    }

    #[test]
    fn bodies_round_trip_and_decoding_is_capped() {
        let bytes = b"{\"type\":\"user\"}\n".repeat(1000);
        let body = encode_session_file_body(&bytes).unwrap();
        assert!(body.len() < bytes.len());
        assert_eq!(decode_session_file_body(&body).unwrap(), bytes);

        assert!(matches!(
            decode_session_file_body(b"not zstd"),
            Err(SessionFileError::InvalidBody(_))
        ));

        // A tiny frame that expands one byte past the cap is refused without
        // materializing an unbounded buffer.
        let oversized = zstd::stream::encode_all(
            std::io::repeat(0u8).take(CHAT_SESSION_FILE_MAX_BYTES + 1),
            SESSION_FILE_ZSTD_LEVEL,
        )
        .unwrap();
        assert!(oversized.len() < 64 * 1024);
        assert_eq!(
            decode_session_file_body(&oversized),
            Err(SessionFileError::TooLarge)
        );
    }

    /// Both wire directions survive the codec upgrade: the frames a
    /// previous-version worker uploads, and the frames the previous API served,
    /// decode to the same bytes and digests, and the current encoder's output
    /// for the same files decodes the same way.
    #[test]
    fn frames_from_the_previous_codec_still_decode() {
        for (name, frame_hex) in PREVIOUS_CODEC_FRAMES {
            let frame = unhex(frame_hex);
            assert_eq!(
                &frame[..4],
                &[0x28, 0xb5, 0x2f, 0xfd],
                "{name}: zstd frame magic"
            );
            let expected = previous_codec_file(name);
            let decoded = decode_session_file_body(&frame)
                .unwrap_or_else(|error| panic!("{name}: previous-codec frame refused: {error}"));
            assert_eq!(decoded.len(), expected.len(), "{name}: size");
            assert_eq!(
                sha256_hex(&decoded),
                sha256_hex(&expected),
                "{name}: digest"
            );

            let reencoded = encode_session_file_body(&expected).unwrap();
            assert_eq!(
                &reencoded[..4],
                &[0x28, 0xb5, 0x2f, 0xfd],
                "{name}: re-encoded magic"
            );
            assert_eq!(
                decode_session_file_body(&reencoded).unwrap(),
                expected,
                "{name}"
            );
        }
    }

    /// A body is exactly one well-formed zstd stream. A frame cut short, a
    /// frame followed by bytes that are not a frame, no bytes at all, and a
    /// corrupt legacy-format frame (zstd v0.4 to v0.7, RUSTSEC-2026-0403) are
    /// refused outright; the decoder never hands back a prefix of the file.
    #[test]
    fn truncated_trailing_and_legacy_bodies_are_refused() {
        let frame = encode_session_file_body(PREVIOUS_CODEC_TRANSCRIPT).unwrap();
        assert!(frame.len() > 64);
        let mut bodies: Vec<(String, Vec<u8>)> = Vec::new();
        for cut in [4usize, 9, 32, frame.len() / 2, frame.len() - 1] {
            bodies.push((
                format!("truncated after {cut} bytes"),
                frame[..cut].to_vec(),
            ));
        }
        let mut trailing = frame.clone();
        trailing.extend_from_slice(b"not a zstd frame");
        bodies.push(("trailing garbage".to_string(), trailing));
        bodies.push(("empty body".to_string(), Vec::new()));
        for legacy_magic in [0x24u8, 0x25, 0x26, 0x27] {
            bodies.push((
                format!("corrupt legacy frame 0x{legacy_magic:02x}"),
                vec![legacy_magic, 0xb5, 0x2f, 0xfd, 0x00, 0x00],
            ));
        }
        for (label, body) in bodies {
            assert!(
                matches!(
                    decode_session_file_body(&body),
                    Err(SessionFileError::InvalidBody(_))
                ),
                "{label} must be refused"
            );
        }
    }

    /// The decompressed-size cap is inclusive: a file of exactly
    /// [`CHAT_SESSION_FILE_MAX_BYTES`] decodes whole.
    #[test]
    fn decoding_accepts_a_file_at_the_cap() {
        let at_cap = zstd::stream::encode_all(
            std::io::repeat(0u8).take(CHAT_SESSION_FILE_MAX_BYTES),
            SESSION_FILE_ZSTD_LEVEL,
        )
        .unwrap();
        assert!(at_cap.len() < 64 * 1024);
        let decoded = decode_session_file_body(&at_cap).unwrap();
        assert_eq!(decoded.len() as u64, CHAT_SESSION_FILE_MAX_BYTES);
        assert!(decoded.iter().all(|byte| *byte == 0));
    }
}
