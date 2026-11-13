# Process isolation mode for the local workspace host

## Human request

On 2026-10-12 the user asked, pointing at the request document
`docs/235253bb-5c9d-44ae-a39a-9156e2c4b00b/request.md` in the docs repository:
"check the request.md and implement or wire the process isolation mode for workers in
localhost and also wire the just recipe, so `just dev-up` can start workers in process
isolation mode".

The request document asks for an explicit, opt-in `process` isolation mode for a
developer's local `g5d host`, so a workspace's dispatcher and modules (chat, preview,
terminal, bootstrap) run as host processes instead of inside a libkrun microVM. The
motivating use is the `dev-up` preview profile on a developer Mac, where nested Docker
inside the microVM stalls or kills the VM. The mode is for localhost development only,
must never be reachable in the cloud or self-hosted lanes, and must never be a fallback.
The document fixes the scope (host-level mode, hybrid rejected), the design shape
(launcher seam, dispatcher path overrides, host-native runtime validation, fail-closed
gates, loopback networking, explicit skips), the ontology work and the testing expected.

## Agent interpretation

- The user's message is the instruction to implement, so planning and implementation
  happen in one pass; the plan is saved first under
  `.g5/pr_ontology_diff_design/process-isolation-local/ontology-plan.md`.
- "Workers in localhost" means the local `g5d host` started by `just worker-host-start`
  (and therefore by `just dev-up`); `G5D_ISOLATION_MODE=process
  G5D_ALLOW_PROCESS_ISOLATION=1 just dev-up` starts it in process mode and the default
  `just dev-up` is unchanged.
- The request document's file paths predate the crate consolidation (`g5d-cli` is now
  `backend/g5d/src/cli`); the seams it names still exist and are reused.
- Where the document offers a choice (placement refusal versus a UI notice) the plan
  decides and records the reason.
