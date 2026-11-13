# HTML preview exception — 2026-11-09

After PR #1188 merged, the owner requested a follow-up PR exempting `.html`,
`.htm`, `.xhtml` and `.shtml` from the preview-size limit, like PDFs and raster
images such as PNG/JPEG. Before implementing, audit whether PDF/PNG/JPEG really
have no limit.

The agent's audit confirmed that those viewers bypass the 1-MiB preview gate,
but still have the shared 32-MiB read ceiling and separate pixel safeguards.
The implementation applies the same preview-gate exception to the four HTML
extensions while retaining read admission, source editing and the HTML sandbox.
This paragraph records the agent's interpretation, not an additional owner decision.
