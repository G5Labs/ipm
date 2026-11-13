# P8.5 dead-code removal — independent review rounds

Design: [ontology plan](pr_ontology_diff_design/p8.5-crate-consolidation/ontology-plan.md).
Reviewer: Codex `gpt-6-astra`, reasoning effort xhigh, read-only sandbox on the branch.

## Tranche 2 — service contexts, PR 2 (the move)

Design: the plan's "Tranche 2, bounded contexts" and its layout decisions. Branch
`kaihui/p8.5-services-move`, PR #377, stacked on the edge PR. Mechanical: `git mv` of every
service and maintenance job into `services/<context>/`, context roots, compatibility
re-exports at both composition roots, `super::` chains that reached a composition root
rewritten to the explicit context path, path-keyed consumers updated, the ratchet rebased on
directories. Verified locally: fmt, the full `g5-api` guard suite, routes and contracts tests,
`check-layout`, `commands-check`, workspace clippy with all targets (the pre-existing local
`g5d-dispatch` lint aside), the core crate's unit tests, the g5d crate-architecture test and
the whole unit lane (only the known Mac-environment failures remained). CI runs the
integration lanes.

### Round 1 (2026-10-08, `dbe1303e` against `5c05480e`)

Verdict: changes requested. Two P1, both repaired: the crate-private and private modules
(`github_app_auth`, `session_file_sha256`) had lost their `crate::services::<name>` paths
because the re-exports were unused and failed clippy; they are back with their old reach and
an explicit allow, and the plan says so. The JSONL store's file-name fixtures had been
rewritten on one side only by the path pass; the arbitrary fixture is restored. Three P2,
all repaired: the g5d architecture guard's forbidden spelling of the old
`g5-api/src/services/job_progress_service.rs` citation had been rewritten too, so the
original obsolete path could reappear unnoticed; the original spelling is back and the
intermediate flat `g5-api-core` spelling is rejected as well. Directory assignment had
dropped the explicit ownership checkpoint, so a helper placed in the wrong context directory
passed; the ratchet now carries a reviewed inventory of every file's context and fails on a
file sitting elsewhere or a new file without an entry. The live module maps (the ontology's
software-architecture concept and the codebase architecture diagram) still named flat or
mis-prefixed locations; they name `g5-api-core/src/services/<context>/` now. Codex inspected
all 112 renames (51 byte-identical), found no wrong `super::` rewrite or visibility
widening, every public and maintenance compatibility export present, every include and
path literal resolving, and the ontology edits factual; it noted the pre-existing routes
guard limit (its `src/services` scan root does not exist in the routes crate).

### Round 2 (2026-10-08, `6d79b1f0` against `5c05480e`)

Verdict: changes requested. One P1, repaired: Rust re-exports do not carry tracing targets,
so a `RUST_LOG` directive against `g5_api::maintenance::repo_gc` or
`g5_api::services::org_service` no longer selected the moved module and rendered targets
changed; the API crate's legacy target family now maps every moved module's historical
target to its physical context path (one hundred entries, a longer prefix winning over the
crate-level ones), with an end-to-end test that expands a filter and renders JSON events.
One P2, repaired: the ratchet recorded only `pub use` items at a composition root, so the
private compatibility path of a crate-private module resolved to the exempt root; every
`use` at a root now forwards, with fixtures for the direct and aliased private path. One P3,
repaired: two module-map locations in the ontology lacked their `g5-api-core/src/` base.
Codex found all 112 renames, every compatibility import (private reach included), the
nested `super::` chains, the includes and path literals, the ratchet's coverage of context,
compatibility, relative and aliased references, and the ontology edits clean; the routes
guard's absent scan root remains a pre-existing limit.

### Round 3 (2026-10-08, `ed3c25b5` against `3208eb63`)

Verdict: changes requested. One P1, repaired: once a legacy prefix has descendant mappings,
the filter expansion goes only through them, so a broad `g5_api::maintenance=info` or
`g5_api::services=warn` directive no longer selected the composition roots' own events (the
runner's start-up line, service initialization warnings); the two roots now map themselves
as well, and the test expands both broad directives and renders a runner event. Codex found
everything else clean for the third time (renames, nested chains, compatibility paths,
consumers, ratchet coverage, ontology). Round 4 decides convergence: the shapes have
narrowed to the tracing-compatibility table.

### Round 4 (2026-10-08, `41863d1a` against `3208eb63`)

Verdict: changes requested. Two P1 on the tracing table, both repaired: mapping
`g5_api::services` to `g5_api_core::services` made a broad `g5_api::services=off` silence
the maintenance jobs that now live under the same directories, where they used to keep the
default level; the root mappings are gone and the two composition roots emit their own
events under the logical targets `g5_api::services` and `g5_api::maintenance` (a crate-wide
constant each), so a broad directive selects the root and, through the table, the services
only. And a translated directive could be replaced by a later, broader one for the same
physical target (`g5_api::services::org_service=trace,g5_api::services=warn` ended at
`warn`), where the base kept the specific level; the shared expansion now ranks each
translated directive by the specificity of its origin, keeps the more specific one in either
order and lets a directive written against the physical target outrank translations, with
unit tests in the contracts crate and exclusion and order cases in the API crate's test.
Codex found the move itself clean for the fourth time. The branch is declared converged
under the repeated-shape policy (rounds 2 to 4 narrowed to the tracing-compatibility table).

### Round 5 (2026-10-08, `9fcdb313` against `3208eb63`)

