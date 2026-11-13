# Base re-verification from Home — chat intent

Workspace branch `ziniuw/periodical_reverification`, 2026-11-05 to 2026-11-06.

## Human instructions

Problem (2026-11-05). The workbench's Re-run Verification is optional even when an administrator turns it on, because it is costly and close to worthless for small PRs. So Base's formal verification foundation goes stale after a few PRs merge without it.

Owner decisions (2026-11-06), answering the agent's questions:

1. "Main" is always Base, and the UI says "Base".
2. Concurrent PRs make per-PR tracking pointless. Base's re-verified point is the last time someone pressed the Home button and merged that verification workspace into Base. No PR's own verification advances it, and the "started exactly at the verified point" case is not handled.
3. "Ran re-verification" means the dedicated re-verification workspace was created and merged. A COMPLETE verdict rests on agent output plus human judgement and is not enforced. No receipt.
4. One database table governs the runs. A row records the workspace that verifies the stale Base up to the commit it forked from, and the row reads completed once that workspace merges into Base. It tells when the last verification happened and whether one is running. A run in progress blocks a second one unless its workspace is deleted, and the database tracks that.
5. Home shows orange text, not a warning, with no threshold. The two metrics are PR count and net LOC (net diff from the stale point to the tip). A third, "LOC touching code with models", is added only if it can be computed deterministically, without LLM calls.
6. While a run is in progress, show "Verification in progress in workspace X (by Y)" and grey the button.
7. Anyone who can fork Base may start a run. Everything is shown and available only while the administrator's switch is on.
8. The pop-up explains re-verification and its cost, and says the work happens in a forked workspace that the user merges manually. It shows the stats and a log of merged PRs and of runs. It offers two options:
   - incremental: from the stale commit `xxx` to the commit the run forked from, `yyy`;
   - full: `yyy` is the latest Base; make every model current, add new models and verify everything.

   The owner tunes the exact prompts later.
9. Either option forks Base and redirects the user to the new workspace with a pre-loaded prompt; the user starts the VM and presses Send. It is a regular workspace: every merge gate and approval applies.
10. A concise, templated Project Doc says what the verification PR must do and what is optional or out of scope. The pre-loaded prompt only says to read it and follow it end to end.
11. When the switch is first turned on, the stale point is unknown, so the full option runs first.
12. The verification workspace rebases before merging. It does not re-verify the PRs the rebase brings in; its scope stops at the commit it forked from.
13. Rebase (owner reply after the agent's options): the worry is exactly that both sides changed the models and the rebase leaves wrong models. A rebase does text resolution, automatic checks and lightweight fixing, so that the models absolutely parse and run, without making them up to date. Add instructions to the rebase prompt, or put them in the skills. When both sides' reconciled ontology statements have associated TLA models, updating those models is a natural part of de-risking the rebase. A rebase does not ensure everything is incrementally re-verified.
14. Bugs: like uplift, report bugs and design simplifications without fixing them. When the uplift's bugs and design-simplification files already exist, extend them instead of creating anything new. Their reader gets a filter: show all, or one re-verification run or uplift.
15. Ignore for now that G5's own repository gitignores `.g5/TLA_verification/*`. The owner will port G5's foundation later and remove that rule.

Owner answers to the question set `base-reverification` (2026-11-06, all recorded as decisions):

16. **Run state:** derive it from existing records (`run-state`).
17. **Project Doc:** "Actually, forget about the project doc, just do a prompt containing these information in the chat prompt. and in the prompt ask the agent to create a project doc. So no extra mechanism." (`project-doc-writer`)
18. **Rebase:** hand off to the resolution agent on Git conflicts only (`clean-replay`), and make the affected models parse and run, not the whole catalog (`affected-models`).
19. **Triage:** in the run's workspace before it merges (`triage-location`).
20. **Lines:** count code repositories outside `.g5/` (`lines-scope`).
21. **Opening the pull request:** "irrelevant, whatever the agent used to do in a regular workspace applied here, we don't any special things" (`agent-opens-pr`).
22. **Writing rules:** new and changed ontology statements follow the writing rules even where older ones do not yet.
23. **Who starts a run:** human-only for now (2026-11-06).
24. **Rebase rule:** its cost is acceptable. "We should make sure to say something like 'if there is no TLC model or one side has no change in TLC model, please ignore the following instruction'." (2026-11-06)

## Agent interpretations

These remain the agent's readings.

- **Lines-in-modelled-code count.** It is deferred. Today's verification formats name implementation code only in prose, so it cannot be computed deterministically.
- **Findings run label.** Each finding names its run on a `Run:` line, which the filter groups by.
- **Runs' own pull requests.** They count as their runs, not as pull requests, and the next stale point is the completed run's fork commits.
- **End-of-run check.** Every run ends with all models parsing and running, which covers what clean replays and merges leave unchecked.
