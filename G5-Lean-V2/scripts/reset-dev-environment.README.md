# reset-dev-environment.sh

Wipe and regenerate the **cloud dev environment** (`dev.g5labs.ai`) to a clean
slate: back everything up, `TRUNCATE` all tenant data, re-run the platform seed
(which provisions only the operator org), then — after the operator signs in
again — recreate a single empty `internal-g5-devs` org through the operator API
and re-invite today's members.

It runs in **two steps** because the wipe also removes the operator org and the
calling operator's membership: the default run wipes and seeds, then stops;
`--regenerate` creates the org once the operator has signed in again.

> ⚠️ **Destructive, shared environment.** It refuses to run against a project
> whose id looks like prod (`ALLOW_PROD=1` to override) and requires you to type
> the instance name to confirm (unless `--yes`). Always run `--dry-run` first.

---

## What it does (in order)

**Step 1 — wipe (default):**

1. **Backup (fail-closed)** — Cloud SQL on-demand backup (`gcloud sql backups
   create`), a **Filestore backup of the git repos** (`gcloud filestore backups
   create` — repos live on Filestore, not Postgres), an optional GCS SQL export
   (`BACKUP_BUCKET`), and a local TSV snapshot of orgs + members/invitations
   (the platform seed's service user is excluded).
   If any backup fails, the script aborts before touching anything.
2. **Deallocate orphaned worker VMs** — terminate non-terminated `worker_hosts`
   GCE instances before the wipe (best-effort) so they aren't left running with
   no DB record.
3. **Wipe** — `TRUNCATE orgs … CASCADE` (as the admin `g5` role) + clear
   `authz.user_sessions` / `session_ctx`. Global `users` / `oauth_scopes` are
   kept so the same people can be re-invited. This includes the operator org.
4. **Platform seed** — runs `scripts/seed-prod-org.sh` against the same
   database. It provisions only the operator org (`operator`, with its docs repo
   and Base) and gives `DEFAULT_ADMIN_EMAIL` — by default the `G5D_TOKEN` user —
   a pending operator admin invitation.
5. **Restart API processes** — `kubectl rollout restart` restarts `g5-api` and `g5-daemon` after the tenant data wipe so no process retains stale tenant caches or subscriptions.
6. **Stop** — prints the step 2 command with the snapshot path.

**Step 2 — `--regenerate --members-file=<snapshot>`**, after signing in at
`dev.g5labs.ai` as the invited operator (sign-in accepts the invitation):

7. **Create org** — `POST /api/v1/platform/orgs` (the operator path behind
   `/operator/home`) with the `G5D_TOKEN` user as owner, which also initializes
   the docs repo working tree server-side. An org with the same slug that an
   earlier attempt created is adopted instead of creating a second one.
8. **Re-invite** — `POST /api/v1/orgs/:id/members` for each captured person.

Step 2 needs only `curl`, `jq`, `openssl` and the API; no database access.
Other platform operators are invited again from the operator org's Members page.

The reset no longer re-credentials the retained bare-host path: the platform seed
mints no worker registration token and writes no `g5-org-id` /
`g5-worker-registration-token` secret, because nothing consumes them (cloud
hosts use per-host JWTs).

Postgres and the Filestore Git repositories are both backed up; nothing else holds state (Redis was removed on 2026-10-09, G5Labs/G5-Lean#405).

---

## Prerequisites

**Tools:** `gcloud`, `kubectl`, `psql`, `curl`, `jq`, `openssl`
(`cloud-sql-proxy` only if you use `--proxy`).

**Cluster access:** `kubectl` pointed at the dev cluster —
`gcloud container clusters get-credentials g5-dev --region us-central1 --project g5-dev-1ccc`.

**Operator API token (`G5D_TOKEN`):**

- Step 1 uses it only to resolve the operator email to invite
  (`GET /api/v1/users/me`); set `DEFAULT_ADMIN_EMAIL` instead to skip it. It is
  validated before anything destructive.
- Step 2 needs a **fresh** session of that operator, taken after signing in
  again (the wipe removed the old memberships). It must **carry the `org:admin`
  scope** — `POST /orgs/:id/members` is gated on it — which an operator admin
  session does. The script preflights this (decodes the JWT scopes) before
  creating the org, and the new org is owned by this token's user. Mint one from
  the dev refresh token:

```bash
export G5D_TOKEN="$(curl -s -X POST https://dev.g5labs.ai/api/v1/auth/refresh \
  -H 'Content-Type: application/json' \
  -d "{\"refreshToken\": \"$(tr -d '[:space:]' < /tmp/g5_refresh.dev)\"}" \
  | jq -r '.data.jwt')"
```

The JWT lasts ~1h (re-run if it expires).

---

## Required IAM roles

Granted to the **user identity running the script** (`gcloud config get-value
account`) on project `g5-dev-1ccc`. Granting these requires
`resourcemanager.projects.setIamPolicy` (Owner / IAM Admin).

| Role | Needed for |
|------|-----------|
| `roles/cloudsql.editor` | Cloud SQL backup (`backupRuns.create`) — and restore |
| `roles/file.editor` | Filestore repos backup (`file.backups.create`) |
| `roles/secretmanager.secretAccessor` | read `g5-db-password` |
| `roles/compute.instanceAdmin.v1` | terminate orphaned worker VMs |
| GKE access (e.g. `roles/container.developer`) | in-cluster `psql` pod + `kubectl rollout restart g5-api` |

```bash
P=g5-dev-1ccc; M="user:$(gcloud config get-value account)"
for R in roles/cloudsql.editor roles/file.editor roles/secretmanager.secretAccessor roles/compute.instanceAdmin.v1; do
  gcloud projects add-iam-policy-binding "$P" --member="$M" --role="$R"
done
```

The DB connection itself authenticates as the Postgres admin user `g5` (password
fetched from Secret Manager `g5-db-password`), not your GCP identity.

---

## Running it

```bash
cd .

# 1. Preview — touches nothing, needs no creds/tools
./scripts/reset-dev-environment.sh --dry-run

# 2. Step 1, live. Cloud SQL is private-IP only, so the reliable path is an
#    in-cluster psql pod (runs inside the VPC). It also lets the platform seed
#    initialise the operator docs and code mirrors and image policy through kubectl:
DEV_DB_IP="$(gcloud sql instances describe g5-dev-postgres --project=g5-dev-1ccc \
  --format='value(ipAddresses[0].ipAddress)')"
G5D_TOKEN="$G5D_TOKEN" USE_K8S_PSQL=1 DB_HOST="$DEV_DB_IP" \
  ./scripts/reset-dev-environment.sh

# Alternative DB path (needs VPC/IAP reachability to the private IP); the seed
# then warns that it skipped the repository mirrors and image policy:
DEFAULT_ADMIN_EMAIL=you@g5labs.ai ./scripts/reset-dev-environment.sh --proxy

# 3. Sign in at https://dev.g5labs.ai as the invited operator, mint a fresh
#    G5D_TOKEN (see above), then step 2 with the snapshot path step 1 printed:
G5D_TOKEN="$G5D_TOKEN" ./scripts/reset-dev-environment.sh --regenerate \
  --members-file=/tmp/g5-dev-reset-backups/members-g5-dev-1ccc-<ts>.tsv
```

Run during a **quiet window** with no active workspace VM or editing session so a live process cannot write tenant state between the `TRUNCATE` and the API restart.

`just reset-dev-environment <args>` is equivalent.

### Flags

| Flag | Effect |
|------|--------|
| `--dry-run`, `-n` | Print every step (no gcloud/psql/kubectl/curl needed) |
| `--proxy` | Auto-start `cloud-sql-proxy` for the DB connection |
| `--yes`, `-y` | Skip the typed instance-name confirmation |
| `--regenerate` | Step 2: create the org through the operator API and re-invite |
| `--members-file=PATH` | Step 2: the member snapshot step 1 wrote |
| `-h`, `--help` | Show the header docs |

### Key env vars

| Var | Default | Purpose |
|-----|---------|---------|
| `G5D_TOKEN` | — | Step 1: resolves `DEFAULT_ADMIN_EMAIL`; step 2: **required** fresh operator session JWT |
| `DEFAULT_ADMIN_EMAIL` | the `G5D_TOKEN` user | Operator invited by the platform seed after the wipe |
| `K8S_CONTEXT` | current kubectl context | GKE context the seed pins its kubectl calls to (`USE_K8S_PSQL=1`) |
| `SEED_SCRIPT` | `scripts/seed-prod-org.sh` | Platform seed to run after the wipe |
| `USE_K8S_PSQL` / `DB_HOST` | `0` / `127.0.0.1` | In-cluster psql pod against the Cloud SQL private IP |
| `GCP_PROJECT` | `g5-dev-1ccc` | Target project (refuses `*prod*` unless `ALLOW_PROD=1`) |
| `DB_INSTANCE_NAME` / `DB_NAME` / `DB_USER` | `g5-dev-postgres` / `g5_dev` / `g5` | Cloud SQL target |
| `NEW_ORG_NAME` / `NEW_ORG_SLUG` | `internal-g5-devs` | Org to leave behind |
| `BACKUP_REPOS` | `1` | Filestore repos backup (set `0` to skip) |
| `BACKUP_BUCKET` | — | Also export a full SQL dump to this GCS bucket |
| `RESTART_API` | `1` | Restart g5-api and the daemon after the wipe (set `0` to skip) |
| `ALLOW_PROD` | `0` | Override the prod-project guard |

---

## Backups & restore

Backups are **kept** (not auto-deleted).

```bash
# Cloud SQL
gcloud sql backups list    --instance=g5-dev-postgres --project=g5-dev-1ccc
gcloud sql backups restore <BACKUP_ID> --restore-instance=g5-dev-postgres --project=g5-dev-1ccc

# Filestore (git repos)  — id printed in the run summary: dev-repos-pre-reset-<ts>
gcloud filestore backups list   --project=g5-dev-1ccc --region=us-central1
gcloud filestore backups delete <BACKUP_ID> --project=g5-dev-1ccc --region=us-central1
```

The local org/member snapshot (the re-invite list) is written under `BACKUP_DIR`
(default `${TMPDIR}/g5-dev-reset-backups`).

---

## Notes & known limitations

- **The wipe deletes Postgres rows only.** Cloud-side artifacts keyed by the
  now-deleted ids become orphaned on disk/in GCP and are NOT torn down by the
  script (they are why the backups exist). By design, clean these up separately
  if storage/cost matters:
  - **Filestore git repo dirs** (keyed by `repos.id`) and **issue-attachment
    bytes** (under `{mount}/{tenant_prefix}/.attachments`) — both live on the
    Filestore share and are covered by the `gcloud filestore backups create`
    backup before the wipe.
  - **Per-org GCP networks** (`g5-org-{org_id}` + firewalls/subnet/router/NAT)
    that `ensure_vpc_for_org` created for orgs using API-managed worker hosts.
  - **API-managed worker VMs whose `worker_hosts.cloud_instance_id` is still
    `NULL`** (a transient provisioning window): step 2 only reaps rows with a
    persisted id, so an in-flight instance created moments before the wipe can
    leak. Sweep the project for stray `g5-worker-*` instances after a reset if
    one was provisioning during the run.
- If the platform seed fails after the wipe, dev has no operator org; re-run
  the printed seed command (it is idempotent) before step 2.
- API-managed worker hosts self-provision against the new org; nothing to do for
  those beyond the orphaned-VM cleanup in step 2.
