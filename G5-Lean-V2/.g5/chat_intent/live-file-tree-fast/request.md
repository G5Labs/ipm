# Files tab slow with a live VM

## User instructions (2026-10-07)

- "can you check why it is so slow to load G5-Lean files in file tab when VM is alive"
  (qa.g5labs.ai, workspace preview-test).
- After the diagnosis: "implement the fix".

## Agent interpretation

Diagnosis on QA: the attached owner's Files tree walked the VM checkout one directory
per runtime call (`fs.watch` + `fs.list`, each paying admission against the database),
about 600 serial calls and 32.5 s for G5-Lean. Every `workspace.file_changed`
WebSocket event (one per written path, including build output under `target/`)
invalidated the tree and Changes queries, which aborts an in-flight fetch, so an agent
`cargo build` in the VM kept the tree from ever completing (394 of 400 requests in 24 h
aborted by the browser).

The fix the user asked for, as interpreted:

1. The VM lists the whole subtree in one `fs.tree` runtime call, applying the
   checkout's `.gitignore` files and keeping tracked paths, bounded by the entry cap.
   The API keeps one watch on the listed root.
2. The browser coalesces `workspace.file_changed` invalidations per workspace so a
   burst refreshes once per interval and never cancels a fetch in flight.
3. A VM still running the previous dispatcher falls back to the committed cut with a
   labeled reason instead of failing the panel.

After the deploy the user reported "it is still very slow" with a screenshot of both
repositories stuck on "Loading files…" (2026-10-08). The remaining wait was the
Changes report the tree used for its head-commit label; the follow-up makes the VM
return the checkout HEAD with the listing so the tree never waits on it.

Review round 3 (Codex) showed the root watch itself was refused for repositories with
`node_modules/` or `target/` (too many directories), so the guest now registers watches
only for the directories the tree walks, pruned by the same `.gitignore` rules. Build
output is neither shown nor watched.
