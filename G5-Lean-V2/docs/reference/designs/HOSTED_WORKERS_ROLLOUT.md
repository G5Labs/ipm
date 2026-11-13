# Hosted Workers Rollout

> **SUPERSEDED (2026-06-09, human decision, PR #367).** The shared
> worker-cluster model this rollout built (Phase 1) and the manual
> tenant-slice path (Phase 2) are retired: the dedicated `g5-{env}-workers`
> GKE clusters were deleted permanently and `provision-tenant-worker-slice.sh`
> is disabled. Hosted worker capacity is bare `g5-worker` GCE hosts — the
> per-environment worker-host MIG (terraform `enable_worker_host_instance`)
> and the Phase-3 API-provisioned per-org hosts (`enable_api_managed_worker_hosts`,
> live in dev), which is the surviving direction of this document. The
> Tungsten worker clusters were retired too (PR #363 validated the DinD
> compliance suite inside libkrun microVMs); the Tungsten org runs on a
> dedicated bare worker host MIG. Current
> operational invariants: `generation/.semlift/specs/g5/infra.semlift.md`
> block D17. The phase descriptions below are kept as historical record.

## Summary

This document turns the hosted-worker rollout into three concrete phases:

1. separate shared worker clusters for `g5labs` dev, staging, and prod
2. manually provisioned tenant-isolated worker slices on the shared worker cluster
3. app-driven dynamic provisioning for hosted worker slices

Phase 1 and phase 2 are executed in the repo. Phase 3 is captured in
`HOSTED_WORKER_DYNAMIC_PROVISIONING.md`.

## Phase 1: Shared `g5labs` Worker Clusters

### Outcome

Each environment now has a repo path for:

- one control-plane cluster for `g5-api`, edge, migrations, and indexer
- one shared worker cluster for hosted `g5d-worker` pods
- one control-plane Helm release
- one worker-only Helm release

This keeps worker execution off the API cluster while preserving a shared
cluster model for hosted workers.

### Repo changes

- Terraform can now provision a dedicated worker cluster per env via
  `worker_cluster_name`, `worker_cluster_pod_cidr`, and
  `worker_cluster_service_cidr`
- the worker subnet can now carry separate pod/service secondary ranges for the
  worker cluster
- External Secrets is now split per cluster:
  - control-plane cluster gets `g5-api-secrets`
  - worker cluster gets `g5d-worker-secrets`
- the worker secret can now point at the public environment URL instead of the
  in-cluster `http://g5-api:7500` address
- the Helm chart now supports three release topologies:
  - `full`
  - `control-plane`
  - `worker`
- Cloud Build can now deploy the control-plane release to one cluster and the
  worker release to another cluster
- bootstrap and post-deploy flows now refresh worker secrets on the worker
  cluster when present
- Workload Identity annotations now resolve to the Terraform-created
  `clusterName-api` and `clusterName-worker` GCP service accounts

### Environment settings

The repo now describes:

- `dev`
  - control-plane cluster: `g5-dev`
  - worker cluster: `g5-dev-workers`
- `staging`
  - control-plane cluster: `g5-staging`
  - worker cluster: `g5-staging-workers`
- `prod`
  - control-plane cluster: `g5-prod`
  - worker cluster: `g5-prod-workers`

Worker replica defaults for the worker cluster release are:

- `dev`: 1
- `staging`: 1
- `prod`: 2

The control-plane release forces `worker.replicaCount=0`, so the API cluster no
longer hosts worker pods once the split-cluster path is applied.

### Bring-up

Apply infra and run the normal cloud flow for each environment:

```bash
cd /Users/lumost/workspace/g5/generation
terraform -chdir=infra workspace select dev
terraform -chdir=infra apply -var-file=environments/dev.tfvars
./scripts/gcp-bootstrap.sh post-deploy dev
```

Repeat for `staging` and `prod` with their env tfvars once `dev` is healthy.

That flow now:

- deploys the control-plane Helm release to the control-plane cluster
- deploys the worker Helm release to the worker cluster
- seeds the default org bootstrap with `seed-prod-org.sh`
- refreshes `g5-api-secrets` on the control-plane cluster
- refreshes `g5d-worker-secrets` on the worker cluster

### Verify

```bash
gcloud container clusters get-credentials g5-dev --region us-central1 --project g5-dev-1ccc
kubectl get pods -n g5 -l app=g5-api

gcloud container clusters get-credentials g5-dev-workers --region us-central1 --project g5-dev-1ccc
kubectl get pods -n g5 -l app=g5d-worker
kubectl rollout status deployment/g5d-worker -n g5
kubectl get secret g5d-worker-secrets -n g5 -o jsonpath='{.data.G5D_API_URL}' | base64 -d
```

Success means:

- the API runs only on the control-plane cluster
- workers run only on the worker cluster
- the worker secret points at the environment URL
- the default `g5labs` worker registers and can execute work end to end

## Phase 2: Tenant-Isolated Worker Slices

### Outcome

The shared worker cluster can now provision one manual worker slice per org
with:

- dedicated namespace
- dedicated worker secret
- dedicated worker ServiceAccount
- default-deny network policy
- public-internet egress allowed by default while private/internal CIDRs stay blocked
- no shared worker GCP service account
- local-only cache to avoid the shared GCS worker identity

### Entry point

Use the worker cluster for the target environment. Example for `dev`:

```bash
cd /Users/lumost/workspace/g5/generation
gcloud container clusters get-credentials g5-dev-workers --region us-central1 --project g5-dev-1ccc
TENANT_OPENAI_API_KEY=... \
./scripts/provision-tenant-worker-slice.sh \
  --env dev \
  --org-name "Acme" \
  --org-slug acme
```

### What the script does

1. Seeds the org bootstrap and a dedicated worker token using `seed-prod-org.sh`
   with org-specific secret IDs.
2. Creates a dedicated namespace for that org on the shared worker cluster.
3. Creates a dedicated `g5d-worker-secrets` K8s secret in that namespace.
4. Deploys a dedicated worker ServiceAccount and worker Deployment.
5. Applies default-deny ingress/egress with explicit allow rules for DNS,
   public internet egress, and optional approved private CIDRs.
6. Restarts and verifies the isolated worker deployment.

### Current limits

- worker slices still share the same worker cluster and worker node pool
- public internet egress is allowed; only private/internal ranges are blocked
- cache is local-only; no tenant-scoped remote cache yet
- provisioning is operator-run, not app-driven
- separate tenant VPCs are not part of phase 2; they remain a future stronger
  isolation tier

These are acceptable for the dev validation phase and are intentionally the
bridge to phase 3.

### Verify

```bash
kubectl get ns g5-tenant-acme
kubectl get pods -n g5-tenant-acme -l app=g5d-worker
kubectl get networkpolicy -n g5-tenant-acme
kubectl get secret g5d-worker-secrets -n g5-tenant-acme
```

Success means:

- the org has its own worker namespace
- the worker does not use the shared `g5` namespace secret
- the worker can reach the public internet, but it cannot freely talk to
  cluster or VPC private ranges outside the explicit allow rules

## Phase 3: Dynamic Provisioning

The design for app-driven hosted worker provisioning lives in:

- `generation/designs/HOSTED_WORKER_DYNAMIC_PROVISIONING.md`

That phase takes the phase 2 resource shape and moves ownership into g5-api via
an org-scoped hosted-worker control plane and reconciler.
