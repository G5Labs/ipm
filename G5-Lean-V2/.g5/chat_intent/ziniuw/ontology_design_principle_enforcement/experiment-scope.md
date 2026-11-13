# Ontology design principle enforcement: experiment scope

## User request

Create `docs/experiments/ontology_design_principle_enforcement` and a file consolidating
the following documents from `g5_docs/docs/lane_1_ontology_design/formal_verification/`:

- `formal_verification_and_good_design.md`
- `enforcing_good_design.md`
- `design_principles_in_ontology.md`

The user specified this PR scope:

> 1. we will be deeply explore the good design, analyze the current codebase against different design principles, find concrete issues.
> 2. Explore how to encode general and/or project-specific good design (including general design principles, concrete abstractions, clear module boundaries, etc).
> 3. Actually refactor the G5-lean codebase to be really modular + some skills to enforce the abstraction and respect the module boundaries.
> 4. Test and compare the agent's efficiency and code quality for incremental works on the clean/modular/somewhat partially verified codebase vs the current codebase.

## Agent interpretation

The immediate request is to create the consolidated document and describe the work
needed in this PR. The four points define the intended subsequent investigation,
design, implementation and evaluation. They do not establish that an audit, refactor
or comparison has already occurred.

The [experiment plan](../../../../docs/experiments/ontology_design_principle_enforcement/ontology-plan.md)
retains the source synthesis, current governing contracts, work sequence, evidence
requirements and proposed comparison. Continue consequential follow-ups in that plan.
