# Iteration 13 observations

| Ordered request | Command | Earlier valid target configuration / fresh reader |
|---|---|---|
| Valid then missing, preview | Error | No entries; unavailable |
| Missing then valid | Error | No entries; unavailable |
| Valid then missing | Error | Package/routine entries present; unavailable |
| Retry pending valid target | Success | Entries present; available; version succeeds |
| Different valid target after pending failure | Success | New target available; old pending target remains unavailable |

Every command returns transaction level0. Fresh unavailable readers throw a missing-class exception. Configured state is observed within the running instance; restart durability and generic provider failures remain open. The whole-config publication prediction was a material miss retained in diagnostics.
