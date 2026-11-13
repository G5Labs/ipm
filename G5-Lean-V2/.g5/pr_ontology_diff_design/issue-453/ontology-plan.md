# Git smart-HTTP request bounds

Work on [issue 453](https://github.com/G5Labs/G5-Lean/issues/453), read with
`g5d issue show` on 2026-10-12. Authorized checkout branch:
`issue-453-git-smart-http-decompression-unbounded` (initial HEAD `8d74bc2b`).

## Ontology changes

The added [request-ingestion contract](../../ontology/software_architecture/git_governance.g5.md#bound-smart-http-request-ingestion)
owns authorization ordering, wire/expanded limits and bounded blocking decode.
The actual ontology diff is retained beside this design in Git.

## Governing contracts

- [Git policy](../../ontology/software_architecture/git_governance.g5.md#git-policy-owns-authoritative-ref-effects)
  retains repository/ref authorization and mutation admission.
- [Native runner](../../ontology/software_architecture/git_governance.g5.md#run-native-git-through-one-runner)
  retains execution deadlines and cleanup; request decoding happens before this owner.

## Design

Both POST handlers accept an uncollected HTTP body, perform their existing JWT and
repository checks, then collect with an explicit wire limit. This also removes the
reported sibling's application-level pre-authentication 256 MiB buffering. Move
receive-pack's repository checks ahead of decoding and pkt-line parsing, preserving
its later exact-ref checks and owned mutation/completion task.

The shared decoder enforces the existing 256 MiB ceiling on decoded bytes too,
reading no more than one byte beyond the ceiling to detect overflow. Identity is
checked as well. Preserve current encoding compatibility. Compressed work runs in
`spawn_blocking`; a process-wide two-slot semaphore refuses excess jobs with 503,
and the blocking closure owns its permit through completion even if HTTP cancels.
This bounds simultaneous inflation without a queue retaining compressed bodies.

An absolute output cap avoids rejecting otherwise valid highly compressible Git
requests merely because of their compression ratio. No independent decode timeout
is added: Tokio cannot terminate a running blocking closure, whereas bounded
input/output and fixed decode concurrency constrain its work. This does not impose
a total process memory cap on all authorized requests or native Git processes.

## Evidence

At the initial checkout, both decoders call unbounded `read_to_end`; `Bytes`
extraction precedes handler authentication, and receive-pack decodes before
repository read/push checks. The candidate retains those authorization owners while moving them before body
collection and applying a shared bounded decoder to both POST consumers.

Executed on 2026-10-12 in this Linux workspace with local PostgreSQL 16 and native
Git, using the fixture JWT identity and production HTTP router:

| Check | Result |
|---|---|
| `cargo test -p g5-api --test api_git_push_policy --no-run --profile g5-dev -j 4`, then the emitted `target/g5-dev/deps/api_git_push_policy-d6738c4e7477f8ce --test-threads=1` with local `TEST_DATABASE_URL` | 13 passed, 0 failed; 247.03 seconds |
| `cargo test -p g5-api-routes --lib routes::git::tests --profile g5-dev -j 4` | 11 passed, 0 failed |
| Scoped `rustfmt --check` and `git diff --check` | Passed |

The two added HTTP tests establish denial without any body bytes for missing
credentials, missing repositories, strangers, read-only push callers and stale
incarnations. Malformed gzip cannot override repository denials. Valid gzip,
x-gzip and deflate requests reach native Git for both fetch and push; highly
compressible bodies over the real 256 MiB output ceiling return 413 on both
routes, and exact-limit bodies reach pkt-line parsing. Uncompressed/chunked and
encoded wire bodies independently exercise the wire ceiling.

The three decoder tests cover below/exact/above-cap expansions, malformed input,
encoding compatibility, a reader that proves no more than limit + 1 output bytes
are consumed, and 503 saturation. A single-worker blocking pool demonstrates that
the async worker stays responsive and a cancelled waiter cannot release its slot
until the queued blocking decode completes.

The sequential HTTP suite peaked at 586,076 KiB RSS as reported by Python
`resource.getrusage(RUSAGE_CHILDREN)`. This is local test evidence, not a total API
memory guarantee or a sustained concurrent deployment load test. No QA/production
behavior has been observed or changed. The existing native Git execution and
completion tests passed; the wider application suite was not run.
