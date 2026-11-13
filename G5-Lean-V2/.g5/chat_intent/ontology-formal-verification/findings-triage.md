# Bugs/issues and design simplifications: owner decisions

Workspace `ziniuw/ontology-formal-verification`, chat of 1 November 2026. The reviewed design is the Project Doc `findings-triage-design.md`; the implementation plan is the [plan's findings section](../../pr_ontology_diff_design/ontology-formal-verification/ontology-plan.md#findings-triage).

## User story

"After the initial uplift or a full formal verification re-run, user will log in to the workspace and find all bugs/issues and design simplification opportunities identified during this formal verification run. User will be able to decide whether to put these issues into internal issue trackers or not."

The mock's buttons "Need fix", "Defer" and "Irrelevant" mean "Create an issue for this with high priority", "Create an issue for this with low priority" and "Not a bug, remove it". The user can change the priority, or file an issue for an entry marked Irrelevant. A chat button opens a chat seeded with a prompt that identifies the entry. The ontology links the entries.

## Decisions

1. The two files live in the code repository's `.g5/TLA_verification/`, "because uplift workspace may not be the workspace showing to the customer". They are not workspace-scoped, and the decisions are stored in them.
2. An issue's priority is a title prefix.
3. Changing a decision is supported.
4. "Do not link to workspace, this just create issues and do nothing else."
5. Only actionable entries get buttons; the rest are listed without them.
6. Severity is optional, but the guidance asks the agent for one of four levels. Without one, the entry is labelled Unranked.
7. The issue holds "whatever that is displayed to the user plus links to the other materials".
8. A re-run updates the same two files incrementally and keeps decided entries intact.
9. The agent builds the files for now, with no UI button for that. A separate instruction file in `.agents/guidance` reviews all material for format and location, including these two files. It is not linked from uplift or anything else; "assuming that this will be called".
10. The only entry point is the Core Ontology panel, with a different symbol (a yellow question mark). The files are not linked from the ontology root and not placed in Project Docs.
11. The guide is dry-run on an uploaded customer checkout, the pilot repository.
12. A decided entry links its issue in the file; an Irrelevant one says "user decided this issue is not important".
13. "The ontology can link to those issues in the design verified expansion."
14. The dry run first remaps the ontology's links and adds severities.
15. The files go in the folder the viewer reads verification material from, which "must be durable and not gitignore".
16. Agent format mistakes are handled by a Python format checker that the guide asks the agent to run.
17. The viewer's field rules move from the artifact guide into the new guide.
18. Links in issues point into the deciding workspace.
19. The reader offers a severity filter or sorting; the mock's category filter is left out.
20. G5's own `.gitignore` keeps the two findings files committed.
21. `.g5/TLA_verification/` stays hidden from Files.
22. A decision is written without its own commit and rides along with the next Save changes or PR update: "this decision will create an issue right? as long as we have that, we should be good."
23. Findings that need a product decision first (open intent questions, uncertain intent) are actionable too, so they can become issues (owner, 2 November 2026: "go with option 2"). Severity never decides whether an entry is actionable.

### Preview review, 2 November 2026

After trying the pilot repository in the preview, the owner decided:

24. "Question is not a tag, uncertain is. One block should have at most one tag for provenance." A Q statement is a block that starts with Q instead of I or D, and gets no Question badge. The `uncertain` badge stays as it is.
25. The severity filter must read as a filter, and while it filters, the page hides "the top-level description sections".
26. The proposed findings look is accepted: each entry a card edged in its severity's colour; Open green, Need fix red, Defer amber and Irrelevant grey; buttons that look clickable; the reason they are unavailable stated once for the page.
27. A finding's chat action and the meeting cards' Discuss open their chat in the side panel rather than replacing the document.
28. Following a link offers a way back: "reuse that button", the existing Back of trace navigation, "and don't try to do anything fancier". Whether it goes back more than one step follows from that button. In this PR.

### Gaps against the mock, 3 November 2026

The owner compared the UI with their HTML mock of the pilot repository (kept outside the repositories because it embeds customer content) and decided:

29. "For Design verified, if there are linked issues/bugs, it should have a small dot next to the text in the button. The color should be based on severity color; if there are only minor bugs, do not show the dot." Minor means Low and Unranked; findings marked Irrelevant do not count; a parent's dot includes its children's findings.
30. "Bugs/issues are also linked to the models that discovered this bug and also the test that reproduced this bug. Make sure this information is in the file and also correctly rendered in the UI." The labels are "Found by" and "Reproduced by", "with some concise explaining"; without a reproduction the entry says "no reproduction evidence".
31. "I don't need the 'Sources: Original G5 report' etc. for bugs/issues and design simplification; that linked source is like our current file."
32. "The design simplification must also have linked model verification evidence": the mock's rendering of the model for the existing codebase and the simplified model, showing the simplified one has fewer states and is also correct.
33. "Implement everything, test them end to end and update the preview."

### Preview check, 3 November 2026

34. "I got 'Not run' for all Proposed simplification, which is not possible because the mock has it, so it must exist in the [pilot] codebase. … Make sure the guidance on material delivery is clear about this."
35. "The table for 'Evidence for the simplification' should be at the very end of this block."
36. "Just remove the line of 'No reproduction evidence' and highlight the 'Reproduced by — A test or probe that made it happen in the code' in a bold text." This replaces the wording of decision 30.

## Agent interpretation

- **Where decisions live.** Question sets live under `docs/<workspace>/questions/`, so they cannot hold decisions that are not workspace-scoped. The entry's own decision line is the record, and Review meeting's one-press pattern and shared issue operations are reused.
- **"Durable" (decision 15).** Inside the workspace VM, Git skips only G5's runtime folders, so `.g5/TLA_verification/` is already committed and flushed like ordinary content. G5's "durable `.g5` content" list in effect decides Files visibility, and decision 21 keeps the folder out of Files. The only ignore rule was G5's own `.gitignore`, narrowed by decision 20.
- **Changing a decision.** A changed decision reuses the same issue: switching priority retitles it, Irrelevant closes it, and a later Need fix or Defer reopens it. On GitHub, G5's close is recorded as "completed", GitHub's only reason besides "not planned".
- **Filter, not reordering (decision 19).** Entries keep the file's order, which the checker keeps most severe first, and the reader filters by severity.
- **Every entry gets a chat button,** not only actionable ones.
- **Irrelevant without a recorded issue** looks the entry's creation key up first and closes an issue an earlier press filed, so the file and the tracker agree after a lost record.
- **Severity guidance** (scale, unconfirmed triggers one level lower, Unranked when no consequence is stated) and the `Findings:` metadata line came from the pilot dry run's notes.
- **Grey buttons in the preview** were the agent's seeding mistake, not a design question: the workspace belonged to a user the preview's sign-in does not offer, so the owner saw it as a reader. It was seeded again under the preview's default user.
- **Filtering (decision 25)** keeps the heading that introduces a run of entries while any of its entries shows, so the page still says which entries need a decision.
- **Discuss (decision 27)** is one control for every question card in a document, meeting cards included, so all of them open beside the document. Other composer seeds keep their place. On a phone, which has no side panel, Chat takes the center as before.
- **Back (decision 28)** is the existing single-step Back. It returns to the statement or finding holding the link, or to the top of the document for a link outside them, and shows only on the file the link opened. It also covers links that open a source file at a line.
- **Dot (decision 29)** replaces the handoff's "do not show red issue dots beside Design verified". It shows only on Design verified, where linked findings are listed.
- **Evidence lines (decisions 30 and 32).** An entry names its evidence on labelled lines the reader renders: `Found by:` with the receipts of the runs that showed the bug, `Reproduced by:` with the test or probe, or `none`, and `Comparison:` with the as-built run and the simplified run. Numbers come from the run index as recorded. The reader compares explored states, as the mock words it (fewer, a 0% change or more, with the proposal/baseline factor), only when both runs passed and the entry does not mark the pair `not like for like`, since a run that stops at a counterexample, or a different property, does not measure a full state space; otherwise it says "No comparable reduction recorded" and why, as the mock does.
- **Where comparison runs come from (decision 34).** The pilot's upload records every run the mock compared, with its variant and configuration, but its simplification records list an entry's runs without saying which one checks the current design and which the simplified model; the mock's author chose the pairs. The preview's conversion had paired only runs of the same check kind from the entry's list, so it wrote `→ none` for entries whose simplified run was recorded, and the reader showed those as Not run, as the file said. The storage guide now asks producers to name the two runs that compare each simplification. The review guide asks preparers to find both by their recorded variants, judge like for like from the two configurations, and write `→ none` only when no run of the simplified model is recorded.
- **Bold heading (decision 36)** shows only when nothing reproduced the bug; a screen reader still hears that none is recorded.
- **Provenance (decision 24).** The format guide listed the three provenance tags as alternatives but did not say "at most one", as it does for verification tags; it now does. 38 blocks of G5's own ontology carry `human-provided, agent`; they are left for their owners.

