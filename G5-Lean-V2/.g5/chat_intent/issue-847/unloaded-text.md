# Unloaded reply text (issue #847)

## Human instruction

2026-10-27, workspace owner, after reading the three options (A: withhold the saved
reply text until the turn's activity loads; B: show it below the marker; C: show the
marker with a collapsed "Show saved reply text" toggle): "option A is fine."

## Interpretation

A turn with no loaded activity renders its prompt and the not-loaded marker only. The
marker loads activity when it nears the viewport, so the text-only state is not shown.
Browser find and copy do not reach a reply's text until its activity loads; the owner
accepted that trade-off with option A.
