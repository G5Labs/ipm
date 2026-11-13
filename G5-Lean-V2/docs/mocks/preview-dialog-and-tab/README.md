# Preview dialog states and the preview app tab

Sample-data prototype (2026-10-26) for issue
[G5Labs/G5-Lean#786](https://github.com/G5Labs/G5-Lean/issues/786): the
Workspace preview dialog's controls and layout, and opening the preview in a
new top-level G5 app tab. It does not call G5.

1. **One dialog shell.** The dialog content renders straight into the shared
   `Modal` (`frontend/src/components/ui/Modal.tsx`): no inner bordered card,
   no fixed inner width, so no horizontal scrollbar. The × is "Close dialog"
   and keeps the preview running; the stop action is labelled **Stop preview**.
2. **Clear control states.** Open preview is green only while the preview can
   open. Starting and stopping render it on the neutral surface with full
   contrast muted text and no reliance on opacity; two cues are compared for
   the starting state: **A** the button's label becomes "Starting…" with a
   spinner (recommended), **B** the label stays and a lifecycle pill sits in
   the header. Stop preview is an outlined, full-contrast button with a red
   stop glyph while starting and serving, and shows "Stopping…" while the
   stop runs. A `role=status` line spells the lifecycle out.
3. **The run log from PR #693** (stage strip and the read-only hosted
   terminal) sits under the actions when Log is open; the dialog widens to the
   `lg` size and the terminal is the one scroll region. At 360 px the action
   row wraps and nothing overflows.
4. **Open preview opens a G5 app tab** beside the workspace's tab, with the
   lit monitor glyph and the label "Preview · <workspace>". The tab's page is
   a slim bar (workspace, profile, port, state pill, Reload, Open in browser,
   Stop preview) over the application, loaded from its isolated preview
   origin. Closing the tab never stops the preview; after Stop the tab shows
   "The preview stopped" with a way back and a restart.

Status (2026-10-26): the dialog half with cue **A** is implemented in
`PreviewControl.tsx` on this branch (`g5.preview.dialog-controls`). The app
tab half was implemented and reverted the same day at the owner's request;
its design and the owner's answers to the open questions are kept in
`.g5/pr_ontology_diff_design/preview-ux-improvement/` and
`.g5/chat_intent/preview-ux-improvement/`.

Open `index.html` in a browser. The toolbar switches the dialog state
(starting, serving, stopping, last failed), the log, the not-ready
cue (A/B), the viewport width (wide, 360 px), the tab state (workspace,
preview open, preview stopped), the theme and reduced motion. Query
parameters preselect the same (for example
`?state=starting&log=open&width=narrow&theme=dark`).

`mock.fragment.html` is the editable prototype source; `index.html` is the
same page with a document skeleton. The notes under the frames list what
changes, the proposed tab behaviour and the assumptions to confirm before
implementation; the main one is loading the isolated preview origin inside
the G5 page (iframe, third-party handoff cookie, frame-ancestors), with
"Open in browser" as the fallback.

The issue's two screenshots (the current QA dialog and the existing tab strip)
are GitHub user attachments that need a signed-in session, so the "before"
state is reproduced from the current `PreviewControl.tsx` rather than copied.

Snapshots (rendered from `index.html` at 2x, reduced motion):

- `dialog-starting.png`, `dialog-starting-B.png`, `dialog-serving.png`,
  `dialog-stopping.png`, `dialog-failed.png`,
  `dialog-serving-dark.png`
- `dialog-serving-log.png`, `dialog-starting-log-narrow.png`
- `tabs-before.png`, `tabs-preview-open.png`, `tabs-preview-stopped.png`,
  `tabs-preview-open-dark.png`
