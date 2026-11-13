//! Canonical non-chat block extraction service.
//!
//! Realizes the consumer clause of the `grounding_identity_and_routing` D11
//! invariant: every surface that serves, compares for display, or fingerprints
//! a non-chat intent record's content obtains it from this module by
//! re-extracting from current source text — never from a stored JSONL body.
//! Hosts supply source text through [`BlockSourceProvider`] implementations
//! (g5d: materialized worktree files; g5-api: live workspace file draft overlay → committed
//! blob → sibling-branch content fallbacks). This module owns the
//! per-file-type dispatch, the canonical extracted-content shape, block
//! label/title metadata, the closed error taxonomy, and the
//! [`content_fingerprint`] drift hash.

use std::path::Path;

use async_trait::async_trait;
use sha2::{Digest, Sha256};

use crate::parsing::{
    extract_shell_blocks, CodeAnchorExtractor, ExtractError, MarkdownAnchorExtractor,
};
use g5_contracts::types::grounding::FileType;

/// Stable whole-file code anchor for qualified prompt assets
/// (`grounding_identity_and_routing` D9).
pub const WHOLE_FILE_ANCHOR: &str = "__file__";

/// Host I/O seam: resolves a stored record identity to current source text.
///
/// Providers only supply bytes — they never parse, never pick blocks, and
/// never read stored JSONL content. `Ok(None)` means "this provider does not
/// have the file" and falls through to the next provider in the chain; an
/// `Err` is a real I/O failure and ends the chain as
/// [`BlockExtractionError::SourceUnavailable`].
#[async_trait]
pub trait BlockSourceProvider: Send + Sync {
    /// Return the current text of the file backing `file_name` (a stored
    /// repo-local record identity), or `Ok(None)` when this provider does not
    /// have the file.
    async fn read_source_text(&self, file_name: &str) -> Result<Option<String>, String>;

    /// Human-readable provider description used in diagnostics.
    fn describe(&self) -> String {
        "source provider".to_string()
    }
}

/// Successful extraction: canonical content plus block metadata from the same
/// parse. Markdown records carry `label` + `title`; code records carry `label`
/// only; external-doc whole-file reads carry neither.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedBlock {
    pub content: String,
    pub label: Option<String>,
    pub title: Option<String>,
    pub boundary_uncertain: bool,
}

/// Closed extraction error taxonomy (block_extraction spec D3). Non-chat
/// consumers map source/extraction failures to a stale record with null
/// content — never to a stored-body fallback or a silent duplicate pick.
/// [`BlockExtractionError::ChatRejected`] instead reports a caller-contract
/// violation; chat rows are served from their stored snapshots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockExtractionError {
    /// No provider had the file, or a provider I/O failure ended the chain.
    SourceUnavailable {
        file_name: String,
        detail: Option<String>,
    },
    /// The file was read but the identifier is absent (including unsupported
    /// code extensions, which yield no symbols).
    BlockNotFound {
        file_name: String,
        identifier: String,
    },
    /// The identifier matches multiple blocks; carries duplicate line numbers
    /// so authoring can repair the source.
    AmbiguousDuplicate {
        file_name: String,
        identifier: String,
        line_numbers: Vec<usize>,
    },
    /// A chat identity reached the file-extraction seam — a caller bug: chat
    /// content is stored, never file-extracted (D11).
    ChatRejected,
}

impl std::fmt::Display for BlockExtractionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SourceUnavailable { file_name, detail } => match detail {
                Some(detail) => write!(f, "File not readable: {detail}. Record may be stale."),
                None => write!(
                    f,
                    "Source file '{file_name}' not available from any provider. Record may be stale."
                ),
            },
            Self::BlockNotFound {
                file_name,
                identifier,
            } => write!(
                f,
                "Block '{identifier}' not found in file '{file_name}'. Record may be stale."
            ),
            Self::AmbiguousDuplicate {
                file_name,
                identifier,
                line_numbers,
            } => write!(
                f,
                "Ambiguous duplicate block '{identifier}' in file '{file_name}' at lines {line_numbers:?}. Repair the source so the label is unique."
            ),
            Self::ChatRejected => write!(
                f,
                "Chat records are never file-extracted; chat content is stored"
            ),
        }
    }
}

impl std::error::Error for BlockExtractionError {}