Verdict: changes requested. Two P1 in the round-4 expansion rewrite, both repaired: the
collision key was the target alone, so a directive with a span or field selector
(`repo_gc[{repo_id}]=debug`) collapsed into the plain one; directives are now keyed by their
whole matcher. And descendant expansion required a `::` boundary while `EnvFilter` matches
targets as lexical prefixes, so an abbreviated historical target
(`g5_api::services::workspace_=debug`) selected nothing after the move; a mapping is now a
descendant of any target that lexically prefixes it. Unit tests in the contracts crate and
cases in the API crate's test cover both, in both directive orders. Codex found the move
itself clean for the fifth time; the branch is converged (rounds 2 to 5 stayed inside the
tracing-compatibility table), and one more round runs only as a check on this last change.

### Round 6 (2026-10-08, `82fb45f6` against `3208eb63`) — converged

Verdict: changes requested. Two P1 in the expansion rewrite, both repaired by narrowing it:
the level separator was the first `=`, which sat inside a field selector such as
`{request_id=a}` and merged distinct selectors; and deduplicating the caller's own directives
changed the meaning of a repeated bare level (`info,off,info`). The caller's directives now
pass through verbatim and in order; only the translations are deduplicated, by a matcher
whose level separator is the `=` outside the brackets, ranked by origin specificity, and a
translation never overrides a matcher the caller wrote against a physical target. One P2,
repaired: the architecture diagram still placed the ontology reconciler under
`maintenance/`. Codex found the move itself clean for the sixth time. Rounds 2 to 6 stayed
inside the tracing-compatibility table that round 2 introduced, so the branch is declared
converged under the repeated-shape policy and merges on green CI.

CI (read from the Cloud Build logs once credentials were back): the rust lane's
`rust-integration` stage failed `workspace_branch_resolution_guard`, because the legacy
tracing-target table spells `remote_integrations` in string literals; the guard looks for
private storage names in literals, and a Rust path or tracing target (`a::b`) is never a
storage identifier, so such literals are now excluded with that reason. The edge PR's red
lane was CI clippy (`RUST_CLIPPY`): the ratchet's resolve loop in the `while let` form that
the pinned toolchain requires; fixed on the edge branch and merged here.

## Tranche 2 — service contexts, PR 1 (break the backward edges)

Design: the plan's "Tranche 2, bounded contexts" (identity < git < hosts < workspace < agents),
entered on the user's 2026-10-08 gate override. Branch `kaihui/p8.5-services-edges`, stacked on
the `g5d-agent` merge. No file moves; `backend/g5-api/tests/service_context_layering.rs` pins
the order and the seams it needed are traits installed in `services/mod.rs`. Verified locally:
core clippy with all targets, the layering ratchet with its unit tests, and the three guard
tests that share the source scanner. Not executed: the API integration suite (needs the local
database); CI runs it.

### Round 1 (2026-10-08, `6f49377e` against `0dfbca3c`)

Verdict: changes requested. One P1, repaired: the replay seam decoded each dispatcher record
after the database scope was opened, so a malformed record waited for a connection and a
scope failure surfaced as `AGENT_STOP_REPLAY_COMMIT_FAILED` where the base answered
`AGENT_STOP_REPLAY_INVALID` first; the trait now validates the record before any connection
is held and applies the validated record on the caller's transaction. Three P2 on the ratchet,
all repaired: composition-root re-exports were resolved without the root's own module prefix
(so `pub use task_service::TaskService` never matched `crate::services::TaskService`), use
aliases and composition-root globs escaped classification, and module identity came from file
names although two `#[path]` declarations make `workspace_pull_request_checks.rs` a module
three levels deep. The scanner now derives every file's module from the declarations that
reach it from `lib.rs` (inline modules included), resolves paths through the names `use`
binds and the modules a file declares, follows macro paths through the same bindings, refuses
a glob import of a composition root, and exercises each case in unit tests on synthetic
sources plus the real tree. Codex found the seams' transactions, locks, error mapping and
actor scoping equivalent to the base, the moved helpers byte-equivalent, the composition and
test constructors matched, and the ontology edits factual; it noted that `AppState` field
access stays invisible until the move makes the dependencies structural.

### Round 2 (2026-10-08, `4662d36b` against `b04d6b0c`)

Verdict: changes requested, no P1. Four P2 on the ratchet, all repaired: `self::svc::X` and
`super::svc::X` reached the file's own module without expanding the alias, so bindings are
now looked up again in the module a `self`/`super` chain reaches; bindings were keyed by
inline module only, so two functions importing the same name overwrote each other and
conditional items kept one alternative, so every `use` now binds in its own block and all
alternatives of a name resolve; a scoped path was recorded whole, hiding the types inside
generic arguments and qualified-trait forms, so the scanner now keeps only the real path
segments and scans generic arguments and `<T as Trait>` types on their own; and the
composition roots' glob re-exports were dropped and the segments after a forwarded name were
lost, so a glob re-export from a root is refused and forwarding keeps the rest of the path.
Each case has a unit test. Codex found all seven seams, the replay ordering, the moved
helpers, the composition and the ontology edits clean, and repeated that `AppState` field
access stays invisible until the move.

### Round 3 (2026-10-08, `185672d3` against `455563e6`)

Verdict: changes requested, no P1. Three P2 on the ratchet, all repaired: a chained alias
(`use svc as api`) was resolved in the referencing scope, so a block-local name shadowing one
link redirected it; each binding now resolves where it was bound. Re-exports inside an inline
module of a composition root were ignored, so `crate::services::exports::X` forwarded to the
exempt root; re-exports are now keyed by their path from the root, inline globs are refused
too, and the longest forwarded name path wins. Raw identifiers (`r#task_service`) missed the
module map; identifiers are normalized in declarations, imports, paths and macro tokens. Each
case has a unit test. Codex found the seams, the replay ordering, the moved helpers, the
composition and the ontology edits clean for the third time; the remaining shapes are
scanner hardening, so round 4 decides convergence under the repeated-shape policy.

