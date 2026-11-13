# fake-provider-cli

Fake `claude` and `codex` binaries for G5-Lean tests (plan P3.1, design v6
§8.6), plus `fake-g5-tools`, a scripted MCP server for transport tests.

- Contract, scenario format, placeholders, environment variables and frame
  shapes: `docs/lean-provider-cli-contract.md`.
- Recorded fixtures: `tests/fixtures/provider-frames/`.
- Re-record: `./record.sh [claude|codex|all]`.
- Probe native steering without credentials: `./record.sh claude-steering`.
  Add `--flush-only` for the production tool/text cases: no user replay, with
  stdin closed immediately after the steering write.
- Test: `cargo test -p fake-provider-cli`.

`fake-claude` accepts more stream-json user messages while it runs. Dynamic
scenarios consume queued input after a tool result, or start another turn when
the current scenario has no tool boundary left. `--replay-user-messages` echoes
the original content and caller UUID at consumption. Scenarios can use
`{{steered_prompt}}` in their answer to reflect consumed input. Raw recordings
containing replay frames retain their explicitly recorded sequence.

Both fakes support the bounded workspace file-write and release-gate directives
used by the shared-VM and restart journeys. Codex continues to consume one input
per process; the queue journeys select it through the atomic session-start API.

The steering recorder runs the **real pinned Claude binary** against a local,
scripted model endpoint. Its frames and timestamped stdin/stdout/request traces
cover the tool boundary, final text, native requests, background wait, stop,
resume and images. This is protocol evidence, not live-model QA. In particular,
the forced-kill recording demonstrates that a model request can contain a
message whose replay never reaches stdout; EOF alone cannot prove safe redelivery.

```sh
cargo build -p fake-provider-cli
printf '{"type":"user","message":{"role":"user","content":"hi"}}\n' \
  | G5_FAKE_SCENARIO=tests/fixtures/provider-frames/claude/text-only.jsonl \
    target/debug/fake-claude -p --output-format stream-json --input-format stream-json --verbose \
      --session-id 6f1c2b6e-0f0e-4b9a-9d0c-1a2b3c4d5e6f
```

`fake-codex` serves only `codex app-server --listen stdio://` and refuses `codex exec`.
A session needs the stable JSON-RPC handshake and must name the started thread in
`turn/start`; `tests/cli.rs::run_codex` and the dispatcher's
`tests/fixtures/codex-app-server/items_peer.py` show the exchange.
