# Migration 45 deployment recovery

## Human request

“please propose the fix” in response to the preceding proposal for a narrow migration-runner change, populated production-runner validation under a non-superuser role, and preservation of existing migration SQL and checksums.

The earlier task was to investigate why `g5-deploy-qa` failed after PR #269.

## Investigation context from this conversation

The preceding investigation confirmed that release `35f9ad1c459960a906dc83dbb2018c6b7c8c09dc` built its images successfully, but Helm's pre-upgrade migration hook failed. Migration 045 from PR #261 includes a transaction-scoped NO FORCE / UPDATE / DROP CONSTRAINT and columns / FORCE sequence. The shared preflight rejects its DROP statement and therefore retains the ordinary row-visibility check, which fails on QA. QA's `g5` migrator owns the ordinary, non-inherited table, has neither superuser nor BYPASSRLS, and sees zero rows while FORCE is enabled. The preceding investigation reproduced the guard classification using the exact release's source and SQL. The ledger remained at 042 after the failed attempts.

These are agent investigation findings, not additional human requirements. The proposal request authorized design only. The subsequent human invocation of `build-to-ship` authorized implementation of the agreed fix, validation, independent review, PR creation and gated merge/cleanup. The latest QA-specific acceptance below supersedes the earlier application/schema release concern.

The populated rollback/retry rehearsal exposed a second shared-runner recovery defect: SQLx 0.8.6 returns early on migration errors before its normal advisory unlock, and a pooled execution connection retains that session lock. The retry can borrow another connection and wait indefinitely. This is an agent investigation finding; disposable execution-session ownership is an agent decision needed to satisfy the already agreed failure/retry requirement.


## Human recovery-scope extension

The human answered “yes please be quick” to “The pending decision is whether to extend the existing recovery design to cover both blockers.” This authorizes extending this same design to cover the populated 047/048 failure and the incompatible deployed API/schema transition. The human also requested: “once goal is achieved, please merge the pr and watch for g5-deploy-qa, in case of failure, please investigate propose fix to me.” That conditional merge/monitor instruction remains subject to the acceptance and review gates; a deployment failure calls for an investigation and proposal.

## Human QA-specific acceptance and minimal implementation

The human explicitly accepts temporary incompatibility between the old API and migrated schema during QA rollout, including a failed rollout: QA is already down and has no active users. This removes concern #2 as a release blocker for this QA recovery only. The human forbids maintenance-window, staged compatibility-release and Helm rollback redesign, and requests the minimal runner fix for 045 and populated 047/048 failures, preserving original SQL/checksums and tenant isolation. Complete the existing build-to-ship review/CI gates, merge, monitor the matching g5-deploy-qa revision and verify upgraded API and host readiness. A subsequent deployment failure requires investigation and a proposal to the human.

Agent implementation decision: retain the bounded 045 classifier/session repair; wrap only the exact original 047/048 apply operations in an owning transaction that locks and qualifies worker_hosts, opens NO FORCE, delegates unchanged SQLx apply through its nested savepoint, restores FORCE and commits. SQLx's normal migration validation, original checksum and ledger remain authoritative; no release mechanisms or privileges change.
