# Iteration 9 state and command distinctions

| State/input | Command | Observation |
|---|---|---|
| Package, unclaimed child | Uninstall | Selected class deleted, module absent |
| Exact child/nested package claimed by other module | Uninstall parent | Other code remains, nested class callable |
| Child separately declared by same module | Expand parent / uninstall | Parent excludes it; full traversal deletes it |
| Both step kinds seeded at home A | Delete A | Both persisted rows gone |
| Same keys registered by B | Delete A / B | Both survive A, disappear with B |
| Failed save/delete, concurrent step writers, mirror | Any | Not established here |
