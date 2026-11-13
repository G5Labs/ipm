# SCM TLS fixture follow-up — independent reviewer 2

Read-only bounded investigation of the reported 253-pass/1-fail SCM serial runs.
Inputs were `/tmp/g5-audit-repaired-scm.log`, `/tmp/g5-audit-scm-final.log`, and
`/tmp/g5-audit-scm-tls-recheck.log`. Main's abstraction repairs remove four unused ref
APIs; this follow-up did not change production transport or repository source and did
not run Cargo builds or whole-suite repeats.

## Established evidence

The failure is reproducible in a **two-test** subset of the existing executable:

```sh
target/debug/deps/g5_scm-eda8b6344c330189 \
  scm_transport_tests::authenticated_native_lease_push_and_compensation_succeed \
  scm_transport_tests::https_challenges_and_redirects_do_not_cross_scm_origins \
  --test-threads=1 --nocapture
```

Result: first test passes, TLS challenge/redirect test fails at
`scm_transport_tests.rs:217` with `must reach the redirect fixture: []`; 1 pass/1 fail,
5.89 seconds. Running only `scm_transport_tests::` also reproduces it (3 pass/1 fail,
8.37 seconds). The abandoned-proxy test is therefore not necessary to trigger it.

A control selecting
`auth::tests::github_token_transport_preserves_identity_and_honors_push_url` followed
by the TLS challenge/redirect test **passes both** (2.26 seconds). Ordinary preceding
libgit2 repository initialization is therefore insufficient to reproduce the issue.
Main's provided exact-test log also passes the TLS test alone.

The sandbox refused local TCP binding; focused original-binary probes were rerun
with approved loopback/network access. Those permission failures are distinct from
the reproduced assertion failure.

## Source inspection and limits

- `scm_transport_tests.rs:134-237` starts an isolated TLS proxy for each of eight
  redirect scenarios, passes an explicit proxy to a detached libgit2 remote, and
  trusts the fixture certificate through the test callback. Its request list is
  populated only after CONNECT, TLS handshake and complete HTTP headers.
- At `:210-212`, `connect_auth(...).is_err()` discards the library's actual failure.
  An empty request list proves no complete HTTP fixture request was observed; it
  does not identify whether the error was before connect, handshake, or header read.
- The predecessor native fixture (`:274-478`) owns temporary repositories, local
  proxy configuration and a local TLS server. No process-wide environment writes,
  cwd changes or explicit TLS-global changes were found in these fixtures. The
  `GitRepo::open` global libgit2 owner-validation option is unrelated by inspection,
  and generic libgit2 initialization was a passing control.
- An isolated copy of the transport fixture, with added `connect_auth` diagnostics
  and an equivalent copied credential callback (the production callback is private),
  passes the same two-test sequence. It was linked with cached SCM libraries and
  rerun with the original test executable's exact git2/OpenSSL/tempfile/URL dependency
  artifacts. This is a diagnostic comparison, **not** proof that the original test
  or production path is sound: the standalone executable differs in linked graph
  and in how the fixture reaches the private callback.
- Diagnostic copies reside under `/private/tmp/g5-abstraction-tls-check-2`; only
  direct `rustc` fixture compilation reused cached libraries. No source checkout or
  shared database was changed. A debugger attempt stalled at process launch and was
  explicitly stopped; it produced no additional evidence.

## Disposition before main's diagnostic recheck

The sequence-dependent failure is confirmed and narrowed, but its root cause remains
unresolved. Do not classify it as a harmless TLS fixture flake or a production
credential leak. The guard's fail-closed assertion remains valuable and should stay.

Recommended minimal diagnostic repair: retain `connect_auth`'s error, and include
that error, destination, and `redirect_before_auth` in the existing reachability
assertion. If the resulting error still lacks detail, count accepted sockets,
completed CONNECT/TLS stages and complete HTTP requests. Keep all credential-origin
and expected-target assertions unchanged. Main elected to apply the first diagnostic
change and rebuild the existing binary; the narrowed pair should be rerun afterward.

This issue is separate from the repaired fork/merge consistency and unchecked API
surface findings. Until the original executable's pair passes for a understood cause,
the complete serial SCM result remains 253/254 in the supplied logs.

## Recheck after main retained connection diagnostics

Main rebuilt the same test executable after adding destination/scenario/library-error
information without changing the assertions. One focused two-test rerun again gives
**1 pass/1 fail** (3.45 seconds). The first TLS scenario fails:

```text
must reach github.com.attacker.example:443 (redirect before auth=true): [];
connection failure: Some(Error { code: -1, klass: 16,
message: "unexpected return value from ssl handshake -9806" })
```

This is libgit2's macOS SecureTransport handshake failure before any complete HTTP
request reaches the fixture, not a credential-origin assertion failure. The local
libgit2 dependency source `streams/stransport.c:91-99` reports this exact failure
around `SSLHandshake`; its `:268-269` describes `-9806` as connection closed via error.
The evidence narrows the failure to TLS/connection setup but does not yet identify
which peer/stage caused closure. Explicitly retaining that unresolved boundary is more
accurate than treating the passing isolated test as a complete-suite repair.

If continuing diagnosis, retain counters for accepted sockets, completed CONNECT,
pre-TLS EOF/timeout, TLS handshake completion and complete HTTP headers. In particular,
`proxy_tls` currently treats pre-TLS EOF/timeout as an abandoned connection and returns
`None`, so the final empty request vector cannot distinguish those expected abandoned
connections from the connection required by this test. Preserve its strict handling
of nonempty invalid TLS data and all credential/reachability assertions.
