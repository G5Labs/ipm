# Actual selected report readers

| Ordinary workflow | Shell/device | JSON file | Stored raw entries |
|---|---|---|---|
| Passing outer alone | Success, run1, 1 method passes, 3 entries pass | Outer/run1, same tree | 2 assertion calls plus passed duration LogMessage |
| Failing outer alone | Error2 failure(s), run2, 1 failed method, 4 entries/2fail | Outer/run2, failed | Passed precondition,2 failed calls, passed duration |
| Failing outer invokes passing child | Child success/run4 then outer error/run3; outer device has2 failures | Outer filename contains child/run4/pass; exactly equals child file | Outer3 failed rows remain; child4 passed rows remain |
| Explicit ToFile(path,outer3) comparison | Existing command/status unchanged | Outer3/failed tree, equals stored outer tree | No framework state seeded or changed |

Provider allocates outer3 before child4; child finishes first. Tracking follows completion, file default follows highest persisted ID. IDs are fixture observations, not universal constants. Initial2/3 total prediction was wrong; duration rows account for corrected3/4 totals. File JSON contains full trees; nonverbose device JSON has summary and failed assertion rows. Empty method error category suppresses the vendor aggregate message as a separate error row.
