# G5-Lean implementation plan

Status: the draft 6 plan, written 2026-09-13 for
[lean-backend-design-v6.md](lean-backend-design-v6.md); superseded on 2026-09-17 by
[lean-implementation-plan-v7.md](lean-implementation-plan-v7.md), which keeps its PR
ids, and kept for history. The tracker file was recreated for plan v7 at the switch; the status rows this plan's prompts refer to are now the v7 rows.

This plan breaks the design into pull requests that a coding agent can complete
in one isolated session each, states the order in which they land and which
can run in parallel, and gives a self-contained prompt for every PR. The
design is authoritative; when a prompt and the design disagree, the design
wins and the prompt should be corrected.

---

## 1. How to use this plan

**For the person running the program.** Land PRs wave by wave (§4). Every PR
in a wave may be started in parallel by a separate agent; a PR may start as
soon as everything it depends on has merged to `main` on
https://github.com/G5Labs/G5-Lean. Agents open pull requests there (§2.9);
you review and merge them on GitHub. Before starting a wave, re-read the
status file and confirm the dependencies are merged and their validation
was run.

**For an agent starting a PR.** Copy the PR's prompt from §5 into a fresh
session in this repository. The prompt tells you what to read first, what to
build or delete, what not to touch, how to prove it works, and how to finish.
The conventions in §2 apply to every PR and are part of the prompt by
reference.

**Sizing.** S under 1K changed lines, M 1K to 4K, L 4K to 10K, XL over 10K.
Deletion-heavy PRs are XL by line count but are mechanical; the reviewer's
job there is checking that nothing retained still references the deleted
code, which the compiler and tests do for us.

---

## 2. Conventions for every PR

1. **Start from the latest `main`, and only when your dependencies are in.**
   Your PR's "Depends on" list must already be merged on GitHub. Check with
   `gh pr list --repo G5Labs/G5-Lean --state merged --search "P<x.y>"` (or the
   status file on `origin/main`); if a dependency is still open, stop and
   report instead of starting. Then fetch and branch from `origin/main`, never
   from a local `main` that may be stale:
   `git -C /Users/zliu/Workspace/G5-Lean fetch origin main && git -C /Users/zliu/Workspace/G5-Lean worktree add ../G5-Lean-wt-<pr-id> -b lean/<pr-id>-<slug> origin/main`.
   Work in that worktree, never in the primary checkout (the remote is
   `origin = https://github.com/G5Labs/G5-Lean`, pushed over SSH as
   `git@github.com:G5Labs/G5-Lean.git`). Run `git branch --unset-upstream`
   after creating the branch so a bare `git push` can never target `main`.
   Do not touch other worktrees. Before your first build, read
   "Pre-existing conditions" at the end of `docs/lean-implementation-status.md`:
   it records known failures on `main` (for example, a fresh checkout needs
   50 ignored files copied under `assets/agents/claude-code-g5/.claude/`
   until the follow-up PR that tracks them lands) so you do not spend time
   rediscovering them or mistake them for your own breakage.
2. **Keep your branch current.** Other PRs in your wave merge while you
   work. Before you open the PR, and again after every round of review
   changes, run `git fetch origin && git rebase origin/main` in your worktree
   (rebase, never merge `main` into your branch, so history stays linear).
   Resolve conflicts yourself; the usual hotspots are your PR's section of
   `001_lean_baseline.sql`, your row in `docs/lean-implementation-status.md`,
   and `Cargo.lock`. If the rebase brought in changes to a crate you touched,
   re-run the validation ladder for that crate before pushing. Push with
   `git push --force-with-lease` after a rebase. Never rebase or amend after
   the PR is approved unless the reviewer asks; the program owner merges from
   GitHub, so an approved branch must stay exactly what was reviewed.
3. **Read first.** Every prompt lists design sections and research notes.
   Read them before opening code; they carry `file:line` pointers that save an
   hour of rediscovery. Line numbers drift; symbol names are stable.
4. **Scope discipline.** Do exactly what the PR says. If you find that the
   PR cannot be completed without a change listed under "Do not", stop and
   report it in the PR description under "Blocked"; do not widen the PR.
   Unrelated cleanups go into a note in the status file, not into the diff.
