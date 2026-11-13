---
name: workspace-tab-strip-test-flaky
description: WorkspaceTabStrip.test.tsx fails intermittently in full vitest runs (CI frontend-test and the VM) on unrelated branches; passes alone and under CPU stress.
metadata:
  node_type: memory
  type: project
  originSessionId: 6ff18d5b-2a2b-490a-9c36-5f9607ab21ed
  modified: 2026-10-24T18:54:52.482Z
---

`frontend/src/components/layout/WorkspaceTabStrip.test.tsx` fails about once per full
`vitest run`, a different case each time. Seen 2026-10-24: "leaving a workspace closed
in another browser tab drops it" (PR #728's CI build 6a11e74a), "keeps showing the routed
workspace without reopening a tab closed in another browser tab" (a full local run), and
"moves focus along the tabs with the arrow keys" (fix/home-issues-tile-header, build
09816224). The cross-tab cases fail as if the arrival effect in `useWorkspaceTabs.tsx`
re-ran after the simulated storage event and reopened the routed tab.

Not reproduced alone, with 79 related suites together, with `/users/me` delayed 300 ms,
or with every CPU saturated. Root cause unknown. The strip renders only once
`enabled` (user loaded), so waiting for loaded tabs in the test is a no-op.

**Why:** branches that never touch the tab strip lose a CI cycle to it.

**How to apply:** if `frontend-test` fails only here and the diff does not touch the
tab strip, `useWorkspaceTabs`, `shellRoutes` or auth, treat it as this flake and
re-run CI. A real fix needs a reproduction first. Related: [[g5-ci-cloudbuild-triage]].

Seen 2026-10-26 on PR #839 (a branch that touches `useWorkspaceTabs.tsx`): "keeps
showing the routed workspace without reopening a tab closed in another browser tab"
got `[ALPHA, BETA]` in storage instead of `[ALPHA]`. Mechanism: the test fires the
storage event as soon as the strip is found, which can precede the mount commit's
passive effects; the deferred arrival effect then reads the already-shrunk list and
writes the routed tab back. Only the arrival effect adds tabs, so a diff that adds no
tab writer is not the cause. Making the arrival a `useLayoutEffect` is NOT a fix: it
runs before the passive effect that loads the stored list, overwrites storage with
the routed tab alone, and fails 36 strip/page cases.
