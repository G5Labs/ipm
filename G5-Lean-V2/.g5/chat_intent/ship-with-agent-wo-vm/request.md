# Ship with agent without an attached VM — owner decisions (2026-11-05)

Human instructions (workspace chat and question cards, verbatim where quoted):

1. "Audit the ship with agent button, now when there is no vm attached, it just queue the
   message. But other buttons behavior are different, other buttons create a prompt in the
   message chatbox and the button become Start and send. … Can you mockup the ship with agent
   buttons new behavior and give me proposal."
2. "I do not want directly send message when i hit ship with agent. It should go to chat with
   prompt prefilled like other buttons." (card Q1: seeded composer, decision)
3. "check the answers, since merge when ready and ship with agent are coupled, i would like to
   go with starting and sending messages immediately." Card Q5: "If this is the case, i would
   prefer at the click and the click directly start and send the message." Cards: Q2 label
   "Start and ship with agent"; Q4 while the VM starts, enabled and queues.

4. "1/ yes, 2/why queued message is withdrawn? it should still be queued right?" then, after the
   shared Start and send rule and a "stays queued" alternative were explained: "no, keep same as
   other bahaviors."

Interpretation: instruction 3 supersedes 2; instruction 4 confirms card Q8 and settles Q3 as the
shared rule (withdraw the bound chat when the Start ends early; keep the merge-when-ready
decision). The final design is the compound start of the
Project Doc `ship-with-agent-without-vm.md` (docs repository, workspace
15159c69-2438…), recorded as card Q8 for confirmation. Q3 is settled by instruction 4. Q6 and Q7 answered the seeded design only.

5. (2026-11-06, with a screenshot of the Resolve Git Conflicts dialog) "resolve with agent
   button should also support non vm attach version, basically if vm is not there, you should
   start a vm and send the message to resolve the conflicts. add this fix to existing pr and
   mock ups too."

Interpretation of 5: the conflicts dialog's agent choice follows the same compound design and
the same decisions (Q8, Q3) as Start and ship with agent, in PR #1128; the label "Start and
resolve with agent" and the joining of a Start under way are the agent's choices by analogy with
Q2 and Q4 and main's joining rule.

