# Frozen ordinary temporal neighbors

| Stage/request | Cache/API/search | Current physical source | Install result | Current module/show/code |
|---|---|---|---|---|
| initial/stable requested1 |1.0.0/source-A|1.0.0/payload101|success|1.0.0/source-A/101|
| completed ordinary file replacement before install |1.0.0/source-A|2.0.0/payload202|no compile/load yet|prior1.0.0/source-A/101 retained|
| then requested1 |1.0.0/source-A|2.0.0/payload202|success|2.0.0/source-B/202|
| same-repo normal reconfiguration |2.0.0/source-B|2.0.0/payload202|rebuildsuccess|prior2/202 retained|
| after rebuild requested1 |2.0.0/source-B|2.0.0/payload202|notfound refusal|prior2/202 retained|
| after rebuild requested2 |2.0.0/source-B|2.0.0/payload202|success|2.0.0/source-B/202|

Stable and valid-new requested-version equality are1; stale requested1 equality is0. Cache and physical source are separately observed, not a supplied final Boolean. Native runs89/107/116/118fields all pass; final20261115T220610978852Z explicitlyreads the postcopy/prereload state1/payload101. Observer-config mutation error before third run's CLI extension is preserved; final118 includes the corrected exact bits. This is a known builder-selected temporal source lead from19's explicitly unassessed mutable-source boundary, not independent heldout evidence. Concurrent/partial file replacement, removed files, failed providers, other versions/ranges, changing module names and history shape remain unassessed.
