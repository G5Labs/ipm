//! Integration tests for markdown I/D/Q block extraction.
//!
//! These exercise `MarkdownAnchorExtractor` against realistic, multi-section
//! spec files that combine YAML frontmatter, nested headings, code fences,
//! HTML comments, and varied I/D/Q label formatting.  They complement the
//! inline unit tests in `markdown_parser.rs` with larger, more representative
//! inputs drawn from real-world `.semlift.md` patterns.
//!
//! Test cases: TC-PRS-001 through TC-PRS-006 from
//! `views/test_views/ontology_grounding.test_view.md` §5.

use g5_semantic::parsing::anchor_types::ExtractError;
use g5_semantic::parsing::MarkdownAnchorExtractor;

// ─────────────────────────────────────────────────────────────────────────────
// TC-PRS-001: Real-world ontology file with mixed constructs
// ─────────────────────────────────────────────────────────────────────────────

/// A realistic `.semlift.md` file condensed into a single string.
const REAL_WORLD_SEMLIFT: &str = r#"---
component_id: g5/g5-api/auth
parent_spec_id: g5/g5-api
child_spec_ids:
  - g5/g5-api/auth/jwt
  - g5/g5-api/auth/oauth
dependencies:
  - g5/g5-common/types
source_files:
  - g5-api/src/auth/mod.rs
  - g5-api/src/auth/jwt.rs
language: rust
complexity: moderate
---

# Auth Module

The auth module provides authentication and authorization for the G5 API.
It supports JWT-based bearer tokens and OAuth2 code-exchange flows.

## Boundary

[I1. Authentication Scope]
This module handles JWT-based authentication for all API endpoints.
Requests without a valid bearer token receive `401 Unauthorized`.

It does NOT handle:
- Rate limiting (see `g5-api/middleware`)
- Role-based access control (see `g5-authz`)

## Behavior

[I2. Token Validation]
Tokens are validated using HMAC-SHA256 with a server-side secret.
The validation pipeline:
1. Extract `Authorization: Bearer <token>` header.
2. Decode and verify the JWT signature.
3. Check `exp` claim — reject if expired.
4. Extract `sub` (user ID) and `org_id` from claims.

[D1. Refresh Token Strategy]
Refresh tokens use rotating key pairs.  When a refresh token is used,
the old token is invalidated and a new pair (access + refresh) is issued.

**Rationale:** Rotating tokens limit the blast radius of a stolen refresh
token — the attacker gets at most one use.

### Implementation Notes

[I3. Key Rotation Interval]
Keys rotate every 24 hours.  The rotation schedule is driven by a
background Tokio task that:
- Generates a new ECDSA P-256 key pair.
- Stores it in the `auth_keys` table with `active_from` timestamp.
- Keeps the previous key valid for 48h (grace period for in-flight tokens).

<!-- [I99. This is inside an HTML comment and must be ignored by the parser] -->

```rust
// Example code snippet showing token validation — labels inside fences
// are NOT real I/D blocks:
// [I50. This I/D label is inside a code fence]
fn validate_token(token: &str) -> Result<Claims, AuthError> {
    let decoded = decode::<Claims>(token, &DECODING_KEY, &Validation::default())?;
    Ok(decoded.claims)
}
```

```sql
-- SQL for key rotation (also inside a code fence)
-- [D99. This should also be ignored]
INSERT INTO auth_keys (key_id, public_key, private_key, active_from)
VALUES ($1, $2, $3, NOW());
```

[Q1. Should we support OAuth2 PKCE for public clients?]
Open question for future iteration.  PKCE is required by RFC 7636 for
native/mobile apps.  If we add it, we need a `code_verifier` column
on the `oauth_sessions` table.

## Edge Cases

[D2. Rate Limit Handling for Auth Endpoints]
Auth endpoints (`/api/v1/auth/token`, `/api/v1/auth/refresh`) have their
own rate limits separate from the global API rate limiter:
- Token endpoint: 10 req/min per IP.
- Refresh endpoint: 5 req/min per token family.

Sliding-window counters stored in Redis (`auth:ratelimit:{ip}:{endpoint}`).

## Testing Contract

[I4. Auth Module Testing Boundary]
Unit tests mock the database and Redis.  Integration tests use `g5-e2e`
with a real Postgres and Redis stack.

Given a valid JWT → handler returns 200 with user context populated.
Given an expired JWT → handler returns 401.
Given a missing header → handler returns 401.

