# PR #335 implementation review

Reviewed starting commit `7bf701c5778774a34a593cbf82712308ba40c260` against
merge base `5af3b761`, on authorized branch `add-gcloud-to-image`, 2026-10-07.
Fresh reviewer context, with no implementation history. The workspace PR description
and saved design (including its implementation record and known verification gaps)
were supplied during discovery, before findings; this is not a blind detection exercise.
Governing input: [saved design](ontology-plan.md#ontology-changes-and-governance)
and [original request](../../chat_intent/add-gcloud-to-image/runtime-tools-on-path.md).

## Initial findings, before repairs

- **F1 — stale guidance (P3):** the three changed `.g5/memory/claude/` files still
  describe excluding the bundle from PATH as current intentional behavior. That
  contradicts this PR and `g5.runtime.bundle.tools`, and directs future agents toward
  workarounds. Scope the observation to older guests and describe the new append.
- **E1 — missing runtime evidence:** the added tests verify PATH strings and the
  local-fixture branch, but do not execute the mounted-binary gate or prove tool
  lookup in a terminal or agent shell. This is an evidence gap, not a demonstrated
  implementation defect. The saved libkrun blocker is avoidable for process tests:
  `cargo test -p g5d --no-default-features --test dispatch
  terminal_runs_g5d_pr_directly_with_workspace_context_and_preserves_errors -- --exact`
  passed during this review, including actual PTY/CLI execution, image Git precedence,
  launcher selection, loader isolation and reserved workspace-token refusal.
- No functional defect established by initial source inspection. `process_environment`
  reaches terminal setup and module launch; agent-turn removes the native loader paths
  and prepends its own launcher; preview appends before applying its explicit env.

Initial checks: diff whitespace check passed. The image Cargo shim has no default
toolchain; `/opt/g5/bin/cargo` works. A broader-than-required `g5d-cli` lib run returned
256 passes and three failures in unchanged doctor/push tests; targeted suites and
failure classification are pending. CI was pending at the reviewed commit.

## Commitment inventory and evidence

| Commitment | Actual path | Counterexample/check | Result or gap |
|---|---|---|---|
| `g5.runtime.bundle.tools`: tools reachable in both shell surfaces | `GitRuntime::install` / `process_environment` → terminal manager and module runner → provider environment | Mounted-binary gate must execute; run the five tool version commands through terminal and agent shell | Passed mounted-path process probe through real PTY and both controlled provider fixtures; VM boot and live models untested |
| Image tools win; G5 launcher wins over raw bundle binary | `command_path`, `append_path`, provider `prepend_path` | Missing bundle tools fall back; colliding image executable and `g5d` shim execute first | 110 dispatcher and 15 core unit tests pass; mounted probe executes bundle tools, image node and launcher precedence |
| Lookup does not expose platform libraries | Terminal setup scrubs loader keys; agent-turn takes loader paths into shim; preview customer scrub | Provider/customer env lacks native loader keys, shim alone supplies them | Real terminal, both mounted provider fixtures and 20 preview tests pass |
| `g5.runtime.bundle`: read-only bundle and existing readiness stay intact | Existing host mount/entrypoint validation; no staging changes | No copying or writes to runtime bundle, no new readiness dependency | Source inspection passes; full mount/readiness closure outside this diff |
| `g5.runtime.workspace-cli`: standard-path launcher and local fixtures preserved | Existing `/usr/local/bin/g5d` installation reuses the same executable gate | Non-bundle test binary must not install a system launcher; inherited image path retained | Local fixture unit test passes; mounted gate installs the standard-path shim; bash -lc finds gcloud on this image; arbitrary image profiles remain untested |
| `g5.security.credentials.reserved-env`: tenant env cannot replace runtime PATH | Terminal admission and task-context reserved-key filter | Reject task PATH overrides; do not confuse explicit preview app env with task environment | Existing guard inspected; no admission changes |
| `g5.preview.service` and saved design: preview fallback once; explicit preview PATH wins | `customer_command` appends, then applies `preview.env` | Inherited bundle entry not duplicated; customer PATH replaces fallback | 20 preview tests pass, including idempotence, explicit PATH override and loader scrubbing |
| `g5.design.minimal-mechanism` | One shared constant/helper; one stored bundle-identity result | No per-tool allowlist, forwarders or new persistent state | Source inspection passes |

Bounded reverse pass: inventory includes the changed rule's image/minimal-image cases,
shim and library exceptions, plus the unchanged bundle read-only/no-copy constraint,
local fixture guard and preview override ordering. Lifecycle, migration and live-access
authority paths are unchanged; their wider links are outside this review. No governing
contract amendment is proposed.

## Repairs and final verification

F1 fixed in all three memory files. Guidance now distinguishes updated runtimes from
older already-running guests and points to the authoritative runtime contract. The
blanket statement that every executable needs no loader setup now explicitly preserves
`g5d`'s launcher requirement. No production Rust change was needed.

Executed on the reviewed production code:

- `cargo test -p g5d-runtime-core -p g5d-dispatch --lib`: **15 + 110 passed**;
  the separate root-to-UID-1000 core test was ignored by its existing root requirement.
- `cargo test -p g5d-cli --lib preview_serve::tests`: **20 passed**.
- `cargo test -p g5d --no-default-features --test dispatch
  terminal_runs_g5d_pr_directly_with_workspace_context_and_preserves_errors -- --exact`:
  **1 passed**.
- The `dispatch_modules` tests
  `agent_turn_claude_reads_the_provided_harness_outside_the_checkout` and
  `agent_turn_codex_rejects_interactivity_before_spawn_and_runs_with_stdin_eof`, using
  the same `--no-default-features` build: **both passed**.
- The mounted-path variants of those same three public-path tests: **all passed**,
  including all five actual bundled tools and image `node` precedence (method below).
- `git diff --check`: passed. Review repairs only affect Markdown; the implementing
  record's Rust formatting/lint evidence remains applicable.

The broader CLI lib run was **256 passed / 3 failed**. These failures occur in files
byte-identical to the comparison base: doctor expects nested-virtualization information
when built without libkrun, and two managed-push tests attempt file-origin pushes
without the required runtime relay. They do not call the changed PATH composition.
No baseline rerun was performed, so this is source-based classification, not a passing
whole-suite claim. They remain outside this PR's repair scope.

### Mounted-path probe method

This is runtime process evidence on the existing x86_64 guest, not a new VM boot or
live-model test. No installed VM runtime was replaced. A private `unshare -Ur -m`
namespace bound the freshly built `target/debug/g5d` over `/opt/g5/bin/g5d` and a
private copy over `/usr/local/bin` (the standard-path shim is written there). The
namespace inherited the existing read-only bundle for all the real shell tools.
The test process started with `PATH=/usr/local/bin:/usr/bin:/bin` and neither loader
variable, so the append was necessary for success.

Temporary copies of `dispatch.rs` and `dispatch_modules.rs` selected
`/opt/g5/bin/g5d` instead of `CARGO_BIN_EXE_g5d`. The existing PTY command and controlled
Claude/Codex provider shell fixtures ran the following before their `g5d pr --help`
and mock-API assertions. The copied tests were compiled with `--no-default-features`,
run individually inside the namespace, and removed afterwards. All existing success
assertions were retained. Test durations were 2.39 s (terminal), 4.49 s (Claude fixture)
and 4.60 s (Codex fixture).

```sh
if env | grep -q '^LD_LIBRARY_PATH='; then exit 61; fi
if env | grep -q '^DYLD_LIBRARY_PATH='; then exit 62; fi
for tool in gcloud gh kubectl gke-gcloud-auth-plugin cargo-nextest; do
  test "$(command -v "$tool")" = "/opt/g5/bin/$tool"
done
gcloud --version >/dev/null
gh --version >/dev/null
kubectl version --client >/dev/null
gke-gcloud-auth-plugin --version >/dev/null
cargo-nextest --version >/dev/null
test "$(command -v node)" = /usr/local/bin/node
test "$(command -v g5d)" != /opt/g5/bin/g5d
test "$(/bin/bash -lc 'command -v gcloud')" = /opt/g5/bin/gcloud
```

The image node collision, raw-g5d exclusion, explicit tool paths and successful
version commands discriminate missing append and accidental prepend. Explicit nonzero
exits reject loader variables; negated pipelines alone would not do so under `set -e`.
The provider fixtures exercise actual dispatcher → agent-turn → child-shell execution;
they do not establish how live Claude/Codex models choose or snapshot shell commands.

## Remaining gaps

| Governing ontology (or N/A) | Gap | Proposed action | Uncertainty / alternatives / blockers |
|---|---|---|---|
| `g5.runtime.bundle.tools`, `g5.runtime.workspace-cli` | Fresh VM boot, live provider shells, arm64, arbitrary customer login profiles and minimal images not exercised | Run the workspace-VM lane and architecture/image smoke cases | No `/dev/kvm` in this guest; mounted-path probe covers production gate/process composition on existing x86_64 image only |
| N/A | Three failures in the broader CLI lib suite | Diagnose under the separate doctor/push owners | Unchanged source; no baseline execution during this review |
| N/A | Provider CI still pending at the reviewed code commit | Await published checks | Observed checks are not merge approval |

Implementation review is complete with no identified functional defect. F1 is repaired;
E1 is narrowed by executed mounted-path evidence, with the deployment/live-model limits
above retained. Ontology metadata and the saved design point to this evidence.
