• This PR has become very large, and completing every remaining item sequentially through one agent is too slow. Our immediate goal is to reach a stable integration milestone that we can intentionally merge as an incomplete checkpoint. After that, zcl-g5, kaihuiz-g5, and rahul-g5 will work in
  parallel on the remaining gaps.

  Focus only on reaching this minimum milestone:

  1. `g5-ci-rust` passes on the exact PR head.
  2. The backend and microVM portions of `g5-ci-e2e` pass, including `/dev/kvm`/libkrun startup, Rust E2E, host startup, and the retained API/value-loop backend stages.
  3. Any P0/P1 backend findings from the current Claude review are either fixed or rejected with concrete evidence.
  4. Fresh database creation, migrations, validation preflight, and QA reseeding work. Existing QA data does not need to be preserved; we are willing to reset and reseed QA.
  5. The branch is committed, pushed, clean, and all evidence refers to the exact candidate SHA.

  Full Playwright/browser completion is not required for this milestone unless a browser failure reveals a backend or microVM defect. Record browser-only failures as deferred work.

  Do not merge the PR yourself. When this milestone is reached, stop and ask me for approval to perform the checkpoint merge. Do not describe this milestone as full MVP delivery, deployment, or completed QA.

  Before assigning the remaining work, construct a dependency graph of the actual remaining gaps. Each task must identify its authoritative state owner, production execution path, affected files, shared contracts, validation requirements, and dependencies.


  4. Shared-interface changes are completed as prerequisite tasks before independent work begins.
  5. Workstreams are balanced by estimated effort, not by the number of packages.
  6. Each workstream can produce PRs that are independently reviewable and mergeable into `main`.
  7. The owner fixes the root cause at its authoritative producer. Consumer owners make compatibility changes and add regressions only after the producer contract is settled.
  8. A single root cause must not be assigned to multiple people.


  - kaihuiz-g5 owns the host and infrastructure substrate: `g5d-host`, `g5d-isolation`, host control, worker-host registry and placement, KVM/libkrun execution, retained roots, VM process lifecycle, and host-side relay.
  - rahul-g5 owns the guest runtime and agent execution substrate: all of `g5d-dispatch`, `agent.turn`, task/session/input lifecycle, continuations, dependencies, module execution and journals, and provider-native requests.
  - zcl-g5 owns the workspace control plane and product surface: workspace lifecycle, ownership, transfer, forks, receipts, flush-block presentation, Save/read authority, public APIs, and frontend.

  Use these root-cause routing rules:

  - Failure before `runtime.ready`, VM exit, placement, capacity, credential, retained-root, or host-relay problem → kaihuiz-g5.
  - Dispatcher, module, agent-turn, task/session/input, continuation, dependency, or provider-request failure after runtime readiness → rahul-g5.
  - Ownership, lifecycle API, fork, transfer, Save/read, receipt presentation, or UI failure → zcl-g5.
  - A fixture that disagrees with a confirmed correct producer → owner of the fixture.
  - A producer/consumer disagreement caused by an unclear contract → create a prerequisite interface task before assigning dependent work.

  Treat the following as frozen shared interfaces:

  - API-to-host control protocol
  - host-to-dispatcher mux protocol
  - dispatcher module schemas
  - workspace lifecycle state transitions
  - task-attempt and queued-input schema
  - public API response shapes
  - seed and E2E fixture contracts

  Also treat `g5-internal-api`, migration numbering/checksums, common E2E helpers, Cloud Build configuration, and shared runtime test peers as controlled shared files. If one must change, create a separate prerequisite task with one named owner and list every blocked workstream. Do not let
  multiple workers modify the same shared contract concurrently.

  Do not force an entire cross-boundary package onto one person. For packages such as P4.3a, P5.1, P5.2, P4.9a, and P4.9b, assign their host, guest-runtime, control-plane, and retirement portions according to the ownership boundaries above.

  Any unresolved P0/P1 issue that still crosses all three boundaries must be fixed, rejected with evidence, or reduced to a frozen interface by the current main agent before declaring the parallel-work milestone reached.

  When the milestone is reached, provide a self-contained handoff report containing:

  1. The exact candidate SHA, PR URL, Cloud Build IDs, test results, and latest completed Claude review round.
  2. A checkpoint matrix for S1 through S6 from `docs/lean-mvp-1stream-implementation-plan-v9.md`. For every checkpoint, distinguish:
     - implemented
     - locally validated
     - Cloud/microVM validated
     - deferred
     - still unverified
  3. A package-level matrix for all 18 MVP packages, using both the package identifier and title:
     - P4.4a — `g5d host`: the `ctl` protocol on both ends, host credential, event journal, and drain
     - P4.13 — Host registry, host states, placement, quotas, drain, and the operator API
     - P4.4b — `g5d host`: `VmSpec`, boot and stop, lease, stream relay, and retained roots
     - P4.3a — Shutdown flush, receipts, restore, and stopped reads
     - P4.5 — API lifecycle service, six actions and transfer, owner authorization, and Workspace Runtime facade
     - P4.3b — Owner live fork capture and durable reader forks
     - P4.11 — The `agent.turn` module and per-turn CLI adapters
     - P4.6 — Dispatch, shared agent lifecycle, same-task continuations, and native requests
     - P4.7 — Typed-input agent sessions and inbox; delete sticky runtimes
     - P4.8 — Templated-input agent sessions and optional jobs; delete job/task branches and merge
     - P2.1 — Attached-owner Save/upload and role-aware Git/live reads
     - P2.2 — Frontend explicit Save; remove CRDT from editors
     - P2.4 — Delete the backend CRDT layer
     - P4.10 — Frontend workspace VM status, owner controls, Transfer, reads, and Changes
     - P5.1 — Owner-only preview service tasks on the workspace VM (`workspace.preview`)
     - P5.2 — Browser terminal
     - P4.9a — Delete per-task execution on the host and in the worker
     - P4.9b — Delete the workers table, per-task routes, loops, and UI

     For every package, distinguish:
     - implemented
     - locally validated
     - Cloud/microVM validated
     - deferred
     - still unverified
     - confirmed remaining gaps
     - assigned owner and workstream
  4. For every remaining gap:
     - confirmed problem or missing evidence
     - authoritative state owner
     - production execution path
     - affected producers, consumers, and sibling paths
     - required regression or acceptance test
     - dependencies on other unfinished tasks
  5. A dependency graph of all remaining tasks.
  6. Three balanced workstream plans for zcl-g5, kaihuiz-g5, and rahul-g5.
  7. For every assigned task:
     - exact scope
     - expected files/directories
     - acceptance criteria
     - focused validation commands
     - shared interfaces that must remain unchanged
     - dependencies and blockers
     - suggested merge order
  8. A short list of prerequisite shared-interface tasks that must merge before the three independent workstreams begin.
  9. A final list of browser-only or deployed-QA work intentionally deferred beyond the checkpoint merge.

  Until the milestone is reached, prioritize backend, Rust, host, dispatcher, API lifecycle, and microVM correctness. Avoid expanding scope or spending time on deferred browser-only failures.