# Discovery runs excluded from credit

The initial probe was not counted. Its one-line ObjectScript `TSTART SET` and conditional `QUIT ... HANG` commands did not compile. The fixed class's `ck` compile and LOADOK=1 are present in accepted receipts.

Initial repository fixtures reused names or roots. `%Save` returned a duplicate ServerDefinitionKey error in some rows; subsequent lock/cache observations from those rows were excluded. Accepted pairs use unique names and roots, verify save/cache success, and repeat twice.

An initial `Read` method opened HistoryTemp as an object to inspect its package. The observer's own `%OnClose` then persisted/deleted that temporary row. `native/processloss.log` and `native/restart.log` are those contaminated trials and are excluded. The corrected reader uses `%ExistsId` and opens only the permanent row. `processloss2.log`, `restart2.log`, and the reproducible fresh replay use the corrected reader.

The first complete reproducible replay captured fixture hashes at the end while the probe gained an additional permanent final reader. Its `fixture-provenance-correction.json` retains the exact loaded source and identifies the difference. A subsequent replay freezes inputs before copying them into its disposable container.

No product file or iter 1 receipt was edited to repair these probes. Native errors, setup failures and observer effects are not treated as product defects or passes.
