# Automatic divergence resolution

## Human instructions

While testing remote tracking on the issue-240 dev stack, the user said: "Divergence currently requires the human to click "resolved" - but I think the system can discover because there's a force-push to the workspace branch. Can we make this automatic?"

## Agent interpretation

When the rebased result of a divergence resolution reaches G5 as a rewrite of the workspace branch, G5 should run the same completion proof that "Mark resolved" runs today and complete the run itself. Provider-rewrite approval checkpoints are outside this request; they remain explicit approvals.