### Round 4 (2026-10-08, `6eae0846` against `455563e6`)

Verdict: changes requested, no P1. Two P2 on the ratchet, both repaired: `self::svc::X` was
resolved with the enclosing block's bindings, so a block-local `use std as svc` shadowed the
module's import where Rust does not; `self::` and `super::` now reach module-level bindings
only. The shared module scanner kept the `r#` prefix on raw module declarations, so a
path-attributed `mod r#task_service` would have keyed the module map under a name no
reference can match; declaration names are normalized, with a scanner unit test. Codex found
the seams, replay ordering, moved helpers, composition wiring and ontology edits clean for the
fourth time. Round 5 runs once more because the repairs were cheap; after it the branch is
declared converged under the repeated-shape policy regardless of further scanner nits.

### Round 5 (2026-10-08, `3a25e805` against `03a40095`) — converged

Verdict: changes requested, no P1. Two P2 on the ratchet, both repaired without a further
round: a path walking through one of the file's inline modules (`imports::svc::X` with the
alias bound inside `imports`) did not expand the alias there, so resolution now walks the
file's inline modules segment by segment and expands the names bound in each; and `$crate`
inside a `macro_rules!` body was not read as the crate root, so a macro-defined later-context
type escaped, and the token scanner now treats it as `crate`. Both have unit tests. Codex
found the seams, replay ordering, moved helpers, composition and ontology edits clean for the
fifth time; rounds 2 to 5 found only scanner gaps, so the branch is declared converged under
the repeated-shape policy and merges on green CI. The remaining known gap is unchanged:
`AppState` field access is not a path, which the move PR makes structural.

## Tranche 2 — `g5d-worker` and `g5d-agent-turn` into `g5d-agent`

Design: "Tranche 2, bounded contexts" of the [ontology plan](pr_ontology_diff_design/p8.5-crate-consolidation/ontology-plan.md),
started on the user's gate override. Branch `kaihui/p8.5-agent-crate`, PR #375, stacked on the
push retirement. Verified locally: `scripts/check-layout.py`, workspace clippy, the merged
crate's unit tests, the dispatch, `g5d` (CI feature set), dispatch-module and crate-architecture
tests, the consumer-boundary and tracing-compat tests, the two `g5-api` guards,
`just commands-check` and the migration checksum catalog.

### Round 1 (2026-10-08, `d9306569` against `ed6ed1d0`)

Verdict: changes requested, no P1. One P2 and two P3, all repaired: the runner-side SCM guard
listed nine files, so a new runner module could reach `g5_scm` unseen; it now discovers every
provider-side source and a fixture proves a newly added runner module is caught. Five docs that
cited the retired crates as current locations and three ontology module paths missing the
`runner` segment are corrected, and the plan's governance inventory records the crate tranche's
ontology edits. Codex also noted the phase-5b Justfile lane used filters that matched no module
since before this change; it now runs the whole agent library suite.

### Round 2 (2026-10-08, `0dfbca3c` against `8fc84c5e`)

Verdict: approve, no findings. Codex rechecked source preservation, the fixtures, the
dependency union, the one-way edges and host exclusion, the discovered-source SCM scan, every
build surface, the tracing map and the ontology edits. Converged after two rounds; merges on
green CI once the branches below it land.

## `g5d push` retirement — native publish for the seed scripts

Design: the plan deferred this until the seed scripts used native git; the user asked for it
on 2026-10-08 (gate override recorded in the plan). Branch `kaihui/p8.5-push-native`, stacked
on tranche 1 PR 3. Verified locally: `scripts/check-layout.py`, workspace clippy, the `g5d`
unit tests with the CI feature set (the known setsid failure only), the CLI command, dispatch
and crate-architecture tests, the three `g5-api` guards, the seed-script structure test,
`bash -n` on every edited script and `just commands-check`. Not executed: the compliance and
demo seeds against a live stack (Docker is down on this Mac); that is the remaining
unexecuted coverage, and the demo path was already broken before this change.

### Round 1 (2026-10-08, `6dc81811` against `f8eb9e68`)

Verdict: changes requested. One P1: the rewritten demo seed pushed a commit to Base's default
branch, which the server refuses (protected default branch, Base admission), so the seed would
exit before the child workspaces; the old path was already broken the same way, so this is an
incomplete repair rather than a regression. Six P2: the pushed refspec used `HEAD` instead of
the validated SHA; the spec sync no longer covered every code member of the target workspace;
members were pushed one by one without a whole-publish preflight; the remote seed lost its
bounded retry for an already-published head; the sync responses were not validated beyond HTTP
success; and the publishing docs described an unchecked force-with-lease and kept an obsolete
lock-update promise. All repaired in the next commit.

### Round 2 (2026-10-08, `05a95069` against `61cff5ee`)

Verdict: changes requested, no P1. Three P2, all repaired: the docs-sync validation accepted
error entries without string fields and a failing blocker extraction could pass as success
under a caller's `if`, so every entry is validated and the extraction failure propagates; the
documented recipe ran the leased push even after a failed ancestry check, so it now chains the
check to the push under fail-fast; and the new shell harness was not in any required CI lane,
so the Rust Cloud Build lane gains a `native-publish-contract` step that runs it. Codex found
the pinned pushes, member planning, manifest-driven syncs and the Base volume install correct.

### Round 3 (2026-10-08, `233b7abf` against `2a73881d`)

