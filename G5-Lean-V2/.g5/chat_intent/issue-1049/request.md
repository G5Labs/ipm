# Managed disk allocation

The owner requested implementation for G5-Lean#1049: organization Admin settings H=120 GiB and V=80 GiB are authoritative; each managed host requires at least H+V×slots, accepts larger explicit overrides, rejects unsupported configurations and keeps an immutable creation-time H/V/slots/resolved-size snapshot. Retries reuse it, replacements use current settings, and existing running VMs retain capacity and custody.

Each private VM disk has recorded V capacity; admission is free bytes >= V. Remove deployment-global selection and additional cold-preparation, archive or bootstrap admission margins. Charge all shared data to H, coordinate concurrent writers with real physical reservations, reclaim only unused data, and retain custody for active readers and unproven processes. Optional seeds must not obstruct ordinary startup. Extend trusted guest Git restore to ordinary boots, preserving exact source/dirty receipts, references, personal mounts, permissions and readiness ordering.

Capacity failures retain phase and available/required bytes, preserve Start intent and retry only after safe cleanup; replies and durable events must agree. Invalid images, source corruption and authorization failures remain distinguishable. Validate the QA cold-image incident (approximately 156.8 GiB free versus an erroneous 160 GiB check), settings/minima/overrides, retry edits, old hosts, boundary admission, warm/cold and seed variants, concurrent starts/growth, pressure/cancellation/cleanup/restart/stale events and real Linux guest restore. Report implementation, tests, deployment and observations separately. Implementation is authorized; no deployment or parent merge was requested.

Mechanisms and qualification limits are agent decisions recorded in the [plan](../../pr_ontology_diff_design/issue-1049/ontology-plan.md).

The owner's [2026-11-04 sparse disk correction](sparse-disk-correction.md) supersedes the physical reservation and shared-H requirements above.
