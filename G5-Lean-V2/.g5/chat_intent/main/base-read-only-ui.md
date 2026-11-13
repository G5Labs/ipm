# Base read-only workspace presentation and organization landing

## User request

> When user open Base (the ancestor workspace without a parent, not just named as Base), as Base is readonly we should
>
> 1. Do not show Chat tab/page/window as Base is read only for every one.
> 2. Instead of shows “Workspace is stopped. Start it to edit; durable files remain available for browsing.”, please show “Read-only workspace. Fork workspace to make edits.”
> 3. Do not show Owner, VM: Stopped, or Fork and open a terminal.

The user requested a design for these requirements.

## Organization landing follow-up

> also, if a user opens https://{qa}.g5labs.ai/{org-slug}, it should by default open page https://{qa}.g5labs.ai/{org-slug}/home

## Design interpretation

Base is identified by canonical workspace ancestry. The presentation applies to every authorized reader, including the recorded owner and organization administrators. A child workspace named `Base` retains normal working-workspace behavior.

Chat suppression covers navigation, direct workspace Chat routes and retained Chat panels/windows. A Base Chat destination falls back to the existing durable file/document browser. The ordinary authorized Fork workflow remains available; hiding the Owner display does not change stored ownership, read grants or administrator transfer authority. These details are design decisions derived from the request and existing ontology.

Organization-root navigation defaults to Home on the same origin and for the requested organization. The design also normalizes a trailing slash, replaces the root history entry, preserves query/fragment data and retains existing login/access checks and explicit child destinations. These are agent routing decisions within the requested default behavior.

The implementation trace found that `isBase` is the registry's protected-root identity: organization creation sets it and checked forks clear it while recording a source. Migration 039 preserves the same-organization clone-parent link with ON DELETE NO ACTION, and removal rejects live children. Base classification uses the canonical registry flag rather than display name, list visibility or incomplete ancestry metadata. The trace also found that Home redirects to Base when `g5_ui_v3` is absent. The design treats the requested Home default as unconditional and keeps Home available independently of that flag, with other feature gates retaining their scope.
