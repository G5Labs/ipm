# Pushing changes from a review workspace

Read this before committing and pushing changes to the parent's files from a review workspace.

A review workspace is a copy of its parent made for review. When you push, each change to a file the parent has is shown to the parent's owner as a suggestion on that file. The owner accepts, edits or rejects each one. Nothing you push changes the parent until they do.

## What a push becomes

G5 divides your change to a text file into suggestions the owner can each decide alone. It reads every text file as Markdown and treats a line break as the end of a sentence:

- A small separate fix, such as one corrected word, is its own suggestion.
- A reworded sentence, or a heavily changed line of code, is one suggestion. So is a paragraph, heading or list item that is mostly rewritten.
- A removed list item is one removal and an added list item one insertion, each with its sub-items.
- Two or three unrelated word fixes in one sentence may arrive as one suggestion.

A new file is one suggestion.

## Keeping suggestions easy to decide

- Change only the text you mean to propose. Re-wrapping lines, changing list markers or tidying nearby text adds suggestions the owner has to read and decide.
- Make one point per sentence or list item, so each suggestion stands alone.
- Put the reason in the commit message or the review note, not in the file.
- A later push replaces your earlier open suggestions. A decision the owner already made carries over only where your text at that place is unchanged, so do not re-edit text they have decided unless you mean to ask again.
