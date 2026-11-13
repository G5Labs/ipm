---
name: vite-harness-browser-check
description: "Browser-verify real React components without g5-api — temporary Vite entry html from index.html plus Playwright page.route stubs for auth and the component's API calls."
metadata:
  node_type: memory
  type: reference
  originSessionId: ec5d9163-19b8-4ee4-8e62-3da69bc16ede
  modified: 2026-10-23T15:36:09.842Z
---

When a UI change needs real-browser evidence (computed colors, layout) but the page sits
deep in WorkspacePage and mock mode (VITE_MOCK) has no handlers for it, a temporary harness
is much cheaper than the [[native-stack-browser-e2e]] stack (used for #763, 2026-10-23):

- Make `frontend/<tmp-dir>/index.html` by `sed`-swapping `/src/main.tsx` in the real
  `index.html` for a `harness.tsx` that imports `../src/index.css`, sets
  `data-theme`/`data-mode` from the query string, hides `#boot-msg`, and renders the
  components inside QueryClientProvider → MemoryRouter → AuthProvider. Copying index.html
  keeps the real `<body>` style in play.
- `npx vite --port <free> --strictPort` from `frontend/`; run a `.mjs` with
  `NODE_PATH=../node_modules` using the bundled `playwright` Chromium. Serve the harness
  from a server *without* `VITE_MOCK`: mock mode substitutes its own signed-in user, so
  role-gated controls (e.g. Approve) never render. For whole-app checks run a second
  `VITE_MOCK=true` server: `/g5labs/home` renders inside the real shell, and
  `home-issues-create` opens a real Modal. Seed the theme with an init script running
  `localStorage.setItem("g5-user-settings", JSON.stringify({themeName, themeMode}))`;
  a non-JSON value silently hydrates the default editor/dark theme, so assert
  `<html data-theme data-mode>` after load.
- Theme names: editor, signal-studio, obsidian-gold, carbon-clay, alpine-graphite, darkroom,
  ink-parchment, studio-night, pm-paper, aero-glass. Aero Glass styles `body` itself
  (aurora desktop), so compare body against html for color *and* background.
- `vite build` OOMs at Node's default heap in the VM; use
  `NODE_OPTIONS=--max-old-space-size=8192` (CI's edge image sets 4096).
- Auth stubs: `POST /api/v1/auth/refresh` → `{data:{jwt}}` where the JWT payload (base64url)
  has `sub`, `org`, `scopes` (AuthProvider reads it), `iat` and `exp`; `GET /api/v1/users/me` → `{data:{id, orgs:[…], scopes:[]}}`. Reuse the
  component's vitest msw fixtures for the rest; log unstubbed routes.
- Measure a token by resolving `var(--g5-text0)` on a probe element, not by hardcoding RGB.
- Stop Vite by PID (see [[pkill-f-kills-own-shell]]) and delete the tmp dir before committing.
