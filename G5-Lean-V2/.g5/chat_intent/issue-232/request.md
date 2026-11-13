# Issue 232 implementation request

## Human instructions

The user asked to implement [GitHub issue 232](https://github.com/G5Labs/G5-Lean/issues/232) from the most recent `origin/main` in a separate branch and worktree. The issue requires native message/item/block identity, a shared deterministic reducer for durable transcript and streaming presentation, a stored terminal answer, deduplication of repeated final text, and accurate status for partial output. Acceptance covers dispatcher/API, canonical chat rows, timeline, browser rendering, reload/reconnect and replay. The user specified a local validation sequence, Claude CLI review after self-verification, at most six review rounds, and Cloud Rust and E2E gates on the final PR head.

## Agent interpretation

Treat completed provider units as durable output while reserving turn completion for authoritative settlement. Keep one ordered assistant transcript across the journal, canonical chat and live UI.
