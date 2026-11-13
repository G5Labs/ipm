# Hosted Worker Dynamic Provisioning

## Summary

Phase 3 moves hosted worker provisioning from operator-run scripts into the
application control plane. The target is: an org admin enables hosted workers
from the app, and g5-api reconciles an isolated worker slice for that org onto
the environment's managed worker cluster without manual `gcloud` or `kubectl`
steps.

This design builds on the manual tenant slice introduced in phase 2:

- one namespace per org
- one worker deployment per org
- one worker bootstrap secret per org
- default-deny network policy per org
- no shared worker K8s secret across orgs
- no shared worker GCP service account in the baseline path

## Goals

- Provision hosted workers per org from the app.
- Keep the phase 2 isolation guarantees by default.
- Allow operators to scale from one shared dev pool to many tenant slices.
- Make provisioning idempotent and repairable through reconciliation.
- Keep the runtime contract compatible with the current `g5d worker` binary.
- Separate customer worker execution from the API/control-plane cluster by
  default.

## Non-Goals

- Full per-customer project or cluster creation in v1.
- Automatic per-org node pool creation in the first implementation.
- A new worker execution runtime.
- Replacing the existing worker token model.

## Baseline Isolation Model

The first dynamic version should preserve the manual phase 2 shape:

- central API/control-plane cluster per environment
- separate shared worker cluster per environment
- namespace per org
- deployment per org
- service account per org
- secret per org
- default-deny ingress
- public-internet egress allowed, but private/internal CIDRs blocked by default
- local-only worker cache
- no shared GCS worker identity

This keeps the first app-driven version simple and avoids reintroducing the
shared worker service-account problem while the control plane matures.

## Cluster Topology

The default hosted topology should be:

- one control-plane cluster for `g5-api`, edge, and platform services
- one managed worker cluster for hosted worker slices in the same environment

Why this should be the default:

- a worker compromise is less likely to share fate with the API/data plane
- worker nodes can keep Ubuntu/KVM/Kata requirements without shaping the API
  cluster
- worker upgrades, autoscaling, and egress policy can move independently of
  the customer-facing services

This is still a shared worker cluster, not a per-tenant cluster. Dedicated VPC
and dedicated cluster isolation remain optional stronger tiers.

## Resource Model

Add an org-scoped control-plane record, for example `hosted_worker_pools`:

