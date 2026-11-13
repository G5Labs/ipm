# sweep-dev-cloud-resources.sh

Interactive sweeper for the **orphaned/stray cloud resources** the
[`reset-dev-environment.sh`](./reset-dev-environment.README.md) wipe leaves
behind. That reset deletes Postgres metadata only — by design it does **not**
tear down cloud-side artifacts and it **keeps** backups. This script finds those
leftovers and deletes them **one at a time, after you confirm each**.

> ⚠️ **Destructive, shared environment.** There is no blanket `--yes` — per-item
> confirmation is the whole point. It refuses a project whose id looks like prod
> (`ALLOW_PROD=1` to override) and targets DEV by default. Run `--dry-run` first.

---

## What it sweeps — "list everything", then classify

To make sure a leaked resource can **never be hidden by a naming convention**, the
sweep enumerates **ALL** resources in each category (no name filter) and then
**classifies** each one. Every item prints with its classification, and offered
items add an **estimated monthly cost**, before prompting `y/N`.

**Classification (for compute instances, MIGs, and networks):**

| Class | Rule | Action |
|-------|------|--------|
| **CONTROL PLANE** | GKE infra (label `goog-gke`/`goog-k8s` or name `^gke-`, extend via `CONTROL_PLANE_REGEX`); non-per-org networks (`default`, the shared VPC) | **PROTECTED** — always; never offered |
| **STANDING HOST** | a tenant's worker host: label `component=*worker-host*`, or name `g5-<tenant>-host-<ws>` | **PROTECTED** unless `SWEEP_PROTECTED=1` |
| **PER-ORG** | has a `g5_org_id` label, or lives in a `g5-org-<uuid>` network | orphan-checked vs the live `orgs` table: **live → PROTECTED**, **deleted → offered**, **unverified → PROTECTED** (could be live — `CHECK_DB=1` to confirm, or `SWEEP_UNVERIFIED=1` to offer) |
| **UNCLASSIFIED** | none of the above | **OFFERED** with a loud `[UNCLASSIFIED — verify]` label (so a real leak is never silently skipped) |

**Categories enumerated:**

| # | Category | Source |
|---|----------|--------|
| 1 | All **MIGs** | `compute instance-groups managed list` (classified; deleting a MIG removes its managed instances) |
| 2 | All **VM instances** | `compute instances list` (classified; per-org hosts also show **REGISTERED / NOT REGISTERED** from `worker_hosts`) |
| 3 | All **VPC networks** | `compute networks list`; deleting a per-org one **cascades** forwarding-rules → instances → firewalls → routers/NAT → subnets → network |
| 4 | **Cloud SQL backups** | `sql backups list` + per-backup `describe`. ⚠ restore points. **ON_DEMAND only** by default; `INCLUDE_AUTOMATED_BACKUPS=1` for the managed schedule. Cost from `maxChargeableBytes` |
| 5 | **Filestore backups** | `filestore backups list`. Cost from real `storageBytes` (used), **not** the 1 TiB share capacity |
| 6 | **GCS reset exports** | `gs://$BACKUP_BUCKET/dev-reset-*.sql.gz` when `BACKUP_BUCKET` is set. Cost from object size |

Backup/storage sizes are **actual** (queried per item); VM and Cloud-NAT figures
are order-of-magnitude rate estimates. All are shown only to inform the decision.

## Standing worker hosts (a protected class)

The **standing tenant worker hosts** — Terraform-provisioned, one MIG+VM per
tenant, named `g5-<tenant>-host-<workspace>` (e.g. `g5-worker-host-dev`,
`g5-tungsten-host-dev`) — are **not** orphans: each is a live worker plane (the
`g5-worker-host` one is what `reset-dev-environment.sh` re-credentials). Deleting
one stops that tenant's task execution until Terraform re-provisions it. They are
matched by the `component=*worker-host*` label or the `g5-<tenant>-host-<ws>` name
pattern (never the API-managed `g5-host-<uuid>` instances) and protected unless
`SWEEP_PROTECTED=1`.

`PROTECT_REGEX` (default `^g5-[a-z0-9]+-host-`) matches that standing-host name
pattern, so **every** tenant's host is **listed but not offered for deletion** —
and it deliberately does **not** match API-managed `g5-host-<uuid>` instances, so
those orphans remain sweepable. Pass `SWEEP_PROTECTED=1` to override (only if you
truly intend to tear a standing host down).

---

## Orphan annotation (optional, `CHECK_DB=1`)

By default, per-org networks and `g5-host-*` worker VMs are labelled
`[unverified]` and you judge orphan-ness from the shown metadata. With `CHECK_DB=1`
the script reads the live `orgs` and `worker_hosts` tables **once** (same
connection model as the reset script) and adds:

