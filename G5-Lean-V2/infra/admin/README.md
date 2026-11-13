# Shared Lean artifacts

`g5lean-admin-1ccc` is the approved shared artifact project for Lean QA, staging
and production. Project number: `543950241992`; organization: `231256058827`.
Use `us-central1` for regional artifact resources.

**Current state:** the project is ACTIVE. Read-only checks on 2026-10-21 verified
billing enabled on `billingAccounts/01A302-C6B3E7-EB2BBD`; [billing issue #702 is closed](https://github.com/G5Labs/G5-Lean/issues/702#issuecomment-6326320890).
It is empty of Lean artifact resources. The roots below are implemented as code
only: no infrastructure has been applied and no images or release records have
moved. Existing QA resources remain available during the transition.

| Root | Operator-owned resources |
| --- | --- |
| `bootstrap/` | Artifact Registry, Container Scanning/Analysis, Compute, IAM, Resource Manager, Service Usage and Storage APIs; protected versioned `g5lean-admin-1ccc-lean-tfstate` bucket |
| `artifacts/` | GAR repositories `g5`, `g5-tiers`, `g5-worker-images`; protected versioned `g5lean-admin-1ccc-lean-releases` and `g5lean-admin-1ccc-g5-worker-images` buckets; artifact IAM |

## Planned ownership

| Location | Responsibilities |
| --- | --- |
| **Admin project** | Shared Artifact Registry application/workspace images; worker-host Compute Engine images and their GCS disk archives; signed worker metadata; immutable release records |
| **QA project (`g5-qa-1ccc`)** | Cloud Build, GitHub connection, build execution, approval queues and release-train coordination |
| **Each target environment** | Its own Terraform state, database, file/object data, runtime service accounts and application secrets |

The worker-host boot image is a Compute Engine image. Its raw disk/import archive
lives in GCS; the corresponding OCI artifact holds signed metadata and a disk
pointer, not the disk itself. Application and workspace images are ordinary OCI
images in Artifact Registry.

## Confirmed access model

Trusted QA build identities publish the artifacts for their own build paths.
The central release account, `g5-lean-release@g5-qa-1ccc.iam.gserviceaccount.com`,
reads the required artifacts but cannot write artifacts or manage admin resources.
Runtime identities receive only required artifact reads/image-use access. PR CI
does not receive shared artifact publishing authority.

An existing authorized infrastructure operator runs both admin roots. They create
no privileged machine account and grant no publisher, runtime or central release
identity access to admin Terraform state. Keep bootstrap's local state securely
outside Git; `artifacts/` uses the admin bucket with prefix `artifacts`. Target
deployment state and native Terraform locks remain in staging/prod. See the
[IAM proposal](../../docs/production-pipeline-iam-review.md#runtime-and-shared-artifact-access).

All publisher identities below belong to QA (`g5-qa-1ccc`) and are fixed in code.

| Identity | Admin access |
| --- | --- |
| `g5-lean-deploy` | Write `g5` and `g5-tiers`; create/read `app-releases/` records; inspect/use worker GCE images |
| `g5-lean-terraform` | Publish Terraform OCI bundles to `g5`, following G5; no GCS source-bundle writes |
| `g5-lean-images` | Write worker OCI metadata; create/read `g5-worker-v…/` disk/archive objects; bucket metadata read; create/get/list/label GCE images and poll global operations |
| `g5-lean-release` | Read registries, project-wide Artifact Analysis occurrences, release/worker artifact buckets and use GCE images; no artifact publication or admin infrastructure authority |
| Explicit runtime identities | Selected registry reads, GCE image use and optional worker archive reads |

`runtime_registry_readers`, `runtime_image_users`, and
`runtime_worker_artifact_readers` default empty. Supply observed API and default
Compute service accounts from Lean QA, staging or prod platform outputs. Project
metadata lookups validate numeric Compute identities. API-created hosts need
image use on the API identity; GKE nodes and worker hosts need registry reads on
their default Compute identity. Archive access is optional when a runtime only
boots a GCE image. Public members, PR CI and arbitrary accounts are rejected.

GCS producers receive prefix-scoped Object Creator plus Object Viewer: exact
object uploads/reads, without bucket-wide listing, deletion or overwrite.
Publish with a generation-zero precondition and compare existing content on
retry. Worker names retain the `g5-worker-v` prefix and include a unique build
identifier. `latest.json`, ISO/build caches, temporary upload objects, signing
keys, VM baking and Cloud Build/GitHub execution remain in QA. No admin VM or
service-account attachment permission is granted. The GCE image is created
directly from its admin archive; no import VM is provisioned.

There are no cleanup policies or age-based deletion rules. Terraform protects
the repositories and buckets from destruction; versioning supports object
recovery. OCI consumers pin digests: registry writer access itself does not
enforce immutable tags. Retain existing QA artifacts while active consumers still
reference them; removing them requires a separate retention decision.

## Operator sequence

1. Review these roots and confirm the existing operator can enable APIs, manage
   repositories/buckets and their IAM, and create the narrow custom GCE role.
   QA publisher/release accounts must already exist. Do not run admin roots as
   the central release account or add them to QA Terraform promotion roots.
2. Initialize and review `bootstrap/` locally, then apply its reviewed plan as
   the operator. Preserve local bootstrap state in restricted recovery storage.
3. Copy `artifacts/backend.hcl.example` to `backend.hcl` and populate reviewed
   `terraform.tfvars` from the example. Initialize with
   `terraform init -backend-config=backend.hcl`; review and apply that root as the
   operator. Runtime readers can be populated after target platforms exist.
4. Verify producer/reader permissions before selecting admin artifacts in QA
   delivery: publication, identical-content retry, rejected overwrite,
   cross-prefix denial, central write/state-read denial, worker GCE image
   creation from its archive, and runtime pull/boot.
5. Exercise QA → staging → prod with identical immutable references. Keep QA's
   existing artifact resources, KMS, caches and build coordination intact.

Local verification uses `terraform init -backend=false`, `terraform validate`
and `terraform test`; all checked-in tests are plan-only with mocked Google
responses. These tests do not establish live IAM propagation or successful
cross-project publication. No DNS, hostname or customer-data migration is
performed by these roots.

Automatic image scanning applies to new Docker pushes and incurs provider charges.
See the [production CVE reporting activation steps](../prod/README.md#container-image-cve-reports);
the central release account receives metadata reads only, not scanner administration.