[D3. Test Token Generation]
Test tokens are generated with a fixed secret (`test-secret-key-0000`)
and a 1-hour expiry.  The test helper `create_test_token(user_id, org_id)`
lives in `g5-common/src/test_utils.rs`.
"#;

#[test]
fn tc_prs_001_extracts_correct_block_set() {
    let result = MarkdownAnchorExtractor::extract(REAL_WORLD_SEMLIFT);

    let ids: Vec<&str> = result
        .blocks
        .iter()
        .map(|b| b.identifier.as_str())
        .collect();

    // Must extract these 8 blocks
    assert_eq!(
        ids,
        vec!["I1", "I2", "D1", "I3", "Q1", "D2", "I4", "D3"],
        "Unexpected block set"
    );

    // I99 (HTML comment) and I50, D99 (code fences) must NOT appear
    assert!(!ids.contains(&"I99"), "HTML comment block must be excluded");
    assert!(!ids.contains(&"I50"), "Code fence block must be excluded");
    assert!(
        !ids.contains(&"D99"),
        "SQL code fence block must be excluded"
    );

    assert!(result.warnings.is_empty(), "No warnings expected");
}

#[test]
fn tc_prs_001_block_titles() {
    let result = MarkdownAnchorExtractor::extract(REAL_WORLD_SEMLIFT);

    assert_eq!(result.blocks[0].title, "Authentication Scope");
    assert_eq!(result.blocks[1].title, "Token Validation");
    assert_eq!(result.blocks[2].title, "Refresh Token Strategy");
    assert_eq!(result.blocks[3].title, "Key Rotation Interval");
    assert_eq!(
        result.blocks[4].title,
        "Should we support OAuth2 PKCE for public clients?"
    );
    assert_eq!(
        result.blocks[5].title,
        "Rate Limit Handling for Auth Endpoints"
    );
}

#[test]
fn tc_prs_001_byte_ranges_no_overlap() {
    let result = MarkdownAnchorExtractor::extract(REAL_WORLD_SEMLIFT);

    for i in 0..result.blocks.len() - 1 {
        assert!(
            result.blocks[i].byte_range.end <= result.blocks[i + 1].byte_range.start,
            "Blocks {} and {} overlap: {:?} vs {:?}",
            result.blocks[i].identifier,
            result.blocks[i + 1].identifier,
            result.blocks[i].byte_range,
            result.blocks[i + 1].byte_range,
        );
    }
}

#[test]
fn tc_prs_001_q_block_type() {
    let result = MarkdownAnchorExtractor::extract(REAL_WORLD_SEMLIFT);
    let q_block = result.blocks.iter().find(|b| b.identifier == "Q1").unwrap();
    assert!(q_block.identifier.starts_with('Q'));
}

#[test]
fn tc_prs_001_i3_under_subheading() {
    let result = MarkdownAnchorExtractor::extract(REAL_WORLD_SEMLIFT);
    let i3 = result.blocks.iter().find(|b| b.identifier == "I3").unwrap();
    // I3 is under "### Implementation Notes" and should have content about
    // key rotation, not the heading text itself.
    let content = MarkdownAnchorExtractor::extract_block(REAL_WORLD_SEMLIFT, "I3").unwrap();
    assert!(
        content.content.contains("rotate every 24 hours"),
        "I3 content should mention rotation: {:?}",
        content.content
    );
    // I3 byte range must end before the code fence labels
    assert!(
        i3.byte_range.end <= REAL_WORLD_SEMLIFT.len(),
        "I3 byte range should be within file"
    );
}

#[test]
fn tc_prs_001_content_boundaries() {
    // I2 content should end where D1 begins
    let result = MarkdownAnchorExtractor::extract(REAL_WORLD_SEMLIFT);
    let i2 = &result.blocks[1]; // I2
    let d1 = &result.blocks[2]; // D1
    assert_eq!(i2.byte_range.end, d1.byte_range.start);

    // D3 (last block) ends at or near EOF
    let _d3 = result.blocks.last().unwrap();
    // Content should include the test helper mention
    let d3_content = MarkdownAnchorExtractor::extract_block(REAL_WORLD_SEMLIFT, "D3").unwrap();
    assert!(d3_content.content.contains("create_test_token"));
}

// ─────────────────────────────────────────────────────────────────────────────
// TC-PRS-002: Duplicate identifiers
// ─────────────────────────────────────────────────────────────────────────────

const DUPLICATE_FILE: &str = r#"# Module

## Section A

[I3. First occurrence of I3]
This is the original I3 block with some authentication logic
that validates user tokens.

## Section B