```sql
CREATE TABLE hosted_worker_pools (
    id UUID PRIMARY KEY,
    org_id UUID NOT NULL REFERENCES orgs(id) UNIQUE,
    mode TEXT NOT NULL,                  -- "disabled" | "manual" | "managed"
    status TEXT NOT NULL,                -- "pending" | "provisioning" | "ready" | "degraded" | "error"
    namespace TEXT NOT NULL,
    cluster_ref TEXT NOT NULL,
    worker_service_account TEXT NOT NULL,
    k8s_secret_name TEXT NOT NULL,
    network_tier TEXT NOT NULL DEFAULT 'shared_vpc', -- "shared_vpc" | "dedicated_vpc" | "dedicated_project"
    placement_selector JSONB NOT NULL DEFAULT '{}'::jsonb,
    worker_token_id UUID,
    deployment_name TEXT NOT NULL DEFAULT 'g5d-worker',
    image_ref TEXT,
    desired_replicas INTEGER NOT NULL DEFAULT 1,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

Optional follow-on tables:

- `hosted_worker_secret_refs`
- `hosted_worker_events`
- `hosted_worker_scale_history`

## API Surface

Add org-admin APIs:

- `POST /api/v1/orgs/:orgId/hosted-workers/enable`
- `GET /api/v1/orgs/:orgId/hosted-workers`
- `POST /api/v1/orgs/:orgId/hosted-workers/disable`
- `POST /api/v1/orgs/:orgId/hosted-workers/scale`
- `POST /api/v1/orgs/:orgId/hosted-workers/rotate-token`

Minimal enable request:

```json
{
  "replicas": 1,
  "imageRef": "optional",
  "llmSecretMode": "org_secret_refs"
}
```

The first version should keep the API narrow. Complicated placement settings
can remain operator-only until the reconciler is stable.

## Control Plane Components

### 1. HostedWorkerService

An application service in g5-api that:

- validates org-admin authorization
- creates or rotates an org-scoped worker token
- persists the desired worker-pool state
- enqueues reconciliation work

### 2. HostedWorkerReconciler

A background reconciler in g5-api that:

- reads the desired org worker-pool state
- chooses the target worker cluster backend
- creates or repairs the namespace-scoped resources
- waits for worker registration and readiness
- updates status and errors in the database

### 3. Kubernetes Adapter

A small adapter layer that applies and reads:

- Namespace
- ServiceAccount
- Secret
- Deployment
- NetworkPolicy

It should be cluster-aware so the reconciler can target the shared worker
cluster now and stronger isolation backends later.

The first version can use direct Kubernetes API calls or generated manifests.
It should not shell out to `kubectl`.

### 4. Secret Backend Adapter

The reconciler should read org-scoped secret references from the application
store and materialize the runtime secret for the worker namespace.

V1 options:

- K8s Secret materialized from org-managed secret refs
- Secret Manager-backed sync if org secrets already live there

The adapter must preserve "no shared worker secret across orgs."

## Provisioning Flow

1. Org admin enables hosted workers.
2. g5-api validates org ownership and admin scope.
3. g5-api creates or refreshes an org-scoped worker token.
4. g5-api writes the desired `hosted_worker_pools` row.
5. Reconciler selects the target worker cluster.
6. Reconciler creates the org namespace if missing.
7. Reconciler materializes the namespace-scoped worker secret.
8. Reconciler creates the org ServiceAccount and default-deny NetworkPolicies.
9. Reconciler creates or updates the worker Deployment.
10. Worker pod starts, registers with g5-api, and reports healthy.
11. Reconciler marks the pool `ready`.

## Scheduling and Routing

Task dispatch needs one extra filter: hosted org tasks should prefer workers
registered for that org's hosted pool.

Recommended routing order:

1. matching org-hosted worker
2. matching user-local worker
3. matching shared fallback pool, if explicitly enabled

For the hosted SaaS path, org-hosted workers should become the default.

## Security Model

### Namespace Isolation

Each org gets its own namespace. No worker pods for org A should run in org B's
namespace.

### Secret Isolation

Each org gets its own worker secret. The reconciler should never reuse the same
K8s secret name across org namespaces with shared contents.

### Network Isolation

Each org namespace gets:

- default deny ingress/egress
- DNS egress
- g5-api egress
- public internet egress
- private/internal CIDRs blocked unless explicitly approved

Future tightening:

- service mesh authorization policy per org namespace
- egress gateway for LLM traffic
- per-org node pools for stronger placement isolation

## Isolation Tiers

### Shared VPC

This is the phase 2 baseline:

- shared worker cluster
- shared VPC
- namespace, secret, service-account, and NetworkPolicy isolation

This is acceptable for dev and early hosted bring-up, but it is not the final
answer for stronger customer isolation.

### Dedicated VPC

This remains a stronger isolation add-on for customers that need it, but it is
not the default rollout path.

- one tenant worker backend per VPC
- separate cluster per tenant backend, because a single GKE cluster cannot span
  multiple VPCs
- public internet egress still allowed, but internal service reachability is
  limited to that tenant backend

Recommended rollout posture:

- keep `shared_vpc` as the default managed backend
- add `dedicated_vpc` as an upgrade path for customers with stricter network
  isolation requirements

### Dedicated Project

Use this for the highest-assurance tenants:

- separate GCP project
- separate VPC
- separate cluster
- separate secrets, billing attribution, and audit surface

### Identity

V1 dynamic provisioning should not depend on the shared worker GCP service
account. If remote cache returns later, add an org-scoped GSA with scoped IAM.

### No Unsafe Fallback

Hosted multi-tenant worker pools must not silently fall back to process mode.
If Kata or KVM is unavailable, the pool should stay degraded and reject work.

## Observability

Track at least:

- pool status transitions
- reconcile duration
- reconcile failures
- worker registration latency
- ready worker count
- task dispatch misses caused by missing hosted capacity

Expose the latest error and the last successful reconcile timestamp in the org
admin UI.

## Migration Plan

### Step 1

Keep phase 2 as the operator-run fallback path.

### Step 2

Introduce `hosted_worker_pools` and read-only status endpoints.

### Step 3

Implement reconciler-backed create/update flows in dev only.

### Step 4

Switch org seeding for new hosted tenants from the manual phase 2 script to the
new API flow.

### Step 5

Optionally add org-scoped GCS/Secret Manager identities if remote cache or
external secret sync is needed for scale.

### Step 6

Add a second provisioning backend for `dedicated_vpc` that creates a tenant
cluster in a tenant-owned VPC and schedules the org worker pool there when the
customer is on the stronger isolation tier.

## Open Questions

- Should org-hosted pools own their own LLM credentials, or should hosted
  billing support platform-provided models as a separate mode?
- Do we want one deployment per org or a small fixed pool per org?
- When do we introduce per-org node pools versus shared worker nodes with Kata?
- Should the reconciler live in g5-api, or in a dedicated controller process?
