# PR C deployment decision

On 2026-10-18 the owner confirmed that PR B (#630) was merged and requested
monitoring its deployment while starting PR C. The owner explicitly ruled out
reverting PRs or rolling back deployments. If host/worker and API compatibility
requires it, the fleet can be manually reprovisioned.

This supersedes the rollback requirement in the earlier retirement plan. The
implementation interpretation is to remove the retired storage and compatibility
hooks in C, while preserving substantive document, task and discussion data and
the guard against unexpected node-comment history. It does not authorize this
session to reprovision hosts merely as a precaution.

Code inspection found that ordinary QA deployments use Helm `--atomic`, which
can restore the previous API image after a failed rollout even though the
pre-upgrade migration has committed. Applying the owner's forward-only decision
there means retaining readiness waits and failure reporting while removing
automatic Helm rollback from the QA delivery pipeline.
