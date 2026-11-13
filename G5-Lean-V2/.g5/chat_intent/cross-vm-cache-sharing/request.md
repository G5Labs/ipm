# Cross-VM cache sharing

## User instructions

From direct chat on 2026-10-08 (background job), after Phases 1, 2 and 4 of the QA guest
VM fix plan merged and Phase 3 (PR #392) entered CI:

- "can you help me create a design for cross vm cache sharing"

Earlier instructions in the same conversation set the goal this serves: "try to include
all of them in the qa g5 image, so every time it should be fully warm for tool chains and
deps" ([warm-toolchain.md](../g5dev-warm-toolchain/warm-toolchain.md)).

Follow-up on 2026-10-08, after the design was published as draft PR #394 and the
Phase 3 hotfixes landed:

- "implement the tier 1 and tier 2 and invoke codex to review and converge. after merge
  and deploy, check in qa, the cache is working"

## Agent interpretation

First a design request, then its implementation: the deliverable is the design and the
ontology changes it needs for human review, followed by the host cache door (Tier 1) and
the fleet tier (Tier 2) implemented on that design, Codex review rounds until a round
finds nothing, a merge, and a check on QA that the door is serving. The two open
questions in the design are taken as their recommendations (no same-organization
derived-object writes; no npm facade or node_modules tarball yet).

"Cross-VM cache sharing" is read as: build and dependency caches that today exist only
inside one workspace VM's incarnation (and are rebuilt or re-downloaded by the next VM,
on the same worker host or another) become shareable across VMs, so a fresh VM starts
warm beyond what the guest image bakes. The caches in scope are the ones the QA audit
showed being rebuilt per VM: compiled Rust dependency objects (sccache), the cargo
registry and git checkouts, npm packages, Playwright browsers, Docker image layers pulled
or built by the in-guest dockerd for the dev-up preview, and apt packages fetched by
those builds.

Constraints read from the ontology before designing: every Start builds a fresh
workspace root and nothing from an earlier incarnation's tree is reused
(`g5.runtime.materialization`); stopped workspaces keep no root, placement prefers no
host, worker hosts live hours not days, and warm caching "returns only with a measured
restart benefit and a design that keeps every Start on a fresh root"
(`g5.scope.warm-roots`); host disk is a shared budget with a fixed allowance for image
cache and template (`g5.runtime.placement.host-disk`); tenant scope survives privileged
operations (`g5.security.tenant`); and host Git never runs in content a guest wrote.
A cache a guest can write and another tenant's VM can read is therefore a poisoning
channel unless the consumer verifies content independently of the cache.
