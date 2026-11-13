# Native lease consolidation — independent review 1

Initial read-only discovery, 2026-10-02. Reviewed the frozen increment from
`156ca53e` against `/tmp/g5-lease-review-snapshot`; all nine supplied source/design
hashes matched at review time. No implementation or ontology edits, delegation,
peer reports or implementation-session explanation were used. The supplied task
described the increment's scope and the parked sibling-object isolation extension;
it supplied no defect hint.

## Initial finding

### R1 — Close the writable askpass file before executing it (P2)

`backend/g5-scm/src/native/process.rs:237–262` retains every new
`tempfile::NamedTempFile`, including the executable `ssh-askpass` script, with its
writable file descriptor open until the invocation finishes. Linux refuses to
execute a file open for writing (`ETXTBSY`). `authenticate` installs that exact path
as `SSH_ASKPASS` at lines 178–186. Consequently native push/delete using
`GitAuth::SshKey { passphrase: Some(..) }` cannot obtain its key passphrase on Linux.
The old `TempSecretFile::new` closed its local `File` before returning its pathname,
so this is a regression introduced by the migration.

This violates the pinned design's **Existing native lease migration** requirement
that SSH keys retain their supported authentication path, and affects
`g5.architecture.git_credentials` / `git_native_runner`.

Evidence: a network-disabled ephemeral `python:3.12-alpine` container reproduced
the relevant kernel transition with a private executable script. Executing while
the writable descriptor was open returned `26 Text file busy`; closing that
descriptor while retaining the path made execution succeed and return the fixture
passphrase. This is an OS-level discriminating reproduction plus source inspection,
not a claim that the production Rust public operation ran on Linux.

Suggested fix: after writing and setting permissions, close the file handle while
retaining unlink ownership until supervisor cleanup (for example retain
`tempfile::TempPath` values produced by `into_temp_path`). Keep key/helper/passphrase
paths alive across normal completion, deadlines and cancellation.

Regression check: actually execute the retained helper on Linux and assert its
successful exit and exact private result; ideally exercise an encrypted key through
the native public push/delete path against an isolated SSH service. The added
`ssh_credential_files_survive_execution_and_are_removed_after_cleanup` test
(`native/tests.rs:1071`) is insufficient for this failure: its shell calls the
helper but ignores its exit status, then deliberately exits 1 or waits. On Linux
the helper's `Text file busy` error can satisfy the current expected Exit/Deadline
and “no secret in public error” assertions just as the intended invocation does.

## Coverage and checks

Read the saved request, the plan beginning with **Ontology changes and governance**,
its actual recorded ontology diff, remaining design and new lease increment, the
current changed/governing blocks and their metadata. This increment changes no
ontology body/footer; the plan continues to identify the broad migration gaps.
Read every production/test/manifest/guard diff in the frozen increment. Followed
the public `GitRepo` methods through their material callers in
`workspace_service.rs`, `workspace_pull_request_provider_lifecycle.rs` and
`remote_integrations/worker.rs`, as well as the shared CLI/native context and
credential-selection shapes. Compensation deletion currently has direct test
callers rather than a discovered production caller.

