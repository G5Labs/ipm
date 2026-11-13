# Review: image-declared environment reaches agents, terminals and previews

Design: [ontology-plan.md](ontology-plan.md). Governing block: `g5.runtime.image-env`.
Base `af8e6867`.

## Checks executed (author)

- `g5d-dispatch` unit tests (`git::`): 9 passed, including
  `image_environment_forwards_named_exportable_keys_only` and
  `runtime_owned_keys_win_over_forwarded_image_keys`.
- `g5d-isolation` unit tests (`--all-features`, virgl stub): 4 passed, including the
  extended `image_env_defaults_are_passed_before_spec_overrides` and
  `image_env_keys_are_not_named_when_the_image_declares_none`.
- `g5d` dispatch terminal regression (`--no-default-features --features test-helpers`):
  8 passed; the owner terminal sees `IMAGE_TOOL_HOME`, not `G5D_IMAGE_ENV_KEYS`, and keeps
  the runtime's HOME although the image named it.
- `g5d-foundation` secret-redaction tests: 4 passed.
- `cargo fmt --check` on the touched crates. `cargo clippy -D warnings` is blocked
  locally by a pre-existing macOS-only `unused_assignments` lint in
  `modules/runner.rs:631`, untouched by this change; CI's Linux lane covers clippy.
- Not executed: a booted VM (ships only through the worker-image bake and rotation).

## Codex (gpt-6-astra) review rounds

| Round | Head | Verdict | Findings | Repair |
|---|---|---|---|---|
| 1 | 228446e0 | APPROVE (1 × P2, 1 × P3, both confirmed) | (a) `image_environment` trimmed each named key before lookup, so a padded image key ` FOO ` would resolve to `FOO`, a spec-only or shell-created variable, breaking image provenance. (b) The generated `G5D_IMAGE_ENV_KEYS` value entered the guest console secret collector through its "KEY" heuristic, so every mention of the named variables in guest diagnostics would read `[REDACTED]`. | (a) Names are used exactly as the host wrote them; one that would change under trimming is rejected. Test: padded ` FOO ` beside `FOO` in the process environment, neither forwarded. (b) `collect_secret_env_values` skips `IMAGE_ENV_KEYS_ENV`. Test: `the_image_key_list_is_metadata_not_a_secret`. |
| 2 | 61fef3b8 | APPROVE (1 × P2, confirmed) | The joined key list rides as one `-e` argv string; an image with 2,048 distinct 64-byte key names would produce a 133 KB argument, over Linux's 128 KiB per-string limit, and fail the `vm-exec` spawn with `E2BIG` although the separate entries fit. | `image_env_key_list` bounds the carrier at `IMAGE_ENV_KEYS_MAX_LEN` (32 KiB), stops at a key boundary in image order and warns with the dropped count. Test: `image_env_key_list_is_bounded_at_a_key_boundary` (2,048 × 64-byte names → 504 kept, no dangling separator; separator-bearing names skipped without consuming budget). |
| 3 | c411d92b | APPROVE (1 × P2, 1 × P3, both confirmed) | (a) An image declaring `G5D_IMAGE_ENV_KEYS` itself survived the merge when the generated list was empty, so the dispatcher would forward a spec-only variable the image named. (b) The 32 KiB bound was an undocumented exception to the forwarding guarantee. | (a) The merge drops an image-declared carrier before naming; only the host-generated value is installed. Test: `an_image_cannot_supply_its_own_key_list` (carrier alone and carrier beside a real key). (b) The block, plan and metadata now state the host-only carrier and the bound. |
| 4 | c47a23b9 | APPROVE (1 × P3) | The Phase 1 block still said image environment never reaches agents, which is false on upgraded hosts. | Sentence qualified as behaviour before the passthrough and on hosts whose runtime predates it. Converged. |

## Commitments

| Commitment | Actual path | Counterexample / check | Result |
|---|---|---|---|
| Only host-named image keys are forwarded | `resolved_guest_env` names the surviving keys; `image_environment` resolves only those | Spec-only `SPEC_ONLY` present in the environment but unnamed; padded name | Pass (unit tests) |
| Reserved policy and loader exclusion | `is_exportable_task_env_key` + `NATIVE_LOADER_PATH_KEYS` | `PATH`, `HOME`, `G5D_DISPATCH_TOKEN`, `LD_LIBRARY_PATH` named by the host | Pass (unit test); terminal regression keeps HOME |
| Runtime-owned keys win on collision | image entries first, runtime entries after; consumers apply in order | `HOME` and `CLAUDE_CONFIG_DIR` named by the image | Pass (unit test + terminal regression) |
| All three surfaces | modules via runner `envs`, terminals via `process_environment`, previews via module env | Terminal regression executes the real path; module and preview by inspection | Pass; VM execution open |
| Spec-overridden keys are neither forwarded nor named | `merge_image_and_spec_env` drops them before naming | image `PATH` overridden by spec `PATH` | Pass (isolation unit test) |
| The key list is metadata, not a secret | collector exclusion | `G5D_IMAGE_ENV_KEYS=JAVA_HOME:MAVEN_HOME` beside a real secret | Pass (foundation unit test) |

## Remaining gaps

| Governing ontology | Gap | Proposed action | Uncertainty / blockers |
|---|---|---|---|
| `g5.runtime.image-env` | Not executed in a booted VM | After the worker bake and rotation, probe an agent turn in a fresh Base workspace for `RUSTUP_HOME` and `PLAYWRIGHT_BROWSERS_PATH` without exports | Needs the deploy |
