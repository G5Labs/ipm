---
name: authz-definer-column-grants
description: "authz_lookup_definer reads worker_hosts via COLUMN-level grants; a migration touching authz definer functions must GRANT any newly referenced columns or every caller fails with \"permission denied for table worker_hosts\"."
metadata:
  node_type: memory
  type: reference
  originSessionId: 14de906a-61ab-4849-9f30-7e0be2d24f4a
  modified: 2026-10-18T08:44:40.499Z
---

`authz_lookup_definer` (owner of the SECURITY DEFINER functions in the `authz`
schema) has only COLUMN-level SELECT/UPDATE grants on `public.worker_hosts`
(baseline 001 around line 9711; migration 046 added `slot_size_reported` the
same way). Referencing an ungranted column inside one of its functions makes
EVERY caller fail at runtime with `42501 permission denied for table
worker_hosts` — and callers like `reconcile_orphans_once` swallow that as a
warn and return an empty pass, so tests see "0 candidates" rather than an
error.

**How to apply:** when a migration edits an `authz.*` function to reference a
new `worker_hosts` column, add `GRANT SELECT (col, ...) ON public.worker_hosts
TO authz_lookup_definer;` (plus UPDATE for written columns) at the top of the
migration, outside the `SET ROLE authz_lookup_definer` block (follow migration
096). Related: [[rust-and-integration-test-setup]].
