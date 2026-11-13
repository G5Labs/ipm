# Select to comment in the rendered document, and an always-visible Comments panel — 2026-10-30

After PR #969 shipped (one comment side list beside both modes), the workspace owner asked
how to leave comments in rich text mode, then wrote:

## Human instructions

> please evaluate the possibility that, in rich text mode, when user select any words, a comment bubble pops up (attached please see a mock) and use can click to leave comments

The mock shows a small "Comment" bubble floating above selected words in the rendered document.

> a other issues
> when user click the comments button on top-right, we should show the panel even if there is no comment at all

Asked where the draft should open and whether the bubble should also appear while editing in rich text:

> 1/ i do not understand what you mean "Where the draft opens:"
> 2/ let's make bubble appear only while reading
> 3/ I do not understand what you said "Readers who may not start a thread see no bubble"

After the agent explained both in plain words:

> 1/ agree with your recommendation on "In the Comments panel on the right"
> 2/ when you say "some viewers cannot start threads at all.", which viewer cannot ?

## Agent interpretation

Selecting text while reading the rendered document offers a Comment bubble, as plain text
already does. Clicking it opens the new-thread draft in the Comments panel on the right, the
page staying as it is; the thread anchors to the selection's first source line and quotes
the selection. The bubble does not appear while Rich text is open for editing. Who may start
a thread is unchanged: the bubble appears exactly when plain text's bubble would (not on Base,
read-only or freezing workspaces; not for a non-owner on unpushed content; not while a reader
edits in Suggest changes). The Comments control shows the panel even when the file has no
threads. Agent choice following the owner's first decision: a record's Comment action opens the
same draft in the panel instead of switching to plain text, so the two ways to start a thread
from the rendered document behave alike.