[I3. Second occurrence of I3]
This is a duplicate by accident — same identifier, different content.
It describes a completely different concern.

[I1. Unique block]
This block has a unique identifier and should always be extracted.

[D1. A decision]
Decided to use SHA-256 for hashing.

[D1. Duplicate decision]
This D1 is also a duplicate.
"#;

#[test]
fn tc_prs_002_both_duplicates_parsed() {
    let result = MarkdownAnchorExtractor::extract(DUPLICATE_FILE);
    let i3_blocks: Vec<_> = result
        .blocks
        .iter()
        .filter(|b| b.identifier == "I3")
        .collect();
    assert_eq!(i3_blocks.len(), 2, "Both I3 occurrences should be parsed");
    assert_eq!(i3_blocks[0].title, "First occurrence of I3");
    assert_eq!(i3_blocks[1].title, "Second occurrence of I3");
}

#[test]
fn tc_prs_002_warnings_emitted() {
    let result = MarkdownAnchorExtractor::extract(DUPLICATE_FILE);
    assert!(
        !result.warnings.is_empty(),
        "Duplicate I3 should produce a warning"
    );
    let has_i3_warning = result.warnings.iter().any(|w| w.contains("I3"));
    assert!(has_i3_warning, "Warning should mention I3");
    let has_d1_warning = result.warnings.iter().any(|w| w.contains("D1"));
    assert!(has_d1_warning, "Warning should mention D1");
}

#[test]
fn tc_prs_002_extract_block_returns_ambiguous_error() {
    let result = MarkdownAnchorExtractor::extract_block(DUPLICATE_FILE, "I3");
    assert!(
        matches!(result, Err(ExtractError::AmbiguousDuplicate(_))),
        "extract_block(\"I3\") should return AmbiguousDuplicate, got {:?}",
        result
    );
}

#[test]
fn tc_prs_002_unique_blocks_still_extractable() {
    let content = MarkdownAnchorExtractor::extract_block(DUPLICATE_FILE, "I1").unwrap();
    assert!(content.content.contains("unique identifier"));
}

// ─────────────────────────────────────────────────────────────────────────────
// TC-PRS-003: Adjacent blocks with no content between them
// ─────────────────────────────────────────────────────────────────────────────

const ADJACENT_BLOCKS: &str = "\
[I1. First Intent]
[I2. Second Intent]
[I3. Third Intent]
[D1. First Decision]
Some actual content here that belongs to D1.
It spans multiple lines.
[D2. Second Decision]
[Q1. Open Question]
Content for the question block.
";

#[test]
fn tc_prs_003_all_blocks_extracted() {
    let result = MarkdownAnchorExtractor::extract(ADJACENT_BLOCKS);
    let ids: Vec<&str> = result
        .blocks
        .iter()
        .map(|b| b.identifier.as_str())
        .collect();
    assert_eq!(ids, vec!["I1", "I2", "I3", "D1", "D2", "Q1"]);
}

#[test]
fn tc_prs_003_empty_content_blocks() {
    let i1 = MarkdownAnchorExtractor::extract_block(ADJACENT_BLOCKS, "I1").unwrap();
    assert!(
        i1.content.trim().is_empty(),
        "I1 should have empty content, got: {:?}",
        i1.content
    );

    let i2 = MarkdownAnchorExtractor::extract_block(ADJACENT_BLOCKS, "I2").unwrap();
    assert!(
        i2.content.trim().is_empty(),
        "I2 should have empty content, got: {:?}",
        i2.content
    );

    let d2 = MarkdownAnchorExtractor::extract_block(ADJACENT_BLOCKS, "D2").unwrap();
    assert!(
        d2.content.trim().is_empty(),
        "D2 should have empty content, got: {:?}",
        d2.content
    );
}

#[test]
fn tc_prs_003_d1_has_content() {
    let d1 = MarkdownAnchorExtractor::extract_block(ADJACENT_BLOCKS, "D1").unwrap();
    assert!(
        d1.content.contains("Some actual content here"),
        "D1 should have content: {:?}",
        d1.content
    );
    assert!(d1.content.contains("spans multiple lines"));
}

