---
name: e2e-testid-sweep-after-rebase
description: "After rebasing E2E specs onto main, sweep every getByTestId against frontend/src; a stale trigger id fails silently as a 60 s click timeout 3/3 and the provider-agent step fails by receipt. g5-ci-e2e runs ~70 min, longer than one 58 min watch."
metadata:
  node_type: memory
  type: feedback
  originSessionId: 9c3e00bf-a56c-40fc-9ad2-961e5fc504f9
  modified: 2026-10-27T03:22:22.717Z
---

A rebase can leave an E2E spec clicking a testid main no longer renders (PR #846:
`workspace-status-band-preview` from the #693-era spec; main's trigger is the composed
`ribbon-preview-preview`, reached through `openRibbonPreview`). Git reports no conflict;
CI shows `locator.click: Timeout 60000ms` on all three retries, and
`provider-agent-playwright-e2e` then fails too, because its receipt gate rejects the shard
whose spec never passed cleanly.

**Why:** the only signal is a slow CI failure about 70 minutes after the push, so a
one-minute sweep before pushing saves a full cycle.

**How to apply:** after a rebase that touches `frontend/e2e`, run a sweep: for each
`getByTestId("...")` literal in the touched specs, `grep -rqF` it in `frontend/src`
excluding tests; composed ids (`workspace-preview-stage-*`, `phase-chevron-*`,
`ribbon-*`) are expected misses. Watch CI with two consecutive `g5d pr checks --watch`
runs: `g5-ci-e2e` takes ~70 minutes, so the first 58-minute watch times out with exit 124
while it is still pending. See [[g5-ci-cloudbuild-triage]] and [[issue-816-pr-822]].
