# Comments beside Source as in Preview — 2026-10-30

## Human instructions

After trying the Preview side list ordered by line ([issue #964](https://github.com/G5Labs/G5-Lean/issues/964), Part A), the workspace owner wrote:

> i tried your changes, now when i click the comments in preview mode, i think the comment line becomes center?

> can you make the comment's line becomes the 1st line of the preview, it is hard to find the center

> comments in preview mode and source mode still very different, in preview model,all comments by default stack and sit at the top, but in source mode user scroll to say comments at the comment's line, can we make them the same behavior as preview mode (ie., sorted and stack at the top by default, when use click, it scrolls so that the comment line become the 1st line of the window )

## Agent interpretation

Both views show a file's threads in one side list, stacked from the top in the text's
order (placed line, then creation time; unplaced threads last). Expanding a card brings
its line to the top of the view: the rendered block in Preview, the source line in
Source. This replaces the Source margin that placed each card beside its row and
scrolled with the text; Source keeps selection commenting, its glyph anchor marks and
the draft composer, which opens first in the list. Placing Preview cards beside their
blocks (Part B of the issue) was not requested and is not done.
