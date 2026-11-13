# Disposable QA cutover: PRs 186 and 213

`reset-qa.py` performs rollout steps 4–7 against **only** `g5-qa-1ccc`:
cluster `g5-lean-qa`, namespace `g5`, Cloud SQL instance
`g5-lean-qa-postgres`, database `g5_lean_qa`, and `https://qa.g5labs.ai`.
It has no production target and no project override. Its default is an offline
preview: it does not contact a cloud, database, Kubernetes cluster, or GitHub.

Execution discards all QA application data and API-managed QA worker VMs.
There is deliberately no backup step. It retains the Cloud SQL instance,
roles, credentials, networking, GitHub repositories, Filestore and object
storage. Old repository directories and UUID-keyed cache observations are not
reused by the newly seeded organization; this is not a storage garbage collector.

## Before execution (rollout steps 1–3)

- Land the script and the `_QA_DATABASE_RESET` deployment option together.
  The selected release must also contain the explicit-target `seed-prod-org.sh`.
- PRs **186 and 213** must both be merged, and each must have successful
  `g5-ci-rust` and `g5-ci-e2e` checks on its final PR head. Prefer merging 213
  first, then rebasing/testing 186 on it so the combined code is tested.
- Pause **all three** triggers before merging into the cutover window:
  `g5-deploy-qa`, `g5-packer-worker-main-qa`, and `g5-terraform-apply-qa`.
  Keep Terraform applies and other manual delivery runs on hold. Finish or
  cancel existing delivery builds, including pending Terraform approvals.
  PR CI stays enabled. The script verifies this state and does not change it.
- Record the final full main SHA and hold further main merges through smoke
  testing. Fetch that commit and both merge commits into the local checkout.
  Use a full Git checkout: the script copies the release commit and its complete
  history into the new Base code repository and refuses shallow checkouts.
- Authenticate `gcloud` to QA and `gh` to `G5Labs/G5-Lean`. Install `kubectl`,
  the GKE auth plugin, Python 3.10+ and Bash. The operator needs QA
  deployment/build, GKE and VM deletion permissions.

For each trigger, use the existing export/import pause procedure (repeat with
each of the three names):

```bash
gcloud beta builds triggers export g5-deploy-qa \
  --project=g5-qa-1ccc --region=us-central1 --destination=g5-deploy-qa.yaml
# Set disabled: true in the exported file, then:
gcloud builds triggers import --project=g5-qa-1ccc --region=us-central1 \
  --source=g5-deploy-qa.yaml
```