Verdict: changes requested. Two P1, both repaired: the demo seed still pushed the `demo` and
`e2e-test` branches with the user token, but the test-token cascade publishes those workspaces
stopped and the mirror admits a branch push only from a running incarnation with a live lease
(`git.rs` admission), so the first child push would have failed after the Base install; the
seed now installs every docs branch through the API repo volume with the same compare-and-swap
as the Base cut and keeps the native helper for the projection refreshes (the old `g5d push`
path used the same user token and was refused the same way). The new CI step ran in the gcloud
builder image, which has no `jq`, so the harness exited before exercising anything; it now
runs in the primed Rust lane image after the target prime. One P2, repaired: the documented
recipe chained the ancestry check with `&&`, which `set -e` does not enforce, so a divergent
head skipped the push and still ran both syncs successfully; the check now exits the recipe.
Codex found the compliance seed's equal-head no-op, member syncs and verification gates, the
pinned remote publication, the helper's error propagation and the reader-guard fixtures intact.

### Round 4 (2026-10-08, `a75f69b0` against `2a73881d`)

Verdict: changes requested. One P1, repaired: the seed-script contract test still searched
for the demo seed's Base-only install function and its planned child pushes, so the required
API integration lane would have failed; the assertions now pin the per-branch volume install
(Base first, only Base moving the mirror's HEAD, which the install function now guards; the
earlier revision would have pointed HEAD at a child branch), the materialization wait and the
projection refresh. Three P2, all repaired: the remote seed planned only its two staged
members, so a workspace whose other code member lacked its branch was published partially and
failed at the projection refresh, where the retired client read the whole manifest first;
the helper now checks every manifest member's branch before any push (unit-tested on the fake
API, including empty and memberless manifests). The push dropped the retired client's
`--no-follow-tags --recurse-submodules=no`, so inherited `push.followTags` could add a tag
update the API rejects; both flags are back with a harness case that sets the option and an
annotated tag. The documented recipe called the sync endpoints with bare `curl -fsS`, which
cannot see the HTTP-200 `errors` array; it now sources the helper's projection refresh. Codex
found the compliance seed, the pinned remote publication, the helper's error propagation, the
Base-first CAS installs and the server-side admission rules intact.

### Round 5 (2026-10-08, `4dc0d8e3` against `2a73881d`)

Verdict: changes requested, no P1. Two P2 and one P3, all repaired: an inherited
`url.<base>.insteadOf` or `pushInsteadOf` could send the leased push (and the unscoped bearer
header) to a repository other than the mirror the plan read, where the retired runner compared
the expanded destination with the expected URL; the helper now refuses a member's plan when
any rewrite applies to its mirror URL (harness case with push and fetch rewrites and an
unrelated one) and scopes the token to the API origin with `http.<url>.extraHeader`. The
development-flow document replaced `g5d push --all` with one plain push and no projection
refresh, so it now shows the plan/apply/refresh helper workflow and links the plain-Git form.
A failed demo install left its bundle on the host and in the container; both sides now clean
up through an exit trap. Codex found the compliance, remote and demo seeds, the retired-item
sweep, the reader guards and the CI step correct. Round 6 decides convergence: the findings
have narrowed to defensive configuration and documentation, and the policy stops after
repeated shapes with green CI.

### Round 6 (2026-10-08, `6fa2e1ca` against `2a73881d`) — converged

Verdict: changes requested, no P1. Three P2 and one P3, all repaired: the mirror reads ran in
the caller's directory while the rewrite check and the push used the member's, so a
caller-local `insteadOf` could redirect the read; reads now run in the member's directory and
the caller's own configuration is checked when the helper is configured (harness cases for
both). The two development-flow examples ran the plans as independent statements, so a failed
docs plan could still reach the apply, and they used member-relative paths from inside a
member checkout; both now chain every step with `&&` from the workspace root with absolute
member paths and the manifest member check. The plain-Git recipes lacked the helper's push
flags, rewrite refusal and scoped token; the CLI recipe now reproduces them and the flow
document defers to it. Six rounds found no behaviour regression since round 3; rounds 4 to 6
narrowed to defensive Git configuration and documentation, so the branch is declared
converged under the repeated-shape policy and merges on green CI.

## Tranche 1 — crate consolidation, PR 3 (`g5d-cli` into `g5d`)

Design: "Tranche 1, crates" of the [ontology plan](pr_ontology_diff_design/p8.5-crate-consolidation/ontology-plan.md).
Branch `kaihui/p8.5-crates-3`, PR #373, stacked on PR 2; completes the tranche at 21 Cargo
members. Verified locally: `scripts/check-layout.py`, workspace clippy, the `g5d` unit tests
with the CI feature set (one pre-existing Mac-only setsid failure), the push-authority, CLI
command, dispatch and PR command tests, the consumer-boundary and tracing-compat tests, the
six `g5-api` guard tests, the g5d crate-architecture test (against the libkrun graphics stub),
`just commands-check`, the migration checksum catalog, and a build and `--help` run of the
`g5d` binary.

### Round 1 (2026-10-08, `0c7dd1c7` against `946e6d6d`)

Verdict: changes requested, no P1. Two P2, both repaired: the process shell's exit and panic
events had taken the physical target `g5d::cli::process`, so a historical
`g5d=info,g5d::cli=off` filter would have silenced them; they now log at `g5d::process`
outside the CLI module (rendered as `g5d` through the mapping), with a mixed-filter regression
test and a check that the mapping table names the shell's target. The module-inward guard
scanned only `crate::` paths; it now also rejects `super::` chains that climb above `cli`,
counting inline `mod` blocks, with negative fixtures for the outer host facade and the test
harness. Codex compared all thirty moved files with the base and found nothing dropped.

