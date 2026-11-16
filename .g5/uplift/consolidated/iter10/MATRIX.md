# Iteration 10 state and command distinctions

| Input/state | Operation | Observation |
|---|---|---|
| Successful class child hook | Package / uninstall | Package succeeds; uninstall removes class/metadata |
| Returned child hook error | Package | Error propagates |
| Same returned error | Uninstall | Success, absent module/manifest, callable surviving class |
| Declared Verify, returned Test scope | Resolve with Test / Verify / both | Path absent / absent / present; no Scope metadata leak |
| Fresh unset / flag / true manifest / explicit command0 | Package/extract | Selected file absent / present / present / absent |
| Stored true, reload omits field after command0 | Package/extract | Stored true and selected bytes return |
| Stored true, explicit false manifest | Package/extract | Stored false; no selected file without flag |
| Arbitrary hook/provider writes, decoder failure, concurrent update steps | Any | Open beyond this contract |