#[test]
fn tc_prs_003_byte_ranges_non_overlapping() {
    let result = MarkdownAnchorExtractor::extract(ADJACENT_BLOCKS);
    for i in 0..result.blocks.len() - 1 {
        assert!(
            result.blocks[i].byte_range.end <= result.blocks[i + 1].byte_range.start,
            "Blocks {} and {} overlap",
            result.blocks[i].identifier,
            result.blocks[i + 1].identifier
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// TC-PRS-004: Varied label formatting
// ─────────────────────────────────────────────────────────────────────────────

const VARIED_LABELS: &str = "\
[I1. Simple label]
Content A — standard format.

[D2.Complex label without space after period]
Content B — no space after period.

[I3.  Extra spaces in label]
Content C — extra whitespace in title.

[Q10. Very high number]
Content D — double-digit number.

[I100. Three-digit number]
Content E — triple-digit number.

[i5. lowercase prefix]
Content F — lowercase letter normalized to I5.

[d20. another lowercase]
Content G — lowercase d normalized to D20.
";

#[test]
fn tc_prs_004_all_blocks_extracted() {
    let result = MarkdownAnchorExtractor::extract(VARIED_LABELS);
    let ids: Vec<&str> = result
        .blocks
        .iter()
        .map(|b| b.identifier.as_str())
        .collect();
    assert_eq!(ids, vec!["I1", "D2", "I3", "Q10", "I100", "I5", "D20"]);
}

#[test]
fn tc_prs_004_titles_trimmed() {
    let result = MarkdownAnchorExtractor::extract(VARIED_LABELS);
    assert_eq!(result.blocks[0].title, "Simple label");
    assert_eq!(
        result.blocks[1].title,
        "Complex label without space after period"
    );
    assert_eq!(result.blocks[2].title, "Extra spaces in label");
    assert_eq!(result.blocks[3].title, "Very high number");
    assert_eq!(result.blocks[4].title, "Three-digit number");
}

#[test]
fn tc_prs_004_case_normalization() {
    let result = MarkdownAnchorExtractor::extract(VARIED_LABELS);
    // Lowercase letters should be normalized to uppercase
    assert!(
        result.blocks.iter().any(|b| b.identifier == "I5"),
        "Lowercase i5 should become I5"
    );
    assert!(
        result.blocks.iter().any(|b| b.identifier == "D20"),
        "Lowercase d20 should become D20"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// TC-PRS-005: Nested code fences and edge-case exclusions
// ─────────────────────────────────────────────────────────────────────────────

const NESTED_FENCES: &str = "\
# Module Spec

[I1. Before code fences]
This block exists before any code fence.

````markdown
# This is a markdown example inside a 4-backtick fence

[I2. This label is inside a nested code fence]

```rust
fn inner() {
    // [I3. Even deeper nesting]
}
```
````

[I4. Between fences]
Content between the two fence blocks.

```python
# [D1. Inside a Python code fence]
def helper():
    pass
```

```
[Q99. Inside a bare code fence with no language]
```

[D2. After all fences]
Final content block.
";

#[test]
fn tc_prs_005_only_outside_fence_blocks_extracted() {
    let result = MarkdownAnchorExtractor::extract(NESTED_FENCES);
    let ids: Vec<&str> = result
        .blocks
        .iter()
        .map(|b| b.identifier.as_str())
        .collect();
    assert_eq!(ids, vec!["I1", "I4", "D2"]);
    // None of the code-fence labels should appear
    assert!(!ids.contains(&"I2"));
    assert!(!ids.contains(&"I3"));
    assert!(!ids.contains(&"D1"));
    assert!(!ids.contains(&"Q99"));
}

#[test]
fn tc_prs_005_content_accuracy() {
    let i1 = MarkdownAnchorExtractor::extract_block(NESTED_FENCES, "I1").unwrap();
    assert!(i1.content.contains("before any code fence"));

    let i4 = MarkdownAnchorExtractor::extract_block(NESTED_FENCES, "I4").unwrap();
    assert!(i4.content.contains("between the two fence blocks"));

    let d2 = MarkdownAnchorExtractor::extract_block(NESTED_FENCES, "D2").unwrap();
    assert!(d2.content.contains("Final content block"));
}

// ─────────────────────────────────────────────────────────────────────────────
// TC-PRS-006: Extract specific block content
// ─────────────────────────────────────────────────────────────────────────────

const MULTI_BLOCK_FILE: &str = "\
# Design Document

[I1. Service Architecture]
The service uses a layered architecture:
- Handler layer (HTTP)
- Service layer (business logic)
- Repository layer (database)

[I2. Error Handling Strategy]
All errors are converted to a unified `AppError` enum at the handler
boundary.  Internal errors (database failures, IO) produce 500 with
a correlation ID.  Validation errors produce 400 with structured details.

Multi-line content continues here — this paragraph is still part of I2.

[D1. Database Choice]
PostgreSQL 14+ with pgvector for embedding storage.

[I3. Caching Layer]
Redis is used as a write-through cache for hot data:
- Workspace metadata: TTL 5 min
- User sessions: TTL 1 hour
- Agent context: TTL matches task lifetime
";

#[test]
fn tc_prs_006_extract_i2_content() {
    let extracted = MarkdownAnchorExtractor::extract_block(MULTI_BLOCK_FILE, "I2").unwrap();
    assert!(
        extracted.content.contains("unified `AppError` enum"),
        "Should contain AppError mention"
    );
    assert!(
        extracted.content.contains("Multi-line content continues"),
        "Should include continuation paragraph"
    );
    // Must NOT contain content from I1 or D1
    assert!(!extracted.content.contains("layered architecture"));
    assert!(!extracted.content.contains("PostgreSQL 14+"));
}

#[test]
fn tc_prs_006_extract_nonexistent_block() {
    let result = MarkdownAnchorExtractor::extract_block(MULTI_BLOCK_FILE, "I99");
    assert!(
        matches!(result, Err(ExtractError::NotFound)),
        "Non-existent block should return NotFound"
    );
}

#[test]
fn tc_prs_006_preserves_whitespace_and_newlines() {
    let extracted = MarkdownAnchorExtractor::extract_block(MULTI_BLOCK_FILE, "I1").unwrap();
    // Content should preserve the bullet list structure
    assert!(extracted.content.contains("- Handler layer"));
    assert!(extracted.content.contains("- Service layer"));
    assert!(extracted.content.contains("- Repository layer"));
}

#[test]
fn tc_prs_006_last_block_extends_to_eof() {
    let extracted = MarkdownAnchorExtractor::extract_block(MULTI_BLOCK_FILE, "I3").unwrap();
    assert!(extracted
        .content
        .contains("Agent context: TTL matches task lifetime"));
}

// ─────────────────────────────────────────────────────────────────────────────
// Integration: Large realistic spec with all constructs combined
// ─────────────────────────────────────────────────────────────────────────────

const LARGE_REALISTIC_SPEC: &str = r#"---
component_id: g5/g5d/agent
parent_spec_id: g5/g5d
child_spec_ids:
  - g5/g5d/agent/loop
  - g5/g5d/agent/tools
  - g5/g5d/agent/context
dependencies:
  - g5/g5-common/types
  - g5/g5-common/files
source_files:
  - g5d/src/agent/mod.rs
  - g5d/src/agent/loop.rs
  - g5d/src/agent/context.rs
language: rust
complexity: high
---

# Agent Module

The agent module implements the autonomous coding agent that executes tasks
within an isolated microVM.  It orchestrates the tool-calling loop, maintains
conversation context, and enforces safety boundaries.

## Boundary

[I1. Agent Lifecycle Scope]
An agent instance is created per-task.  It:
- Receives a `TaskContext` with workspace path, spec references, and tool set.
- Runs a tool-calling loop until the LLM emits a `finish` tool call or a
  hard timeout is reached.
- Reports results (files changed, tests run, errors) back to the API.

It does NOT:
- Manage its own microVM (that's `g5d/worker`).
- Persist state across tasks (each task starts fresh).

## Core Loop

[I2. Tool-Calling Loop Invariants]
The tool loop follows a strict turn-based protocol:

1. **System prompt** → constructed from `TaskContext` + active skills.
2. **User message** → the task instruction.
3. **Assistant turn** → LLM returns text + zero or more tool calls.
4. **Tool execution** → each tool call is dispatched, results appended.
5. **Repeat** from step 3 until `finish` or max turns reached.

**Invariants:**
- Tool calls are executed **sequentially** — no parallel execution.
- Each tool result is validated before appending to conversation.
- If a tool returns an error, the error text is sent as the tool result
  (the LLM can decide to retry or move on).

[D1. Max Turn Limit]
Default: 50 turns.  Configurable via `agent.max_turns` in task config.

Rationale: Prevents runaway agents from consuming unbounded LLM tokens.
50 turns covers most realistic tasks (average completion: 8-15 turns).

### Context Management

[I3. Context Window Strategy]
The agent uses a sliding-window strategy for context management:

- **Priority 1:** System prompt + current tool results (always retained).
- **Priority 2:** Recent conversation turns (last N turns, configurable).
- **Priority 3:** Summarized older turns (compressed by the LLM).

When the context window reaches 80% capacity, older turns are compressed.

```rust
// Internal implementation detail — NOT an I/D block
// [I88. This is inside a code fence and should be ignored]
pub struct ContextWindow {
    turns: VecDeque<Turn>,
    max_tokens: usize,
    compression_threshold: f64,
}
```

[D2. Compression Threshold]
Compress at 80% of `max_tokens`.  This leaves ~20% headroom for the next
assistant response + tool results without needing immediate re-compression.

[I4. Tool Dispatch Architecture]
Tools are registered as `Box<dyn Tool>` implementations.  The dispatcher
matches the tool name from the LLM's function call to the registered tool
and invokes it with the parsed arguments.

Built-in tools:
- `read_file`, `write_file`, `search`, `run_command`
- `create_intent`, `resolve_edge`, `register_all` (grounding tools)
- `finish` (terminates the loop)

## Error Handling

[D3. Tool Execution Timeout]
Each tool invocation has a 30-second timeout.  If exceeded, the tool
returns a timeout error result to the LLM.

[I5. Error Recovery Strategy]
On tool error:
1. Error message is formatted as the tool's result text.
2. The agent continues to the next LLM turn (does not abort).
3. After 3 consecutive tool errors, a warning is injected into the
   system prompt: "Multiple tool failures detected, consider alternative."

<!-- 
[D99. This entire block is inside an HTML comment]
This should NOT be extracted by the parser.  It's a scratchpad for
the spec author's notes that don't belong in the parsed output.
-->

[Q1. Should agents self-evaluate before finishing?]
Proposal: require the agent to run a "self-check" tool before calling
`finish`.  This would catch common mistakes (e.g., file not saved,
tests not run) but adds 1-2 extra turns per task.

## Testing Contract

[I6. Agent Module Testing Boundary]
Given a `TaskContext` with a simple "create file" instruction →
  the agent should produce exactly one `write_file` tool call and one `finish`.
Given a tool that returns an error →
  the agent should retry at least once before giving up.
Given max turns reached →
  the agent should emit a timeout result (not crash).
"#;

#[test]
fn large_spec_extracts_all_blocks() {
    let result = MarkdownAnchorExtractor::extract(LARGE_REALISTIC_SPEC);
    let ids: Vec<&str> = result
        .blocks
        .iter()
        .map(|b| b.identifier.as_str())
        .collect();
    assert_eq!(
        ids,
        vec!["I1", "I2", "D1", "I3", "D2", "I4", "D3", "I5", "Q1", "I6"]
    );
}

#[test]
fn large_spec_excludes_comment_and_fence_blocks() {
    let result = MarkdownAnchorExtractor::extract(LARGE_REALISTIC_SPEC);
    let ids: Vec<&str> = result
        .blocks
        .iter()
        .map(|b| b.identifier.as_str())
        .collect();
    assert!(!ids.contains(&"I88"), "Code fence I88 must be excluded");
    assert!(!ids.contains(&"D99"), "HTML comment D99 must be excluded");
}

#[test]
fn large_spec_no_warnings() {
    let result = MarkdownAnchorExtractor::extract(LARGE_REALISTIC_SPEC);
    assert!(
        result.warnings.is_empty(),
        "No duplicates in this file: {:?}",
        result.warnings
    );
}

#[test]
fn large_spec_byte_ranges_contiguous_and_non_overlapping() {
    let result = MarkdownAnchorExtractor::extract(LARGE_REALISTIC_SPEC);
    for i in 0..result.blocks.len() - 1 {
        assert!(
            result.blocks[i].byte_range.end <= result.blocks[i + 1].byte_range.start,
            "{}→{} byte ranges overlap",
            result.blocks[i].identifier,
            result.blocks[i + 1].identifier
        );
    }
}

#[test]
fn large_spec_content_extraction_accuracy() {
    let i2 = MarkdownAnchorExtractor::extract_block(LARGE_REALISTIC_SPEC, "I2").unwrap();
    assert!(i2.content.contains("strict turn-based protocol"));
    assert!(i2.content.contains("Tool calls are executed"));

    let d1 = MarkdownAnchorExtractor::extract_block(LARGE_REALISTIC_SPEC, "D1").unwrap();
    assert!(d1.content.contains("50 turns"));

    let i5 = MarkdownAnchorExtractor::extract_block(LARGE_REALISTIC_SPEC, "I5").unwrap();
    assert!(i5.content.contains("3 consecutive tool errors"));

    let q1 = MarkdownAnchorExtractor::extract_block(LARGE_REALISTIC_SPEC, "Q1").unwrap();
    assert!(q1.content.contains("self-check"));
}
