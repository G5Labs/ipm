# Persistent Codex memory

On 2026-10-28 the owner requested persistent Codex memory across G5 chats,
suggesting `codex exec --enable memories` and
`codex exec resume --enable memories`, with reads and writes under
`/work/personal/memory/codex/`.

This supersedes the earlier decision to keep Codex memory disabled. It extends
the existing private personal-repository memory binding and best-effort sync
after code pushes. Native credentials, session history and SQLite state stay
in the private Codex home. Formal reviews and launches without a personal
repository keep memory off. Native generation timing remains Codex's own;
enabling memory does not guarantee a save at the end of each turn.