### Round 2 (2026-10-08, `ba592b4a` against `946e6d6d`)

Verdict: changes requested, no P1. One P2, repaired: the module-inward guard could be bypassed
by aliasing the crate root or an ancestor (`use crate as outer;`), so it now rejects such
aliases with fixtures for the outer host facade and the test harness. Codex confirmed the
first-round repairs, the thirty moved files, the dependency carry-over, the build surfaces and
the reader-guard scopes; the one non-resolving scope string it noted is a synthetic label for
an inline negative-test input, not a file read. The merge of main into the tranche's first PR
(after #363 landed) is carried forward through the stack with one doc conflict resolved
towards main's newer wording at the moved path.

### Round 3 (2026-10-08, `10e556bd` against `e60c90b0`)

Verdict: changes requested, no P1. One P2 and one P3, both repaired: the module-inward guard
still accepted grouped imports (`use super::{super::host, Cli};`) and whitespace variants, so it
is rebuilt on the Rust parser (`syn`, a dev dependency already in the lockfile): every path is
resolved from its first segments, `use` groups and renames are expanded, inline modules are
tracked while visiting, macro tokens and unparsable files fall back to a whitespace-normalised
text scan, and fixtures cover grouped, glob, renamed, whitespace, expression and macro forms. The
runtime purge block's metadata now names `backend/g5d/src/cli/host.rs`.

### Round 4 (2026-10-08, `5109d7ec` against `e60c90b0`)

Verdict: changes requested, no P1. Two P2, both repaired: the inward guard accepted a
crate-root alias through `extern crate self as outer;` and grouped imports inside macro bodies,
so it now rejects extern-crate aliases and parses macro bodies as statements before falling
back to the text scan, with fixtures for both; the CI unit-coverage assertion lists `g5d`
(which now owns the CLI unit suite) and checks the local lane runs its library tests and the
Cloud Build archive filter pairs `package(=g5d)` with `kind(=lib)`. Four rounds reached the
loop's limit; every round since the first found only further holes in the replacement guard,
none in the move itself, so one confirming round is run before merging on green CI.

### Round 5 (2026-10-08, `f8eb9e68` against `e60c90b0`)

Verdict: changes requested, no P1. Three P2, all follow-through on the replacement guard and
one test: the inward guard accepted the crate's own external name (`g5d::host`, reachable in
unit tests through the self dev-dependency); `macro_rules!` transcribers escaped the parsed
import check; and the verify-changed script's test dropped its CLI owner assertion without
adding `g5d`. Repaired: external `g5d` paths (plain, aliased, grouped, leading `::`) are
rejected, macro-rule transcribers are visited with the same import resolver, and the test pins
`backend/g5d/src/cli/mod.rs` as production source. Five rounds now; every round since the
first probed the guard rather than the move, so the guard is treated as converged on its parsed
form and the PR merges on green CI.

### Round 6 (2026-10-08, `61cff5ee` against `33ee50ac`)

Verdict: changes requested, no P1. Two P2, both again holes in the replacement inward guard:
`use g5d::*;` and `extern crate g5d;` slipped past the crate-name check, and `macro_rules!`
transcribers delimited by parentheses or brackets were not inspected. Repaired structurally:
the text fallback is replaced by a token-level scan of the whole file and of every transcriber
group (strings and comments are not tokens), the crate's own name is rejected in glob and
extern-crate forms, and fixtures cover each. Six rounds; every round after the first probed the
guard, never the move, so the guard is treated as converged on its parsed-plus-token form.

### Round 7 (2026-10-08, `2a73881d` against `33ee50ac`)

Verdict: changes requested, no P1. One P2: raw identifiers (`r#g5d`) compared unequal to the
crate name in both scans. Repaired by normalising every identifier before comparison, with
raw-identifier fixtures for path, alias and glob forms. Seven rounds; since round two every
finding has been another spelling of the same bypass of the replacement inward guard, and
Codex finds the move itself, the dependencies, the build surfaces, the reader guards and the
tracing map clean each time, so the loop is stopped here on the repeated shape and the PR
merges on green CI.

## Tranche 1 — crate consolidation, PR 2 (`g5-workspace-ops` into `g5-scm`)

Design: "Tranche 1, crates" of the [ontology plan](pr_ontology_diff_design/p8.5-crate-consolidation/ontology-plan.md).
Branch `kaihui/p8.5-crates-2`, PR #372, stacked on PR 1. Verified locally: `scripts/check-layout.py`,
workspace clippy, the `g5-scm` unit tests with the moved ones, the consumer-boundary test, the
`g5-api` guard tests, the `g5d-foundation` workspace tests, the `g5d-cli` init, add, push and
manifest tests, the g5d crate-architecture test (against the libkrun graphics stub), the
tracing-compat tests, `just commands-check`, the migration checksum catalog and the indexer
pipeline database test.

### Round 1 (2026-10-08, `bfaa4742` against `86d06e98`)

Verdict: approve; nothing introduced by the move. Codex confirmed the four moved files keep
their behaviour, tests and conditional compilation, that the only default-features-disabled
consumer still builds without libgit2, and that the reader guard treats `g5-scm/workspace/`
exactly as it treated the old crate while SCM mechanics keep their exemption. Three
pre-existing gaps the move makes relevant are repaired as follow-through: the worker-image
trigger did not watch `g5-scm` (nor the other shared crates the `g5d` binary links), the two
seed scripts' binary freshness inventories omitted it, and a dispatch doc named a root export
that lives under `native::configuration`. The plan records the module and guard decision.

### Round 2 (2026-10-08, `946e6d6d` against `f95ec192`)

