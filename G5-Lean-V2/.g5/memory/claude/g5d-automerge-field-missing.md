---
name: g5d-automerge-field-missing
description: Guest g5d (2026-10-27) rejects `--json autoMergeRequest`; merge-when-ready state is not readable from the VM, so infer from state/localMergeStatus/blockingMessage.
metadata:
  type: reference
---

On 2026-10-27 both `/opt/g5/bin/g5d` and `~/.g5/bin/g5d` refused `g5d pr view <id> --json autoMergeRequest`
("Unknown JSON field"), although AGENTS.md and the merge-when-ready loop prompt name it. Read
`state,localMergeStatus,blockingMessage,targetHeads,sourceHeads` instead and treat merge when ready as
still on unless a person says otherwise; re-try the field on newer guests. See [[guest-g5d-lags-api-visibility]].