/// Extract a block's canonical content through an ordered provider chain: the
/// first provider returning text wins and later providers are not consulted;
/// `Ok(None)` falls through; an empty or exhausted chain yields
/// `SourceUnavailable`; a provider I/O error ends the chain with its detail.
pub async fn extract_block(
    providers: &[&dyn BlockSourceProvider],
    file_type: FileType,
    file_name: &str,
    identifier: &str,
) -> Result<ExtractedBlock, BlockExtractionError> {
    if file_type == FileType::Chat {
        return Err(BlockExtractionError::ChatRejected);
    }
    for provider in providers {
        match provider.read_source_text(file_name).await {
            Ok(Some(text)) => {
                return extract_block_from_text(file_type, file_name, identifier, &text)
            }
            Ok(None) => continue,
            Err(detail) => {
                return Err(BlockExtractionError::SourceUnavailable {
                    file_name: file_name.to_string(),
                    detail: Some(format!("{}: {}", provider.describe(), detail)),
                })
            }
        }
    }
    Err(BlockExtractionError::SourceUnavailable {
        file_name: file_name.to_string(),
        detail: None,
    })
}

/// Synchronous dispatch core over already-read source text. This is the one
/// canonical content model (block_extraction spec D2), absorbed verbatim from
/// the historical g5d MCP extractor:
///
/// - `view`/`ontology` → markdown I/D/Q block (label line, including any
///   canonical semantic metadata suffix, stripped and LF-normalized) with
///   `label`/`title` metadata;
/// - `code` → shell anchors for `sh`/shebang sources, else tree-sitter
///   byte-range slice for the named symbol; the `__file__` anchor returns the
///   complete file;
/// - `external_doc` → the complete file;
/// - `chat` → rejected.
pub fn extract_block_from_text(
    file_type: FileType,
    file_name: &str,
    identifier: &str,
    text: &str,
) -> Result<ExtractedBlock, BlockExtractionError> {
    match file_type {
        FileType::Chat => Err(BlockExtractionError::ChatRejected),
        FileType::View | FileType::Ontology => markdown_block(file_name, identifier, text),
        FileType::ExternalDoc => Ok(ExtractedBlock {
            content: text.to_string(),
            label: None,
            title: None,
            boundary_uncertain: false,
        }),
        FileType::Code => {
            if identifier == WHOLE_FILE_ANCHOR {
                return Ok(ExtractedBlock {
                    content: text.to_string(),
                    label: Some(WHOLE_FILE_ANCHOR.to_string()),
                    title: None,
                    boundary_uncertain: false,
                });
            }
            code_block(file_name, identifier, text)
        }
    }
}

fn markdown_block(
    file_name: &str,
    identifier: &str,
    text: &str,
) -> Result<ExtractedBlock, BlockExtractionError> {
    match MarkdownAnchorExtractor::extract_block_with_metadata(text, identifier) {
        Ok((extracted, block)) => Ok(ExtractedBlock {
            content: extracted.content,
            label: Some(block.identifier),
            title: if block.title.is_empty() {
                None
            } else {
                Some(block.title)
            },
            boundary_uncertain: extracted.boundary_uncertain,
        }),
        Err(ExtractError::NotFound) => Err(BlockExtractionError::BlockNotFound {
            file_name: file_name.to_string(),
            identifier: identifier.to_string(),
        }),
        Err(ExtractError::AmbiguousDuplicate(line_numbers)) => {
            Err(BlockExtractionError::AmbiguousDuplicate {
                file_name: file_name.to_string(),
                identifier: identifier.to_string(),
                line_numbers,
            })
        }
    }
}

fn code_block(
    file_name: &str,
    identifier: &str,
    text: &str,
) -> Result<ExtractedBlock, BlockExtractionError> {
    let ext = file_extension(file_name);
    let blocks = if ext == "sh" || is_shell_script_content(text) {
        extract_shell_blocks(text, file_name)
    } else {
        CodeAnchorExtractor::default().extract(text, ext)
    };
    match blocks.iter().find(|b| b.name == identifier) {
        Some(block) => {
            let content = text.get(block.byte_range.clone()).ok_or_else(|| {
                BlockExtractionError::BlockNotFound {
                    file_name: file_name.to_string(),
                    identifier: identifier.to_string(),
                }
            })?;
            Ok(ExtractedBlock {
                content: content.to_string(),
                label: Some(block.name.clone()),
                title: None,
                boundary_uncertain: false,
            })
        }
        None => Err(BlockExtractionError::BlockNotFound {
            file_name: file_name.to_string(),
            identifier: identifier.to_string(),
        }),
    }
}

