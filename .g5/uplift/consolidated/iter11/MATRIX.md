# Iteration 11 observations

| Schedule/provider | Entry count / payload | Caller/persisted result |
|---|---|---|
| A enters, then B with different module lock | 2 / 1 | Both success; B saves homeB then A saves homeA |
| A and B serial, same row | 1 / 1 | B skips successful complete step but registers homeB |
| Second caller uses same module as held A | 1 / 1 | Second returns success after timeout, no entry |
| Shared-lock model variant | Second excluded | Completion/progress repair unproved |
| Class editing lock, calibrated direct delete | Error, class retained | Full uninstall warns/succeeds, metadata gone, code callable |
| Unlocked class | Class removed | Uninstall success |
| Initial insertion race/callback error/other delete error | Unassessed | No generalized prediction |