- **Orphan status** — each `g5-org-<uuid>` network and each worker VM (via its
  per-org network) is labelled `[ORPHAN]` (org no longer exists) or `[IN USE]`.
  **IN-USE networks and worker VMs are protected** — listed but never offered for
  deletion, so a live org's VPC/worker can't be torn down.
- **Org name** — the owning org's slug is shown (e.g. `live org 'zcl'`).
- **Registration** — each worker VM shows `REGISTERED (status=…, heartbeat …)` if
  it has a `worker_hosts` row, else `NOT REGISTERED`. A RUNNING-but-NOT-REGISTERED
  host in a deleted org is a classic leak (it can't serve anything).

`worker_hosts.org_id` is `ON DELETE CASCADE`, so its rows vanish when an org is
deleted — which is exactly why the network→org mapping (not a `worker_hosts`
lookup) is the authoritative orphan signal.

```bash
# Cloud SQL is private-IP only; the reliable path is an in-cluster psql pod:
DEV_DB_IP="$(gcloud sql instances describe g5-dev-postgres --project=g5-dev-1ccc \
  --format='value(ipAddresses[0].ipAddress)')"
CHECK_DB=1 USE_K8S_PSQL=1 DB_HOST="$DEV_DB_IP" \
  ./scripts/sweep-dev-cloud-resources.sh --dry-run
```

If the DB is unreachable the script logs a note and proceeds gcloud-only.

---

## Running it

```bash
cd .

# 1. Preview — list every candidate + cost, no prompts, no deletes:
./scripts/sweep-dev-cloud-resources.sh --dry-run

# 2. Interactive sweep — confirm each deletion:
./scripts/sweep-dev-cloud-resources.sh

# Limit to one or more categories:
./scripts/sweep-dev-cloud-resources.sh --only networks
./scripts/sweep-dev-cloud-resources.sh --only workers,backups
```

`just`-style invocation is not wired up (interactive); call the script directly.

### Flags & key env

| Flag / Env | Default | Effect |
|------------|---------|--------|
| `--dry-run`, `-n` | off | List candidates + cost; prompt/delete nothing |
| `--only <cats>` | all | Limit to `workers` / `networks` / `backups` (comma-sep, repeatable) |
| `CHECK_DB` | `0` | Annotate `g5-org-*` networks ORPHAN/IN-USE from the live `orgs` table |
| `INCLUDE_AUTOMATED_BACKUPS` | `0` | Also offer Cloud SQL AUTOMATED backups (normally Cloud-SQL-managed) |
| `SWEEP_PROTECTED` | `0` | Also offer standing tenant worker hosts (off by default) |
| `SWEEP_UNVERIFIED` | `0` | Offer per-org resources whose org couldn't be confirmed (default: protected) |
| `SWEEP_ALL_BACKUPS` | `0` | Offer all backups, not just reset-created ones |
| `CONTROL_PLANE_REGEX` | `^gke-` | Extra always-protected name prefixes (extend to protect more critical infra) |
| `BACKUP_BUCKET` | — | Also sweep `gs://<bucket>/dev-reset-*.sql.gz` exports |
| `GCP_PROJECT` | `g5-dev-1ccc` | Target (refuses `*prod*` unless `ALLOW_PROD=1`) |

Exit code is non-zero if any delete failed.

---

## Required IAM roles

Granted to the identity running the script on `g5-dev-1ccc`:

| Role | For |
|------|-----|
| `roles/compute.instanceAdmin.v1` | delete worker MIGs / VMs |
| `roles/compute.networkAdmin` | delete per-org networks, firewalls, routers, subnets |
| `roles/cloudsql.editor` | `sql backups delete` |
| `roles/file.editor` | `filestore backups delete` |
| `roles/storage.objectAdmin` | delete GCS exports (only if `BACKUP_BUCKET` set) |
| `roles/secretmanager.secretAccessor` + GKE access | only for `CHECK_DB=1` (read `g5-db-password`, in-cluster psql pod) |

---

## Notes

- **MIG-then-VM order matters.** MIGs are swept before standalone VMs; a VM still
  managed by a MIG you chose to keep will just be recreated — delete the MIG.
- **Network cascade.** Confirming a `g5-org-*` network deletes its dependents in
  dependency order. A dependent that fails (e.g. an in-use address) is logged and
  the network delete will then fail too — rerun after clearing it.
- **Backups are restore points.** Each Cloud SQL backup prompt warns explicitly.
  Keep at least the most recent reset backup until you're sure dev is healthy.