Verdict: approve. No P1 or P2; one P3: the plan said the seed scripts' freshness inventories
now watch every shared crate the `g5d` binary links, but only the worker-image trigger does;
the seed scripts add `g5-scm` alone. The plan wording is corrected on the next branch up.
Converged after two rounds; merges on green CI.

## Tranche 1 — crate consolidation, PR 1 (`g5-common`, `g5d-runtime-core`, `g5-blob`, `g5-api-contracts`)

Design: "Tranche 1, crates" of the [ontology plan](pr_ontology_diff_design/p8.5-crate-consolidation/ontology-plan.md).
Branch `kaihui/p8.5-crates-1`, PR #370. Verified locally after each merge: `scripts/check-layout.py`,
workspace clippy, the consumer-boundary and moved tests, the `g5-api` guard and fence tests, the
moved unit tests, the sqlx cache check, the g5d crate-architecture test (run against a stub of
the libkrun graphics library this Mac lacks), `just commands-check` and the migration checksum
catalog.

### Round 1 (2026-10-08, `cc00501a` against `fca91168`)

Verdict: changes requested, no P1. Two P2 and one P3, all repaired: the runtime foundation's
dependency validator now rejects every `g5-api-*` crate, the guarantee the deleted
runtime-core validator carried, with a negative fixture; the historical `g5_api_contracts`
tracing targets map to `g5_api_foundation::contracts` in both directions, with a fixture test;
the deployment audit no longer implies a module moved that never existed. Codex compared all
seventeen moved files with the base and found them identical apart from paths and doc wording,
and found every build surface and the lockfile free of the four retired crates.

### Round 2 (2026-10-08, `86d06e98` against `fca91168`)

Verdict: approve. No P1 or P2; one P3 with two source comments still naming retired crates
(the CLI git module's credentials note and the frontend workbench type's source pointer),
repaired. Codex rechecked the seventeen moves (fourteen byte-identical, three with only path
and doc changes), the dependency and feature carry-over, the lockfile, every build surface,
the remaining one-way guards, the tracing mappings and the ontology edits. Converged after two
rounds; merges on green CI.

## Slice 4 — the human-escalation attention kind and its frontend

Design: tranche 0 item 4 of the [ontology plan](pr_ontology_diff_design/p8.5-crate-consolidation/ontology-plan.md).
Branch `kaihui/p8.5-dead-code-frontend`, stacked on slice 3b. Verified locally: clippy on the
API crates, the home, workspace and WebSocket unit tests, the home and workspace API tests on
fresh databases through migration 065, the populated-upgrade migration replay, the static route
contracts, the reader guards, the sqlx cache check, and the frontend type check, lint and
full unit suite. Remaining unexecuted coverage: the Home human-escalation scenarios had no
document-backed replacement, so Home pagination is covered by the approval and Project Doc
tests only.

### Round 1 (2026-10-08, `69a42e94` against `5eb28176`)

Verdict: changes requested, no P1. Three P2 findings that all trace to one fact the inventory
missed: the escalation-questions-doc capture has returned an empty list since the lean
reorganization and nothing writes those docs, so the "live path" the slice preserved (the
status band's Human Escalations tab, the Home `question_document` target, the
`humanEscalationOpenQuestions` and `escalationDocSources` wire fields) could never carry
data, the band's new navigation rows could never render, and the Home pagination tests had
no document-backed scenario to migrate to. Repaired by widening the cut: the escalation-doc
capture, summarizer and materializers, the `human_escalation` attention kind with its
`humanInputTarget`, the three wire fields and the band's tabs are removed with their tests;
the band modal is one list of Project Doc sources and Home attention has two kinds. The plan's
item 4 records the finding and the decision.

### Round 2 (2026-10-08, `8d2e2803` against `5eb28176`)

Verdict: changes requested, no P1. One P2 and one P3, both repaired: the mixed-kind Home
pagination test had collapsed to a single approval and the filtered-pagination test had been
deleted with its persisted fixtures; both are rebuilt on Project Doc question sources (one
approval plus two docs walked at page sizes one and two with uniqueness and membership
checks; three docs filtered by `kind=open_question` with the capture-pruning observation, a
mismatched-kind cursor rejected, and the retired `human_escalation` kind rejected as unknown).
The orphaned escalation-doc capture comment that had attached itself to the metrics capture
is deleted. Codex notes the review checklist still described the first cut's scope; the plan
now carries the widened one.

### Round 3 (2026-10-08, `62c330d3` against `5eb28176`)

Verdict: changes requested, no P1. One P2, repaired: deleting the escalation-doc reader
contract test had also dropped its assertions that the retained `open_question_metrics` entry
point and the committed branch-file read hold no tenant DB scope; those are restored as a
Project Doc metrics test. Three rounds; the third found only this follow-through, so a fourth
is run to confirm convergence per the loop's limit.

### Round 4 (2026-10-08, `a6a35ffe` against `5eb28176`)

Verdict: approve. No P1 or P2; one P3 comment in the client WebSocket module that still said
terminal tasks removed escalation documents from the question aggregate, now rewritten for the
Project Doc-only aggregate. Codex confirmed from the base that the escalation-doc capture was
never a functioning path, that nothing retained reads the dropped tables or the deleted events,
and that the rebuilt Home tests and the restored reader contract cover what the deletions
touched. Converged after four rounds; merges on green CI.

## Slice 3a — API dead services, routes, helpers, models and nine table drops

Branch `kaihui/p8.5-dead-code-api`, from main. Rounds follow.

### Round 1 (2026-10-08, `da214640` against `67ec37fd`)

Verdict: changes requested. No wrong deletion among the Rust and TypeScript consumers. Seven
findings, all repaired:

