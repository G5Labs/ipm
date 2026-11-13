# Meeting decision cards: task buttons, no votes, arrival cue, question index

Session of 2026-10-22 (Claude Code, Davide). Design: [ontology-plan](../../pr_ontology_diff_design/meeting-task-buttons/ontology-plan.md). Mocks: [meeting-task-buttons.html](../../pr_ontology_diff_design/meeting-task-buttons/mocks/meeting-task-buttons.html), [review-meeting-entry-options.html](../../pr_ontology_diff_design/meeting-task-buttons/mocks/review-meeting-entry-options.html).

## Human instruction

After an audit of `g5.workbench.meeting` against the 2026-09-30 standup walkthrough
(where Accept / Delay / Out of scope each "create a task" and Reject dismisses):

> let's change the name of the buttons into:
> - Create a task and attach to the current workspace
> - Create a detached task
> - Reject
>
> Assuming we have these 3 buttons, do some research using the skills in ~/UI-Skills
> (and searching the web based on that) to see if it would make sense to have a small
> animation that when you click on create attached task, flashes the issue sidebar to
> show that there is an issue now

Then, on the mock that spotlights the new row and pulses the rail badge once:

> Let's implement this design. Let's also remove the vote option from each card (but
> keep voting capabilities in the platform, they are useful for later). Before starting
> two more things:
> 1) The review meeting button in the action bar has two options in the dropdown to
>    upload a transcript and to extract decision, they are the same thing? Use the
>    design skills to check what's best here
> 2) When pressed the review button starts immediately. Should we have a confirmation
>    button? Use the design skills to check what's best here
> 3) Add an index of the side of the html so we can quickly navigate across questions

> if there are more UX patterns that could apply for the first 2 questions, feel free
> to create multiple mocks and I'll choose the best

On the entry-options mock (recommendation was 1A + 2A):

> let's do option 1c, then question 2 is moot

> on the menu that opens with option 1c, let's also add a list of already generated
> documents with the extracted decisions, so a user can quickly go to one of those that
> was generated before

> remove Apply accepted decisions completely and also Extract decisions. Let's also rename
> the "Open questions" text under the button in the action bar, to something like detect
> blind spots or detect conflicts. What are good alternative names?

> let's rename to "Find gaps" and open a PR

## Agent interpretation

- "Create a task" means a tracker issue: G5 has no task object apart from the external
  issue tracker (`g5.external-issues.creation`). Attach means the quick-add link path
  (`g5.workbench.issues.create`): related link, the primary issue as parent when it is in
  the creation repository. Detach means no link and no parent.
- A task press is one press, like the earlier Accept: the issue is created and the
  decision recorded without a second Answer. The shared Create issue dialog still appears
  for an untrusted destination, because the admission rule requires the payload review
  there; a trusted destination creates from the press.
- "Remove the vote option from each card" is read as the meeting decision cards, the
  subject of the conversation: one press decides there, so a vote has nothing to inform.
  Ordinary referenced question cards keep their vote controls, and the vote store, the
  vote page and the API are unchanged.
- The animation is the data-driven cue the research settled on, not a whole-sidebar
  flash: the new row is spotlighted once when its link lands in the list, the rail icon
  carries the linked count and pulses once when it rises, and a status line announces
  the attachment. Nothing fires for a detached task.
- "Index of the side of the html" is read as the editor's existing Outline rail listing
  the `[Q<N>.]` decision records with their state, since the decisions document has no
  headings for it to show today.
- Variant 1C: the ribbon's Review meeting becomes one button opening the transcript
  dialog, which offers an upload or a transcript already under `meetings/` and starts
  extraction from its primary action. The dialog is the confirmation step, so the
  extraction template starts without another one (question 2). The two templates keep a
  home under the Analyze menu for a document that is already open; this is the agent's
  reading of the mock's note that Apply accepted decisions needs another home.
- The dialog also lists the `meetings/*.decisions.md` documents already extracted;
  pressing one opens it in the centre (the cards carry the decisions made so far).
- "Remove Extract decisions" is read as removing it from the action bar: the dialog still
  starts that template, and the server resolves templates by action id from the config,
  so it stays in the config as a hidden action. Apply accepted decisions is deleted.
- The button is renamed "Find gaps" (the owner's pick on 2026-10-22 among the alternatives
  the agent offered: Find blind spots, Detect conflicts, Stress-test requirements, Surface risks); the journey step wording ("Surface the open questions") and the status band's
  Open questions count are different surfaces and keep their text.
- Existing decision documents whose sets carry the earlier four options keep their
  current behaviour; the new set applies to extractions made after this change.