Manual builds use the paused triggers with an explicit `--sha`; the trigger's
[`disabled` field prevents automatic execution](https://cloud.google.com/build/docs/api/reference/rest/v1/projects.locations.triggers#BuildTrigger).
No trigger is temporarily enabled during the reset.

## Preview and execute

From the repository root, replace the SHA and admin address:

```bash
python3 scripts/reset-qa.py \
  --commit <final-40-character-main-sha> \
  --admin-email you@g5labs.ai \
  --external-issues on
```

The preview prints the target and sequence. Add `--execute` to perform it:

```bash
python3 scripts/reset-qa.py \
  --commit <final-40-character-main-sha> \
  --admin-email you@g5labs.ai \
  --external-issues on \
  --execute
```

The script:

1. Verifies main, both merged PRs and their checks, paused triggers, absence
   of competing delivery builds, Cloud SQL's private IP, and workload inventory.
   It uses a private kubeconfig, explicit context on every Kubernetes command,
   a cluster lock, and a local process lock. It refuses conflicting inherited
   production settings. Unsupported controllers (HPA, CronJob, StatefulSet,
   DaemonSet), unmanaged pods/worker hosts, and non-GKE managed instance groups
   must be quiesced by the operator before proceeding.
2. Scales API/daemon to zero, deletes the release's jobs, waits for writer
   pods to stop, and deletes API-managed QA worker VMs. GKE nodes are excluded.
3. Records the database owner, encoding and locale; disallows new connections;
   terminates remaining sessions; and recreates **only** `g5_lean_qa`.
4. Runs the worker bake at the selected SHA with the old API repin disabled.
   It requires a READY image from that exact commit, then runs the deployment
   at the same SHA with `_QA_DATABASE_RESET=true`. The normal migration hook
   replays the complete chain. Helm still waits for readiness but does not
   automatically roll back to an incompatible old release. Normal QA
   deployments also retain readiness waits and recover forward on failure.
5. Checks the deployed worker image, workload readiness and full migration
   ledger against the release's checksum manifest. It runs the platform seed
   read from that exact commit, which provisions only the operator
   organization (`operator`): its Base docs and code mirrors, image policy and a pending
   admin invitation for `--admin-email`. It writes no Secret Manager secret and
   mints no worker registration token. The first run stops here, because the
   remaining steps act as that admin.
6. With a fresh admin token (see below), creates the `g5labs` organization
   (slug `g5`) with `POST /api/v1/platform/orgs`, owned by `--admin-email`,
   as an operator would from `/operator/home`. A lost response is reconciled
   by finding `g5` in the platform directory, never by creating a second org.
7. Replaces the new Base's code repository. The creation cascade gives every
   Base a local `code` repository; the script removes that untouched local
   repository (refusing any other code repository, a remote binding or a
   child workspace) and installs `https://github.com/G5Labs/G5-Lean.git` as
   Base's code repository. It materializes `main` at the selected cutover
   commit, including its full history, and records Base's code membership and
   remote tracking policy. It sets the Git tracking baseline to that same
   commit. Base's code binding starts with auto-pull on, like every Base
   binding; until the GitHub App grants access, its background cycles stop at
   "Authentication required". Seeding does not push to GitHub or store a Git
   credential. The removed local repository's mirror directory stays on disk,
   like other repository directories from before the reset.

The default receipt directory is `.dev/qa-reset/<commit>/`, mode 0700. It
contains a private kubeconfig, a mode-0600 `state.json`, and the release's Git
object pack. It does not record bearer tokens or database passwords.
Use `--state-dir` only if you want another private location; keep the same
directory, commit, admin address and external-issues setting when resuming.
The release remains paused after this script finishes.

## Finish bootstrap with a fresh login

The reset deletes old login sessions. After the first command completes,
log in to QA as the invited admin; signing in accepts the operator
invitation, so you land in the `operator` organization as an operator admin.
Obtain a fresh bearer token for that session (it carries `org:admin`,
`workspaces:read` and `workers:admin`). Save it in a private file
(mode 0600), then rerun the same command with that file. The script uses it
to create `g5` and, as that org's owner, to finish its bootstrap; each org
route checks your current membership in the target org. Finish Base
bootstrap before creating child workspaces:

```bash
python3 scripts/reset-qa.py \
  --commit <final-40-character-main-sha> \
  --admin-email you@g5labs.ai \
  --external-issues on \
  --admin-token-file /private/path/qa-admin-token \
  --execute
```

Completed phases are skipped. The script creates `g5` and replaces its Base
code repository (steps 6 and 7 above). Through the ordinary authenticated QA
APIs, it then verifies Base's code remote, tracking branch and materialized Git
head, then enables PR 213's `external_issues_enabled` **at organization scope**,
checks that it persisted, provisions one `c3-standard-4` worker using the
`g5d-g5dev` image track, and waits for a ready host with a heartbeat. It does
not store or print the returned host JWT. `--external-issues off` is available
for a separate rollout that intentionally leaves the feature disabled.

In the browser, switch to the `g5labs` organization, then bind the GitHub
App to it in its settings and select the
test repositories. This requires the administrator's interactive GitHub
authorization. The org switch alone does not install the App or grant access.
Include `G5Labs/G5-Lean` in the installation's repository access, then use
**Retry** on Base's code repository card to clear any "Authentication required"
pause and verify that Base's remote sync reads healthy.

The reset and Base code seed work before the GitHub App is configured: the code
comes from the operator's full local Git history, and repository/PR checks use
the operator's `gh` login. App credentials are optional for API startup. If
deployment credentials are also missing, configure those before binding the
installation. App-only auto-push and external issues become usable only after
the App has been configured, bound to the organization and granted repository
access. The script enables the external-issues org switch independently.

Then perform rollout step 8: login/workspace start/resume/save, App auto-push,
PAT-only explicit push, PR creation/merge, ownership transfer and external
issue search/link/comment behavior. Re-enable delivery only after those checks
pass, and release the main merge hold. The script never commits, pushes or merges.

## Failure and recovery

The script leaves writers stopped when failure occurs before deployment and
never rolls back or re-enables triggers. Keep its journal and cluster lock.
After an ordinary interruption, rerun the same command; completed phases are
skipped and submitted builds are followed by their recorded IDs. A lost build
response is reconciled by the unique run ID, not by submitting another build.

A failure during database recreation is deliberately **not** retried: the
database might already have been recreated or used. Inspect the database and
the journal's `database_reset_started` marker before recovery. Likewise, a
failed build is reported without an automatic rebuild, and an ambiguous worker
creation requires inspection of Worker Hosts before another request. Keep old
binaries off the new database. Do not delete the journal or create a new one
as a shortcut around these checks. The `qa-database-reset` ConfigMap in the QA
namespace identifies the owning run; only clear it after resolving that run.

Validation is hermetic (`python3 scripts/test_reset_qa.py`) and included in
local preflight and Rust Cloud CI. It covers target isolation, both PR gates,
interrupted submissions, partial database failure, resume behavior, worker
failure ordering, operator-org seeding, org creation through the platform API
(including a lost response), the Base code repository replacement guard,
external-issues bootstrap and the actual Helm argument construction. A real local Git fixture checks Base's origin, history, tracking
refs, repeat seeding and refusal to overwrite a moved Base head. It is not
evidence that a live QA reset has been rehearsed.
