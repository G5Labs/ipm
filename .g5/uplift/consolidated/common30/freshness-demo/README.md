# Freshness gate demonstration (continuation run, 2026-11-16)

Command: `python3 .g5/uplift/consolidated/common30/freshness.py [--json OUT]`, run from a clean `git archive HEAD` extraction. Each world below is a separate disposable copy in `/tmp`, removed afterwards. The product checkout was not modified.

| World | Change | Exit | What the gate reopened |
|---|---|---|---|
| [base](base.json) | none | 0 | nothing |
| [formal](formal.json) | iteration32 `sync.rs`: `nothing` ignores deletions (the `nothing-ignores-deletes` control text) | 1 | iteration32 theorem only; correspondence unaffected |
| [source](source.json) | one byte appended to `src/cls/IPM/General/Sync/Pipeline.cls` | 1 | iteration32 correspondence for the six bound Pipeline members; theorem unaffected |
| [newfile](newfile.json) | new production class `General/Sync/NewWriter.cls` | 1 | census: one added production file, which reopens the subsystem because a new writer or reader may be unbound |

A theorem-stale result is resolved by rerunning `run.py --no-native` for that iteration. The harness reuses the native receipt only when the native fixture bytes are unchanged. A correspondence-stale result needs the deciding source reread and a fresh native run (`run.py`). The gate compares whole-file hashes, so any edit to a bound file conservatively reopens every binding in that file.
