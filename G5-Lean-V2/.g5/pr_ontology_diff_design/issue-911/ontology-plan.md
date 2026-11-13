# Seed a code repository in the operator Base (issue #911)

Request: [G5Labs/G5-Lean#911](https://github.com/G5Labs/G5-Lean/issues/911). Governing
block: [base-seed](../../ontology/features/organizations.g5.md#g5.organization.platform-operator.base-seed),
added in this change and linked from [bootstrap](../../ontology/features/organizations.g5.md#g5.organization.platform-operator.bootstrap).
It references the tenant [creation cascade](../../ontology/features/organizations.g5.md#g5.team.onboarding),
whose text is aligned with the code repository that cascade already creates. Earlier
design: [issue #776](../issue-776/ontology-plan.md#bootstrap-platform-seeding-provisions-the-operator-org).

## Outcome

`scripts/seed-prod-org.sh` gives the operator Base the same structure a tenant Base gets
from `OrgService::create_org`: a docs member and an internal code member, each with a
first commit on its branch. A child forked from Base starts. It has no
`.g5/workspace.yaml`, so [image selection](../../ontology/software_architecture/hosts.g5.md#g5.host.image-selection)
falls back to the organization default image. The runtime rule that a non-personal
workspace needs a code member stays as it is; the issue chose this over relaxing it.

## Owners and interactions

| Concern | Owner | Change |
| --- | --- | --- |
| Operator org rows | seed SQL, as today | new step after Base exists: ensure the code repository and its Base membership |
| First commits | the existing mirror initializer, run in the API pod (`USE_K8S_PSQL=1`) | generalized from docs-only to each seeded repository |
| Branch heads | Git refs in API repo storage (`<mount>/<prefix>/<repo-id>`), which are the head [authority](../../ontology/software_architecture/git.g5.md#g5.git.authority) | none; the seed writes the first commit to an unborn ref, as it already does for docs |
| Image at Start | `resolve_workspace_image` | none; a README-only code member resolves to the org default |

### Code repository and membership (SQL, one transaction)

Same identity as the tenant cascade: name and slug `code`, `git_url`
`g5-local://workspaces/<base-id>.git`, branch `main`, Base member with `root_path`
`code` and role `code`.

1. If Base already has any member with role `code`, change nothing. This keeps a code
   repository an administrator attached, for example the one `reset-qa.py` installs.
2. Otherwise, insert the org's `code` repository if no repository has that slug
   (`ON CONFLICT (org_id, slug) DO NOTHING`; existing rows are never updated). If the
   `code` slug belongs to a provider-backed repository, fail with an explicit error.
   The seed must not invent history for a provider repository, and choosing another
   repository is an administrator decision.
3. Attach it to Base (`ON CONFLICT (workspace_id, repo_id) DO NOTHING`).

The step returns the org's providerless `code` repository's id and default branch
whenever it is a Base member, including one attached by an earlier run whose first
commit was skipped. It returns nothing when Base's code member is another repository.
Only the returned repository is handed to mirror initialization. The docs upsert
(step 3 today) is unchanged.

### First commit (API pod, per seeded repository)

The existing rules are kept and applied to each repository: create the bare repository
if it is absent. If the branch ref is missing and the repository holds no refs, push one
parentless commit. If refs exist but the branch is missing, refuse with `MISSING_REF`,
because that mirror needs restoring from backup. An existing branch is never touched.
Docs keeps its empty commit. Code gets a `README.md` with the tenant cascade's text
(`# code` / "This is a G5-managed local project repository."). `DOCS_MIRROR_INIT` becomes
`REPO_MIRROR_INIT`; the old name is still honoured as a fallback. Without kubectl or
with `skip`, both repositories stay unborn, with the warning docs already prints.

### Why not reuse the API cascade

The seed writes rows directly because the operator org has no person who could call an
API, and the API exposes no "add a code repository to Base" operation. Adding a
platform endpoint or a startup hook for this would bring in a new owner and new
authority just for a one-off seed. The bash initializer already works this way for
docs, so the seed reuses it, with the same refuse-instead-of-invent rule.

### Concurrency

The seed runs alone, by an operator, but the API may be serving at the same time.
- Repeated or overlapping seed runs: the slug and membership unique constraints mean
  each row exists at most once. Two concurrent first pushes cannot both win: the loser
  is not a fast-forward, so the push is rejected and the run fails loudly. A rerun then
  finds the existing branch.
- Fork admitted during a repair: before the membership commits, the child is
  docs-only, like children forked before the fix. Between the membership commit and the
  first commit, fork refuses because the source branch is absent, as it does for docs
  today. Once the first commit lands, children get both members. This window lasts only
  while the seed is running, so the seed does not take the workspace commit fence.
  Taking it would tie a bash literal to the Rust lock key and would not close the
  window, because fork reads membership before it takes the fence.
- No API writer creates the first commit of a Base code ref outside the tenant cascade,
  so nothing else races the initial push.

### Not covered

Children forked from the docs-only Base before the repair keep their docs-only
membership and still cannot start. After the repair, merging them into Base is also
refused, because source and target must have the same repository set
(`WorkspaceMergeService`), and comparing them with Base reports
`repository_set_mismatch`. Discard them and fork again. The seed never edits children
and does not report or block on them. Owner decision (2026-10-28): leave this as is;
exactly one such workspace exists, and it will be deleted.

Personal workspaces of operator members are unaffected apart from a `code` reference
checkout at their next Start. Their image is still the organization default, and their
`personal` root does not collide with `code`.

## Validation plan

- `scripts/test_seed_prod_org.py` (in CI): asserts the SQL shape (insert-only code
  repository, membership guarded by "no code member", provider refusal). Runs the real
  pod initializer locally through the kubectl stub, using real Git in a temporary
  storage root, to check: a fresh seed gives both branches with the expected trees; a
  rerun keeps the same SHAs; a code repository with extra history keeps it; a nonempty
  mirror without the branch is refused; with an administrator's code member present,
  only docs is initialized.
- Real Postgres (this VM, migrated schema): run main's seed, which leaves Base
  docs-only, then the new seed (repair), then the new seed again (no changes), then
  attach an administrator code repository and run again (kept). Check row counts,
  membership and refs each time.
- Backend integration (`platform_operator::`): an operator Base in the seeded shape
  (service-user owner, docs and code members, the seeded first commits) with an org
  default image. An operator administrator forks it and starts the child. The Start is
  admitted, the image source is the org default, and the child has both members. In
  the same test, a docs-only Base child is refused with `IMAGE_INVALID`, recording that
  the runtime requirement is intentionally kept.
- Regressions: `seed_workspace_scripts`, the `platform_operator` module, and the
  script tests CI runs alongside the seed test.

## Validation results

Run in the workspace VM on 2026-10-28 against Postgres 17 (the test stack's image), with
the migrations applied one transaction per file.

- **Real seed runs.** A shim stood in for the cluster: `kubectl exec` psql went to the
  local database, and the API-pod shell ran locally against a temporary storage root.
  - Main's seed left a docs-only Base. The new seed repaired it: it added the `code`
    repository (`g5-local://workspaces/<base>.git`, `main`) and attached it at `code`
    with role `code`. Docs kept its existing commit, and code got the README commit.
    A rerun left every row, ref and history unchanged.
  - A fresh database got docs and code with one commit each.
  - With `REPO_MIRROR_INIT=skip`, the rows were written and no mirror was created. A
    rerun then created both first commits.
  - Commits added to the code mirror after seeding were kept.
  - An administrator's provider-backed code member was left as the only code member,
    and no `code` repository was created.
  - A provider-backed `code` slug that was not attached was refused with the explicit
    error, and no member was added.
  - A nonempty code mirror without `main` was refused (`MISSING_REF`), and its refs
    were left as they were.
- **Script tests.** `test_seed_prod_org.py` (16 tests, 5 new) and `test_reset_qa.py`
  pass.
- **Backend integration.** The new `platform_operator::` tests pass:
  - A child of the seeded Base has both members, starts, gets the org default image
    in its `vm.start` spec, and records `image_source` `org_default`.
  - A docs-only Base child is refused with `IMAGE_INVALID`.

  The `platform_operator`, `seed_workspace_scripts`, `api_workspace_fork_names` and
  `api_personal_workspace_runtime` modules pass (47 tests). `cargo fmt --check` and
  `cargo clippy -p g5-api --tests -D warnings` are clean.
- **Not run.** A live QA or dev seed, and shellcheck, which is not installed here.