| # | Finding | Repair |
|---|---|---|
| 1 | P1 `seed-test-workspace.sh`, which the CI e2e lane runs, still inserted into `intent_record` and `ontology_grounding`; seeds still deleted from `task_checkpoints` | The step 7c SQL projection is removed (the JSONL fixtures in step 7d are what the API reads); the checkpoint deletes are removed from five seed scripts. |
| 2 | P1 `seed-prod-org.sh` and `reset-dev-environment.sh` still mint the bare-host registration token into `worker_tokens` under `ON_ERROR_STOP` | The table drop and its authz lookup function are withdrawn from migration 063; the admin routes stay deleted. The table goes with that provisioning in the P8.4 infra cleanup. |
| 3 | P2 `api_ontology_format.rs` and the frontend `api-smoke` test still called the three removed handlers; contract tests parsed their schemas | Four endpoint tests removed, two 404 tests retargeted to `read-ontology`; smoke cases, zod schemas and the format DTO contract tests removed. |
| 4 | P2 migration left the orphan trigger function `job_submission_receipts_touch_updated_at` | Dropped after its table; checksum entry regenerated. |
| 5 | P2 `EscalationScopeContext` and static contract entries for the removed helpers, service and handlers | Removed. |
| 6 | P2 the `dev` Cargo feature only fed the deleted `scoped_db_unscoped` | Feature and both forwardings removed. |
| 7 | P3 unused `sha2` import in the test fixtures | Removed. |

### Round 2 (2026-10-08, `cc26205e` against `67ec37fd`)

Verdict: changes requested. No retained production path broken; migration 063 confirmed safe.
Five findings, all repaired: the seed regression test that asserted on the deleted SQL generator
is removed; the ontology auth-boundary test is retargeted to `read-ontology`; `derive_id` and its
`TaskTriggerPayload` import go with `from_payload`; the frontend `CheckpointDTOSchema` and its
contract test are removed; two type imports left by the deleted format tests are removed.

### Round 3 (2026-10-08, `6eb7ba48` against `67ec37fd`)

Verdict: changes requested. No retained production path broken; migration and cache safety
confirmed. Two findings, both repaired: the ontology-format helper chain in `ontology_markup.rs`
(`validate_format`, `compute_uncertainty_summary`, `extract_tagged_blocks`,
`detect_protected_span_violations` with their result types and tests) had only same-module callers
left; the `read_node_content` guard token named a deleted helper.

### Round 4 (2026-10-08, `d29a731b` against `67ec37fd`)

Verdict: two P2, one P3, no P1. Repaired: the retained parser's fence regression test is
restored without its one `validate_format` assertion; the frontend format-validation types
that only the deleted hooks and contract tests used are removed; the authz crate and router
docs no longer describe the deleted org-guard middleware. Four rounds reached the loop's
limit; each round found only follow-through on the previous repair, no wrong deletion, so
the slice is declared converged and merges on green CI.

### CI follow-through (2026-10-08, `0ea61ac3`)

The first Rust lane that ran to completion on this slice failed one test: the populated
QA upgrade replay, which runs as the restricted migrator. That role owns the tables
(migration 045) but not the trigger function the original owner created, so migration
063's `DROP FUNCTION` was refused. The drop is now conditional on the migrator being able
to own the function; a fresh install still removes it, a populated upgrade leaves an inert
orphan. Earlier Rust lanes on this branch were cancelled as superseded before finishing,
which is why the failure surfaced only now.

## Slice 3b — uncalled routes and the workspace dependencies feature

Branch `kaihui/p8.5-dead-code-api-b`, stacked on 3a. PR #359.

### Round 1 (2026-10-08, `96a9b0e0` against `6eb7ba48`)

Verdict: changes requested, with one P1. `GET /auth/preview/reauth/:taskId` was wrongly listed as
uncalled: the auth middleware builds that URL when a preview cookie expires and the preview control
opens it. The route, its sanitizer and failure response are restored from the base and the design's
inventory corrected. Five further findings, all repaired: a second cross-branch comments test; five
stale SQL-guard exemptions for the deleted preflight loaders; the service methods, row types and
request type that only the deleted handlers used; mock handlers and fixture fields for the removed
dependencies and ontology diff/deps contracts; unused declarations in the sync-removal spec.

### Round 2 (2026-10-08, `67017f8b` against `6eb7ba48`)

Verdict: changes requested, no P1. Six findings, all repaired: the request, response and
query DTO clusters, conversions and service row types that only the deleted handlers used; the
auth middleware still admitted the deleted internal staleness route; the unused ontology-diff
mock fixture chain; two cache entries for deleted queries; the Git/governance owner table still
listed `get_node_dependencies`; and the e2e-only view-seed route, which the inventory wrongly
called a CI dependency, is removed with its handler. The repair's own orphan scan then found the
next layer in the doc and ontology services (folder DTOs, the isolated-sync and sync-chat
target resolvers, the generated-document projector, the node mutation and comparison helpers,
`UpdateNodeRequest`) and removed it with the unit tests that only exercised those helpers; the
two workspace API tests that still asserted the removed `dependencies` field were trimmed.

### Round 3 (2026-10-08, `4374c1cd` against `d29a731b`)

Verdict: changes requested, no P1. Four P2 and two P3, all repaired: the view-seed service
method survived behind an always-false flag on the create-doc handler, so the handler is
folded back to one path and the seeding-only name rule goes with it; the scope-check request
and response contracts of the deleted `/auth/check` route are removed with the contracts
crate's `g5-auth` dependency and its consumer-boundary entry; the delete-preflight and
sync-preflight DTO clusters, the comment service's user summary and the ontology service's
file reader lost their last callers; the frontend node-dependencies schemas, type, hook import
and test fixture are removed; the unused diff-data mock import and the stale cross-branch and
`dependencies` comments are cleaned. Merging slice 3a (now carrying main) in also showed the
uplift-sync feature label unreferenced once this slice's sync-preflight routes were gone, so
it is removed.

