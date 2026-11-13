# Preview live indicator and run log mock

Sample-data prototype (2026-10-20) for two changes to the workspace preview
control (`frontend/src/components/preview/PreviewControl.tsx`). It does not
call G5.

1. **Trigger indicator.** The ribbon's Preview tile (and the status band's
   Preview button) shows whether a preview is running without opening the
   dialog: the monitor glyph turns "on". Three indicator variants are
   compared: A lit screen (recommended), B corner dot, C glyph swap. States:
   none, starting, serving, closing, last failed.
2. **Run log.** The Workspace preview dialog gets a Log toggle beside Close
   preview. It opens a stage strip (Queued → Starting → Building →
   Serving → Closing → Closed) and the run's lines, from the preview task's
   timeline. The failure notice links to the same log.

Open `index.html` in a browser. The toolbar switches the indicator variant,
the preview state, the action bar style (extended icons, minimal icons,
text), the theme and reduced motion. Query parameters preselect the same
(for example `?variant=B&state=starting&theme=dark`).

`mock.fragment.html` is the editable prototype source; `index.html` is the
same page with a document skeleton. The notes under the frames list the data
sources and the assumptions to confirm before implementation.

Snapshots (rendered from `index.html`):

- `trigger-A-serving.png`, `trigger-A-starting-dark.png`, `trigger-minimal-A-serving.png`
- `states-A.png`, `states-B.png`, `states-C.png`
- `dialog-starting.png`, `dialog-serving.png`, `dialog-closing.png`, `dialog-failed.png`
