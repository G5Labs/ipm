# Personal workspace tab avatar mock

Sample-data prototype (2026-10-29) for showing the owner's avatar on a personal
workspace tab in the shell's tab strip. It does not call G5.

Today the strip gives each workspace tab its phase icon from the organization's
workspace list (`g5.workbench.tabs`). A personal workspace is not in that list,
so its tab renders an empty 13 px glyph slot. The proposal fills that slot with
the owner's round avatar: the OAuth picture, or the deterministic
initial-and-tint fallback that `IdentityAvatar` already uses on Home.

Open `index.html` in a browser. The toolbar compares Before (today) with After,
switches light and dark themes, photo versus initial fallback, 14 px versus
16 px avatars, and which tab is active. The zoomed row below the frame shows the
glyph variants at 2×. The notes list what changes and the assumptions to
confirm before implementation.

`mock.fragment.html` is the editable prototype source; `index.html` is the same
page with a document skeleton. `after-light.png`, `after-dark.png`,
`after-initial.png` and `before-light.png` are element snapshots of the frame
rendered with the frontend's bundled Playwright Chromium.

Code touched by the implementation: `frontend/src/components/layout/WorkspaceTabStrip.tsx`
(`WorkspaceTabIcon`, the personal branch of the `listed` lookup), reusing
`frontend/src/components/shared/IdentityAvatar.tsx` and the signed-in user from
`useAuth`.
