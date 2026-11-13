# Issue 248 implementation review

Agent review comments, attribution and round tools described in this historical record
were retired by the [October 24 removal](chat_intent/workspace-colab-harden/remove-agent-review-comments.md). They are not supported interfaces.

## Scope and mode

Three independent read-only discovery reviews in parallel fresh contexts, then main-agent
consolidation and one repair. Each reviewer received the same request, the [saved
design](ontology-plan-issue-248.md#ontology-changes-and-governance), the ontology change, the
review skill and the uncommitted candidate against `HEAD` `a1c3e29e`. All three sessions
stalled once and were resumed only to write their reports. Raw reports are retained outside
the repository in the session scratchpad (`review-1.md`, `review-2.md`, `review-3.md`). All
three confirmed the design's recorded ontology diff matches the working tree.

## Initial findings and disposition before repair

| ID | Finding (reviewers) | Governing obligation and path | Disposition |
| --- | --- | --- | --- |
| C1, medium | The Claude harness directory is not exact: planted `.claude/CLAUDE.md`, `.claude/rules`, `.claude/agents`, `.claude/commands` and stray files survive a refresh, and Claude Code loads them on later turns (1, 3) | `g5.harness.provided`; `harness.rs::install_claude` | Fix: keep only `CLAUDE.md` and the two skill trees; regression test plants each file. |
| C2, medium | "Explain in chat" seeds lost the `explainers/` destination and dated, non-overwriting names that only the removed skill supplied (1, 2, 3) | `g5.documents.explainer.location`, `.filename`; `WorkspacePage.tsx::explainNodeChatPrompt`, `htmlDocAnswerBridge.tsx::explainDocSectionChatPrompt` | Fix: share the modal prompt's output instructions; tests assert the destination. |
| C3, low-medium | The removed-tool refusal misses the server rule `mcp__g5-tools`, and API admission accepts removed tools that only fail in the guest (1, 2, 3) | `g5.agents.admission.allowed-tools`; `policy.rs`, `agent_dispatch.rs::validate_agent_spec` | Fix: one shared predicate in `g5-contracts` used at API admission and turn start. |
| C4, medium | Stale metadata: the new block says "not yet implemented"; affected footers cite deleted profiles, skills or crates (1, 2, 3) | Metadata rule in the authoring guide | Fix: current-state footer updates only; no new blocks. |
| C5, low | Design gap list misstates spec validation (`g5d spec validate` remains), understates approval review and reviewer-side review hand-off, omits workflow-profile prompts and JSONL merge helpers; governing table omits affected blocks (1, 2, 3) | Saved design | Fix: update the design. |
| C6, low | `G5_HARNESS_DIR` export in the Docker provider environment has no consumer: host environment never reaches the guest (1, 2, 3) | Minimal implementation; `g5d-provider-env.sh` | Fix: remove the export, its test and the doc claim. |
| C7, low | The g5dev image still bakes the `@playwright/mcp` Chromium revision and `G5_BROWSER_TOOLS_AVAILABLE` for removed browser tools (1, 3) | Design "Remove"; `Dockerfile.g5d-g5dev` | Fix: remove them; keep the test-surface browser and apt set. |
| C8, low | The CLI analysis port lost its only callers; the `g5d::analysis` engine and `g5d-foundation::search` are now uncalled (1, 3) | Design "Remove"; `g5d-cli/src/ports.rs`, `g5d/src/cli_ports.rs` | Fix: remove the port. Keep both engines: an architecture guard retains local code analysis and H076 keeps agent-local search; a follow-up decision is proposed. |
| C9, low | Build-time guards accept a skill directory without `SKILL.md`, which every turn then refuses (1, 2) | `g5.runtime.bundle`; staging and Packer scripts | Fix: check every skill in staging and Packer; host readiness gap recorded in footers. |
| C10, low | Home write failures are reported as a non-retryable bundle defect (1) | `runtime.rs` harness notice | Fix: distinguish an incomplete bundle from a failed install. |
| C11, low | Provided Codex skills have no retirement path; a later rename leaves the old skill active (1, 2, 3) | `g5.harness.provided`; `harness.rs::install_codex` | Fix: record provided skill names in the Codex home and remove ones a later harness drops. |
| C12, low | Retired per-turn MCP environment files holding workspace tokens remain in retained homes (2, 3) | Security hygiene; legacy cleanup | Fix: remove them with the legacy profile cleanup. |
| C13, low | `prepare-g5-workspace.sh` lost its libkrun-free validator default (1, 3) | Retained user script | Fix: build a libkrun-free `g5d` when no validator is given. |
| C14, nit | Stale wording: `list_specs` error, review hand-off comment, `g5-code-review` doc comments, harness module doc, profile comments (1, 2, 3) | — | Fix. |
| C15, evidence | Harness loading is unverified on the pinned CLIs; the old opt-in real-CLI gate has no replacement (1, 2, 3) | Acceptance 1 | Unverified limit. Reviewers' offline probes on local Claude 2.1.245 and Codex 0.140.0 showed the four skills and the instructions loaded. |
| C16, question | A Codex `AGENTS.override.md` in the owner home replaces the provided instructions (1) | `g5.harness.provided` | Owner state; recorded as a limitation for human decision. |
| C17, question | Agents the owner starts from the workspace terminal do not get the harness (2) | `g5.harness.provided` | Recorded as a limitation for human decision. |
| C18, question | `g5.workflow.basic-chat` says "with no G5 harness"; `AGENTS.md` exempts harness tuning from review although the harness now ships (1, 2, 3) | Ontology wording and governance | Wording resolved by the user: basic chat should behave very close to chat without any harness. The harness-tuning exemption remains open. |
| C19, question | "Review with my agent" asks for decisions the agent cannot read or apply (2) | Documented gap | Human decision. |
| C20, docs | Historical guides still name retired settings and scripts (1, 3) | `docs/reference/`, `docs/relocation-validation.md` | Historical reference by repository policy; unchanged. |

## Verification available before review

Workspace check, strict all-target Clippy and rustfmt passed. Affected crate tests passed,
including the dispatcher harness cases (38/38), API core library (1012/1012), CLI commands,
architecture and API search-removal guards. Frontend type-check, changed-file lint and 358
related tests passed. Script regressions passed except three suites that fail identically on
an untouched `HEAD` copy and one that needs `G5D_BIN`.

## Repairs and rechecks

One repair applied C1–C14. C1: `harness.rs` keeps only `CLAUDE.md` and the two skill trees;
the refresh test plants rule, agent, command, memory and stray files. C2:
`explainerSaveInstructions` serves the modal and both chat seeds; tests assert the
destination and no-overwrite rule. C3: `names_removed_agent_tool` in `g5-contracts` guards
`validate_agent_spec` and `resolve_policy`, covering `mcp__g5-tools`. C4: footers of the new
block and 17 affected blocks now state current implementation and gaps. C6–C8: the Docker
export, the g5dev MCP browser bake and the CLI analysis port are removed. C9: staging and
Packer check every skill for `SKILL.md`; the staging test adds a stray-directory case. C10:
a failed home write is an ordinary retryable turn failure; only an incomplete bundle reports
`harness_unavailable`. C11: the Codex home records provided skills and retires dropped ones.
C12: retired token files are removed; the Codex dispatch test covers it. C13: the
preparation script builds a libkrun-free validator in its own target. C14: wording fixed.

Post-repair checks: strict workspace all-target Clippy, rustfmt and `git diff --check`
passed. Library tests passed for `g5-contracts` (494), `g5-api-core` (1013), `g5d-agent-turn`
(40), `g5d-cli` (215), `g5d-dispatch` (114), `g5-internal-api` (67), `g5-scm`, `g5d-worker`,
`g5d-runtime-core`, `g5-common` and `fake-provider-cli`. `g5d` dispatcher (38), CLI (50),
architecture (45), dispatch (30) and ontology audit (6) tests passed. Frontend type-check,
changed-file lint and 1606 related Vitest tests passed. Staging, launcher, provider
environment, layout and command checks passed.

## Commitment ledger after repair

| Commitment | Actual path | Check | Result or remaining gap |
| --- | --- | --- | --- |
| `g5.harness.provided` | Packaging from the repository root; `harness.rs`; `runtime.rs` launch | Harness unit tests (exact Claude directory, Codex retirement, error kinds); dispatcher Claude and Codex fixture turns | Implemented for platform-launched turns. Gaps C15–C17 and host readiness; pinned CLIs and live VMs unexecuted. |
| `g5.workflow.ontology.skill-sources` | Docker, Packer and staging copy `.agents/skills`; launch mirrors it | Byte comparisons in staging and harness tests | Preserved. |
| `g5.git.exclusions` | Harness under `$HOME/.g5/harness` and `$CODEX_HOME` | Dispatcher test compares `git status --ignored` before and after | Preserved. |
| `g5.runtime.bundle` | `/opt/g5/assets/harness` from the read-only runtime mount | Missing-harness refusal; staging and Packer guards | Preserved; readiness does not check the harness. |
| `g5.agents.admission.allowed-tools`, `.permission-policy` | Shared removed-tool rule at admission and turn start; launch flags unchanged | API and policy unit tests; runtime argv tests | Removed tools refused; other unknown Claude tool names remain unchecked. |
| `g5.quality.cutover.history`, `g5.quality.migrations` | No migration change; stored records, routes and renderers retained | Inspection | Preserved; old-record reads not executed. |
| `g5.documents.format` | Parser untouched; `spec_selector.rs` is a verbatim extraction | CLI tests | Preserved; agents now write `.g5` Markdown the editor does not read. |
| `g5.documents.explainer.location` | Shared save instructions | Vitest seed and modal tests | Restored. |
| `g5.git.network.lease`, `g5.documents.shared-checkout`, `.grounding`, `g5.governance.review-workspace` | No agent-side support remains | Footer review | Gaps recorded in footers and the design. |
| `g5.quality.delivery` | — | — | Cloud Rust and E2E not run on a PR head. |

Open for human decision: the harness-tuning exemption in C18. The other open items are
settled below.

## Follow-up decisions

The user settled the open items. C16: the Codex `AGENTS.override.md` case is an edge case;
no action. C17: terminal-started agents stay as they are, recorded as a gap in the
`g5.harness.provided` footer. C19's temporary retention of review-round tools
is superseded by the [October 24 removal](chat_intent/workspace-colab-harden/remove-agent-review-comments.md), which removes the server,
registration and credentials. C8: both
engines, `component_id_convention.rs`, the retention assertion and the dependencies only
they used are removed; `ontology_audit.rs` reads the foundation inventory directly.

The original follow-up checks applied to the historical retained-tool implementation.
Current removal checks belong to the linked removal plan.