/// Deterministic drift-metadata hash over canonical extracted content: the
/// first 16 lowercase hex characters of SHA-256. Inputs are LF-normalized
/// before hashing so the fingerprint is line-ending-invariant
/// (block_extraction spec D4). It is never served as display content.
pub fn content_fingerprint(content: &str) -> String {
    let canonical: std::borrow::Cow<'_, str> = if content.contains('\r') {
        std::borrow::Cow::Owned(content.replace("\r\n", "\n").replace('\r', "\n"))
    } else {
        std::borrow::Cow::Borrowed(content)
    };
    let digest = Sha256::digest(canonical.as_bytes());
    let mut out = String::with_capacity(16);
    for byte in digest.iter().take(8) {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// File extension (without the leading dot) of a stored record identity.
pub fn file_extension(file_name: &str) -> &str {
    Path::new(file_name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
}

/// Whether source content is a shell script by shebang (`sh`/`bash`/`zsh`,
/// including `env`-mediated shebangs). Mirrors the registration-side dispatch
/// so extraction discovers exactly the anchors registration created.
pub fn is_shell_script_content(content: &str) -> bool {
    let Some(first_line) = content.lines().next().map(str::trim) else {
        return false;
    };
    let Some(shebang) = first_line.strip_prefix("#!") else {
        return false;
    };

    let mut parts = shebang.split_whitespace();
    let Some(command) = parts.next() else {
        return false;
    };
    if is_shell_command_name(command) {
        return true;
    }
    if !matches!(file_name_component(command), Some("env")) {
        return false;
    }

    for arg in parts {
        if arg == "-S" || arg.starts_with('-') {
            continue;
        }
        return is_shell_command_name(arg);
    }
    false
}

fn is_shell_command_name(raw: &str) -> bool {
    matches!(file_name_component(raw), Some("sh" | "bash" | "zsh"))
}

fn file_name_component(raw: &str) -> Option<&str> {
    Path::new(raw).file_name().and_then(|name| name.to_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use g5_contracts::types::grounding::IntentRecord;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Spy provider counting reads; `text: None` reports the file absent.
    struct SpyProvider {
        text: Option<String>,
        error: Option<String>,
        calls: AtomicUsize,
    }

    impl SpyProvider {
        fn with_text(text: &str) -> Self {
            Self {
                text: Some(text.to_string()),
                error: None,
                calls: AtomicUsize::new(0),
            }
        }
        fn absent() -> Self {
            Self {
                text: None,
                error: None,
                calls: AtomicUsize::new(0),
            }
        }
        fn failing(detail: &str) -> Self {
            Self {
                text: None,
                error: Some(detail.to_string()),
                calls: AtomicUsize::new(0),
            }
        }
        fn call_count(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl BlockSourceProvider for SpyProvider {
        async fn read_source_text(&self, _file_name: &str) -> Result<Option<String>, String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if let Some(error) = &self.error {
                return Err(error.clone());
            }
            Ok(self.text.clone())
        }
        fn describe(&self) -> String {
            "spy".to_string()
        }
    }

    const MD_FIXTURE: &str = "# Node\n\n## Section\n\n[I1. Alpha Intent]\nAlpha body line one.\nAlpha body line two.\n\n[D44a. Suffixed Decision]\nSuffixed body.\n\n[Q2b. Open Question]\nQuestion body.\n";

    // ── TA1.1 — markdown label matrix ───────────────────────────────────

    #[test]
    fn ta1_1_markdown_label_matrix() {
        for (id, want_label, want_title, want_content) in [
            (
                "I1",
                "I1",
                "Alpha Intent",
                "Alpha body line one.\nAlpha body line two.",
            ),
            ("D44a", "D44a", "Suffixed Decision", "Suffixed body."),
            ("Q2b", "Q2b", "Open Question", "Question body."),
        ] {
            let block =
                extract_block_from_text(FileType::Ontology, "spec.semlift.md", id, MD_FIXTURE)
                    .unwrap();
            assert_eq!(block.content, want_content, "content for {id}");
            assert_eq!(block.label.as_deref(), Some(want_label));
            assert_eq!(block.title.as_deref(), Some(want_title));
            assert!(
                !block.content.contains(&format!("[{id}.")),
                "label line must be stripped for {id}"
            );
        }
        // Case-insensitive lookup normalizes the stored label casing.
        let lower = extract_block_from_text(FileType::View, "v.md", "d44a", MD_FIXTURE).unwrap();
        assert_eq!(lower.label.as_deref(), Some("D44a"));
        assert_eq!(lower.content, "Suffixed body.");
    }

    // ── TA1.2 — structure hygiene ───────────────────────────────────────

    #[test]
    fn ta1_2_code_fence_lookalike_never_fakes_a_block() {
        let text = "```\n[I9. Inside Fence]\nfenced body\n```\n\n[I1. Real]\nreal body\n";
        let err = extract_block_from_text(FileType::Ontology, "s.md", "I9", text).unwrap_err();
        assert!(matches!(err, BlockExtractionError::BlockNotFound { .. }));
        let real = extract_block_from_text(FileType::Ontology, "s.md", "I1", text).unwrap();
        assert_eq!(real.content, "real body");
    }

    #[test]
    fn ta1_2_canonical_metadata_stays_on_label_and_crlf_is_normalized() {
        let text = "[D2. Guarded Decision] (Tags: human)\r\nGuarded body.\r\n";
        let block = extract_block_from_text(FileType::Ontology, "s.md", "D2", text).unwrap();
        assert_eq!(block.content, "Guarded body.");
        assert!(!block.content.contains('\r'), "content must be LF-only");
        assert!(!block.content.contains("(Tags: human)"));
        assert_eq!(block.title.as_deref(), Some("Guarded Decision"));
    }

    // ── TA1.3 — code symbols ────────────────────────────────────────────

    #[test]
    fn ta1_3_rust_impl_method_owner_qualified() {
        let src = "pub struct Service;\n\nimpl Service {\n    pub fn poll(&self) -> bool {\n        true\n    }\n}\n\npub fn free_fn() {}\n";
        let method =
            extract_block_from_text(FileType::Code, "g5d/src/service.rs", "Service::poll", src)
                .unwrap();
        assert!(method.content.contains("pub fn poll(&self) -> bool"));
        assert_eq!(method.label.as_deref(), Some("Service::poll"));
        assert!(method.title.is_none());
        let free =
            extract_block_from_text(FileType::Code, "g5d/src/service.rs", "free_fn", src).unwrap();
        assert!(free.content.contains("pub fn free_fn()"));
    }

    #[test]
    fn ta1_3_typescript_python_go_symbols() {
        let ts = "export function greet(name: string): string {\n  return name;\n}\n";
        let got = extract_block_from_text(FileType::Code, "client/src/a.ts", "greet", ts).unwrap();
        // The tree-sitter `def` capture spans the declaration node (the
        // `export` wrapper is outside it) — the same canonical range the
        // registration path has always stored.
        assert!(got.content.contains("function greet"));
        assert!(got.content.contains("return name;"));

        let py = "def hello():\n    return 1\n";
        let got = extract_block_from_text(FileType::Code, "tools/x.py", "hello", py).unwrap();
        assert!(got.content.contains("def hello()"));

        let go = "package main\n\nfunc Hello() string {\n\treturn \"hi\"\n}\n";
        let got = extract_block_from_text(FileType::Code, "cmd/main.go", "Hello", go).unwrap();
        assert!(got.content.contains("func Hello()"));
    }

    #[test]
    fn ta1_3_shell_anchors_by_extension_and_shebang() {
        let sh = "deploy() {\n  echo hi\n}\n";
        let got = extract_block_from_text(FileType::Code, "scripts/run.sh", "run.sh::deploy", sh)
            .unwrap();
        assert!(got.content.contains("deploy() {"));

        let shebang = "#!/usr/bin/env bash\nboot() {\n  echo boot\n}\n";
        let got =
            extract_block_from_text(FileType::Code, "scripts/boot", "boot::boot", shebang).unwrap();
        assert!(got.content.contains("boot() {"));
    }

    #[test]
    fn ta1_3_unsupported_extension_is_block_not_found() {
        let rb = "def hello\n  1\nend\n";
        let err = extract_block_from_text(FileType::Code, "x.rb", "hello", rb).unwrap_err();
        assert!(matches!(err, BlockExtractionError::BlockNotFound { .. }));
    }

    // ── TA1.4 — whole-file types ────────────────────────────────────────

    #[test]
    fn ta1_4_external_doc_and_file_anchor_return_whole_file() {
        let text = "# Doc\n\nWhole file body.\n";
        let doc =
            extract_block_from_text(FileType::ExternalDoc, "docs/a.md", "9bde6bad", text).unwrap();
        assert_eq!(doc.content, text);
        assert!(doc.label.is_none());

        let tera = "You are {{ agent }}.\n";
        let anchor = extract_block_from_text(
            FileType::Code,
            "g5d/src/prompts/templates/chat.tera",
            WHOLE_FILE_ANCHOR,
            tera,
        )
        .unwrap();
        assert_eq!(anchor.content, tera);
        assert_eq!(anchor.label.as_deref(), Some(WHOLE_FILE_ANCHOR));
    }

    // ── TA1.5 — error taxonomy ──────────────────────────────────────────

    #[tokio::test]
    async fn ta1_5_empty_chain_is_source_unavailable() {
        let err = extract_block(&[], FileType::Ontology, "gone.md", "I1")
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            BlockExtractionError::SourceUnavailable { detail: None, .. }
        ));
    }

    #[tokio::test]
    async fn ta1_5_provider_error_carries_detail() {
        let failing = SpyProvider::failing("permission denied");
        let providers: [&dyn BlockSourceProvider; 1] = [&failing];
        let err = extract_block(&providers, FileType::Ontology, "s.md", "I1")
            .await
            .unwrap_err();
        match err {
            BlockExtractionError::SourceUnavailable { detail, .. } => {
                assert!(detail.unwrap().contains("permission denied"));
            }
            other => panic!("expected SourceUnavailable, got {other:?}"),
        }
    }

    #[test]
    fn ta1_5_ambiguous_duplicate_carries_line_numbers() {
        let text = "[I3. First]\na\n[I3. Second]\nb\n";
        let err = extract_block_from_text(FileType::Ontology, "s.md", "I3", text).unwrap_err();
        match err {
            BlockExtractionError::AmbiguousDuplicate { line_numbers, .. } => {
                assert_eq!(line_numbers, vec![1, 3]);
            }
            other => panic!("expected AmbiguousDuplicate, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn ta1_5_chat_is_rejected_before_any_provider_read() {
        let provider = SpyProvider::with_text("irrelevant");
        let providers: [&dyn BlockSourceProvider; 1] = [&provider];
        let err = extract_block(&providers, FileType::Chat, "session", "abc12345")
            .await
            .unwrap_err();
        assert!(matches!(err, BlockExtractionError::ChatRejected));
        assert_eq!(provider.call_count(), 0, "chat must not consult providers");
    }

    // ── TA1.6 — provider-chain ordering ─────────────────────────────────

    #[tokio::test]
    async fn ta1_6_first_provider_with_text_wins() {
        let first = SpyProvider::with_text("[I1. From First]\nfirst body\n");
        let second = SpyProvider::with_text("[I1. From Second]\nsecond body\n");
        let providers: [&dyn BlockSourceProvider; 2] = [&first, &second];
        let block = extract_block(&providers, FileType::Ontology, "s.md", "I1")
            .await
            .unwrap();
        assert_eq!(block.content, "first body");
        assert_eq!(first.call_count(), 1);
        assert_eq!(
            second.call_count(),
            0,
            "later providers must not be consulted"
        );
    }

    #[tokio::test]
    async fn ta1_6_absent_falls_through_to_next_provider() {
        let first = SpyProvider::absent();
        let second = SpyProvider::with_text("[I1. Fallback]\nfallback body\n");
        let providers: [&dyn BlockSourceProvider; 2] = [&first, &second];
        let block = extract_block(&providers, FileType::Ontology, "s.md", "I1")
            .await
            .unwrap();
        assert_eq!(block.content, "fallback body");
        assert_eq!(first.call_count(), 1);
        assert_eq!(second.call_count(), 1);
    }

    #[tokio::test]
    async fn ta1_6_provider_error_stops_the_chain() {
        let first = SpyProvider::failing("explicit deletion");
        let second = SpyProvider::with_text("[I1. Resurrected]\nwrong body\n");
        let providers: [&dyn BlockSourceProvider; 2] = [&first, &second];
        let err = extract_block(&providers, FileType::Ontology, "s.md", "I1")
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            BlockExtractionError::SourceUnavailable {
                detail: Some(_),
                ..
            }
        ));
        assert_eq!(first.call_count(), 1);
        assert_eq!(second.call_count(), 0, "provider errors are terminal");
    }

    // ── TA1.7 — fingerprint determinism ─────────────────────────────────

    #[test]
    fn ta1_7_fingerprint_shape_stability_and_sensitivity() {
        let fp = content_fingerprint("canonical body");
        assert_eq!(fp.len(), 16);
        assert!(fp
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert_eq!(fp, content_fingerprint("canonical body"), "must be stable");
        assert_ne!(
            fp,
            content_fingerprint("canonical bodY"),
            "must flip on a 1-char change"
        );
    }

    #[test]
    fn ta1_7_fingerprint_is_line_ending_invariant() {
        assert_eq!(
            content_fingerprint("line one\r\nline two"),
            content_fingerprint("line one\nline two")
        );
    }

    // ── TA1.8 — serde tolerance for legacy and forward rows ─────────────

    #[test]
    fn ta1_8_legacy_row_with_content_and_unknown_content_sha_parses() {
        let legacy = r#"{"file_type":"ontology","file_name":"x.semlift.md","intent_record_number":"I1","content":"stored body","content_sha":"abc123","is_human":false}"#;
        let record: IntentRecord = serde_json::from_str(legacy).unwrap();
        assert_eq!(record.content.as_deref(), Some("stored body"));
        assert_eq!(record.intent_record_number.as_deref(), Some("I1"));
    }

    #[test]
    fn ta1_8_forward_row_with_content_fingerprint_parses() {
        let forward = r#"{"file_type":"code","file_name":"g5d/src/a.rs","code_block_name":"f","content_fingerprint":"00112233aabbccdd","is_human":false}"#;
        let record: IntentRecord = serde_json::from_str(forward).unwrap();
        assert!(record.content.is_none());
        assert_eq!(record.code_block_name.as_deref(), Some("f"));
    }

    // ── TB4.3 — canonical fingerprint shape (pinned fixture) ────────────
    //
    // The literal fingerprints below pin the canonical `content_fingerprint`
    // shape (mcp_intent_server spec D9/D10); a drift fails this test. The
    // Python mirror that once pinned the same values was retired with the
    // auxiliary MCP server.

    const PARITY_DOC: &str = "# Fixture Node\n\n[I1. Alpha Intent]\nAlpha body line one.\nAlpha body line two.\n\n[D44a. Suffixed Decision] (Tags: human)\nSuffixed body.\n\n[D2. Heading Cut]\nBody before the heading.\n\n## Trailing Section\n\nProse after the heading that must not be in D2.\n\n[Q1. Fence Guard]\nBefore fence.\n```text\n# not a heading\n```\nAfter fence.\n\n[D3. Same Line] Same-line tail body\nfollow-up line.\n";

    #[test]
    fn tb4_3_cross_surface_fingerprint_parity_fixture() {
        for (id, pinned) in [
            ("I1", "a84fa936814b0cdc"),
            ("D44a", "4aadb4c1d88f930c"),
            ("D2", "2e509b3ae4c94f2f"),
            ("Q1", "e15b20dd9003c0f5"),
            ("D3", "c2a391e103fd2f5d"),
        ] {
            let block =
                extract_block_from_text(FileType::Ontology, "fixture.semlift.md", id, PARITY_DOC)
                    .unwrap_or_else(|e| panic!("{id}: {e}"));
            assert_eq!(
                content_fingerprint(&block.content),
                pinned,
                "{id} canonical content was: {:?}",
                block.content
            );
        }
    }

    // ── Shell/shebang helper contract ───────────────────────────────────

    #[test]
    fn shebang_detection_matches_registration_dispatch() {
        assert!(is_shell_script_content("#!/bin/sh\necho hi\n"));
        assert!(is_shell_script_content(
            "#!/usr/bin/env -S bash -eu\necho hi\n"
        ));
        assert!(!is_shell_script_content(
            "#!/usr/bin/env python3\nprint(1)\n"
        ));
        assert!(!is_shell_script_content("plain text\n"));
        assert_eq!(file_extension("a/b/c.rs"), "rs");
        assert_eq!(file_extension("no_ext"), "");
    }
}