| Commitment | Actual path / counterexample or check | Assessment |
|---|---|---|
| `git_execution`, `git_native_runner` | `GitRepo::push_with_lease` now delegates typed push/delete to `NativeGit`; removed direct subprocess implementation; blocking wrapper uses the same supervisor | Consolidation is minimal and keeps policy above execution. Native-only dependency tree contains neither `git2` nor `libgit2-sys`. Other native/backend owners remain named migration work. |
| `git_ref_updates`, network CAS/ref scope | `change_branch` validates single branch and exact expected OID, disables follow-tags/submodules; public rewrite/existing-commit/compensation methods retain policy | Real local Git tests cover reviewed rewrite, stale lease refusal, create, delete-only-at-expected-tip, and unchanged remote after refusal. No new scope bypass found. |
| `git_outcomes` | Porcelain status retains refused/confirmed/unknown distinctions, including deletion `-`; legacy adapter retains explanatory effect text | Native real lost-ack and confirmed-then-deadline tests pass. Legacy `G5Error::GitError` remains string-shaped; the public native outcome is typed. No new reporting regression found. |
| `git_retry` | Changed code, publication callers and native failure paths | No tracking state, replay worker or retry loop added. Native errors tell callers to inspect state. Existing worker/provider recovery ownership remains unchanged. |
| `git_credentials`, scoped provider grants, redaction | `authenticate`, effective remote checks, `GitAuth::transport_url`, TLS fixture and SSH resource lifecycle | Real authenticated HTTPS provider push plus leased delete succeeds for GitHub, configured GitLab port, and legacy GitHub SSH identities normalized to HTTPS; persisted origin remains unchanged. Exact-origin refusal, redirect fencing, argv/output redaction pass. **R1 violates Linux encrypted-SSH support.** |
| `git_native_runner` child/stream cleanup | Shared supervisor, private blocking runtime and temporary resource ownership | Native cancellation/deadline/process-group/output tests pass on macOS, including new credential-file existence/mode/removal cases. The latter does not establish successful helper execution on Linux. Existing API outer timeouts still wrap `spawn_blocking`; this increment does not newly claim end-to-end API cancellation propagation. |
| `git_locations` | Worktree-or-Git-directory selection remains `GitRepo` adapter input; CLI canonical/relay mapping preserved | Local bare/worktree cases and both runtime-relay tests pass. No location-owner reconstruction introduced. |
| `git_guard`, authorized reader | Extended tree-sitter ownership guard checks production `repo.rs`/`native/mod.rs`, missing targets and valid libgit2 use inside owner | Exact-source seven-test guard passes, with rejecting and harmless-refactor fixtures. Existing reader/projection guards retained. Whole-platform guard closure remains unfinished in the plan. |
| Publication/authorship/readiness owners | Existing source-push, provider lifecycle, existing-branch integration consumers still select credentials/admit policy before the same public methods | Inspected paths preserve authority selection and downstream verification. Broader API/DB consumer suites are assigned to the main reviewer; this report does not claim those executions. |
| `g5.governance.publish.proof` | Current ontology metadata explicitly records pre-existing equal-OID first-publication proof gap; migration retains Git's no-op behavior | Known inherited gap, not caused or repaired by this increment. Do not treat this review as proof the broader publication contract is complete. |

Executed using existing exact-source build artifacts (avoiding broad rebuilds):

- `g5_scm-554aebe82f3c130a`: all 19 native-only tests pass across the initial run
  and loopback-enabled reruns. Three initial failures were sandbox listener denials,
  not behavioral failures; the redirect and two relay cases pass after escalation.
- `g5_scm-08d6ca130345ed39 lease --test-threads=1`: six non-listener cases pass;
  both TLS authenticated native lease cases initially hit the same sandbox denial
  and pass when rerun with loopback access (eight selected cases total).
- `/tmp/g5-lease-reader-guard --test-threads=1`: seven exact-source guard tests pass.
- `cargo tree -p g5-scm --no-default-features --features native -e normal --offline`:
  no `git2`/`libgit2` entries.
- Network-disabled Linux writable-executable comparison: open file fails with
  `ETXTBSY`; same path succeeds after closing the writable file handle.

Unexecuted: production Rust SSH public-path handshake on Linux, full consumer/API/DB
suites, deployed credentials and services. The separate broad provider/client and
host/API execution migrations remain outside this increment, as recorded by the
governing plan and metadata. Sibling-object isolation remains explicitly parked.

## Disposition

Initial report saved before sharing. One actionable migration regression (R1);
no production repairs or ontology changes applied by this read-only reviewer.
Main-agent consolidation should retain the Linux support gap until a discriminating
repair check passes, then update affected native-runner/credential evidence metadata.
