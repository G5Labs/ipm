# Make the G5 development image fully warm for toolchains and dependencies

## User instructions

From direct chat on 2026-10-08 (background job, branch `kaihui/g5dev-warm-toolchain`):

- "can you take a look at all qa vm errors that related to missing env or deps, try to
  include all of them in the qa g5 image, so every time it should be fully warm for tool
  chains and deps. give me a report first"
- "i mean in G5 lean on G5 lean env. like this org https://qa.g5labs.ai/g5/Base for the vm"
- "can you audit the current latest g5-lean code base again, and also take [the Slack
  thread] as reference. Propose a new fix plan"
- "implement the fix"

Team input relayed from the Slack thread (2026-10-08, 11:32–11:48):

- Zhengchun: PR #335 "fixed those already in `/var/lib/g5d/bin`"; "please go ahead to fix
  the rest, especially 1, 2 and 3 that i saw them happened even yesterday"; "better to
  also ask your agent to valid with current main b4 action".
- Ben: "Maybe don't add gh? might confuse agents that try to reach out to the remote repo".
- Kaihui: agreed that gh need not be added if agents are not supposed to call it.

## Agent interpretation

The requested outcome is that a Base-org workspace VM booting the `g5d-g5dev` image can
run the repository's Rust, Node and browser test workflows without per-VM installs,
downloads or hand-exported environment. The report and plan live in the shared doc
"QA Guest VM Error Summary and Fix Plan" (Claude Docs artifact
`8f24337e-3db4-4e70-a0d2-f3fec8b3639f`), revised against main `795d1ab6`.

"The fix" is read as Phase 1 of that plan: the image-only change for errors #1, #2, #3
and #12 (rustup proxies failing without `RUSTUP_HOME`, hand-installed unpinned
toolchains, missing rustfmt/clippy on the bundle fallback, cold compiles because the
baked sccache cache is never used) plus the baked Playwright browsers being invisible
at the agent's default lookup path. Validation against main established:

- PR #335 (merged 2026-10-06, in worker image `g5-worker-v20261008-1023-d93c080`)
  appends `/opt/g5/bin` after the image's directories, so bundle tools resolve, but the
  image's `/usr/local/bin/cargo` rustup proxy still wins and still fails; the bundle's
  Rust lacks rustfmt and clippy.
- `g5d-dispatch/src/modules/runner.rs:258` still clears the environment;
  `git.rs::process_environment` supplies only PATH, HOME and Git identity to modules,
  terminals and (through the module) previews. The image ENV is sourced into the
  dispatcher at boot, so a runtime passthrough is a small follow-up (Phase 2), shipped
  by a worker-image bake plus host rotation, not by this image change.
- `.cargo/config.toml` already defaults `DATABASE_URL`, `TEST_DATABASE_URL` and
  `SQLX_OFFLINE`, so no repository env change is needed.
- Per Ben, gh is not added to the image. It stays reachable from the bundle under #335.

Out of scope here and tracked in the doc: Phase 2 (runtime passthrough), Phase 3
(wider cargo prime, node_modules tarball, libkrun closure, test-DB autostart, preview
Docker archives) and Phase 4 (repository hygiene).
