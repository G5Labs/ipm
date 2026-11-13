# Human intent: private agent memory in the personal repository

Source: this workspace's conversation with the owner (zcl-g5), 2026-10-27.

## Human direction

- Problem: "today Claude Code agent was instructed to write memory to folder
  `.g5/memory/claude/` as it writes to the code repo, it basically got shared with
  everyone in the org." The owner asked for the feasibility of writing to
  `/work/personal/memory/claude/` instead, with the code and harness changes needed.
- After the feasibility report, the owner decided:
  - "memory is best effort, it is fine if we lose it in some corner cases"
  - "we want to go fully private"
  - "in terms of path, it should be agent provider dependent, claude code goes to
    `/work/personal/memory/claude/`, codex should go to `/work/personal/memory/codex/`"
  - "I understand Codex is disable, but for instruction of path, we placeholder for codex"
  - "How memory is saved, it should follow the same commit as to code repo"; asked when
    G5 should commit and push the personal memory, the owner chose "Every code push": each
    time the VM pushes the code repository (the agent's own `git push`, Save, Create or
    Update PR), G5 commits only the memory folder in `/work/personal` and pushes it, best
    effort; memory written after the last code push is lost at Stop.
  - "Existing shared memories: do not touch them", then "please COPY whatever I (zcl-g5)
    commit to my personal folder/repo".

## Agent interpretation

- "Fully private" replaces the human-provided story that shared native memory with
  teammates through the codebase's Git. Memory is now per owner, in their personal
  repository, readable under that repository's audience (the owner and current
  organization administrators).
- Where no personal repository is checked out (formal review workspaces, or Start left
  the mount out), memory is off for that launch instead of falling back to a workspace
  repository, which would share it again.
- "Every code push" covers every successful branch push of a workspace repository from
  the VM. Flush captures are not code pushes.
- Codex native memory stays disabled (the earlier owner decision recorded in
  [agent-memory](../g5-lean/agent-memory.md#codex-memory-decision)); its folder is only
  named in the instructions and covered by the same commit step.
- The one-time copy was done by hand, not by the platform: the 26 files under the code
  repository's `.g5/memory/claude/` with commits from zhengchun.liu@g5labs.ai (25 the
  owner created, plus one Ben created and the owner edited twice) were copied byte for
  byte to the personal repository with an index of their shared `MEMORY.md` lines
  (personal commit `29dc56b`). The shared copies are unchanged.