### Round 4 (2026-10-08, `e9ef447f` against `b971f9b9`)

Verdict: changes requested, no P1. Three P2 and one P3, all repaired: the committed-view
line-mapping batch, its request type, the SCM line mapper and its reader-guard entry, the
ahead/behind helper and the effective-view diff pair lost their last production callers with
the cross-branch and preflight routes and are removed with the tests that only exercised them;
`doc_folders` and `node_dependencies` had no reader or writer left once the folder and deps
routes went, so migration 064 (unmerged, re-checksummed) drops them with
`workspace_dependencies`; the deleted connection list's frontend schema is removed; the
ontology test header, comment-service and ontology-service comments and the boundaries block's
raw-SQL exemption count are corrected. Four rounds reached the loop's limit; each round found
only follow-through on the previous repair, no wrong deletion, so the slice is declared
converged and merges on green CI.

## Slice 2 — shared crates, engine-only e2e scenarios, validation package

### Round 1 (2026-10-07, `d6b51e08` against `828fbc38`)

Verdict: changes requested. No wrong deletion and no lost coverage confirmed; the removed
response field was always omitted. Six follow-through findings, all repaired:

| # | Finding | Repair |
|---|---|---|
| 1 | P2 `sha2` left in `g5-scm/Cargo.toml` after `worktree.rs` went | Dependency removed. |
| 2 | P2 dead helper chains: `is_active_wait_for_session`, `is_code_uplift_sync`, `normalize_intent_record_for_merge` with `canonical_content_from_stored_body`, `review_metadata_substantive_key`, `ProvenanceActor::local_git` and `display_string`, `validate_relationship`, `validate_relevance` | All removed with their dedicated tests. `CODE_UPLIFT_SYNC_FEATURE` stays: the workspaces route still reads it. |
| 3 | P2 `JsonlStore` instance API (constructor, paths, filesystem read/write/upsert/delete, directory scan) only constructed in its own tests | Reduced to the unit struct with `parse_all_strict`, `serialize_all` and the file-name helpers that services use. |
| 4 | P2 `scripts/run-rust-e2e-lanes.sh` kept a `grounding` selector and the engine explanation | Selector and comment removed. |
| 5 | P2 `docs/local-validation.md` still told readers to fetch the deleted package; preflight comments in the architecture tests | Paragraph rewritten to the remaining checks and the `g5d` target; comments trimmed. |
| 6 | P3 unused imports in `g5d/tests/helpers/mod.rs` and `scripts/test_validation_review.py` | Removed. |

Verification after repair: `cargo check --workspace --all-targets`, clippy `-D warnings` on the
touched crates, `g5-contracts` 426 and `g5-semantic` 224 unit tests, the 8 consumer-boundary
tests and the Python harness tests pass.

### Round 2 (2026-10-07, `b536a187` against `828fbc38`)

Verdict: changes requested. Again no wrong deletion and no lost coverage. Four follow-through
findings, all repaired:

| # | Finding | Repair |
|---|---|---|
| 1 | P2 `natural_key`, `matches_natural_key`, `IntentNaturalKey` and `ProvenanceActor::unknown` kept only same-file test callers once the engine and resolver went | Removed with their test assertions; serialization and vocabulary tests kept. |
| 2 | P2 `tracing` and `tempfile` left in `g5-semantic/Cargo.toml` after the filesystem store methods went | Both removed. |
| 3 | P2 the e2e API client still deserialized `view_context` through `ViewTaskContextResponse`, which no producer can emit | Field and struct removed. |
| 4 | P3 architecture-test comment still named the deleted standalone runner | Comment updated. |

Slice 1 follow-up found by CI on #346 in the same window: `migrated_cli_native_operations_use_shared_runner`
still listed the deleted `g5d-cli/src/job.rs`; the entry is removed on the slice-1 branch and slice 2
is rebased on it.

### Round 3 (2026-10-07, `cb11e9ce` against `c7d99d20`)

Verdict: changes requested. Two findings, both repaired: `scripts/test_rust_e2e_lanes.py` still
expected the removed `grounding` lane (P2, the e2e CI lane runs that test), and an empty
`impl GroundingEdge {}` remained (P3). No wrong deletion and no lost coverage.

### Round 4 (2026-10-07, `7d05f168` against `c7d99d20`)

Verdict: **approve**. No wrong deletion, no lost coverage, no newly orphaned code or dependency.
One P3: this record claimed `CODE_UPLIFT_SYNC_FEATURE` was removed; it remains for its two
callers in `routes/workspaces.rs`, and the round-1 row above now says so.

## Slice 1 — runtime CLI commands and orphan modules (merged as #346)

### Round 1 (2026-10-07, `c7d99d20` against `77687360`, run after the merge)

Verdict: changes requested. No wrong deletion and no lost coverage: push and workspace-refresh
coverage remain. Four follow-through findings, folded into slice 2 (#351): the orphaned
`ApiClient::put`/`do_put` helpers and the constant `allow_dev_auth_fallback` field; the orphaned
`tracing-subscriber` dev-dependency of `g5d-worker`; the reader-guard exceptions and synthetic
cases that named the deleted `job.rs`, `sync_repo_state`, `do_post_action` and `do_put`; and the
`g5-internal-api` crate docs that still advertised the AQL compiler.
