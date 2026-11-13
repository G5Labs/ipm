# Dynamic action bar — request and interpretation

## User instructions (2026-10-19)

- "The Action bar has different settings: extended icons, extended text, etc.. Let's add
  one more that is dynamic: It should behave more like with Excel. First the button size
  should reduce a bit and then it should become scrollable."
- Design first; implement with a subagent; verify on the local dev stack
  (`just dev-down` + `just dev-up`, then localhost).

## Agent interpretation

- A fifth value of the existing per-person "Action bar" setting, `dynamic`; the default
  stays `extended-icons`.
- At full width `dynamic` looks exactly like `extended-icons`. When the row no longer fits,
  every button takes one smaller glyph-over-label size ("reduce a bit": one step, not a
  continuous scale). When even the compact buttons do not fit, the action groups scroll
  horizontally instead of wrapping; the workspace controls at the right end stay in place.
- Excel's further steps (collapsing a group into one dropdown) are not requested and not
  implemented.
