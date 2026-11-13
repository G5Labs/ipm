---
name: ui-mocks-convention-and-snapshots
description: UI mocks live in docs/mocks/<topic>/ (README, index.html, mock.fragment.html, PNGs); render PNGs with the frontend's bundled Playwright Chromium, and put system-ui last in the font stack.
metadata:
  type: project
---

UI design mocks are self-contained HTML pages under `docs/mocks/<topic>/` with a README,
`index.html` (document skeleton) and `mock.fragment.html` (the same page from `<title>`
to `</script>`), using the G5 editor theme tokens; optional PNG snapshots sit beside them.
Snapshots can be rendered in the VM via a small `.mjs` script and element screenshots.
After a VM reset `frontend/node_modules` is gone but `~/.cache/ms-playwright/chromium-NNNN`
survives: `npm i playwright-core@<version in frontend/package-lock.json>` in a scratch dir
(e.g. /tmp/pw) and launch with `executablePath` pointing at that Chromium binary.

**Why:** The owner asked for "mocks first" on 2026-10-20 (preview live indicator); the
existing titlebar-removal and external-issues mocks set this convention.

**How to apply:** Reuse the token block from an existing mock. In the VM `system-ui`
resolves to a monospace font, so list `-apple-system, "Segoe UI", Roboto, Arial,
"Liberation Sans"` before `system-ui` or the PNGs render in mono. Only Liberation and
Unifont families are installed.
