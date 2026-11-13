# Preview without an attached VM

## Human request (workspace chat, 2026-11-05)

> Can you read the current code and proposal a way that when there is no vm attached to a
> workspace, and user clicked preview button and pick the profile, the preview will trigger
> a vm attach and start the preview. Other buttons related to chat has already implemented
> that way, can you audit if you can reuse the code. Give me a design.

## Agent interpretation

A design proposal, not an implementation: one action from the Preview dialog on a stopped
or failed owner VM that records Start consent and launches the chosen preview once the VM
runs, reusing the chat Start and send machinery where it transfers. Design:
[ontology-plan](../../pr_ontology_diff_design/preview-wovm/ontology-plan.md).
