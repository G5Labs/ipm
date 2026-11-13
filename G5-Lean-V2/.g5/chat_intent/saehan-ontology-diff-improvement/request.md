# Change highlights within modified statements

## Human instructions — 2026-11-03

1. "Right now, it is difficult to know which part of the ontology changed when an
   ontology statement with multiple sentences changed. What is your proposed
   improvement options?"
2. After the agent's options (below): "Can we try your recommendation? Work up to the
   ontology edits."
3. Clarification, after the agent asked whether to add F and G and how to word the
   summary: "I meant 'C. Combine A and B (my recommendation). Line up sentences first,
   then highlight changed words only inside sentences that changed. If a statement
   changed too much for that to be useful, fall back to today's whole-statement view.'"
   and "Answer 1 makes this irrelevant" (about the summary wording).
4. "Don't do strikethrough since it is difficult to read. Can we only do color?"
5. "Implement, start preview full-stack, seed g5-lean, and the feature"
6. "Could you try to make more number of edits? Ideally, by referring to the open PRs or
   merged PRs and their workspaces if possible? Also test the Chat card since I
   configured the chat credentials"
7. "Can we make the background color of the changed text a bit less apparent? It is a
   bit too apparent so it makes it hard to read."
8. "Any remaining items? Otherwise, create the PR. Also create a html file in project
   docs with screenshots of the feature so that the UI reviewer can quickly check the
   feature."

## Agent proposal

The agent offered: A sentence-level marks, B word-level highlights, C sentence alignment
then words inside rewritten sentences with a whole-statement fallback for heavy rewrites,
D an inline or stacked layout with folding of unchanged sentences, E a per-statement
summary ("2 of 5 sentences changed"), F whitespace-insensitive matching so re-wrapping is
not Modified, G the same highlights in Diff Graph and the rebase/merge preview, and H a
one-sentence-per-line authoring convention. Its recommendation: **C with stacked
highlights, plus E, first**; then F; then G; hold off on H.

## Interpretation

- The scope is option C alone, in the existing stacked layout. The summary (E), F and G
  are not part of it; the owner treated the summary wording as moot, so no summary is
  shown, including in fallbacks, which show today's pair without highlights.
- Marks are background color only, one color per row; the agent's earlier "never color
  alone" rule (underline or strikethrough) is withdrawn. Underline is dropped too, since
  read-only links already use underlines.
- "Work up to the ontology edits" authorized design only; instruction 5 later
  authorized implementation, and instruction 8 the PR.
- Instruction 7 lowers the marks' tint while keeping them visible; dark mode keeps a
  slightly stronger tint because dark backgrounds absorb it.
- The request lifts the "word-level highlighting" deferral in
  `g5.documents.ontology.statement-changes`.

## Human instructions — 2026-11-04

UI review feedback on the PR, relayed by the owner, and the owner's decisions:

1. Reviewer: "I don't think the big RED blocks contribute much when you've managed to
   render the delta into the green block? I think we're trying for a union diff (show
   additions in green and removals in red)."
2. Owner: "One concern I have is that the UI feedback makes sense when there are only
   additions but it is less readable when there are also deletions as well as
   additions?"
3. Reviewer, after the agent's analysis: "I'm fine with not showing red if it's only
   additions. I wonder if there's the inverse too: no green if all deletions."
4. Owner: "The main purpose of the feedback is to reduce space. If deletions still show
   both rows, no need for the inversion. Is there any other recommendations?"
5. Owner, choosing the agent's option 1 (a Before row of only the sentences that lost
   text): "Let's do 1. Metadata footer doesn't matter since it is hidden?"
6. Owner: "Also, in the additions-only case, the Before row should be hidden with
   'Show before'"
7. Owner: "I also feel like Neutral background would be weird. Let's keep the current
   background"

### Interpretation

- The goal is less vertical space. Additions only: no Before row, behind Show before.
  Any removal: a Before row of only the sentences that lost text, behind Show full.
  Fallback pairs keep the whole Before row.
- A merged single row is declined. Without strikethrough, which the owner ruled out on
  2026-11-03, it reads poorly once text is removed, and it would put removed text into
  the live statement. Hiding Before for deletions only, with removed words marked in the
  Current row, is declined for the same reasons.
- Both rows keep their current backgrounds; the agent's neutral Current background for
  deletions only is dropped.
- The metadata footer is hidden until the statement's details are opened, so the
  shorter Before row needs no footer rule.

## Human instructions — toggle placement, 2026-11-04

After the agent's UI review of the Show before / Hide before and Show full / Show less
controls (Hide before appeared under the Before label, about five lines below where
Show before had been):

1. Owner: "I like 'Simplest fix: render "Hide before" under Modified, in the same spot
   "Show before" was.' What do you think about moving the location of the button to the
   bottom instead of right below 'Modified'? Also the same for 'Show full' and 'Show
   less'?"
2. The agent agreed for Show before / Hide before and Show full, and for Show less on
   the condition that collapsing keeps the reader's place.
3. Owner: "Make the following changes: move both buttons to the bottom of their
   gutters, add the collapse scroll fix."

### Interpretation

- Each control sits at the bottom of its row's label column: Show before and Hide
  before on the Current row, Show full and Show less on the Before row.
- Collapsing keeps the control where it was on screen. Expanding may move it down, as
  a "read more" does.
- The agent's other suggestions from the same review (padding the hit area, a
  non-breaking space before each ellipsis) were not requested.

## Human instructions — collapse keeps the statement, 2026-11-04

After trying the collapse scroll fix in the preview, where Show less kept its position on
screen by scrolling the page:

1. Owner: "It feels strange since show less should keep the statement location the same
   but instead moves the statements. Isn't it better to remove the collapse scroll fix
   and keep the location of the statement?"
2. The agent offered A (remove the fix), B (scroll only when the pair would otherwise end
   up above the view) and C (keep the fix), recommending B.
3. Owner: "I want A. Rebuild the preview."

### Interpretation

- Collapsing Before scrolls nothing: the statement stays where it is and content below
  it moves up. Show full comes back higher by however much of Before collapsed.
- The owner accepts that after Show less on a long Before row whose top had scrolled
  away, the pair can end up above the view.
- The controls stay at the bottom of the label column, and focus still follows from Show
  less to Show full.
