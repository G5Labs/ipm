# QA Slack failure notifier

## Human request

While investigating failing QA deploys on 2026-10-14 ([G5-Lean#517](https://github.com/G5Labs/G5-Lean/issues/517)), the human asked to connect QA to the Slack `#deployment-notifications` channel and whether that needs a code change. After hearing how the retired dev environment did it, they chose:

- Failures only, the same as dev.
- Reuse dev's webhook, which already posts to that channel. Asked about `infra/qa/README.md`'s rule against copying values from an original G5 Secret Manager project, they chose to copy it as a documented exception.
- Apply the bootstrap-root prerequisites (`run.googleapis.com`, the runner's `roles/run.admin`) directly with gcloud under their owner access, with `bootstrap/main.tf` updated to match, because the bootstrap root's local state is not on this machine.

## Agent interpretation

"Same as dev" means dev's failure notifier and its coverage rule: every trigger that is not a pull-request check is watched, PR checks are not. Dev's other two instances (prod approval, canary verdict) have no QA counterpart. `EXPIRED` is added to dev's failure statuses because an expired build is a failure nobody started on purpose; `CANCELLED` stays out because superseded builds are cancelled deliberately.
