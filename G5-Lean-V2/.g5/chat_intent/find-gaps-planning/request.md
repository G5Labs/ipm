# Find gaps plans first — request and interpretation

## User instructions (2026-11-03)

- "The Find gaps button should also trigger the ontology planning before generating the
  questions. What is your proposal?"
- On the [proposal](../../pr_ontology_diff_design/find-gaps-planning/ontology-plan.md):
  Find gaps may amend the ontology where the requirements settle intent ("1. Yes"), and it
  shares the one workspace plan with Create plan ("2. Yes").
- On tightening the prompt (2026-11-03): drop the agent's rule that technology and
  architecture choices are left to System Engineering; keep only the limit on ontology
  amendments ("Go with 1").
- On the gaps it lists (2026-11-03): drop "within the documents or between them and the
  ontology", since requirements are meant to change the ontology; ask only about
  contradictions between the documents and leave ontology differences to the plan.
- On linking (2026-11-03): drop the instruction to link each of the plan's open assumptions
  to its question; keep "leave answered questions alone".
- On Create plan (2026-11-03): drop the added clause about continuing Find gaps' plan and
  settling `decision: true` answers; keep main's prompt with the minimal edit "read the
  requirement documents and human-answered questions".
- On a live retest that wrote one side of a contradiction into the ontology (2026-11-04):
  "Yes, make the change and re-run the test" — where the documents disagree, write neither
  side into the ontology until the question is answered.

## Agent interpretation

- Intent a requirement leaves open stays an open assumption with a question; Find
  gaps does not encode it as a commitment.
