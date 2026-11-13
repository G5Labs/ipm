# Native lease migration — independent review 3

Read-only discovery against the frozen increment after `156ca53e` (snapshot
`/tmp/g5-lease-review-snapshot`). No production/ontology changes or peer reports
were used. Inputs: original request, pinned design beginning with its ontology
changes/governance section and actual current governing commitments. The parked
sibling-object isolation extension is excluded. Initial findings were saved before
sharing; this report now records completed discovery.

## Initial findings

### P1 — encrypted SSH keys cannot invoke the retained askpass file on Linux

`backend/g5-scm/src/native/process.rs:180-188, 230-264`: `secret_file` retains a
`tempfile::NamedTempFile` with its writable descriptor open in `credential_files`,
then assigns the executable file directly to `SSH_ASKPASS`. Linux refuses execution
of files open for writing (`ETXTBSY`). The legacy helper wrote and closed its file;
this migration introduces the open descriptor. Thus the supported
`GitAuth::SshKey { passphrase: Some(..) }` lease push/delete path fails when OpenSSH
needs the passphrase, although the new macOS test passes.

Governing: `g5.architecture.git_credentials`, `git_native_runner`, and the design's
Existing native lease migration compatibility promise. Preserve cleanup ownership
using a closed `TempPath` (or another lifetime owner without a writable open handle),
then execute the existing fixture on Linux and verify a real encrypted-key SSH
authentication path when available.

Discriminating Linux OS check (network-disabled disposable `python:3.12-alpine`
container, executable tmpfs): an executable shebang file retained by an open writable
temporary-file handle fails direct subprocess execution with
`OSError(26, 'Text file busy')`; closing that handle makes the exact same file execute
successfully. This checks the OS mechanism, not the complete Rust/OpenSSH public path.

The new test at `native/tests.rs:1080` ignores the askpass command's exit status,
then expects a generic exit failure. Its redaction assertion only checks that the
passphrase is absent, so failed execution also satisfies it on Linux. Require
positive evidence that askpass emitted its expected response before checking
redaction and cleanup; merely rerunning this test on Linux does not discriminate.

### P2 — the migrated GitRepo lease path rejects valid relative local remotes

`backend/g5-scm/src/repo.rs:3064-3081` now sends existing GitRepo operations through
`NativeGit::cli`, whose `native/mod.rs:791-805` URL validation accepts local paths
only when absolute. A configured unauthenticated remote such as `../remote.git`
worked through the old native lease helper; the new public GitRepo API returns
`Invalid repository URL` before a leased push or compensation deletion can run.
There is no changed authority/credential contract requiring this restriction.

Governing: preservation of existing Git semantics in the design's scope and native
lease migration section; `g5.architecture.git_locations` leaves Git-directory/path
mechanics inside the adapter. Accept the supported relative local-transport form
for unauthenticated/SSH-key operations, resolving it relative to the repository
where necessary while retaining configured-destination checks. Add an actual
GitRepo leased push/delete test with `../remote.git` and a stale-lease refusal.

Discriminating public-path check: compiled an isolated Rust probe against the
current `libg5_scm-d0a5004204312319.rlib`, created local/bare fixtures, then called
`GitRepo::push_refspec_with_lease`. With an absolute remote, create and exact-head
update succeed. Changing only origin to `../remote.git` makes the same operation
fail with `Invalid repository URL`; executing the legacy native Git command
directly against that exact relative destination succeeds and advances its ref.
Fixture and probe remain under `/tmp/g5-lease-review-3/`.

## Coverage and available evidence

| Commitment | Actual path / counterexample | Result so far |
|---|---|---|
| Execution/ref boundary, minimal implementation | GitRepo lease adapter → BranchPush/BranchDelete → private BranchChange → Invocation; exact refspec, follow-tags/submodule suppression | Source-inspected; typed deletion and removal of duplicate subprocess/auth helpers are appropriately scoped. Cargo libgit2 → native dependency preserves the old lease capability; native remains independently selectable. |
| Ref leases/publication authority | GitRepo rewrite, `push_existing_commit`, workspace service (`push_commit_ff_or_lease`), PR lifecycle (`push_branch_ff_or_lease`), integration worker (`push_existing_commit`) | Inspected caller authority and lease selection; no new caller authority or recovery storage. Actual current public GitRepo probe verifies absent create, exact-head update, stale compensation refusal preserving remote OID, and successful exact-head compensation. |
| Credentials and context | ScmToken exact origin, rewritten push URL checks, URL-scoped headers, SSH temp files, GitHub SSH identity normalization | Source-inspected; Linux askpass defect above. Credential owner remains outside adapter. CLI guest relay still avoids credential injection. Test additions cover trusted provider origins and canonical origin retention. |
| Lifecycle/outcomes | Invocation supervisor, cancellation task, group kill, bounded output, porcelain interpretation, G5Error adapter, CLI signal/caller projections | Source-inspected. New deletion status `-` is recognized and stale refusal carries expected head/action. Supervisor retains credential resources until child cleanup; Linux executability is defective. CLI per-member partial reporting remains unchanged. API wrappers still use spawn_blocking/outer timeouts; this increment does not demonstrate caller-cancellation propagation to those wrappers. |
| Retry ownership | Current increment has no replay, worker, table, queue or journal | Inspected. Existing owner retries remain unchanged. |
| Reader/provider/location boundaries | No content-reader or provider HTTP migration in this increment; GitRepo derives worktree/bare directory internally | Inspected; inherited migration gaps are retained, not claimed repaired. Relative-location regression above. |
| Guard calibration | New shared_repository_leases guard and typed_git_violations changes | Source-inspected; disallows native process imports/paths in migrated GitRepo/native facade, rejects missing/test-only required operations, permits libgit2 internals and harmless names. Existing API content-reader and projection rules remain separate; g5-scm internals are properly exempted from the projection consumer rule. |

Main agent owns broad consumer/build checks; this reviewer uses isolated fixtures.

## Evidence limits and inherited gaps

- No production or ontology files were changed. Linux verification exercised the
  operating-system failure directly; the complete Rust/OpenSSH encrypted-key route
  remains unexecuted. The test's missing positive askpass assertion is material.
- Default/native-only dependency and consumer suites are owned by the main agent;
  this review independently linked and exercised the current public GitRepo path.
- The create-if-absent/equal-OID guarantee is already imperfect in the prior native
  runner/legacy helper: the public probe with empty expected head and an already
  equal remote returns success. Existing native tests explicitly accept an equal
  tip even under a stale lease. This is an inherited conformance gap, recorded in
  current publication-proof metadata, not counted as a newly introduced defect.
  The plan's assertion that empty expected head means strict create-if-absent should
  remain qualified until this boundary is addressed.
- Provider HTTP consolidation, raw libgit2 API closure, other API/host/dispatcher
  native paths, and host/guest dependency closure remain the declared later migration
  work. This review makes no completion claim for them or sibling-object isolation.
- Relevant footer evidence was read for Git execution/ref/native/outcomes/retry/
  credentials/location/guard, provider scopes, publication proof/actor, and security
  credential/redaction commitments. Main-agent post-repair metadata should include
  the newly migrated GitRepo path while retaining these limits.
