# Workbench header styles

Source: [chat intent](../../chat_intent/header-style-settings/request.md) (2026-10-09), after
the reviewed mock page.

## Ontology changes

| Block | Change | Why |
|---|---|---|
| `g5.workbench.header-styles` (new D, workbench) | Two per-person settings, "Phase bar" (`extended` · `minimal` · `vertical`, default `minimal`) and "Action bar" (`extended-icons` · `minimal-icons` · `extended-text` · `simplified`, default `extended-icons`), beside Workspace panels, offered only while the alignment layout is; action icons are client presentation keyed by action id with target/kind fallbacks; no branch chip. | The user chose these styles and defaults on the mock page. |
| `g5.workbench.gate` (amended) | The instance default layout is `alignment` (server fallback and hosted QA/dev/test); CI keeps `both`; the feature-off fallback to `advanced` is unchanged. | "Set also the default for Workspace panels=alignment." |
| `g5.workbench.actions` (amended) | The ribbon shows no branch chip; the ship/merge target is named by the surfaces those actions open. | "Remove the branch chip unconditionally." |

## Governing ontology

- [g5.workbench.gate](../../ontology/features/workbench.g5.md#offer-the-alignment-ui-as-a-workspace-panels-layout):
  the new settings only affect the `alignment` layout, so they are offered under the same
  gate and resolved from the same per-person settings store.
- [g5.workbench.phases](../../ontology/features/workbench.g5.md#three-phases-organize-a-workspace)
  and [g5.workbench.phases.state](../../ontology/features/workbench.g5.md#phase-is-explicit-workspace-state):
  every style keeps the three phases, the done/active/upcoming states, the owner-only move
  and the approval meters; styles change presentation only.
- [g5.workbench.actions](../../ontology/features/workbench.g5.md#ribbon-actions-are-templated-conversations):
  the action set stays data; icons must not require a schema change to `actions.yaml`.
- [g5.workspace.ui.center-tabs](../../ontology/features/workspaces.g5.md): the precedent for
  a per-person layout setting (type, default, normalization, settings row, test seeding).

## High-level design

- **Settings owner**: `UserSettings.phaseBarStyle` and `UserSettings.actionBarStyle`
  (client-local, normalized like `sidebarDisplayMode`; unknown stored values fall back to the
  defaults). Two selects under Workspace panels on the settings page, rendered while the
  `alignment` option is offered.
- **PhaseChevrons** takes `variant`. `extended` is today's bar; `minimal` is the same bar
  without the deliverable subline; `vertical` is a column of three short right-pointing
  chevrons (phase icon + full name, check when done) that the page places at the left of the
  action-bar row, so no top row remains. The refusal line and approval meters render in all
  three.
- **ActionRibbon** takes `variant` and loses `branchName`. `extended-text` is today's grouped
  text buttons with a leading icon; `extended-icons` is the UXv2 column button (22px glyph,
  label under it) with divider-separated groups and no captions; `minimal-icons` is icon-only
  with the label as tooltip and accessible name; `simplified` is one wrapping row of
  icon-plus-label buttons with dividers. Groups keep `role="group"` and their accessible
  name in every variant. The preview trigger (`PreviewControl`) accepts the variant's icon
  size and label visibility so it matches its neighbours.
- **Icons**: `components/workbench/workbenchIcons.ts` maps built-in ids to lucide glyphs and
  falls back by `target`, then by `kind`; committed custom actions therefore always render.
- **Instance default**: `default_workspace_layout()` accepts `alignment` and falls back to it;
  `defaultWorkspaceLayoutFrom` matches; hosted QA, dev and test values become `alignment`.
  `resolveWorkspaceLayout` is unchanged, so an instance without `alignment_ui` still lands on
  `advanced`.

## Decision notes

- Simplified has no overflow menu: the row wraps instead. An overflow needs width
  measurement; deferred until a narrow pane shows it is needed.
- The vertical column drops the ribbon's full-width banners to the ribbon's width; accepted.
- CI's `G5_DEFAULT_WORKSPACE_LAYOUT=both` stays so classic-layout flows keep their default;
  the alignment flows select the layout explicitly.
- Acceptance: settings rows persist and normalize; each variant renders the same action ids,
  group names and phase buttons under the same test ids; no `ribbon-branch` anywhere; with the
  feature off the settings select still shows a valid option when the default is `alignment`.
