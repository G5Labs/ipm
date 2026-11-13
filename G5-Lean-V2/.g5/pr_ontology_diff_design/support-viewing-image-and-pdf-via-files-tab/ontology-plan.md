# View images and PDFs from FILES

Work for [issue #982](https://github.com/G5Labs/G5-Lean/issues/982) on the assigned
`support-viewing-image-and-pdf-via-files-tab` branch, authorized by the owner's
[request](../../chat_intent/support-viewing-image-and-pdf-via-files-tab/request.md).
The owner's two issue comments are the product decision: an image viewer on the
browser's own decoding and a PDF viewer on Mozilla PDF.js. This plan records the
owners, the guarantees relied on, the choices the comments left open and the evidence.
No backend route, permission, store or Save change is introduced.

## Governing decisions

[Viewers](../../ontology/features/documents.g5.md#g5.documents.viewers),
[image viewer](../../ontology/features/documents.g5.md#g5.documents.viewers.images) and
[PDF viewer](../../ontology/features/documents.g5.md#g5.documents.viewers.pdf) are new
and expand [editor modes](../../ontology/features/documents.g5.md#g5.documents.editor.modes),
which now names them and SVG's rendered mode. They rely on
[file identity](../../ontology/features/documents.g5.md#g5.documents.identity),
[one editing experience per file](../../ontology/features/documents.g5.md#g5.documents.editor.experience),
[bounded reads](../../ontology/features/documents.g5.md#g5.documents.read.bounds),
[drafts](../../ontology/features/documents.g5.md#g5.documents.files-view.drafts) (SVG only)
and [owner](../../ontology/features/workspaces.g5.md#g5.workspace.owner-read) /
[non-owner](../../ontology/features/workspaces.g5.md#g5.workspace.durable-read) read
authority, all unchanged.

## Ownership and reuse

- **The read.** `WorkspacePage` already loads the opened file through
  `useWorkspaceFileContent`, which resolves the actor to the live checkout or the
  committed cut, refuses a file above 32 MiB from its metadata (`FILE_TOO_LARGE`, 413)
  before reading bytes, and returns binary bytes base64-encoded. The viewers consume
  that same `WorkspaceFileContent`; nothing fetches bytes by another route, so the
  authorization, live-versus-committed selection and ceiling are the reader's, not the
  viewer's. The page's error branch names the ceiling for a refused file instead of the
  generic failure.
- **Classing.** `fileText.ts` owns which files open where: `fileViewerKind(path, file)`
  returns `pdf`, `image`, `text`, `binary` or `too-large`, from the extension for PDFs
  and raster images and from the existing text eligibility otherwise. `FileView`
  dispatches on it before the binary card, as the issue asks; `TextFileView` is
  unchanged except that an SVG gets an image preview as its `renderPreview` so the
  rendered/plain switch, drafts, Edit, Suggest changes and Save work as for HTML.
- **Bytes to URLs.** `fileBytes.ts` (new, pure plus one hook) decodes base64 to bytes,
  builds a typed `Blob` and gives an object URL whose lifetime is the React effect's:
  created when the blob changes, revoked on replacement and unmount. The Markdown
  `WorkspaceImage` moves from a `data:` URL onto this hook, so Markdown and standalone
  viewing share one image path (the issue's reuse condition).
- **Image viewer.** `ImageFileView.tsx` owns zoom state and the failure card; it never
  touches drafts or Save. The header probe `imageDimensions.ts` (pure) reads PNG, JPEG,
  GIF, BMP and WebP headers so the viewer can refuse a decode above the pixel bound
  without starting it.
- **PDF viewer.** `PdfFileView.tsx` owns the document lifecycle (loading task, worker,
  page proxies, render tasks, text layers, search cache) and the toolbar state;
  `pdfjs.ts` owns how PDF.js is loaded: a runtime import of the emitted build, the
  worker beside it, the asset base URL and the fixed document options. A small Vite plugin in `vite.config.ts`
  emits the character maps, standard fonts, image-decoder WebAssembly and license
  files from the pinned package into `assets/pdfjs/<version>/`, which the edge already
  serves immutable and hashed per version.

## Operation

### Dispatch

Opening a file resolves the tab as today. With content loaded, `FileView` classes it:
a `.pdf` shows the PDF viewer; a raster extension shows the image viewer whatever the
bytes hold (a text file misnamed `.png` reaches the viewer, fails to decode and offers
Download, which is the designed outcome for corrupt or misnamed files); an SVG that is
valid text opens as text with the image as its rendered mode; anything else keeps the
existing text view or the binary card. The viewers receive the file's name, bytes and
version (`hash`, `source`) and nothing of the editing props. The page's `key` already
remounts the view on principal, workspace, repository or path change, so a user or
workspace change discards the old viewer with its resources.

### Image viewer

The viewer builds a `Blob` of the file's MIME type (by extension) and an object URL.
Before assigning it to the `<img>`, it probes the header; a declared size above
64 megapixels (8192×8192, about 256 MiB decoded) shows a "too large to display" card
with the dimensions and Download. Otherwise the `<img>` loads; `load` records the
natural size and `error` shows the failure card with Download. Zoom is one of fit
(scale to the panel, capped at 1), actual (1) or a number between 0.05 and 8 stepped by
×1.25; the stage is a scroll container that centres a smaller image and scrolls a larger
one; a `ResizeObserver` keeps fit current. Transparency shows over a theme-aware
checkerboard. Animation and EXIF orientation are the browser's own behaviour for `<img>`.
SVG, whether from a text draft or from bytes, is shown the same way; an `<img>` is an
image context in which scripts, event handlers and external loads never run.

### PDF viewer

On mount or a new file version, the viewer loads the PDF.js chunk, creates one
`PDFWorker` for this document and calls `getDocument` with the bytes and the fixed
options (character maps, standard fonts and WebAssembly from G5's assets, XFA off, no
scripting since no annotation layer or scripting manager is instantiated). Progress is
shown with a Cancel; the cleanup of the effect destroys the loading task and the worker,
and a guard flag discards any late result. `onPassword` opens a password form: a
response resends the password, a wrong one re-asks with the reason, Cancel destroys the
task and shows the cancelled card with Download and Try again. Load failures map
`InvalidPDFException` to "not a valid PDF" and show other messages verbatim, with
Download.

Pages lay out vertically with page 1's size as every page's placeholder until it loads.
An `IntersectionObserver` with one viewport of margin marks pages near view; only those
fetch their page, render to a canvas (output scale = min(devicePixelRatio, 2), lowered so
width × height × scale² ≤ 16 megapixels) and render a PDF.js `TextLayer` for selection and
search. Leaving the margin cancels the render task and releases the canvas and text
layer; a zoom or rotation change re-renders only pages near view. Fit width is the
opening zoom and follows the container; zoom presets, rotation by 90°, a page field with
previous/next and the current page from scroll position complete the toolbar. Search
fetches each page's text content on demand (cached, abandoned when the term changes),
counts text runs containing the term, highlights them in rendered text layers and steps
through them across pages; a document with no text reports no matches.

## Capacity and limits

- 32 MiB per file, by the shared reader; base64 transport adds a third in flight. Larger
  files are refused before bytes load and cannot be downloaded from the tab until a
  bounded streaming download exists (not in scope).
- Image decode bound: 64 megapixels by header probe (PNG, JPEG, GIF, BMP, WebP). AVIF,
  ICO and APNG without a probe rely on the browser; this is an accepted gap.
- PDF: 16 megapixels per page canvas; pages near view only; one worker per document.
  Embedded images inside a PDF are resized by PDF.js's own canvas-area logic.
- Bundle: the PDF.js main build (about 0.5 MB minified) and worker (about 1.3 MB) are
  fetched from the versioned asset path only when a PDF opens, cached immutable
  afterwards; assets (about 4 MB of maps, fonts and WebAssembly) are fetched by the
  worker on demand, per document need. G5's own bundle does not grow.

## Choices and alternatives

- **Native image viewer, no gesture library.** The owner preferred starting with the
  small viewer and adding `react-zoom-pan-pinch` only if smooth drag, pinch and
  trackpad gestures are required; none are requested, so no dependency is added.
- **PDF.js compatibility build.** The modern build is tested upstream on current
  Firefox and Chrome only; the compatibility build documents Chrome/Edge 125+, Firefox
  ESR and Safari 18+ for about 60 KB more, which matches the browsers the owner named.
- **Per-document worker** rather than one shared worker: closing or replacing a
  document releases all of its memory, which the owner's release condition asks for,
  at the cost of starting a worker per open (cached script, well under a second).
- **No annotation layer.** Rendering links and forms would add link handling and the
  scripting surface the 2026 advisory concerns; the owner's control list does not
  include them. Documented as inert; a later change can add a link layer.
- **Search semantics.** A match is a text run (PDF.js text item) containing the term,
  which keeps highlighting exact and avoids reconstructing line breaks; terms spanning
  two runs are missed. This is stated in the ontology.
- **Classing by extension, not by sniffing bytes.** Sniffing would open a `.txt`
  holding PNG bytes as an image and hide misnamed files; the extension is what the
  person sees in FILES, and a wrong one lands on the failure card with Download.
- **No comments on viewed files.** The comment model anchors to lines; images and PDFs
  have none. The viewers show no Comments control, as the binary card shows none today.

## Licensing

`pdfjs-dist` 6.4.299 is Apache-2.0; the bundled chunks keep its `@licstart` notice and
the emitted asset directory carries the package `LICENSE` plus the decoder notices
(`LICENSE_JBIG2`, `LICENSE_OPENJPEG`, `LICENSE_QCMS`, the standard-font licenses). The
package is used as published, with no modified upstream files. The scripting sandbox
(`quickjs-eval.*`) is excluded from the emitted assets. No image-decoder library is
added for images.

## Verification

- Unit (vitest): classing and MIME tables; the header probe on crafted PNG, JPEG, GIF,
  BMP and WebP headers and on truncated input; object URL creation and revocation on
  change and unmount; the image viewer's fit/actual/zoom arithmetic, failure and
  too-large cards; the PDF viewer against a mocked `pdfjs` module for load, progress,
  cancel, password retry and Cancel, invalid document, replacement discarding a late
  result, visible-page rendering and release, canvas cap, rotation, zoom and search;
  `FileView` dispatch for PDF, PNG, misnamed PNG, SVG text and the unchanged binary and
  text paths; the Markdown image through the shared hook.
- Browser (Playwright, Chromium, local): a harness page mounting `FileView` on generated
  fixtures (transparent PNG, animated GIF, a 900-megapixel-by-header PNG, a corrupt PNG,
  an SVG with script and `onload`, a three-page text PDF, a sixty-page PDF, an
  RC4-protected PDF, a page without text, a corrupt PDF) checking real decoding, the
  checkerboard, zoom geometry, that the SVG runs nothing, PDF page rendering with
  standard fonts, search, rotation, the password flow and the failure cards.
- Flows (CI stack): write a PNG and a PDF through the authorized file API, click them
  in FILES and see the rendered viewers, which also proves the worker and asset paths
  are served by the edge.
- Real Firefox, Safari on macOS and Edge on Windows are not available here; they remain
  the open validation named in the ontology.

## Self-review

The design adds no state outside the viewers' own React state and no second content
authority; the reader's admission, authority and ceiling are reused rather than
reconstructed. Lifecycles are tied to effects whose cleanups cancel and release, and a
stale completion can only ever be discarded. The material risks are a browser decoding
difference (mitigated by the failure card and Download), asset paths that differ between
the dev server, the build and the edge (covered by the flows test through Caddy) and
memory from large pages or images (bounded above). SVG stays editable text so the shared
draft and suggestion rules are untouched.

## Implementation and verification — 2026-10-31

Implemented as designed on this branch, frontend only:

- `fileText.ts` classes reads (`fileViewerKind`, `IMAGE_MIME_TYPES`); `fileBytes.ts`
  decodes reads, builds typed blobs and owns object URLs (`useObjectUrl`);
  `imageDimensions.ts` probes PNG, JPEG, GIF, BMP and WebP headers; `imageZoom.ts`
  and `pdfGeometry.ts` hold the viewers' bounds and arithmetic; `FileNoticeCard.tsx`
  is the shared card with Download.
- `ImageFileView.tsx` and `PdfFileView.tsx` are the viewers; `pdfjs.ts` imports the
  pinned compatibility build at runtime from the emitted asset path and points it at
  the worker there, so nothing of PDF.js is bundled or re-minified (the shipped files
  are upstream's, banner included) and the build's memory is untouched; the version
  reaches it through the plugin's `virtual:g5-pdfjs` module; `pdfTextLayer.css` is the verbatim
  `.textLayer` block of `pdf_viewer.css` 6.4.299 (listed in `.prettierignore` so it
  stays byte-identical); `fileViewers.css` holds G5's own viewer rules.
- `FileView.tsx` dispatches on the classing before the binary card and gives an SVG
  draft the image as its rendered mode; `MarkdownPreview.tsx`'s `WorkspaceImage` uses
  the shared blob and object-URL path; `WorkspacePage.tsx` names the read ceiling when
  the reader refuses a file (`FILE_TOO_LARGE`).
- `vite.config.ts` gains `pdfjsAssets`, which emits `assets/pdfjs/<version>/` (the
  build as `pdf.min.js` and worker as `pdf.worker.min.js`, named `.js` so every edge
  serves them as JavaScript; character maps, standard fonts, ICC profile, image-decoder
  WebAssembly; the package and decoder license files; without `quickjs-eval.*`) and
  serves the same paths in dev.
- `pdfjs-dist` 6.4.299 (Apache-2.0) is the only new dependency, pinned exactly; its
  optional `@napi-rs/canvas` (Node only) enters the lock file but never the bundle.

Evidence:

- Unit (vitest, jsdom): `fileText`, `imageDimensions`, `fileBytes`, `imageZoom`,
  `pdfGeometry`, `ImageFileView` (6), `PdfFileView` (9, against a controllable
  PDF.js stand-in: worker and copied bytes, visible-page rendering and release,
  progress and Cancel, password retry and Cancel, invalid document, replacement
  discarding a late result, unmount destroying the task, zoom and rotation
  re-rendering only visible pages, page navigation, search across pages, no text) and
  `FileView` dispatch (5); the Markdown image test now checks the typed blob. The
  existing `FileView` suite (72) still passes.
- Browser (Playwright, Chromium 1208, local Vite dev server): the ten cases in
  `e2e/browser/file-viewers.spec.ts` pass against the generated fixtures in
  `e2e/fixtures/viewers` (`generate.py`, standard library only; the protected PDF is
  the standard security handler, RC4 40-bit, password `secret`): transparent PNG over
  the checkerboard with zoom and Download, a 1600×1200 image fitting then scrolling,
  an animated GIF, the 900-megapixel header refused before decoding, a corrupt PNG and
  a misnamed text file on the failure card, the scripted SVG rendering as an image with
  its script never running and its source editable, the three-page PDF rendering ink
  with standard fonts from the asset path, selectable text, search stepping across
  pages, navigation, zoom, rotation and Download, the sixty-page PDF rendering fewer
  than ten pages and releasing page 1 after a jump to page 60, the protected PDF
  refusing `wrong`, opening with `secret` and keeping Download after Cancel, the
  text-less PDF finding nothing, and the corrupt PDF on the failure card.
- Flows (CI stack): `e2e/flows/files-viewers.spec.ts` writes a PNG and a PDF through
  the file API, clicks them in FILES and expects the rendered viewers, which proves the
  worker and asset paths through the edge; it runs in g5-ci-e2e, not locally.
- The whole frontend unit suite passed locally (365 files, 5,594 tests), as did
  `eslint src/` (no errors; the warnings are pre-existing) and `tsc`. `vite build`
  succeeded with `NODE_OPTIONS=--max-old-space-size=4096`: this VM's default 2 GiB
  heap is too small for the unchanged base commit as well, so the limit is the VM's,
  not this change's. The output has no PDF.js chunk; `assets/pdfjs/6.4.299/` holds
  `pdf.min.js` and `pdf.worker.min.js` byte-identical to the package files, the maps,
  fonts, profile and decoders, and the license files, without `quickjs-eval.*`.
- CI (g5-ci-rust, g5-ci-e2e) results are recorded on the PR.

Gaps and follow-ups:

- Firefox, Safari on macOS and Edge on Windows were not exercised; the ontology keeps
  their support as pending a person's validation.
- Links and forms inside PDFs are inert (no annotation layer); comments are not offered
  on viewed files; search misses a term split across two text runs; AVIF, ICO and APNG
  have no header probe. All are stated in the ontology.
- A file above 32 MiB cannot be downloaded from the tab; the page says so.
