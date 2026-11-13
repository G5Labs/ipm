# Dynamic action bar style

Source: [chat intent](../../chat_intent/dynamic-action-bar/request.md) (2026-10-19).
Continues the [header styles design](../header-style-settings/ontology-plan.md) (2026-10-09).

## Ontology changes

| Block | Change | Why |
|---|---|---|
| `g5.workbench.header-styles` (restored in `ui.g5.md`, amended) | The main ontology rebuild (2026-10-13) dropped this block. Restored under its original ID with both settings, and the Action bar gains `dynamic`: extended icons that shrink one step to a compact glyph-over-label button when the row stops fitting, and scroll horizontally when the compact buttons still do not fit. | The user asked for an Excel-like style; the lost block must exist for the amendment to have a home. |

## Governing ontology

- `g5.workbench.header-styles` (restored): every style renders the same actions, group
  names and workspace controls; the workspace controls take the actions' button shape.
- [g5.ui.workbench-config](../../ontology/features/ui.g5.md#g5.ui.workbench-config): the
  action set stays data; a style never changes which actions render.
- [g5.ui.accessibility](../../ontology/features/ui.g5.md#g5.ui.accessibility): the compact
  button keeps its label as text; a clamped label keeps the full text as the tooltip.

## Design

**Owner.** `ActionRibbon` owns the dynamic decision. Nothing outside it measures.

**Two styles, one setting.** `ActionBarStyle` (the setting) gains `"dynamic"`. A new
`RibbonStyle = Exclude<ActionBarStyle, "dynamic"> | "compact-icons"` names what actually
renders. `ribbonButtonShape` takes a `RibbonStyle`; `WorkbenchWorkspaceControls.variant`
becomes a `RibbonStyle`. Inside the ribbon, `dynamic` resolves to `extended-icons` (density
`full`) or `compact-icons` (density `compact`). Any consumer of the setting outside the
ribbon (settings page aside) renders `dynamic` as `extended-icons`; the Guided panel does
not measure.

**Compact shape.** Same column as `extended-icons`, one step smaller: 60px wide, 18px
glyph, 10px label, tighter padding. The label clamps to two lines (`line-clamp-2`,
`overflow-wrap: anywhere`); when it is clamped the full label is the tooltip unless the
button already carries a reason. `RibbonButtonShape` gains `column: "full" | "compact"`, so
`RibbonShapedButton` no longer infers the glyph-over-label layout from `iconSize === 22`. The menu caret stays inline with the label as today.

**Density decision (dynamic only).** One state, `density: "full" | "compact"`, and one
remembered number, `need`: the outer row width at which the full layout just fits.

- The groups container is `nowrap` with `overflow-x: auto` (thin scrollbar); the trailing
  controls stay outside the scroller, pinned at the right end as today.
- Measure in a layout effect and on a `ResizeObserver` of the outer row (the `px-3` flex
  row holding groups and trailing) and of the groups row (catches trailing controls that
  change width inside an unchanged outer row). In `full`: if `groups.scrollWidth > groups.clientWidth`,
  set `need = outer.clientWidth + (groups.scrollWidth − groups.clientWidth)` and go
  `compact`. In `compact`: if `outer.clientWidth ≥ need`, go `full`.
- The outer row's width does not depend on density (the trailing controls shrink with
  the actions, but they sit inside the outer row), so the two rules cannot oscillate.
- Reset `need` and `density` to `full` whenever phase, action set, variant or the
  presence of trailing changes; the layout effect re-measures before paint, so no flash.
- Scrolling is a CSS consequence of `overflow-x: auto` once compact content still
  overflows; no third state.
- A non-dynamic variant attaches no observer and keeps today's wrapping rows.

**Menus under a scroller.** `overflow-x: auto` also clips vertically, so the in-flow
absolute dropdown would be cut. In `dynamic`, an open menu stays a DOM descendant of its
trigger (same test ids, same `useDismissOnOutside` boundary) but is positioned `fixed`
from the trigger's `getBoundingClientRect()` taken on open; the ribbon closes the menu on
the scroller's `scroll` (only when `scrollLeft` moved from its value at open) and on window
`resize`, and nudges the menu left to stay inside the viewport's right edge. This relies on
no ribbon ancestor creating a containing block (`transform`, `filter`, `contain: paint`):
the browser check verified it (no such ancestor, 2026-10-19). The other styles keep the in-flow dropdown.

**Trailing controls follow the density.** `trailing` accepts a render function
`(style: RibbonStyle) => ReactNode` beside today's node (both accepted). The page passes a
function returning `WorkbenchWorkspaceControls` with `variant={style}`; the ribbon
computes the effective style once and uses it for actions, `renderAction` shapes (so the
preview trigger follows for free) and trailing.

**Settings.** Fifth `<option value="dynamic">` ("Dynamic — icons shrink to fit, then
scroll"); `ACTION_BAR_STYLES` gains it, so normalization and the e2e round trip follow.
Default unchanged.

**Test anchors.** `data-variant` on the ribbon root stays the setting (`dynamic`);
`data-density="full" | "compact"` is added, present only under `dynamic`; the groups
scroller carries `data-testid="ribbon-groups"`.

## Tests (derived from the contract)

- `ActionRibbon.test.tsx` (styles): `dynamic` renders the extended-icons shape and
  `data-density="full"` in jsdom; with a stubbed `ResizeObserver` and
  `clientWidth`/`scrollWidth` it goes compact (smaller glyph, `data-density="compact"`,
  same action ids and group names), the trailing render function receives
  `compact-icons`, and widening returns it to full; a menu opens, dispatches and closes
  under `dynamic`; a phase change resets to full.
- `useUserSettings.test.ts`: `dynamic` persists and normalizes; settings test lists five
  options; `WorkspacePageAlignment.test.tsx`: `data-action-bar="dynamic"` and the controls
  render.
- `e2e/flows/alignment-shell.spec.ts`: select `dynamic` (with the vertical phase bar, in
  System Engineering); at 1920px the ribbon is `full`; at 1400px `compact`; at 1024px the
  groups scroller has `scrollWidth > clientWidth`, the controls stay in view, the last
  action scrolls into view and is clickable, and an opened menu is `fixed`, inside the
  viewport and unoccluded, and closes when the row scrolls; back at 1920px `full`. Run
  against the local dev stack.
  *Amended 2026-10-19 during implementation:* the planned 900/640px widths are below the
  page's 1024px mobile breakpoint, where the workbench header does not render; the vertical
  column and SE's twelve actions put the three desktop widths in the three states with
  margin. Dev-stack resize sweep (2026-10-20, 2560→1024→2560): full→compact at viewport
  1291 (SE, stacked phase bars), 1510/1173/1076 (SE/BE/IE, vertical); compact→scroll at
  1050 (SE stacked) and 1269 (SE vertical); BE/IE never scroll at desktop widths. The sweep
  found a 1px direction-dependent band from a `+ 1` overflow tolerance; any overflow now
  goes compact, so the state depends only on the width.

## Limits and decisions

- One compact step, not a continuous scale; no group collapse into a dropdown.
- Native horizontal scrolling only (trackpad, shift+wheel, keyboard focus): no scroll
  arrows.
- The trailing workspace controls never scroll; a very long owner label still wraps inside
  them as today.
- Mobile keeps its own layout; the desktop-only arrangement is unchanged.
- The `simplified` overflow menu stays unimplemented (prior decision).
