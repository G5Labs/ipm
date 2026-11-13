# Filestore Zonal migration

## User decisions

On 2026-11-07, Kaihui confirmed Zonal Filestore with custom performance set to
4,000 IOPS and backups, and requested that migration begin. The ongoing scope is
Lean QA, staging and production. The existing backup policy is daily native
backups with 30-day retention. Do not merge on the user's behalf. Call out impacts
to QA; DNS changes are outside this work.

Kaihui subsequently directed: "Just prepare, do not migrate yet." That preparation delivery
was code, tests, review and runbook only. Keep provisioning disabled and legacy
storage selected in the committed map. That preparation step did not authorize new instances, data copies, cutovers,
service interruptions or merges.

Kaihui then approved: "I approve using zonal + 4000iops custom performance for all
three envs for g5 lean pipeline, and enable backup." After being told the next
step was QA provisioning and the initial copy, Kaihui merged preparation PR #1166.
Proceed with the reviewed rollout, validating QA before later cutovers. Kaihui requested the QA maintenance window "as soon as possible". Proceed after
the copy and safety checks pass. The no-agent-merge rule remains.

Kaihui clarified: "I mean you need this pr to enable, but all into one pr,
no more prs after". Existing PR #1167 must contain the final enabled Zonal
configuration for QA, staging and prod; no additional migration PRs. The agent
must not merge. Hold automatic apply/deploy/promotion paths before the owner
merges the final configuration, then carry out the controlled runtime migration.

## Agent interpretation

Preserve the current 1,024 GiB capacity, region, NFSv3 protocol and share name.
Provision separate Zonal instances, keep the existing HDD instances protected,
and migrate one environment at a time, starting with QA. Migration includes the
whole share, including attachment bytes. Fixed IOPS is the purchased combined
performance budget, not independent read and write limits.

The target configuration and an ASAP QA write-pause window are approved.
Earlier plans are historical evidence and must be regenerated before execution.
No unrelated IAM/cache rollout or application/schema upgrade is included.