5. **Buildable at every merge.** The workspace must compile and the retained tests must pass at the end of the PR. Retiring a command or API requires updating its retained test workflows and fixtures in the same PR. A `// LEAN-TODO(P<x.y>)` marker may track future cleanup, but cannot exempt a failing retained test or defer a broken CI lane to P8.6. Both Cloud Build checks must pass on the current PR head before merge, including after a rebase. Shared failures must be repaired before merging dependent PRs.
6. **Schema edits.** QA has an applied SQLx baseline. The one-time baseline
   edit batch (#35, #44, #46 and #47) merged on September 16, 2026 and is now
   closed. Migration files are immutable: use `002_*.sql`, `003_*.sql`, etc.
   The CI/CD owner coordinates QA's reset to the combined baseline while
   deployment is paused. This rule supersedes
   task prompts below that say to edit/rewrite the baseline. Never update an
   applied migration's checksum to silence SQLx or CI. Follow
   [the migration policy](database-migrations.md), including the checksum
   catalog and fresh-install/upgrade checks. Regenerate the SQLx offline
   cache when queries change (`just db-prepare`, with `just dev-up`).
7. **Tests.** Add the acceptance tests named in the prompt. Retarget or
   delete tests only for behavior the PR removes; never delete a test to make
   a retained behavior pass. Fake providers: after P3.1, tests point
   `G5_CLAUDE_PATH` / `G5_CODEX_PATH` at the fake binaries.
8. **Validation ladder** (run what applies; the prompt says which are
   required):
   - `SQLX_OFFLINE=true cargo check --workspace --all-targets --offline`
   - `just fmt-check` and `just lint`
   - `SQLX_OFFLINE=true cargo test -p <crate> --lib` for touched crates
   - `just test-integration` (needs `just dev-up`) when routes or services change
   - `npm --prefix frontend run build && npm --prefix frontend test` when the frontend changes
   - `just docker-ci` for PRs that touch worker, host, isolation, or CI configuration
   - `python3 scripts/check-layout.py` and `just commands-check` when files move or recipes change
9. **Commits and PR description.** Conventional subject line `<area>: <what>` (for example `git: enforce workspace-branch push policy`). End every commit message with `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>`. The PR description uses the template in §2.1 and ends with `🤖 Generated with [Claude Code](https://claude.com/claude-code)`. When the work is validated, push the branch and open the pull request on GitHub: `git push -u origin lean/<pr-id>-<slug>` then `gh pr create --repo G5Labs/G5-Lean --base main --head lean/<pr-id>-<slug> --title "P<x.y> <title>" --body-file <file with the §2.1 template>`. Record the PR URL in your status row. Do not merge your own PR; the program owner merges on GitHub after review. Cloud Build publishes the `g5-ci-rust` and `g5-ci-e2e` GitHub PR checks. Both must pass on the current head. Also list the local validation ladder results in the PR description; local validation supplements these checks.
10. **Status file.** Before finishing, update your row in
   `docs/lean-implementation-status.md`: state (`in review` once the PR is
   open), branch and PR URL in the Branch column, what landed, what was left
   as `LEAN-TODO`, and any deviation from the prompt. Make that update in
   the same PR.
11. **No deployment.** Nothing in this plan deploys anything. Cloud Build
    stays the CI system; changes to it are P8.6 only.

### 2.1 PR description template

```
## P<x.y> <title>

Design: lean-backend-design-v6.md §<sections>
Depends on: P<..> (merged)

### What changed
- ...

### Deleted
- ...

### Tests
- added: ...
- retargeted: ...
- removed (behavior removed by this PR): ...

### Validation run
- cargo check / lint / fmt: pass
- <crate> tests: pass (N tests)
- integration / docker-ci: pass | not run (why)

### LEAN-TODO markers left
- ...

### Blocked / deviations
- none
```

---

## 3. Deviations from the design's phase text

The design's phase list (§13) is the reference; four sequencing details are
adjusted here because they cannot be built in the stated order without
throwaway work. None changes the target architecture.

1. **Workers push natively in P2.3, not in Phase 1.** While the CRDT still
   exists, commits are produced from the CRDT document. Phase 1 makes git refs
   authoritative and applies commits with a direct ref update; Phase 2 moves
   the producers (agents) to native `git commit && git push`.
2. **Job and task branches are removed in P4.8, not in Phase 1.** They are
   consumed by task completion and job merge, which change in Phase 4;
   deleting them earlier would require throwaway shims. Job merge is deleted
   in P4.8 for the same reason, ahead of the rest of the governance cleanup.
3. **No temporary "live" read from per-task VMs in Phase 2.** There is no RPC
   into today's per-task VM; building one would be thrown away in Phase 4.
   Between P2.1 and P4.10 the UI reads `source: committed` only, and Save is
   an interim server-side commit to the workspace branch. Live reads and
   `fs.write` arrive with the dispatcher.
4. **The six-state task model is built in P4.6, not in Phase 7.** The dispatch
   and event contract cannot sit on the old 12-state kernel. Phase 7 keeps
   the enum alignment, kernel deletion, and the baseline rewrite.

---

## 4. PR map, dependencies, and waves

### 4.1 PR table

| PR | Title | Phase | Size | Depends on |
|---|---|---|---|---|
| P0.1 | Baseline tag, before-metrics, status tracker | 0 | S | — |
| P0.2 | Squash 292 migrations into `001_lean_baseline.sql` | 0 | M | P0.1 |
| P1.1 | Git refs authoritative for reads | 1 | L | P0.2 |
| P1.2 | Receive-pack policy: workspace branches, protected refs | 1 | M | P0.2 |
| P1.3 | Projection convergence loop and `projection_state` | 1 | M | P1.1 |
| P1.4 | Delete the head-operation registry and staging refs | 1 | XL | P1.1, P1.2, P1.3 |
| P1.5 | Remove remote tracking; add protected-branch mirroring and Publish | 1 | XL | P1.4 |
| P1.6 | Frontend: remove remote-tracking UI, add Publish | 1 | M | P1.5 |
| P2.1 | Interim Save API (server-side commit) and committed-only reads | 2 | M | P1.4 |
| P2.2 | Frontend explicit Save; remove CRDT from editors | 2 | XL | P2.1 |
| P2.3 | Worker native writes and native push at task end | 2 | L | P1.2 |
| P2.4 | Delete the backend CRDT layer | 2 | XL | P2.2, P2.3 |
| P2.5 | Agent profiles rewritten for native git | 2 | M | P2.3 |
| P3.1 | Claude CLI pin bump; fake `claude` and `codex` binaries | 3 | M | P0.1 |
| P3.2 | Rust provider runner; delete both Node sidecars | 3 | XL | P3.1, P2.3 |
| P3.3 | `task_questions`, `ask_user`, `request_permission`, answer polling | 3 | L | P3.2 |
| P3.4 | `chat_session_files` harvest and restore | 3 | M | P3.2 |
| P3.5 | Remove `naive_react` and the `fake` provider value | 3 | M | P3.2 |
| P4.1 | Schema: `workspace_vms`, execution identity, `task_events` | 4 | S | P0.2, P2.4 |
| P4.2 | `g5d dispatch`: RPC listener, fs/git/proc, event journal | 4 | XL | P3.2, P3.3, P3.4 |
| P4.3 | Checkpoint task and restore | 4 | L | P4.2, P1.2 |
| P4.4 | `g5d host`: VM lifecycle commands, tokens, reconnect | 4 | L | P4.2 |
| P4.5 | API: VM lifecycle, four actions, stop barrier, fence | 4 | L | P4.1, P4.4, P4.3 |
| P4.6 | Dispatch loop, two-phase starts, event contract, reconciler | 4 | L | P4.5 |
| P4.7 | Chat turns on the dispatcher; delete sticky runtimes | 4 | XL | P4.6 |
| P4.8 | Agent tasks and jobs on the dispatcher; delete job/task branches and job merge | 4 | XL | P4.6 |
| P4.9 | Delete per-task execution, golden images, workers table | 4 | XL | P4.7, P4.8 |
| P4.10 | Frontend: workspace VM status, live reads, Changes, Commit/Push | 4 | L | P4.6 |
| P5.1 | Preview on the workspace VM | 5 | L | P4.5 |
| P5.2 | Browser terminal | 5 | M | P4.4 |
| P6.1 | `pull_requests`, `approval_rules`, `approval_decisions`, `g5d pr` | 6 | L | P1.5, P4.1 |
| P6.2 | The merge operation | 6 | L | P6.1 |
| P6.3 | Governance frontend | 6 | L | P6.1 |
| P6.4 | Delete governance legacy | 6 | XL | P6.2, P6.3 |
| P7.1 | Lifecycle cleanup: one status enum, delete the kernel | 7 | L | P4.9, P6.4 |
| P7.2 | Rewrite the baseline schema to the final table set | 7 | M | P7.1 |
| P8.1 | Remove internal issue tracking | 8 | XL | P3.2 |
| P8.2 | Remove IDE mockup, hidden settings UI, takeover remnants | 8 | M | P0.1 |
| P8.3 | GitHub-only platform login | 8 | M | P0.1 |
| P8.4 | Infra cleanup: dormant MIG, customer images, disabled workflows | 8 | M | P4.9 |
| P8.5 | Crate consolidation | 8 | XL | P7.2 |
| P8.6 | CI review | 8 | S | P8.5 |

### 4.2 Dependency graph

```
P0.1 ─► P0.2 ─┬─► P1.1 ─► P1.3 ─┐
              ├─► P1.2 ─────────┼─► P1.4 ─► P1.5 ─► P1.6
              │        └─► P2.3 ─┤    │
              │                  │    └─► P2.1 ─► P2.2 ─┐
              │                  │                       ├─► P2.4 ─► P4.1 ─┐
              │                  └──────────────────────┘                 │
P0.1 ─► P3.1 ─┬────────────────────► P3.2 ─┬─► P3.3 ─┐                    │
              (P2.3 also required)         ├─► P3.4 ─┼─► P4.2 ─┬─► P4.3 ──┼─► P4.5 ─► P4.6 ─┬─► P4.7 ─┐
                                           ├─► P3.5  │         └─► P4.4 ──┘                 ├─► P4.8 ─┼─► P4.9 ─► P8.4
                                           └─► P8.1  │             │                        ├─► P4.10 │
                                                     │             └─► P5.2                 └─► P5.1  │
P1.5 + P4.1 ─► P6.1 ─┬─► P6.2 ─┐                                                                      │
                     └─► P6.3 ─┴─► P6.4 ─────────────────────────────────────────────────────────────┴─► P7.1 ─► P7.2 ─► P8.5 ─► P8.6
P0.1 ─► P8.2, P8.3 (any time)
P2.3 ─► P2.5 (any time after)
```

### 4.3 Waves (what can run in parallel)

| Wave | PRs that can start together | Notes |
|---|---|---|
| 0 | P0.1, then P0.2 | sequential |
| 1 | P1.1, P1.2, P3.1, P8.2, P8.3 | five agents |
| 2 | P1.3, P2.3 | after P1.1 and P1.2 respectively |
| 3 | P1.4, P3.2, P2.5 | P1.4 needs P1.1 to P1.3; P3.2 needs P3.1 and P2.3 |
| 4 | P1.5, P2.1, P3.3, P3.4, P3.5, P8.1 | P1.5 and P2.1 both follow P1.4; P3.x follow P3.2 |
| 5 | P1.6, P2.2, P4.2 | P4.2 needs P3.2 to P3.4 |
| 6 | P2.4, P4.3, P4.4 | P2.4 needs P2.2 and P2.3 |
| 7 | P4.1, then P4.5 | P4.1 after P2.4; P4.5 after P4.1, P4.3, P4.4 |
| 8 | P4.6, P5.2 | |
| 9 | P4.7, P4.8, P4.10, P5.1, P6.1 | five agents; P6.1 needs P1.5 and P4.1 |
| 10 | P4.9, P6.2, P6.3 | |
| 11 | P6.4, P8.4 | |
| 12 | P7.1, then P7.2 | sequential |
| 13 | P8.5, then P8.6 | sequential |

Critical path: P0 → P1.1 → P1.3 → P1.4 → P2.1 → P2.2 → P2.4 → P4.1 → P4.5 →
P4.6 → P4.7/P4.8 → P4.9 → P7.1 → P7.2 → P8.5.

---

## 5. PR prompts

Each prompt is self-contained apart from §2 of this document, which it
references. Paths are relative to the repository root.

### P0.1 — Baseline tag, before-metrics, status tracker

```
You are implementing P0.1 of docs/lean-implementation-plan-v1.md in the G5-Lean
repository at /Users/zliu/Workspace/G5-Lean. Read §2 (conventions) of that plan
first, then docs/lean-backend-design-v6.md §13 (Phase 0).

Goal: freeze the starting point so later PRs can measure what they removed.

Do:
1. Tag the current main as `lean-before` (annotated tag, message "G5-Lean
   baseline before the v6 re-architecture").
2. Record "before" metrics in docs/lean-implementation-status.md §Baseline:
   lines of Rust under backend/ (find backend -name '*.rs' | xargs cat | wc -l),
   lines of TypeScript under frontend/src, number of migration files, number of
   distinct CREATE TABLE names across migrations, number of Rust crates in
   Cargo.toml, and the sizes of the ten largest source files. Use the exact
   commands and paste their output.
3. Confirm the validation ladder works on main: run `SQLX_OFFLINE=true cargo
   check --workspace --all-targets --offline`, `just fmt-check`, `just lint`,
   `npm --prefix frontend run build`, `npm --prefix frontend test`. Record the
   outcome in the status file. Do not fix failures beyond what is needed to
   describe them; if something is red on main, note it as a pre-existing
   condition.
4. Fill in the status-file rows for every PR in the plan with state "not
   started" (the file already has the table; verify it matches §4.1 of the
   plan and fix any mismatch).

Do not: change any application code, migrations, or CI configuration.

Acceptance: the tag exists; the status file has real numbers and the
validation outcome; `git diff --stat` touches only docs/.
Validation: none beyond step 3.
```

### P0.2 — Squash 292 migrations into one baseline

```
You are implementing P0.2 of docs/lean-implementation-plan-v1.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, then design v6 §12 (last
paragraph) and §13 Phase 0. Depends on P0.1 (merged).

Goal: replace every file under backend/g5-api/src/db/migrations (289 files on
`main` after P0.1; the design's "292" counted the pre-relocation checkout) with a
single `001_lean_baseline.sql` that produces an identical schema on a fresh database.
There is no deployed data; compatibility with existing databases is not a
goal.

Do:
1. Bring up a fresh Postgres from the dev stack (`just dev-up`, or start only
   the postgres service from infra/docker/docker-compose.dev.yml), run the
   existing migrator (backend/g5-api/src/bin/g5-db-migrate.rs; see
   backend/g5-api-data/src/db.rs where `sqlx::migrate!` embeds the directory)
   to apply all existing files.
2. Dump the result: schema (`pg_dump --schema-only --no-owner --no-privileges`
   is not enough because migrations 017/048/080/081/085 create roles, grants,
   SECURITY DEFINER functions and RLS policies; use `pg_dump --schema-only`
   with privileges, plus `pg_dumpall --roles-only` for the runtime roles
   created by 017_role_provisioning.sql, plus `--data-only` for seed rows such
   as 044_seed_default_g5_org.sql and any lookup tables). Assemble one
   idempotent-on-fresh-DB file `001_lean_baseline.sql` in migration order:
   roles, extensions, schemas (`authz`), tables, functions, triggers,
   policies, grants, seed data. Remove `_sqlx_migrations` bookkeeping from the
   dump.
3. Delete the 292 old files. Keep the directory and the `sqlx::migrate!`
   invocation.
4. Verify: on a second fresh database, apply only the new file with the
   migrator, then compare `pg_dump --schema-only` of both databases after
   normalizing (sort, strip comments). They must match. Check the runtime
   roles and grants used by g5-api's three pools (`G5_DATABASE_URL`,
   `G5_AUTHZ_DATABASE_URL`, `G5_TENANT_DATABASE_URL`) still work: run
   `just test-integration`.
5. Fix or delete tests and scripts that assert on migration numbers or
   files: backend/g5-api/tests/migration_*.rs, *_migration.rs,
   scripts/check-migration-version-uniqueness.sh, scripts/rollback-sqlx-migration.sh,
   scripts/ci-provision-migrator.sh and anything grep finds for
   "migrations/2" or specific migration numbers. Delete migration-specific
   tests (their behavior is now the baseline); keep tests that exercise
   behavior through the schema.
6. Add a section marker convention comment at the top of the baseline
   explaining the `-- P<x.y> begin/end` rule from plan §2.6.

Do not: change any table, column, function, or policy. The point is a
byte-for-byte equivalent schema in one file.

Acceptance: the normalized dumps match; `just test-integration` passes;
`.sqlx` offline cache is unchanged (queries did not change); `just
docker-ci` phases that bootstrap the database pass.
Validation: cargo check, lint, fmt, `just test-integration`, `just docker-ci`.
```

### P1.1 — Git refs authoritative for reads

```
You are implementing P1.1 of docs/lean-implementation-plan-v1.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v6 §4.3 (invariant 1),
§5.1 to §5.3, and docs/research/research-git-workspace.md §C and §D. Depends on
P0.2.

Goal: every read of workspace content resolves the repository's head from the
git ref `refs/heads/<branch>` in the internal mirror, not from
`workspace_branch_repos.head_commit`. Postgres stops being the head authority
for reads.

Do:
1. Add a `RepoStorage`/`GitRepo` helper that reads a ref OID for a repo
   (`backend/g5-storage`, `backend/g5-scm/src/repo.rs`), used by the API.
2. In backend/g5-api-core/src/services/workspace_reader.rs, make
   `resolve_committed_snapshot`, `RepoBindingVersion.head_commit`, and every
   `CommittedRepoReadView` construction take the OID from the ref of
   `workspace_branches.branch_name` in the member repo. Keep the overlay
   plumbing (CRDT) untouched; only the committed base changes.
3. Update backend/g5-api-routes/src/routes/files.rs, docs.rs, ontology.rs,
   source_control_change_graph.rs and any other reader that consults
   `head_commit` columns for content reads. `workspace_branch_repos.head_commit`
   stays as a column for now (P1.3 turns it into a cache); readers must not
   depend on it.
4. Remove `reconcile_branch_ref_to_authoritative` and `project_branch_ref` as
   sources of truth: a ref that disagrees with the column is correct, the
   column is stale. Do not delete the finalizer yet (P1.4).
5. Add tests in backend/g5-api/tests (extend api_workspaces.rs or add
   git_refs_authoritative.rs): move a workspace branch ref with plain git on
   the mirror, do not touch the DB, and assert file, tree, docs, and ontology
   reads follow the ref.

Do not: touch routes/git.rs (P1.2), advance/notifier/consumer (P1.3), the
head-operation tables (P1.4), or any CRDT code.

Acceptance: the new tests pass; existing api_workspaces, api_drafts,
workspace_overlay_drafts tests pass (drafts still work on top of the
ref-resolved base); no reader references `head_commit` for content.
Validation: cargo check, lint, fmt, `cargo test -p g5-api-core --lib`,
`just test-integration`.
```

### P1.2 — Receive-pack policy

```
You are implementing P1.2 of docs/lean-implementation-plan-v1.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v6 §5.2, §5.3, §10.1, and
docs/research/research-governance.md §A.5 and §E.2. Depends on P0.2.

Goal: replace the staging-ref-only push rule in
backend/g5-api-routes/src/routes/git.rs with the policy table of design §5.3.

Do:
1. Add `repos.protected_branches text[]` to the baseline schema (default:
   `{default_branch}`), plus a helper that returns the protected set for a
   repo (default branch, `protected_branches`, and every branch a base
   workspace `is_base=true` tracks).
2. In git.rs `git-receive-pack`: for each ref update decide per the table:
   protected refs denied for every network identity; `refs/heads/<workspace
   branch>` allowed for a user with write access to that workspace and for a
   worker/task token whose task belongs to that workspace (interim identity
   until P4.4 introduces the workspace token); allow fast-forward and
   non-fast-forward updates whose `old_sha` equals the current ref value;
   refuse zero `old_sha` on an existing ref; refuse deletes; refuse tags and
   other namespaces; keep `X-G5-Force-Push` refused. Reserve
   `refs/g5/checkpoints/<workspace id>` as denied for now (P4.3 enables it for
   the workspace token). Remove `X-G5-Operation-ID`, `require_provisional_push`,
   `authorize_staging_ref`, and `mark_staging_ref_uploaded` from the route;
   leave the finalizer functions in place for P1.4 to delete.
3. After a successful receive-pack, publish `workspace.branch_updated` on the
   existing EventBus (backend/g5-api-core/src/ws/events.rs already has the
   event) with `{repo_id, ref, old, new, actor}`; this is a hint only.
4. Keep `git-upload-pack` authorization as is.
5. Tests in backend/g5-api/tests/api_git.rs: protected ref denied for user
   and worker; workspace branch FF accepted; non-FF accepted with matching
   old sha; zero old sha on existing ref refused; delete refused; tag
   refused; other workspace's branch refused; `X-G5-Force-Push` refused; a
   push whose old sha is stale (ref moved during negotiation) gets 409.
   Document in a test comment that a plain `--force` after negotiation
   passes the check by design (design §5.3, decision C).

Do not: change how commits are produced (drafts, workers), delete the
finalizer, or touch remote tracking.

Acceptance: api_git tests pass; the existing e2e scenario
tests/e2e/src/scenarios/smart_http_git.rs passes or is retargeted to the new
rules; `just test-integration` passes.
Validation: cargo check, lint, fmt, `just test-integration`.
```

### P1.3 — Projection convergence

```
You are implementing P1.3 of docs/lean-implementation-plan-v1.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v6 §9.6, §5.3 (last
paragraph), and docs/research/research-git-workspace.md §C (advance notifier,
consumer, handoff tables). Depends on P1.1.

Goal: projections (search reindex, docs, ontology, `head_sha_cache`) are
rebuilt from authoritative ref OIDs by a convergence loop; events are hints.

Do:
1. Add `projection_state(projection, repo_id, ref, applied_oid, applied_at,
   PRIMARY KEY (projection, repo_id, ref))` to the baseline.
2. Implement backend/g5-api-core/src/maintenance/projection_convergence.rs:
   at startup and every 60 s, for every repo referenced by a live workspace,
   list `refs/heads/*` OIDs via git2 on the mirror, compare with
   `projection_state`, and for each difference enqueue or run the projection:
   search reindex (existing `search_reindex_queue`), docs
   (`DocService::sync_committed_docs_for_branch`), ontology
   (`OntologyService` sync-specs reconciliation), and update
   `workspace_branch_repos.head_commit` as a cache. Remove projection rows
   for refs that no longer exist. Register the loop in
   backend/g5-api-core/src/maintenance/mod.rs.
3. Subscribe the loop to `workspace.branch_updated` so a push triggers an
   early pass for that repo.
4. Delete backend/g5-api-core/src/services/advance_notifier.rs and
   advance_consumer.rs, the tables `workspace_advance_adapter_receipts` and
   `workspace_advance_consumer_inputs`, and the `advance_id` columns on
   `search_reindex_queue` and `branch_integration_queue` (edit the baseline).
   Where callers of the notifier/consumer remain (draft commit, task
   completion, fork), replace the call with "publish the hint event"; the
   loop does the rest. Leave `LEAN-TODO(P1.4)` markers where a caller sits
   inside the finalizer that P1.4 deletes.
5. Tests: backend/g5-api/tests/projection_convergence.rs — update a ref with
   plain git and no event, run one loop iteration, assert search queue rows,
   docs projection, ontology projection, and head cache are updated; delete a
   ref and assert projection rows are removed; a dropped event converges
   within one iteration.

Do not: delete the finalizer or head-operation tables (P1.4); change git.rs.

Acceptance: new tests pass; existing docs/ontology/search tests pass;
`workspace_advance_*` no longer referenced anywhere.
Validation: cargo check, lint, fmt, `cargo test -p g5-api-core --lib`,
`just test-integration`.
```

### P1.4 — Delete the head-operation registry

```
You are implementing P1.4 of docs/lean-implementation-plan-v1.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2 and §3 (deviation 1), design v6
§3.1 (authority row), §5.3, and docs/research/research-git-workspace.md §C,
§F, §G. Depends on P1.1, P1.2, P1.3.

Goal: remove the two-phase head-operation protocol. Commits produced by
server-side flows land by a direct compare-and-swap ref update on the mirror.

Do:
1. Introduce one server-side helper `apply_branch_commit(repo_id, branch,
   expected_old_oid, new_commit_oid)` in backend/g5-workspace-ops (or
   g5-scm): CAS update of `refs/heads/<branch>` via git2, publish
   `workspace.branch_updated`. Every producer below uses it.
2. Reroute producers: draft commit and commit-all
   (backend/g5-api-core/src/services/draft_service.rs `commit_draft`,
   `commit_all_for_worker_task`), workspace fork creation
   (clone_service.rs `push_create_ref`), docs initial commit
   (doc_service.rs `prepare_initial_api_workspace_operation`), and the
   worker's `publish-branch-state` internal routes
   (backend/g5-api-routes/src/routes/internal.rs `prepare_publish_branch_state`
   / `finalize_publish_branch_state`): the worker uploads objects with plain
   `git push` to the branch (allowed since P1.2) and the finalize route
   becomes a thin "record completion" call that reads the ref. Keep the
   worker CLI (`g5d task complete`) working by changing its client side in
   backend/g5d-cli/src/task.rs and push.rs to push refs/heads directly
   (P2.3 rewrites completion fully; here only the transport changes).
3. Delete: backend/g5-api-core/src/services/workspace_finalizer.rs, the
   head-operation parts of workspace_ops.rs, routes for
   `/workspaces/:id/head-operations*`, tables `workspace_head_operations`,
   `workspace_head_operation_repos`, `_source_repos`, `_membership_proposals`,
   `_make_remote_applications`, all eight `pr1884_*` tables,
   `workspaces.authority_advance_seq`, `workspace_branches.{authority_scope_kind,
   job_id, task_id, authority_scope_id}` (edit the baseline), the
   `AuthorityTarget`/`CrdtScope`/`RepoBindingVersion` DTO parts that only the
   registry used (backend/g5-contracts/src/types/dto.rs), CLI
   backend/g5d-cli/src/git/head_reconciliation.rs and
   connected_head_port.rs, `g5d workspace push`, and the maintenance loop
   `workspace_operation_resolver`.
4. Delete tests exclusive to the registry (workspace_authority_*.rs,
   workspace_head_operation_migration.rs, task_to_job_publication.rs parts
   that assert staging refs) and retarget the ones that assert a commit lands
   (they now assert the ref).

Do not: touch CRDT document code beyond rerouting its commit call; touch
remote tracking (P1.5).

Acceptance: `grep -r "head_operation\|staged/\|refs/g5/committed" backend`
returns only the baseline's comment history; drafts commit-all, fork, docs
creation, and worker task completion still land commits (integration tests
and tests/e2e scenarios `connected_crdt_commit.rs`, `crdt_draft_ops.rs`,
`doc_creation_persistence.rs` pass).
Validation: cargo check, lint, fmt, `just test-integration`, `just docker-ci`.
```

### P1.5 — Remove remote tracking; add protected-branch mirroring and Publish

```
You are implementing P1.5 of docs/lean-implementation-plan-v1.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v6 §5.5, §3.2 (Make
Remote row), and docs/research/research-git-workspace.md §C.3 step 4, §C.4,
§G. Depends on P1.4.

Goal: the only provider interactions are (a) inbound mirroring of protected
branches and (b) explicit server-side publish of a workspace branch.

Do:
1. Inbound: backend/g5-api-core/src/maintenance/protected_branch_mirror.rs —
   every 5 minutes (configurable) and on the provider webhook if one exists
   (grep for webhook handling; `repo_secrets.webhook_secret_enc` suggests one),
   fetch `refs/heads/<protected>` from the provider into the mirror with the
   connecting user's stored token (`ScmTokenManager`), under a per-repo lock
   that P6.2 will share. Publish the branch hint on change.
2. Outbound: `POST /api/v1/workspaces/:id/publish` (scope workspaces:write):
   push every member repo's workspace branch to the provider with the acting
   user's token and `--force-with-lease`, same branch name. Reuse
   `push_branch_ff_or_lease` from workspace_pull_request_provider_lifecycle.rs
   (move it to a small `provider_push.rs`; the lifecycle file is deleted in
   P6.4). Reduce Make Remote to "bind repo to provider URL, then publish".
3. Delete: `branch_integration_service.rs`, `branch_sync_runs_service.rs`,
   `remote_tracking_merge.rs`, maintenance `branch_integration_worker.rs`,
   `branch_integration_scheduler`, `base_auto_pull_scheduler`,
   `remote_advisory_pinger`; routes `/push_to_remote`, `/push_mode`,
   `/base_auto_pull`, `/sync`, `/branches/:b/sync_runs*`, `/merge_runs*`,
   `/remote_tracking*`, `/approved_rebase_publish`, `/auto_pull`; CLI
   `g5d remote` (backend/g5d-cli/src/remote.rs); tables
   `branch_integration_queue`, `branch_integration_remote_pair_leases`,
   `branch_sync_runs`; columns `workspaces.{push_mode, base_auto_pull_enabled,
   base_auto_pull_actor_user_id}`, `workspace_branch_repos.{remote_branch,
   baseline_commit, sync_paused_reason, last_sync_at, remote_tip_advisory_sha,
   remote_tip_advisory_observed_at, auto_pull_enabled, tracking_mode}`; the
   `job_merge_runs.source_kind='remote_tracking'` path; `refs/g5-tracking/*`.
4. Tests: retire remote_tracking_*.rs and job_merge_* remote-tracking tests;
   add protected_branch_mirror.rs (fetch updates the mirror, workspace
   branches are never fetched) and api_workspace_publish.rs (publish pushes
   the branch with lease; base workspace publish refused).

Do not: touch PR creation or merge (P6), frontend (P1.6).

Acceptance: `grep -r "tracking_mode\|push_mode\|branch_integration\|auto_pull" backend`
is empty outside the baseline; mirror and publish tests pass; docker-ci
passes with the remote-tracking Playwright flows removed or skipped (P1.6
deletes them; mark them skipped here with a LEAN-TODO(P1.6)).
Validation: cargo check, lint, fmt, `just test-integration`, `just docker-ci`.
```

### P1.6 — Frontend: remove remote-tracking UI, add Publish

```
You are implementing P1.6 of docs/lean-implementation-plan-v1.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v6 §5.5, and
docs/research/research-frontend-preview-infra.md §1.3 row 16 and §5.1.
Depends on P1.5.

Goal: the workspace UI offers "Publish branch" and nothing about tracking
modes, auto-pull, sync runs, or merge runs.

Do:
1. Delete frontend/src/components/workspace/{RemoteTrackingActions,
   BranchTrackingIcon, BranchTrackingPopover, BranchAutoPullControl,
   CoordinatedRepoSyncModal}.tsx, the merge-routing parts of
   MakeRemoteDialog.tsx (keep "connect repo to provider"), and
   hooks/useRemoteTracking.ts; remove push-mode and auto-pull controls from
   settings; remove the WS handlers for `workspace.remote_tip_advisory_updated`.
2. Add a Publish button (ReposBar) calling `POST /workspaces/:id/publish`
   with success/failure toasts; disabled on base workspaces.
3. Delete Playwright flows remote-tracking.spec.ts, remote-tracking-fork.spec.ts,
   coordinated-repo-sync.spec.ts, workspace-sync.spec.ts; add a publish flow
   to workspace.spec.ts.
4. Update frontend/src/lib/types.ts and api-schemas.ts.

Do not: touch editors, drafts, chat, PR/approval components.

Acceptance: `npm --prefix frontend run build`, `npm --prefix frontend test`,
and `just test-playwright` pass; grep for `remote_tracking|pushMode|autoPull`
in frontend/src is empty.
Validation: frontend build and tests, Playwright lane of docker-ci.
```

### P2.1 — Interim Save API and committed-only reads

```
You are implementing P2.1 of docs/lean-implementation-plan-v1.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2 and §3 (deviation 3), design v6
§9.1, §9.2, and docs/research/research-crdt-docs.md §B.4, §D. Depends on P1.4.

Goal: the file, doc, and ontology write routes no longer target the CRDT
overlay. Until the dispatcher exists (P4), Save is a server-side commit to
the workspace branch, and reads are committed-only.

Do:
1. `PUT /api/v1/branches/:branchId/files/*path` (routes/files.rs): body
   `{content, expected_hash}`; read the blob at the branch head, compare
   sha256 of its content with `expected_hash` (409 with current content and
   hash on mismatch), build a commit with the single blob replaced using the
   tree helpers in backend/g5-workspace-ops/src/commit.rs, apply with
   `apply_branch_commit` (P1.4), author = the user, message
   `Save <path> from G5`. Same for create and delete. `GET` returns
   `{content, hash, source: "committed"}`. Remove `draft_dirty`.
2. Route `PUT /workspaces/:id/docs/:docId` and
   `PUT /workspaces/:id/ontology/:nodeId/content` through the same helper.
3. `RepoReadView` (workspace_reader.rs): drop the overlay; reads come from
   the committed view only. Leave the CRDT code compiled but unused (P2.4
   deletes it); mark with LEAN-TODO(P2.4).
4. Tests: api_files.rs (new): save with matching hash commits and moves the
   ref; stale hash returns 409 with current content; create and delete;
   docs and ontology PUTs land commits; reads report `source: committed`.

Do not: touch the frontend (P2.2) or delete CRDT modules (P2.4).

Acceptance: new tests pass; api_drafts.rs tests that assert overlay writes
through these routes are retired (the overlay is no longer a write target);
docker-ci passes with CRDT Playwright flows temporarily skipped
(LEAN-TODO(P2.2)).
Validation: cargo check, lint, fmt, `just test-integration`.
```

### P2.2 — Frontend explicit Save; remove CRDT from editors

```
You are implementing P2.2 of docs/lean-implementation-plan-v1.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v6 §9.2, §9.3, and
docs/research/research-crdt-docs.md §F and research-frontend-preview-infra.md
§1.3 rows 4 to 8. Depends on P2.1.

Goal: no Yjs in the frontend. Editors hold local state and save explicitly.

Do:
1. CodeEditor.tsx: Monaco with a local model; Save (Ctrl+S) calls the P2.1
   PUT with `expected_hash`; on 409 show a reload-or-overwrite dialog;
   optional autosave-on-blur behind a user preference; dirty indicator per
   tab.
2. SpecEditor.tsx (block editor), StructuredQuestionEditor, EditableInline,
   questionAnswerWrite.ts, htmlDocAnswerBridge.tsx, ExplainerOutputView.tsx,
   JobOutputView.tsx: replace the `Y.Text` binding with a local document
   model and the same Save path; JobQuestionDocView.tsx already renders
   without Yjs and is the template.
3. Comments: anchor by `(path, line, line hash)`; adjust useComments.ts and
   the comment components; the backend comment_index already stores line
   anchors, add the hash if missing (small baseline edit is allowed here).
4. Delete frontend/src/lib/crdt.ts, hooks/useDraft.tsx, components/draft/*,
   useAutoCommitBeforeAction.ts, CRDT tests and mocks (test/crdt*.test.tsx,
   test/__mocks__/y-websocket.ts, vitest.config.crdt.ts, e2e/crdt-*.test.ts),
   the `draft.*` handlers in lib/ws.ts, and the deps yjs, y-websocket,
   y-monaco from package.json. Remove the WorkspacePage `crdt.yDoc/provider`
   plumbing and the 2.5 s file-tree polling (replace with refetch on
   `workspace.branch_updated`).
5. Playwright: delete crdt-*.spec.ts, drafts.spec.ts,
   manual-commit-crdt-rollover.spec.ts, ui-auto-commit-before-actions.spec.ts,
   workspace-overlay-collaboration.spec.ts, draft-session-scope-isolation.spec.ts,
   typing-latency.spec.ts; add explicit-save.spec.ts (save, conflict dialog,
   spec editor save, comment anchor survives a save).

Do not: touch chat, jobs, or PR components; change backend code except the
comment anchor column.

Acceptance: `grep -rn "yjs\|y-websocket\|y-monaco\|Y\.Doc" frontend/src` is
empty; frontend build and tests pass; Playwright explicit-save flow passes.
Validation: frontend build and tests, Playwright lane of docker-ci.
```

### P2.3 — Worker native writes and native push at task end

```
You are implementing P2.3 of docs/lean-implementation-plan-v1.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2 and §3 (deviation 1), design v6
§11.2, §11.3, and docs/research/research-agent-tooling.md §A.1 (commit, push,
task complete, task rebase rows), §C, and research-crdt-docs.md §B.1, §B.2.
Depends on P1.2.

Goal: agents write ordinary files and finish a task with native git; the
worker no longer runs a CRDT peer.

Do:
1. backend/g5d-worker/src/lifecycle.rs: remove `run_agent_with_provider_fs_bridge`,
   `spawn_provider_fs_crdt_sync`, the lifecycle barriers, `crdt_peer_plan`,
   and the CRDT connect-with-retry in host_support.rs; run the provider
   directly. Delete fs_diff.rs and attribution.rs.
2. Task completion (worker side): after the provider exits successfully,
   `git add -A && git commit` (if dirty) and `git push origin HEAD:<target
   branch>` with the task JWT credential helper already configured by
   task_env.rs; target branch is the task's assigned branch as today (job
   branch or workspace branch; P4.8 makes it always the workspace branch);
   then call the transition route. Push failures fail the task with the git
   stderr in the outcome.
3. Delete CLI commands and modules: backend/g5d-cli/src/{commit.rs,
   connect.rs, conflicts.rs, revert.rs, fork.rs, refresh.rs, merge.rs,
   intent_store.rs}, `task rebase`, the task-commit request-file bridge in
   task.rs, the spec-delta completion gate in task.rs (`task complete`
   becomes: push, then transition), backend/g5d-foundation/src/task_rebase_gate.rs,
   the rebase-pause checks in backend/g5d-cli/src/tool.rs, the git
   blocklists in backend/g5-scm/src/safety.rs and backend/g5d-cli/src/git/safety.rs,
   and the `.g5/` socket carve-out in backend/g5d-foundation/src/docs_write_policy.rs.
   Update backend/g5d-cli/src/lib.rs, dispatch.rs, stable_context.rs.
4. Leave the CRDT crates compiled (P2.4 deletes them) but ensure nothing in
   g5d-worker, g5d-cli, g5d-tooling references `CrdtPeer` after this PR
   except g5d-foundation's own module.
5. Tests: retarget backend/g5d/tests/worker_lifecycle.rs and
   tests/e2e/src/scenarios/task_lifecycle_contract.rs to assert a native
   commit lands on the target branch; delete crdt_sync.rs, rebase_workflow_git.rs,
   mcp_jsonl_rebase.rs parts about the pause, connected_crdt_commit.rs.

Do not: change the provider adapters' launch code (P3.2) beyond removing
CRDT hooks; touch the API.

Acceptance: a fake-provider task (existing Fake harness) that writes a file
ends with the file committed on the target branch in the mirror; docker-ci
worker and e2e lanes pass.
Validation: cargo check, lint, fmt, `cargo test -p g5d-worker -p g5d-cli
-p g5d --lib`, `just docker-ci`.
```

### P2.4 — Delete the backend CRDT layer

```
You are implementing P2.4 of docs/lean-implementation-plan-v1.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v6 §3.1 (CRDT row), §9,
and docs/research/research-crdt-docs.md §G and "Simplification candidates".
Depends on P2.2, P2.3.

Goal: nothing in the repository references Yjs, drafts, or the task-commit
protocol.

Do:
1. Delete crates/modules: backend/g5-crdt (remove from Cargo.toml members),
   backend/g5-api-core/src/crdt/*, backend/g5d-foundation/src/crdt/*,
   backend/g5-api-core/src/services/draft_service.rs,
   backend/g5-api-routes/src/routes/drafts.rs, backend/g5-workspace-ops/src/draft.rs
   (make `DraftState` callers assume clean), backend/g5-api-data/src/models/draft.rs,
   the `/api/v1/yjs/:workspaceId/:branchId` route and `crdt_ws_adapter`
   (routes/mod.rs), the internal route `/tasks/:id/crdt/edit-file-replace`
   and `/tasks/:id/drafts/commit-all`, `draft.*` WsEvents and
   `Channel::Draft`, `CrdtConfig` env vars, `yrs` from workspace deps.
2. Baseline: drop `draft_sessions`, `draft_updates`, `draft_participants`,
   `task_commit_requests`, `task_commit_frontier_cuts`,
   `task_commit_frontier_receipts`; drop `task_crdt_head_commits` from the
   assignment DTO; drop the sticky-runtime `teardown_repository_scopes`
   CRDT semantics (stub the field to empty; P4.7 deletes sticky entirely).
3. Remove `crdt` from the instance-info feature list and every mention in
   Helm/compose/env examples (`G5_CRDT_*`, `G5_TASK_CRDT_PERSISTENCE_ENABLED`,
   `G5_CRDT_PERSISTENCE_BARRIER_EXACT_V2_ENABLED`), and the single-replica
   CRDT comment in infra/helm/g5/values.yaml.
4. Delete exclusive tests (api_drafts.rs, workspace_overlay_drafts.rs,
   draft_session_scope_contract.rs, e2e crdt_collab.rs, crdt_draft_ops.rs,
   persistent_task_crdt_restart.rs, and Playwright leftovers) and retarget
   any test that only used drafts as a way to create a commit to use the
   P2.1 Save API or plain git.

Do not: touch task lifecycle, sticky runtimes beyond the stub, or governance.

Acceptance: `grep -rn "yrs\|crdt\|Crdt\|draft_session\|DocumentHandle" backend
frontend/src --include='*.rs' --include='*.ts' --include='*.tsx'` returns
nothing except historical comments in the baseline; the workspace compiles;
docker-ci passes.
Validation: cargo check, lint, fmt, `just test-integration`, `just docker-ci`.
```

### P2.5 — Agent profiles rewritten for native git

```
You are implementing P2.5 of docs/lean-implementation-plan-v1.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v6 §11.2, §11.5, §6.3
(concurrency paragraph), §5.3 (rewrites paragraph), and
docs/research/research-agent-tooling.md §A.4. Depends on P2.3.

Goal: the shipped profiles describe the laptop model, not the CRDT model.

Do:
1. assets/agents/claude-code-g5/CLAUDE.md and assets/agents/codex-g5/AGENTS.md:
   replace every `g5d commit|push|remote|task complete|task rebase|revert`
   instruction with native git; state the rules: stay on the workspace
   branch; other agents may be editing the same folder and the last writer
   wins; commit often; push to `origin` when a unit of work is done; rebase
   onto `origin/main` to update; push rewrites only with
   `--force-with-lease` (explicit `<branch>:<expected>` form after a fetch),
   never bare `--force`; never push to `main`. Keep the ontology and
   grounding sections. Remove the "Do not run raw git" rules.
2. Delete or rewrite skills that encode the CRDT workflow:
   `g5-coordinated-git`, `g5-rebase-workflow`, `g5-provided-agent-workspace`
   (both profiles), and update `profile_manifest.json` `requiredSkills`.
   Update `.claude/commands/commit.md` and `pr.md` to be the reference
   behavior; `gh pr create` stays for now with a LEAN-TODO(P6.1) to switch
   to `g5d pr create`.
3. Reduce `.claude/settings.json` hooks to `SessionStart` and `Stop` and
   delete the hook scripts that are no longer referenced (keep
   `g5_hook_utils.py` if the two remaining hooks use it).
4. Update the Rust side that validates `requiredSkills` and the MCP tool
   list (backend/g5d-worker/src/provided_agent_profile.rs, mcp_preflight.rs)
   and the profile tests (backend/g5d/tests/skills_subagents.rs,
   scripts/check_codex_profile_parity.py).

Do not: change tool registration or the MCP server.

Acceptance: profile parity script passes; skills tests pass; grep for
`g5d commit`, `g5d push`, `g5d task rebase` in assets/agents is empty.
Validation: cargo check, `cargo test -p g5d --lib`, `python3
scripts/check_codex_profile_parity.py`, `just docker-ci` provided-agents lane.
```

### P3.1 — Claude CLI pin bump; fake `claude` and `codex` binaries

```
You are implementing P3.1 of docs/lean-implementation-plan-v1.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v6 §8.3, §8.6, §15
(provider runtime bullet), and docs/research/research-agent-tooling.md §B.
Depends on P0.1.

Goal: a pinned Claude CLI with the flags the runner needs, and two fake
provider binaries that replay recorded frames so the runner can be tested
without provider credentials.

Do:
1. Bump scripts/claude-code-cli-version to the newest release that supports
   `-p --output-format stream-json --input-format stream-json
   --include-partial-messages --resume --session-id --permission-mode
   --permission-prompts host --permission-prompt-tool --mcp-config
   --strict-mcp-config --settings --setting-sources --disallowedTools
   --append-system-prompt --max-turns` (verified on 2.1.269). Record the
   verified flag list in docs/lean-provider-cli-contract.md together with the
   `stream-json` event shapes the runner relies on and the Codex `exec --json`
   item shapes. Update the runtime staging script
   (scripts/stage-local-worker-runtime.sh), Dockerfiles that install the CLIs
   (infra/docker/Dockerfile.g5d*, packer scripts), and verify-worker-image
   scripts.
2. Create `tools/fake-provider-cli/` (Rust, two binaries `fake-claude` and
   `fake-codex`): each accepts the real argv, reads the prompt from stdin per
   the real CLI's rules, and replays a scenario file (JSONL of output frames
   with optional delays) selected by `G5_FAKE_SCENARIO`. Scenarios can
   include: text output, tool_use/tool_result frames, an MCP tool call
   against the configured `g5-tools` server (the fake must speak MCP stdio to
   `g5d mcp` as a client for `ask_user` and, for fake-claude,
   `request_permission`), a file write in cwd, a nonzero exit, and a session
   id in the init frame. fake-codex never emits approval frames.
3. Add `tools/fake-provider-cli/record.sh` that runs the real pinned CLIs
   once with a scripted prompt and saves frames as fixtures; commit a small
   recorded fixture set under tests/fixtures/provider-frames/.
4. Wire `G5_CLAUDE_PATH`/`G5_CODEX_PATH` so tests can point at the fakes
   (the env vars exist; document them).

Do not: change the runner or the adapters (P3.2).

Acceptance: `cargo test -p fake-provider-cli` passes; running fake-claude
with a scenario prints valid stream-json; the contract doc lists every flag
with the version it was verified on.
Validation: cargo check, lint, fmt, `cargo test -p fake-provider-cli`.
```

### P3.2 — Rust provider runner; delete both Node sidecars

```
You are implementing P3.2 of docs/lean-implementation-plan-v1.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v6 §8.3 (all), §8.6,
docs/lean-provider-cli-contract.md (from P3.1), and
docs/research/research-agent-tooling.md §B, §E, §F. Depends on P3.1, P2.3.

Goal: one Rust provider runner spawns `claude -p` or `codex exec` per turn,
maps their JSON frames to the worker's existing agent events, and the Node
sidecars are gone.

Do:
1. New module backend/g5d-worker/src/runner/{mod.rs, claude.rs, codex.rs,
   events.rs, process.rs}: `Runner::run_turn(spec) -> TurnOutcome` where spec
   carries provider, provider_session_id (Option), prompt, attachments,
   permission mode, model, effort, env, cwd; argv builders exactly as design
   §8.3; stdin writer (stream-json user message for Claude; prompt text and
   `-i` files for Codex) then close stdin; line reader mapping frames to the
   existing `AgentEvent`/timeline entry types used by task_agent.rs; session
   id capture (`--session-id` generated for a new Claude session; Codex id
   from the first item); cancel via SIGINT then SIGKILL after 15 s; exit code
   plus final result item decide success.
2. Replace `ClaudeCodeTaskAgent` (claude_code.rs) and `CodexTaskAgent`
   (codex.rs) internals with the runner; keep their `TaskAgent` trait surface
   so lifecycle.rs changes minimally. Delete backend/g5d/sidecars/claude and
   codex entirely, sidecar_transport.rs, the sidecar halves of
   vsock_transport.rs, tool_path_projection.rs, screenshot_retention.rs
   (attachments already live in chat_attachments), host_lifecycle.rs, the
   retained-sidecar/held-runtime code paths in claude_code.rs (sticky
   runtime deletion proper is P4.7; here only the SDK-specific parts go and
   `HeldClaudeSidecar` becomes a no-op holder if still referenced), and the
   `mcp_preflight` full listing (keep a startup self-check that the MCP server
   starts).
3. LLM proxy: unchanged; ensure `ANTHROPIC_BASE_URL` and the Codex provider
   block still point at it and the placeholder key logic is preserved.
4. Runtime images: remove sidecar bundling from Dockerfiles and the staging
   script; keep Node only if the pinned Codex CLI needs it (check the packer
   notes: "Codex (node wrapper)").
5. Tests: runner unit tests with the fake binaries (both providers: new
   session, resume, cancel, nonzero exit, frame mapping); retarget
   backend/g5d/tests/parity_e2e.rs, agent_pipeline.rs, synth_lifecycle.rs to
   the fakes; delete sidecar `__tests__`; keep provider replay tests by
   converting the recorded JSONL fixtures.

Do not: implement questions or permissions (P3.3), session files (P3.4), or
touch the API.

Acceptance: `grep -rn "claude-agent-sdk\|sidecar.mjs\|codex-sidecar" backend
infra scripts` is empty; a fake-claude and a fake-codex chat turn run through
the worker in docker-ci; provider replay tests pass.
Validation: cargo check, lint, fmt, `cargo test -p g5d-worker -p g5d`,
`just docker-ci`.
```

### P3.3 — `task_questions`, `ask_user`, `request_permission`, answer polling

```
You are implementing P3.3 of docs/lean-implementation-plan-v1.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v6 §7.1 (task_questions),
§7.2 (Answering, Interruption), §8.3 ("Two kinds of human input", "Answer
delivery"), §11.1, and docs/research/research-task-lifecycle.md §F,
research-chat.md §A.7. Depends on P3.2.

Goal: one human-in-the-loop table and two MCP tools that block by polling
committed state; Claude permission prompts and both providers' questions
work end to end.

Do:
1. Baseline: `task_questions(id, task_id, exec_id NULL, kind
   question|approval|escalation, payload jsonb, status
   open|answered|expired|cancelled, answer jsonb, expires_at, answered_by,
   answered_at, consumed_at, successor_task_id NULL UNIQUE)`; org RLS like
   sibling tables. `exec_id` is text until P4.1 defines it.
2. API: internal `POST /api/internal/tasks/:id/questions` (worker JWT) creates
   a row (expiry from workspace settings, default 60 min); internal
   `GET /api/internal/questions/:id?wait=<secs>` long-polls until answered,
   expired, or cancelled; internal `POST /api/internal/questions/:id/consumed`;
   user routes `GET /api/v1/workspaces/:id/questions` and
   `POST /api/v1/questions/:id/answer` (member with write), `POST .../cancel`.
   Answering commits the row first; nothing is pushed to the VM.
3. MCP: in backend/g5d-cli/src/mcp.rs and tool.rs add `ask_user(question,
   options?)` and `request_permission(tool_name, input)` implemented as
   create-then-poll with the task token; `request_permission` returns
   `{"behavior":"allow"|"deny","updatedInput":…,"message":…}`. Add `g5d ask`.
   Register them in the platform projection
   (backend/g5d-runtime-core/src/provided_agent_tools.rs). Set the Claude
   argv `--permission-prompts host --permission-prompt-tool
   mcp__g5-tools__request_permission` and `MCP_TOOL_TIMEOUT` above the
   expiry; set `tool_timeout_sec` for `g5-tools` in the generated Codex
   config.toml. Map `AskUserQuestion` disallowed for Claude.
4. Task state: while a question is open the task is `waiting` (use today's
   `waiting` status); when consumed it returns to `running`; expiry fails the
   task with reason `timeout` (reconciler hook in lease_reaper.rs for now;
   P4.6 moves it).
5. Replace consumers: the `human_escalation` MCP tool and `g5d`-side deferred
   escalation files become `ask_user(kind=escalation)`; chat tool approvals
   for Claude no longer go through `agent_approval_handoffs` (leave the table
   until P4.7; mark LEAN-TODO). Frontend: render `task_questions` in the
   existing HIL/escalation panel and the chat inline approval prompt
   (components/execution/HilEscalationPanel.tsx, chat approval UI), answer
   through the new route.
6. Tests: api_task_questions.rs (create, long-poll returns on answer, expiry,
   cancel, consumed idempotent); runner tests with fake-claude scenarios that
   call `request_permission` and `ask_user`, and fake-codex `ask_user`, with
   a human delay longer than the default MCP timeout; Playwright: answer a
   question from the UI (extend human-escalation.spec.ts).

Do not: build successor turns after interruption (P4.6); delete escalation
tables (P7.1).

Acceptance: a fake-claude turn blocks on a permission prompt until the UI
answers, then continues; a fake-codex turn blocks on `ask_user` the same way;
docker-ci passes.
Validation: cargo check, lint, fmt, `just test-integration`, `just docker-ci`.
```

### P3.4 — `chat_session_files` harvest and restore

```
You are implementing P3.4 of docs/lean-implementation-plan-v1.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v6 §8.1 (table), §8.4,
§11.4, and docs/research/research-chat.md §A.4, §A.5, §D.1. Depends on P3.2.

Goal: provider session files live in Postgres only, harvested after every
turn and restored before `--resume` when absent on the VM.

Do:
1. Baseline: `chat_session_files(session_id, provider, provider_session_id,
   relative_path, bytes bytea, sha256, size, harvested_from_vm_id text NULL,
   updated_at, PRIMARY KEY (session_id, provider_session_id, relative_path))`
   with org RLS; drop `claude_session_store_entries` and
   `codex_session_artifacts`.
2. Internal routes: `GET /api/internal/chat-sessions/:id/session-files/manifest`,
   `GET .../session-files/content?path=`, `PUT .../session-files` (upsert,
   zstd-compressed body, 128 MiB cap). Generalize today's codex artifact
   harvester (backend/g5d-worker/src/codex.rs `harvestable_codex_session_artifacts`,
   `restore_codex_session_artifacts_from_api`) into the runner for both
   providers: Claude paths under `$CLAUDE_CONFIG_DIR/projects/<escaped
   cwd>/<session id>.jsonl` and `<session id>/subagents/*`; Codex paths
   under `$CODEX_HOME/sessions/**/rollout-*<session id>*.jsonl`.
3. Paths: set `HOME=/home/agent`, `CLAUDE_CONFIG_DIR=/home/agent/.claude`,
   `CODEX_HOME=/home/agent/.codex` in the guest env (backend/g5d-host/src/assignment.rs
   env builder; guest_runtime.rs creates the directory owned by uid 1000);
   in process-mode workers use `$HOME`. Remove the `/work/.g5` session
   locations.
4. Resume: before a turn with a session id, if the files are absent
   locally, restore; if Postgres has nothing, start a new session and prefix
   the prompt with a history preamble built from chat_messages (reuse the
   existing "history-bootstrap" code), record the new provider_session_id.
5. Delete backend/g5d-worker/src/g5_api_session_store_proxy.rs,
   backend/g5-api-core/src/services/session_store_service.rs,
   codex_session_artifact_service.rs, and their routes.
6. Tests: api_chat_session_files.rs (manifest, upsert idempotent, cap);
   runner tests: harvest after a fake turn, restore into an empty config dir
   before resume, preamble fallback when Postgres is empty; e2e
   codex_native_resume.rs retargeted to the new table.

Do not: touch sticky runtimes or the dispatcher.

Acceptance: a resumed fake-claude turn after wiping the local config dir
receives its transcript from Postgres; grep for `session_store\|codex_session_artifact`
in backend is empty.
Validation: cargo check, lint, fmt, `just test-integration`, `just docker-ci`.
```

### P3.5 — Remove `naive_react` and the `fake` provider value

```
You are implementing P3.5 of docs/lean-implementation-plan-v1.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v6 §8.6, and
docs/research/research-agent-tooling.md §0. Depends on P3.2.

Goal: the provider enum is exactly `claude_code | codex` everywhere.

Do:
1. `grep -rn "naive_react\|NaiveReact" backend frontend/src assets scripts`
   and remove every occurrence: the `ChatAgentProvider::NaiveReact` variant
   and defaults (backend/g5-contracts/src/types/{task_context,agent_launch,
   provider_account}.rs), the normalization in routes/chat.rs and
   WorkspacePage.tsx, prompt-template branches (backend/g5d/src/prompts),
   bootstrap references, api-schemas.ts, mock.ts, fixtures, tests. Edit the
   baseline CHECK constraints and defaults for `chat_sessions.agent_provider`,
   task/job provider columns.
2. Remove the `fake` provider value: the Fake harness is selected by pointing
   `G5_CLAUDE_PATH`/`G5_CODEX_PATH` at the P3.1 fake binaries; delete
   `FakeTaskAgent`, `agent_provider = 'fake'` branches, the `Fake` variant,
   and frontend/Playwright references (provided-agents.fake-provider.spec.ts
   becomes a fake-binary-backed flow).
3. Update tests that enumerate providers.

Do not: touch runtime behavior of the two real providers.

Acceptance: the greps are empty; `cargo check` passes with no dead-variant
warnings; docker-ci passes with the fake-binary flows.
Validation: cargo check, lint, fmt, frontend tests, `just docker-ci`.
```

### P4.1 — Schema: `workspace_vms`, execution identity, `task_events`

```
You are implementing P4.1 of docs/lean-implementation-plan-v1.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v6 §5.1, §7.1, §8.1.
Depends on P0.2, P2.4.

Goal: the baseline has the tables and columns the dispatcher, the fence, and
the event contract need. Schema only, plus models.

Do (edit 001_lean_baseline.sql in a `-- P4.1` section, and
backend/g5-api-data/src/models):
1. `workspace_vms(workspace_id PK, host_id NULL, vm_id text NULL, status
   stopped|starting|running|stopping|failed, lease_expires_at, started_at,
   last_activity_at, last_checkpoint_at, last_error)` with org RLS.
2. `tasks`: add `attempt int default 1`, `vm_id text NULL`,
   `start_confirmed bool default false`, `acked_event_seq bigint default 0`,
   `wall_clock_limit_secs int NULL`; keep existing status values for now.
3. `task_events`: rename `timeline_entries` to `task_events`, add `vm_id text`,
   `seq bigint`, `UNIQUE (task_id, vm_id, seq)` (nullable vm_id/seq for rows
   written by the pre-dispatcher path; P7.2 tightens).
4. `task_questions.exec_id` becomes structured text `<task_id>:<attempt>:<vm_id>`.
5. `chat_session_files.harvested_from_vm_id` (already text; keep).
6. Regenerate the SQLx cache; adjust models and the few queries that name
   `timeline_entries`.

Do not: change behavior; no service logic.

Acceptance: cargo check with the regenerated cache; `just test-integration`
passes; `just docker-ci` bootstrap phase passes.
Validation: `just db-prepare`, cargo check, `just test-integration`.
```

### P4.2 — `g5d dispatch`: RPC listener, fs/git/proc, event journal

```
You are implementing P4.2 of docs/lean-implementation-plan-v1.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v6 §6.3 (all), §7.6
(journaled events, replay), §4.2, and docs/research/research-exec-isolation.md
§E.6 (guest listener + mux + token pattern), §H. Depends on P3.2, P3.3, P3.4.

Goal: a new `g5d dispatch` subcommand that runs inside a workspace VM, serves
the RPC surface over the stream multiplexer, supervises processes, and
journals every event until acknowledged. This PR delivers the dispatcher as a
process that can be run locally against a directory; host integration is
P4.4 and API integration is P4.5/P4.6.

Do:
1. New crate backend/g5d-dispatch (add to Cargo.toml members and to the
   `g5d` binary as `Command::Dispatch`). Config from env: `G5D_WORKSPACE_ID`,
   `G5D_VM_ID`, `G5D_WORKSPACE_TOKEN`, `G5D_API_URL`, `G5D_DISPATCH_PORT`,
   repo layout (`/work/<root_path>` list from a JSON file the host writes at
   boot), `HOME=/home/agent`, state dir `/var/lib/g5d`.
2. Listener: accept one authenticated connection from the host on the TSI
   port (32-byte token frame first, as backend/g5d-host/src/preview_attach_wire.rs
   does), then multiplex with `g5_internal_api::stream_mux`: channel `rpc`
   (JSON-RPC 2.0, newline-delimited), plus channels for `pty`, `http:<port>`
   opened by the peer. Reconnect-safe: a new connection replaces the old.
3. RPC groups from design §6.3: `fs.*` (with `expected_hash` on write and
   the `file.changed` watcher), `git.*` (thin wrappers over the git binary:
   status, diff, log, assert_branch, switch_back, commit, push,
   fetch_rebase), `proc.spawn/signal/list/wait` with disposition semantics
   (`started`, `already_running`, `already_finished`), `agent.run_turn`
   delegating to the P3.2 runner with `exec_id` idempotency,
   `events.replay/ack` and `outcomes.pending`, `admission.close/open`
   (close waits for in-flight `fs.write`), `health.ping/resources`,
   `session_files.restore/harvest` (wrapping P3.4), `checkpoint.run/last`
   (stub that P4.3 fills), `ports.list`, `preview.*` (stub for P5.1),
   `pty.*` (stub for P5.2).
4. Event journal: per `exec_id`, append-only file under
   `/var/lib/g5d/journal/<exec_id>` with per-execution `seq`; every event is
   journaled before it is streamed; `events.ack(exec_id, up_to_seq)` prunes;
   `events.replay(exec_id, from_seq)` streams from the journal then live;
   disk budget with oldest-acknowledged-first pruning; the terminal outcome
   is the last event.
5. Platform excludes: write `/var/lib/g5d/excludes` (`.g5/cache/`, other
   runtime subdirs) and set `core.excludesFile` in a global gitconfig for
   agent and dispatcher git invocations (reuse task_git_config.rs).
6. Tests (crate unit + an integration test that runs `g5d dispatch` against
   a temp directory with a fake host client): auth token rejected; every
   RPC group; write with stale hash returns 409; spawn idempotency by exec_id
   returns the right disposition; journal replay after client disconnect
   mid-run yields a complete ordered event list; ack prunes; admission close
   waits for an in-flight write.

Do not: touch the host or the API; implement checkpoint (P4.3).

Acceptance: the crate's tests pass; `g5d dispatch --help` works from the
built binary; design §6.3 RPC table is implemented or stubbed with a
LEAN-TODO naming the PR that fills it.
Validation: cargo check, lint, fmt, `cargo test -p g5d-dispatch`.
```

### P4.3 — Checkpoint task and restore

```
You are implementing P4.3 of docs/lean-implementation-plan-v1.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v6 §5.4 (all, including
the exact recipe), §5.3 (checkpoint ref row), §9.1, and
docs/lean-backend-design-v5-review-response.md F1 and F2 (why the recipe is
what it is). Depends on P4.2, P1.2.

Goal: the dispatcher's background checkpoint task and restore-on-boot, and
the receive-pack rule for checkpoint refs.

Do:
1. Dispatcher: a cron-like task every N minutes (config, default 10) that,
   when the working tree or local HEAD of any member repo changed since the
   last successful checkpoint, for each changed repo runs exactly the recipe
   in design §5.4: observe the internal head with `git ls-remote origin
   refs/heads/<branch>` (never fetch); `cp -p .git/index $TMP`;
   `GIT_INDEX_FILE=$TMP git update-index --really-refresh`; `git add -A`;
   `write-tree`; `commit-tree -p HEAD` with the message fields; force-push to
   `refs/g5/checkpoints/<workspace id>`. Serialize runs with a mutex;
   `checkpoint.run` triggers and waits; `checkpoint.last` reports. Report
   `checkpoint.updated` events.
2. Restore on boot (dispatcher): clone/fetch each member repo, fetch the
   checkpoint ref, `git checkout -B <branch> <C^>` with upstream
   `origin/<branch>`, materialize `C`'s tree into the working tree without
   moving HEAD (including untracked files present in `C` but absent from
   HEAD), leave changes unstaged; no automatic merge when origin moved.
3. receive-pack (routes/git.rs): allow force-update of
   `refs/g5/checkpoints/<workspace id>` for a token whose claims name that
   workspace (worker/task token now; workspace token from P4.4), deny for
   users.
4. API reads: `source: checkpoint` in files/docs/ontology reads when the VM
   is not running and a checkpoint ref exists (resolve the tree of `C`);
   `checkpoint.updated` feeds the P1.3 convergence loop for the checkpoint
   ref.
5. Tests (dispatcher integration with a local bare "origin"): interval run
   only when changed; tracked-but-ignored file with a dirty edit captured
   and restored, including a same-second same-size edit; after a checkpoint
   `origin/<branch>` is unchanged and a `--force-with-lease` push that would
   discard an unseen commit is still rejected; committed-but-unpushed file on
   a clean tree restores with the commit and the branch ahead of origin;
   local commits plus dirty edits restore both; untracked, deleted, and
   ignored files handled as specified; API test for `source: checkpoint`.

Do not: implement the stop barrier (P4.5) or host commands (P4.4).

Acceptance: all listed tests pass; the recipe in code matches design §5.4
verbatim in effect.
Validation: cargo check, lint, fmt, `cargo test -p g5d-dispatch`, `just
test-integration` for the git route and read tests.
```

### P4.4 — `g5d host`: VM lifecycle commands, tokens, reconnect

```
You are implementing P4.4 of docs/lean-implementation-plan-v1.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v6 §6.1, §6.2, §6.8,
§5.2 (identities), and docs/research/research-exec-isolation.md §A, §D, §E.
Depends on P4.2.

Goal: the host boots and stops workspace VMs on command from the API over
the existing tunnel, connects to each VM's dispatcher, relays mux channels,
mints and renews the per-VM workspace token, and reports its VMs on
reconnect. Per-task assignment stays in place until P4.7/P4.8 move work off
it; this PR adds the new path beside it.

Do:
1. Host command channel: extend backend/g5d-host/src/tunnel.rs so the API can
   send `vm.start(workspace_id, vm_id, spec)`, `vm.stop(workspace_id, vm_id,
   checkpoint)`, `vm.status`, and open mux streams `rpc`, `pty`,
   `http:<port>`, `llm` targeted at a `vm_id`; on connect the host sends
   `vm.inventory [(workspace_id, vm_id)]` and stops any VM the API does not
   list as current.
2. VM spec: boot with libkrun as today (backend/g5d-isolation) with entry
   `/opt/g5/bin/g5d dispatch`, mounts for `/work`, runtime bin, provider
   assets, scratch disk; env from design §6.3 (`G5D_WORKSPACE_ID`, `G5D_VM_ID`,
   `G5D_WORKSPACE_TOKEN`, `G5D_API_URL`, `HOME=/home/agent`, provider paths,
   LLM proxy socket); write the repo layout JSON into `/work/.g5/`. Connect to
   the dispatcher listener with the per-VM token (reuse the preview-attach
   wiring) and relay.
3. Workspace token: the API mints `G5D_WORKSPACE_TOKEN` (claims
   `workspace_id`, `vm_id`, TTL idle timeout + margin) in the `vm.start`
   command; the dispatcher renews through the host (`token.renew` on the
   command channel) and the host forwards to the API; refusal for a stale
   `vm_id` stops the VM locally. Lease: the host stops a VM whose lease
   expired without renewal.
4. LLM proxy: tag requests with the VM's `vm_id` and refuse when the API
   reports it stale (a cached "current incarnations" set refreshed on
   renewals).
5. Keep `G5D_HOST_MAX_MICROVMS` as the slot count shared by old per-task VMs
   and new workspace VMs during the transition.
6. Tests: backend/g5d/tests/host_up_smoke.rs extended; a fake API side that
   drives start/stop/inventory over the tunnel; token renewal refused stops
   the VM; docker-ci isolation lane boots a workspace VM to `ready`.

Do not: touch the API's workspace_vms logic (P4.5) or delete per-task paths
(P4.9).

Acceptance: a workspace VM boots, the dispatcher reports `ready(vm_id)`, an
`fs.read` over the relayed `rpc` channel works, `vm.stop` stops it, and a
reconnect with an unknown `vm_id` kills that VM.
Validation: cargo check, lint, fmt, `cargo test -p g5d-host -p g5d`,
`just docker-ci`.
```

### P4.5 — API: VM lifecycle, four actions, stop barrier, fence

```
You are implementing P4.5 of docs/lean-implementation-plan-v1.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v6 §5.7 (all), §6.1,
§6.8 (all), §9.1, §9.2, and docs/lean-backend-design-v5-review-response.md
F5. Depends on P4.1, P4.3, P4.4.

Goal: the API owns `workspace_vms`, the four workspace actions, the stop
barrier, placement, the idle reconciler, and the incarnation fence at every
boundary.

Do:
1. `WorkspaceVmService`: `resume` (CAS on the row: `starting`, new `vm_id`,
   chosen host with a free slot preferring the last host, lease; send
   `vm.start`), `restored` (dispatcher ready → `running`), `stop` (the
   barrier: `stopping` + `admission.close` → wait in-flight → cancel running
   tasks (interim: today's cancel path; P4.6 adds `interrupted`) → close
   previews/terminals → `checkpoint.run` and wait for every push; on failure
   return to `running`, reopen admission, record `last_error`, retry with
   backoff → `session_files.harvest` → `events` drain → release: `stopped`,
   `vm_id` cleared → `vm.stop`), `delete` (stop or force, delete refs,
   rows), and `create` (fork branch, rows, resume). Idle reconciler: stop
   when `last_activity_at + idle_timeout_min` passed and no running task,
   open question, terminal, or recent preview/file activity.
2. Fence: reject a token whose `vm_id` is not the row's current one at
   receive-pack/upload-pack (git.rs), session-file routes, task event and
   outcome routes, and RPC replies; accept `starting|running|stopping`. Host
   reconnect: answer `vm.inventory` with current incarnations. Reconciler:
   lease expiry in any status → `failed`, `vm_id` cleared, confirmed-running
   tasks `failed(vm_lost)` (interim mapping onto today's failure path).
3. Reads and writes: `source: live` via `fs.read`/`fs.list` when `running`
   (P4.3 already handles checkpoint/committed); Save → assert branch → resume
   if stopped → `fs.write` (delete the P2.1 server-side commit path);
   `WORKSPACE_STOPPING` during the barrier. Docs and ontology PUTs follow.
4. Routes: `POST /workspaces/:id/{resume,stop,delete}`, `GET /workspaces/:id/vm`.
   Chat send and task create call `resume` when needed (full dispatch loop is
   P4.6; here call it inline).
5. Tests: api_workspace_vms.rs — four actions; idle stop; stop barrier
   refuses new Saves, completes an in-flight Save, fails on a failed
   checkpoint push and reopens; reconciler tick right after admission close
   does not mark loss; stale `vm_id` refused at git, session files, events,
   RPC replies; reconnect inventory kills stale VM; live read and write
   through a running dispatcher (docker-ci with a workspace VM).

Do not: implement the dispatch loop, two-phase starts, or the six-state
model (P4.6).

Acceptance: the listed tests pass; a workspace can be created, used for Save
and live reads, stopped, and resumed with its checkpoint restored.
Validation: cargo check, lint, fmt, `just test-integration`, `just docker-ci`.
```

### P4.6 — Dispatch loop, two-phase starts, event contract, reconciler

```
You are implementing P4.6 of docs/lean-implementation-plan-v1.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2 and §3 (deviation 4), design v6
§7.1 to §7.3, §7.6 (all), §8.2, and docs/lean-backend-design-v5-review-response.md
F3, F4, F6, F8. Depends on P4.5.

Goal: the six-state task model and the dispatch and event contract, with
`TaskService::apply(event)` as the only writer of task state. Existing
producers (chat send, job creation) create `queued` tasks; consumers are
switched in P4.7 and P4.8.

Do:
1. New `backend/g5-api-core/src/services/task_lifecycle.rs` implementing
   the six states and transitions of design §7.2, including outcome
   `interrupted`, `retry_on_vm_loss` re-queue, question expiry, wall-clock
   limit, successor-turn creation for answered-but-unconsumed questions
   (unique `successor_task_id`). The old kernel stays for tasks not yet
   migrated (P4.7/P4.8 migrate them; P7.1 deletes the kernel).
2. Dispatch loop (maintenance, 2 s + triggers): select dependency-ready
   `queued` tasks regardless of VM state; resume stopped/failed workspaces;
   for running VMs with a free slot record `attempt`, `vm_id`,
   `status=running`, `start_confirmed=false` in one update and send
   `agent.run_turn`/`proc.spawn`; on disposition set `start_confirmed=true`
   (apply `already_finished`); re-send unconfirmed starts every iteration
   and on reconnect; never re-send for terminal tasks.
3. Event ingestion: per-execution `seq`, unique `(task_id, vm_id, seq)`
   insert-ignore, cumulative `acked_event_seq` watermark acknowledged to the
   dispatcher; terminal outcome persisted with the chat assistant message
   update in one transaction before ack; on (re)connect send watermarks and
   outstanding start intents, replay journals, reconcile: confirmed-running
   unknown → `failed(vm_lost)`; processes for terminal tasks → cancel.
4. Reconciler (30 s): incarnation no longer current → `failed(vm_lost)`
   (draining is live); question expiry; successor turns; wall clock;
   re-offer queued and unconfirmed. Replace the lease_reaper hooks added in
   P3.3.
5. Answer flow: `POST /questions/:id/answer` commits first; the MCP server's
   poll (P3.3) delivers; `consumed` route sets `consumed_at` and returns the
   task to `running`; stop/cancel while waiting → `interrupted`/`cancelled`
   with the question left answerable/cancelled per design.
6. Tests: api_task_lifecycle.rs — every transition; new task and new chat
   turn with all VMs stopped resume and run; crash between record and send
   re-sends the same exec_id and runs once (fake dispatcher client);
   cancel racing an unconfirmed start never launches; channel loss before
   spawn, after spawn, and at exit each yield one run and one outcome; API
   restart mid-tool-call yields a complete ordered timeline including a late
   lower-seq event; answer committed then process gone → exactly one
   successor turn; stranded pending assistant message repaired.

Do not: delete sticky runtimes, worker pins, or job branches (P4.7, P4.8);
delete the old kernel (P7.1).

Acceptance: listed tests pass; `TaskService::apply` is the only writer for
tasks that carry a `vm_id`.
Validation: cargo check, lint, fmt, `cargo test -p g5-api-core --lib`,
`just test-integration`.
```

### P4.7 — Chat turns on the dispatcher; delete sticky runtimes

```
You are implementing P4.7 of docs/lean-implementation-plan-v1.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v6 §8.1, §8.2, §8.5,
§8.7, and docs/research/research-chat.md (all sections, especially B, D, G).
Depends on P4.6.

Goal: a chat turn is a `chat_turn` task run by the workspace dispatcher; no
sticky runtime, worker pin, route classification, forced fork, or resident
process exists.

Do:
1. Chat send (routes/chat.rs, chat_service.rs): insert user + pending
   assistant under the per-session lock, create a `chat_turn` task with
   `spec` (provider, model, permission mode, prompt, attachments,
   provider_session_id), and return; the P4.6 loop dispatches it. Streaming:
   dispatcher events → `chat.chunk`, `chat.message_updated`, `chat.done`,
   `chat.error` with today's names; timeline into `task_events`. Add
   `chat_messages.author_id`; any workspace member may send (relax the
   owner-only guard in scoped.rs for send; keep it for delete/rename if you
   wish). Fork stays a row copy; workspace carry copies rows.
2. Delete: `chat_sticky_runtimes` and every sticky column, the
   `sticky_runtime.rs` service, both runtime keepers, parked runtimes in the
   supervisor, `chat_worker_loss.rs`, `ChatWorkerRoute` classification,
   `required_worker_id`, `branch_mode`, `attach_preview_task_id`,
   `chat_sessions.status`, `session_type='task'` and
   continue-in-merged-workspace, chat context links
   (`chat_context_service.rs`, the two tables, 4 routes, `g5d chat context`,
   ChatContextLinks.tsx), `agent_approval_handoffs` (chat approvals are
   `task_questions` since P3.3), heartbeat sticky riders in
   g5-internal-api DTOs, and the `authz.*sticky*` SQL functions and trigger
   from the baseline.
3. Frontend: ChatPanel loses worker pins, sticky badges, context links,
   "continue in merged workspace", worker conflict logic
   (lib/chatWorkerConflict.ts, WorkspacePageChatWorkerPin); concurrent
   sessions are ordinary; an author label per message.
4. Tests: api_chat.rs retargeted to the task-backed turn; e2e chat scenarios
   (chat_agent.rs, chat_wait_input.rs, timeline_streaming.rs) on the
   dispatcher with fake providers; Playwright chat.spec.ts and
   chat-timeline-recovery.spec.ts; delete sticky/context-link tests
   (api_chat_context*.rs, chat-context-links.test.ts, codex-native-resume-release
   sticky parts).

Do not: touch non-chat job kinds (P4.8) or the host's per-task code (P4.9).

Acceptance: two chat sessions run concurrently in one workspace VM with fake
providers; a message to a stopped workspace resumes it; stop mid-turn yields
`interrupted` and the answer to an open question starts one successor turn;
`grep -rn "sticky\|required_worker_id\|context_link" backend frontend/src` is
empty.
Validation: cargo check, lint, fmt, `just test-integration`, `just docker-ci`.
```

### P4.8 — Agent tasks and jobs on the dispatcher; delete job/task branches and job merge

```
You are implementing P4.8 of docs/lean-implementation-plan-v1.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2 and §3 (deviation 2), design v6
§7.1, §7.3, §7.4, §7.5, and docs/research/research-task-lifecycle.md §A.3,
§A.4, §E; research-governance.md §C.3. Depends on P4.6.

Goal: every non-chat job kind (uplift, downlift, sync, explainer,
agent_review, custom) runs as tasks in the workspace VM on the workspace
branch; there are no job or task branches and no job merge.

Do:
1. Job kinds become profiles: a `jobs.spec` carrying the profile name and
   parameters; child tasks are `agent` (provider run with the profile's
   prompt) or `command` (`g5d bootstrap` for uplift's deterministic DAG,
   which submits child tasks through the API). Remove `job/<id>` and
   `task/<id>` branch creation (job_service.rs, task_service.rs), the
   `branch_name`/`base_commit` task columns' git meaning (keep informational
   `base_sha`), and make agent tasks push to the workspace branch natively
   (P2.3 already pushes to "the target branch"; the target is now always the
   workspace branch).
2. Delete `job_merge_service.rs`, `job_merge_runs`, `job_merge_artifacts`,
   the `/jobs/:id/merge*` routes, `g5d job merge`, the `merge` job and task
   types, the job-merge Playwright and API tests (job-merge-back.spec.ts,
   workspace-merge.spec.ts parts about job merge, api_jobs merge tests), and
   the job-merge frontend (job-merge-workflow/*, useJobs merge hooks).
3. Job status derived from children; cancel cascades; no pause (delete
   pause/resume routes and UI); no rollback.
4. Map today's uplift/downlift/sync/explainer/approval_review flows onto the
   new tasks with minimal prompt changes; the ontology, docs, and explainer
   projections keep working from pushed commits.
5. Tests: e2e full_pipeline.rs, job_lifecycle.rs, task_dependencies.rs,
   sync_job_one_task.rs, explainer_creation.rs, bootstrap_ontology.rs on the
   dispatcher with fake providers; api_jobs.rs retargeted; Playwright
   jobs.spec.ts, workspace-generate.spec.ts, job-progress.spec.ts.

Do not: touch approvals or PRs (P6); delete the host per-task code (P4.9).

Acceptance: an uplift job with two child tasks runs concurrently in one VM
and both commits land on the workspace branch; `grep -rn "job/\|task/\|job_merge" backend`
finds no branch naming or merge-run code.
Validation: cargo check, lint, fmt, `just test-integration`, `just docker-ci`.
```

### P4.9 — Delete per-task execution, golden images, workers table

```
You are implementing P4.9 of docs/lean-implementation-plan-v1.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v6 §3.1 (per-task
execution, golden images, fleet rows), §6.2 (Removed), §7.5, and
docs/research/research-exec-isolation.md §C, §D, §F, "Simplification
candidates". Depends on P4.7, P4.8.

Goal: the only execution path is the workspace VM. Everything that existed
for per-task VMs is gone.

Do:
1. Host: delete from backend/g5d-host the per-task assignment loop
   (poll/claim/ack/nack/observation), `Materializer` per task,
   `WorkspaceMaterializer`, rootfs-per-task COW (keep template extraction and
   one COW per workspace VM), the handoff file, deferred/superseded
   bookkeeping, capacity backoff, saturated probe, `preview_vms` registry
   (P5.1 replaced it), golden_builder.rs, golden_consumer.rs,
   golden_maintenance.rs, discard fence, per-task attestation.
   backend/g5d-foundation: delete golden/, workspace/materialize.rs and
   repo_cache.rs (keep the bare-repo cache only if P4.3's restore uses it;
   otherwise delete), oci/ stays.
2. Worker: delete backend/g5d-worker/src/{poll.rs, inbox.rs (dispatcher
   replaces the inbox), task_agent.rs per-task lifecycle parts, task_env.rs
   per-task credential injection (workspace token only), build_telemetry.rs,
   port_scan.rs if unused by the dispatcher, view_task_builder.rs parts that
   assumed task branches}; `g5d worker` command and `g5d task poll|claim|retry`;
   `g5d golden-build`, `g5d preview-attach-listener`.
3. API: delete `workers` table and routes (`/api/v1/orgs/:id/workers*`,
   `/workspaces/:id/workers*`), worker registration/heartbeat/poll/claim/ack
   internal routes, `worker_tokens`, `WorkerTokenScope`,
   `worker_session_generation`, `host_discard_fence_*`, `task_execution_attempts`
   (replaced by `task_attempts` from design §7.1: add it here),
   `builder_task_specs`, `session_build_telemetry`, `workspace_prebuild_state`,
   `runtime_attribution_events`, `cloud_orphan_sightings` if unused, worker
   host columns that only per-task bring-up used; maintenance loops
   `golden_builder_triggers`, `lease_reaper` (P4.6's reconciler replaced it),
   `build_telemetry_aggregation`, `attribution_retention`.
4. Frontend: delete WorkersDropdown, ConnectedWorkerPill,
   WorkspaceCapacityIndicator, capacity/previewLinks, worker pinning,
   `useWorkers.ts`, settings "concurrent tasks / cloud workers / idle
   timeout" (keep idle timeout as the VM idle setting).
5. Infra: remove golden knobs and per-task env from Helm, compose,
   startup_script.rs, self-hosted scripts; keep `G5D_HOST_MAX_MICROVMS`.

Do not: touch approvals/PRs or the Terraform MIG (P8.4).

Acceptance: `grep -rn "materializ\|golden\|poll_for_task\|claim_task\|worker_session_generation" backend`
is empty; docker-ci passes with the worker lanes replaced by dispatcher
lanes.
Validation: cargo check, lint, fmt, `just test-integration`, `just docker-ci`.
```

### P4.10 — Frontend: workspace VM status, live reads, Changes, Commit/Push

```
You are implementing P4.10 of docs/lean-implementation-plan-v1.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v6 §5.6, §5.7, §6.3
(concurrency), §9.1, §9.2, and docs/research/research-frontend-preview-infra.md
§1.1, §1.3. Depends on P4.6.

Goal: the workspace shell shows and controls the VM, reads live files, and
exposes Changes, Commit, Push, Update from origin, and the association check.

Do:
1. Status strip: VM state (stopped/starting/running/stopping/failed) with
   Resume and Stop, "resuming workspace" when a Save wakes it,
   `WORKSPACE_STOPPING` handling, idle countdown optional.
2. Reads: use the `source` marker (live/checkpoint/committed) in the file
   tree, editors, docs, ontology; refetch on `file.changed`.
3. Changes panel from `git.status` (`GET /workspaces/:id/git/status`), diff
   from `git.diff`, Commit (message dialog) and Push buttons, "Update from
   origin" (`git.fetch_rebase`) with ahead/behind display; association
   mismatch banner with "Switch back".
4. Multi-task notice when more than one task or chat turn is running.
5. Remove `useFileTree` polling, draft-era commit UI leftovers, and any
   remaining `workspace.authoritative_cut_advanced` handling.
6. Playwright: workspace-vm.spec.ts (resume/stop/resume with restored edits
   using a fake provider that writes a file), changes-and-commit.spec.ts,
   association-mismatch.spec.ts (a terminal `git checkout` then Save).

Do not: touch preview/terminal panels (P5), PR/approvals (P6).

Acceptance: build and tests pass; Playwright flows pass against the
docker-ci stack with a workspace VM.
Validation: frontend build and tests, Playwright lane of docker-ci.
```

### P5.1 — Preview on the workspace VM

```
You are implementing P5.1 of docs/lean-implementation-plan-v1.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v6 §6.4, and
docs/research/research-frontend-preview-infra.md §2 (all). Depends on P4.5.

Goal: preview is a process in the workspace VM reached through the existing
relay keyed by workspace and port.

Do:
1. Dispatcher: `preview.start(profile)` runs the profile command
   (`.g5/preview_profiles/<name>.yaml` or `.g5/workspace.yaml preview:`),
   records the port, `preview.stop`, `preview.status`; `ports.list` from
   `/proc/net/tcp` with `port.opened/closed` events (reuse
   backend/g5d-worker/src/port_scan.rs).
2. API: `/api/v1/workspaces/:id/preview/:port/*` relay: keep the HTTP and
   WebSocket relay code of routes/preview.rs, replace `resolve_preview_target`
   with (workspace → current host and `vm_id` → `http:<port>` mux stream);
   workspace-scoped ticket cookie and mint route; subdomain mode keyed by
   workspace id; `POST /workspaces/:id/preview/start|stop`, `GET .../ports`.
3. Delete the `preview` job type, `job_preview_sessions`, boot generations,
   `preview_reaper`, chat-attach listener/token/host drive
   (`g5d preview-serve`, `preview_attach_wire.rs`, agent_drive preview parts),
   `attach_supported`, and their tests (api_preview_attach.rs,
   preview_chat_attach.rs).
4. Frontend: PreviewControl starts/stops by profile without commit/TTL
   plumbing; PreviewBrowserView unchanged apart from the URL; a Ports panel
   listing open ports with "open".
5. Tests: dispatcher preview and ports tests; api_preview.rs (relay reaches a
   port opened by a fake provider process, ticket scope is one workspace,
   stop kills the process); Playwright preview.spec.ts.

Do not: touch terminal (P5.2).

Acceptance: a dev server started by a fake provider turn is reachable
through the relay in docker-ci; `grep -rn "job_preview_sessions\|preview_attach" backend`
is empty.
Validation: cargo check, lint, fmt, `just test-integration`, `just docker-ci`.
```

### P5.2 — Browser terminal

```
You are implementing P5.2 of docs/lean-implementation-plan-v1.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v6 §6.7, §6.3 (`pty`
row), and docs/research/research-exec-isolation.md §H. Depends on P4.4.

Goal: a browser terminal into the workspace VM.

Do:
1. Dispatcher: `pty.open(cols, rows)`, `pty.write`, `pty.resize`,
   `pty.close` over a dedicated `pty` mux stream using
   backend/g5d-foundation/src/terminal (TerminalManager); shell as uid 1000
   in `/work`.
2. API: `WS /api/v1/workspaces/:id/terminal` (workspace access, wakes the VM)
   bridging the browser WebSocket to the `pty` stream through the host
   tunnel; terminal activity counts as VM activity.
3. Frontend: xterm.js panel in the bottom drawer (`terminal` tab replaces
   the timeline-output-only TerminalPanel.tsx); resize handling; survives
   editor navigation within the page.
4. Tests: dispatcher pty test; API bridge test with a fake dispatcher;
   Playwright terminal.spec.ts (echo a command, resize).

Do not: record terminal sessions or add auditing.

Acceptance: a command typed in the browser runs in the VM and its output
appears; docker-ci passes.
Validation: cargo check, lint, fmt, frontend build and tests, `just docker-ci`.
```

### P6.1 — `pull_requests`, `approval_rules`, `approval_decisions`, `g5d pr`

```
You are implementing P6.1 of docs/lean-implementation-plan-v1.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v6 §10.1 to §10.3,
§11.1, and docs/research/research-governance.md §B, §D. Depends on P1.5, P4.1.

Goal: the minimal governance data model and routes; agents open PRs and
record reviews through `g5d pr`; provider PRs are mirrored at publish.

Do:
1. Baseline: `pull_requests` (design §10.2 shape including the
   merge-operation columns), `approval_rules`, `approval_decisions` (design
   §10.3); drop the old `workspace_pull_requests`, `_events`,
   `_provider_repos` tables (their code is deleted in P6.4; here replace the
   routes' data source).
2. Routes: `POST /workspaces/:id/pull-requests` (reads the source OID from
   git, records `published_sha`, publishes via P1.5's provider push, opens a
   provider PR when the repo has a provider), `GET`, `POST .../close`,
   `POST .../decisions` (records `source_sha` the approver saw),
   org routes for `approval_rules` CRUD (org admin), provider
   branch-protection read at repo connect with a warning flag.
3. `head_sha` as a projection of the source branch under P1.3's convergence
   (`pr_head` projection).
4. CLI: `g5d pr create|status|review --approve|--request-changes --sha` using
   the workspace/task token; agent review tasks are launched with the OID to
   review (job kind `agent_review` from P4.8) and must pass it to `g5d pr
   review`.
5. Tests: api_pull_requests.rs (create publishes and mirrors; decisions
   bind to the reviewed sha; a new push makes them stale), api_approval_rules.rs,
   CLI tests for `g5d pr`.

Do not: implement merge (P6.2); touch the frontend (P6.3); delete the old
approval services (P6.4).

Acceptance: tests pass; an agent can open a PR from a fake provider turn via
`g5d pr create`.
Validation: cargo check, lint, fmt, `just test-integration`.
```

### P6.2 — The merge operation

```
You are implementing P6.2 of docs/lean-implementation-plan-v1.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v6 §10.6 (all), §5.5
(per-repo lock), and docs/lean-backend-design-v5-review-response.md F7.
Depends on P6.1.

Goal: `POST /pull-requests/:id/merge` as the only writer of protected refs,
implemented as the persisted, resumable operation of design §10.6.

Do:
1. Implement the six steps exactly: pin (or resume an unresolved operation
   by `merge_op_id`/`merge_state`), authorize against decisions at the pinned
   source OID, mergeability on the bare mirror, provider path (republish if
   `published_sha` differs; provider merge API with the expected SHA on
   GitHub and GitLab; record `provider_merged` durably; import the provider's
   target head under the per-repo lock shared with inbound mirroring; retry
   import without re-pinning; reconcile ambiguous replies by reading the
   provider PR and attribute only if the merged head equals the pinned
   source), local-only path (write the merge commit, persist
   `merge_result_sha`, then CAS; recognize an already-applied result on
   recovery), failure only before application.
2. Receive-pack: the API server identity is the only writer of protected
   refs; no network path.
3. A retry endpoint or automatic retry loop for unresolved operations
   (maintenance, backoff).
4. Tests: api_merge_operation.rs — refused without decisions at head; an
   internal push after publication makes the merge republish and pass the
   expected SHA; a source push racing the merge is refused by the provider
   precondition (fake provider server); crash before and after the provider
   call, lost success reply, inbound target update during merge, provider
   success then import failure, local CAS success then crash before the
   metadata commit — each converges on retry; a remotely merged PR at a
   different head is not attributed.

Do not: touch the frontend (P6.3).

Acceptance: listed tests pass against a fake GitHub/GitLab HTTP server.
Validation: cargo check, lint, fmt, `just test-integration`.
```

### P6.3 — Governance frontend

```
You are implementing P6.3 of docs/lean-implementation-plan-v1.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v6 §10, and
docs/research/research-frontend-preview-infra.md §1.3 rows 13 to 15.
Depends on P6.1.

Goal: one PR panel and one approvals surface on the new tables.

Do:
1. PullRequestPanel/List/CreatePullRequestDialog on `pull_requests`; show
   the pinned source SHA, decisions at that SHA, provider PR link, merge
   state and error; a Merge button calling the P6.2 route (visible when the
   rule is satisfied, otherwise explains what is missing).
2. Approvals: replace WorkspaceApprovalsPageContent and
   OrgApprovalsPageContent with an org-level `approval_rules` editor and a
   per-PR decision UI (approve / request changes with note).
3. Delete PrepareToShipModal, ReviewChangesModal, RebaseChangeGraphPreview,
   the change-graph views (SourceControlChangeGraphView*), workspace merge
   modals, `useWorkspaceApprovals`, `useOrgApprovals`, approvalStages, and
   the `stages` status band panels.
4. Playwright: pull-requests.spec.ts, approvals.spec.ts; delete
   org-approvals.spec.ts, workspace-pull-requests.spec.ts old versions,
   source-control-change-graph.spec.ts.

Do not: touch backend code beyond DTO alignment.

Acceptance: build and tests pass; Playwright flows pass in docker-ci.
Validation: frontend build and tests, Playwright lane.
```

### P6.4 — Delete governance legacy

```
You are implementing P6.4 of docs/lean-implementation-plan-v1.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v6 §10.4, §10.5, and
docs/research/research-governance.md §G, "Simplification candidates".
Depends on P6.2, P6.3.

Goal: nothing remains of the approval ledger, coordinated publication, PR
provider lifecycle, mutation policy, or fine-grained scopes.

Do:
1. Delete services approval_config_service.rs, approval_definition_service.rs,
   approval_review_service.rs, coordinated_repo_publication.rs,
   workspace_pull_request_provider_lifecycle.rs (after moving any helper P6.2
   still uses), workspace_mutation_policy.rs and every `ensure_workspace_mutable`
   call, routes org_approvals.rs, workspace_approvals.rs, the merge/rebase
   routes left in workspaces.rs, `source_control_change_graph_service.rs` and
   its route.
2. Baseline: drop approval_definitions, approval_configs, approval_requests,
   approval_statuses, approval_review_runs, merge_evaluations,
   `jobs.approval_context`, `workspaces.{mutation_policy*, stages,
   merge_target_workspace_id}`, `user_scope_grants`,
   `workspace_permission_overrides`, `org_default_scopes`, `oauth_scopes`,
   the 17 validator functions and 22 governance triggers; collapse
   permission_service.rs to the four org roles plus workspace visibility and
   members (delete routes/permissions.rs except `/auth/me/scopes` which
   scripts use).
3. Delete tests: api_agent_approval_*.rs, api_workspace_approvals.rs,
   api_org_approvals.rs, approval_*_static.rs, approval_request_retention.rs,
   route_auth_static.rs entries for removed routes, e2e approval_review_live.rs,
   merge_source_publish.rs.
4. Maintenance: delete `approval_request_retention`.

Do not: touch login providers (P8.3).

Acceptance: `grep -rn "approval_definition\|approval_request\|mutation_policy\|scope_grant" backend frontend/src`
is empty; docker-ci passes.
Validation: cargo check, lint, fmt, `just test-integration`, `just docker-ci`.
```

### P7.1 — Lifecycle cleanup: one status enum, delete the kernel

```
You are implementing P7.1 of docs/lean-implementation-plan-v1.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v6 §7.1, §7.2, §7.5,
§13 Phase 7, and docs/research/research-task-lifecycle.md §B, §H. Depends on
P4.9, P6.4.

Goal: one task status vocabulary (`queued|running|waiting|succeeded|failed|
cancelled`) in the database, Rust, and frontend; the old execution kernel is
gone.

Do:
1. Delete backend/g5-api-core/src/services/execution_lifecycle/*, the
   `LifecycleCommand`/`JobRollupKind` families, lock plans, `apply_lifecycle`
   call sites (all now go through P4.6's `TaskService::apply`), task status
   values `pending|claimed|paused|blocked|rolled_back|recovering|recovery_blocked`
   and `merged`, `task_question_windows`, `human_escalations`,
   `human_escalation_questions` (all HITL is `task_questions`),
   `task_completion_constraints`, `task_checkpoints`, review-checkpoint
   columns on jobs, `pruned`, `is_checkpoint`, `max_retries` semantics, the
   `scheduler` remnants, `TaskType` enum drift (one enum matching the DB).
2. Frontend: `TaskDTO.status` typed to the six values; job status derived;
   remove pause/resume/rollback controls if any remain.
3. Tests: api_tasks.rs and api_jobs.rs retargeted to the six states; delete
   task_lifecycle_contract parts for removed states; e2e task_lifecycle_contract.rs
   updated.

Do not: rewrite the baseline file wholesale (P7.2).

Acceptance: `grep -rn "recovery_blocked\|rolled_back\|LifecycleCommand" backend frontend/src`
is empty; the three status vocabularies are identical.
Validation: cargo check, lint, fmt, `just test-integration`, frontend tests.
```

### P7.2 — Rewrite the baseline schema to the final table set

```
You are implementing P7.2 of docs/lean-implementation-plan-v1.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v6 §12 (all). Depends on
P7.1.

Goal: `001_lean_baseline.sql` contains exactly the design's table set (about
36 tables), clean of every dropped table, column, function, trigger, and
policy, and bootstraps from zero.

Do:
1. Rewrite the baseline by hand from the design's §12 list and the tables
   the code still queries (grep `sqlx::query` and the models). Keep the
   `authz` schema, RLS tenant isolation, the runtime roles and grants that
   the three pools need, and seed rows. Remove the `-- P<x.y>` section
   markers; the file is now the canonical schema.
2. Tighten what earlier PRs left loose: `task_events.vm_id/seq` NOT NULL,
   `tasks.vm_id` semantics, drop `head_commit` columns replaced by
   `head_sha_cache`, provider enums, `configs(scope, scope_id, key, value)`
   replacing org/user/workspace config tables if the code was migrated
   (otherwise keep the three and note it).
3. Regenerate the SQLx offline cache; verify bootstrap from an empty
   database; verify `just test-integration` and `just docker-ci`.
4. Update docs/lean-implementation-status.md with the "after" metrics
   (same commands as P0.1).

Do not: change behavior.

Acceptance: table count matches the design within a few tables and every
difference is explained in the PR; bootstrap from zero passes; docker-ci
passes.
Validation: `just db-prepare`, cargo check, `just test-integration`, `just docker-ci`.
```

### P8.1 — Remove internal issue tracking

```
You are implementing P8.1 of docs/lean-implementation-plan-v1.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v6 §3.2 (issue tracking
row), §11.3, and docs/deployment-gate-audit.md §1 (issue tracking row).
Depends on P3.2.

Goal: the issue tracker module is gone; platform bug reporting, comments,
and notifications stay.

Do: delete backend/g5-api-routes/src/routes/issues.rs, the internal issue
endpoints in routes/internal.rs, backend/g5-api-core/src/services/{issue_service,
issue_attachment_store}.rs, maintenance `issue_inference` and
`issue_attachment_scanner`, backend/g5d-runtime-core/src/issue_tracking.rs,
backend/g5d-tooling/src/tools/{issue_ops,issue_context,search_issues,
add_issue_comment,update_issue}.rs and their MCP registration, `g5d issue`,
frontend IssuesPage and components/issues/*, hooks/useIssues.ts, the issue
tab/URL/WS wiring, `issue.*` WS events, `G5_ISSUE_TRACKING_ENABLED`, the
thirteen issue and hotlist tables from the baseline, and the `g5-report-issue`
skill. Keep `platform_issue_outbox` and the platform bug report button.

Do not: touch anything else.

Acceptance: `grep -rni "issue_tracking\|hotlist\|IssuesPage\|file_issue" backend frontend/src assets`
is empty except platform-issue code; docker-ci passes.
Validation: cargo check, lint, fmt, frontend build and tests, `just docker-ci`.
```

### P8.2 — Remove IDE mockup, hidden settings UI, takeover remnants

```
You are implementing P8.2 of docs/lean-implementation-plan-v1.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v6 §3.2, and
docs/deployment-gate-audit.md §1 (IDE plugin, hidden notification settings,
takeover rows). Depends on P0.1.

Goal: three dead frontend surfaces are removed.

Do: delete the `/ide` route and pages/IDEPluginView.tsx with mockup
fixtures; the dev-mode notification-delivery toggles and channel form in
components/settings/WorkspaceSettingsContent.tsx and their tests (trace and
drop the persisted preference fields they wrote, in the baseline if any);
the takeover banners, `takeoverUiEnabled`, `onTakeOver` plumbing in
BottomDrawer.tsx and WorkspacePage.tsx, and exclusive fixtures. Keep the
notification bell, mentions, toasts, and the question-document interaction.

Do not: touch anything else.

Acceptance: build and tests pass; grep for `IDEPluginView|takeover|renderDevMode`
in frontend/src is empty.
Validation: frontend build and tests.
```

### P8.3 — GitHub-only platform login

```
You are implementing P8.3 of docs/lean-implementation-plan-v1.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v6 §3.2, §10.4, and
docs/deployment-gate-audit.md §1 (platform sign-in row). Depends on P0.1.

Goal: platform login supports GitHub only; SCM repository connections for
GitHub and GitLab and provider-account OAuth are untouched.

Do: remove the Google, GitLab, and Microsoft platform-login adapters in
backend/g5-api-core/src/auth/oauth.rs and routes/auth.rs (callback and
config mappings, Entra role map), their config keys in
backend/g5-api-foundation/src/config.rs, Helm/compose/env examples and
secrets, frontend login icons and catalog entries (lib/authProviders.tsx),
and exclusive tests. `G5_OAUTH_ENABLED_PROVIDERS` becomes GitHub-only or is
removed.

Do not: touch `user_scm_connections`, SCM OAuth for repositories, or
Claude/Codex provider accounts.

Acceptance: login with GitHub works in docker-ci; grep for
`microsoft|entra|google_oauth` in auth code is empty.
Validation: cargo check, lint, `just test-integration`, `just docker-ci` login
and Playwright login.spec.ts.
```

### P8.4 — Infra cleanup

```
You are implementing P8.4 of docs/lean-implementation-plan-v1.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v6 §6.6, §13 Phase 8,
and docs/research/research-frontend-preview-infra.md §3, §5.3. Depends on
P4.9.

Goal: infrastructure matches the workspace-VM fleet and the two deployment
targets.

Do: remove the Terraform `worker_host` and `tungsten_worker_host` MIG modules
and daily scheduler (target size is zero everywhere), the Tungsten/PairSoft/
PNC customer image Dockerfiles and their Cloud Build steps, the disabled
GitHub workflows (ci.yml, e2e.yml, release.yml, base-image.yml), Helm worker
Deployment/HPA templates and Istio template, per-task env from
startup_script.rs, self-hosted installers, and compose; replace the 24 h host
JWT with a renewable host credential (design §6.6); keep the Azure canary
lane and update it to the workspace-VM host. Update runbooks and README.

Do not: change CI (P8.6).

Acceptance: `terraform validate` on each environment, `helm template` on
each overlay, self-hosted bundle contract tests, and the Azure canary
workflow's local contract tests pass.
Validation: `just helm-lint`, `just infra-plan env=dev` (plan only), script
regression suites (`python3 scripts/run-ci-script-tests.py`).
```

### P8.5 — Crate consolidation

```
You are implementing P8.5 of docs/lean-implementation-plan-v1.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v6 §13 Phase 8, and
docs/relocation-validation.md. Depends on P7.2.

Goal: fewer crates with clear ownership, no behavior change.

Do: propose the target layout in the PR description first (for example:
`g5-api` = routes + core + data + foundation + contracts; `g5d` = cli +
dispatch + host + isolation + runner + tooling + foundation; shared:
`g5-scm`, `g5-storage`, `g5-auth`, `g5-authz`, `g5-semantic`), then move
modules, fix paths, keep public APIs used by tests, update Cargo.toml,
Dockerfiles, Justfile, scripts/check-layout.py, and docs/architecture.md.

Do not: change behavior or delete code.

Acceptance: workspace builds; all tests pass; `just commands-check` and
`python3 scripts/check-layout.py` pass; docker-ci passes.
Validation: full ladder and `just docker-ci`.
```

### P8.6 — CI review

```
You are implementing P8.6 of docs/lean-implementation-plan-v1.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2 and design v6 §13 Phase 8.
Depends on P8.5.

Goal: Cloud Build reflects the lean repository; nothing else changes.

Do: prune infra/ci/cloudbuild-*.yaml of retired lanes (worker per-task
lanes, golden canaries, customer images, CRDT lanes), rename dispatcher and
workspace-VM lanes, make sure `just docker-ci` phases map to the new lanes,
update docs/just-commands.md, and run the CI contract tests
(scripts/test_ci_docker_contract.py, scripts/test_cloudbuild_waitfor_order.py).

Do not: change application code.

Acceptance: contract tests pass; a full `just docker-ci` passes locally.
Validation: `python3 scripts/run-ci-script-tests.py`, `just docker-ci`.
```

---

## 6. Phase exit criteria (from design §13, restated as checks)

| Phase | Done when |
|---|---|
| 0 | `lean-before` tag exists; one baseline migration; status file has before-metrics |
| 1 | plain `git push` to a workspace branch lands and is read back; push to `main` refused; fork creates a ref; dropped event converges; no remote-tracking code |
| 2 | no `yjs`/`yrs` anywhere; explicit Save with conflict dialog; agents commit and push natively; profiles describe native git |
| 3 | fake-claude and fake-codex turns run through the worker with `--resume` continuity, permission prompts (Claude) and questions (both) answered from the UI; session files in Postgres; no Node sidecars; no `naive_react` |
| 4 | one VM per workspace: two concurrent chats, wake on message, stop/resume with checkpoint restore, host kill recovery, stale VM fenced, complete timeline across an API restart; no per-task execution code |
| 5 | preview and terminal on the workspace VM |
| 6 | PR opened by an agent cannot merge without decisions at the pinned SHA; merge operation recovers from every listed crash point |
| 7 | one status enum; kernel deleted; baseline is the final schema |
| 8 | approved removals done; infra matches; crates consolidated; CI reflects the lean repo |
