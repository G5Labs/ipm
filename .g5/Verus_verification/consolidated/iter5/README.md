# Iteration 5 contracts

Start from the canonical owners: [web applications](../../../ontology/concepts/modules.g5.md#module.webapp), [blocked uninstall](../../../ontology/features/uninstall.g5.md#uninstall.dependents.as-built), [shell entry forms](../../../ontology/root.g5.md#ipm.shell.forms) and [registry redirects](../../../ontology/concepts/repositories.g5.md#repo.remote.download.redirect). The [evidence index](../../../uplift/consolidated/iter5/INDEX.json) binds each owner to the pinned member that decides the behavior and the members that call or read it.

From the IPM root, in a checkout or an extracted seal:

```sh
python3 .g5/uplift/consolidated/iter5/run_all.py [path/to/ipm-iter5/baseline/members.json]
```

It runs `run_models.py` (contracts and wrong variants), `run_native.py` (fresh disposable container `ipm-iter5-native-<stamp>` from `ipm-consolidated-iris:571e04a1-fresh`, removed afterwards), `replay_native.py` (observed fields as Verus assertions with one flipped field per model) and the validator. It needs pinned Verus `0.2026.09.27.3cf1832` (`VERUS` overrides the path) and Docker. The native step reads the pinned source hashes from `../iter4/native/pin-sources.json`.

| File | What it represents |
|---|---|
| `webapp.rs` | Create or adopt on install, declared-key merge, duplicate-name refusal, transaction rollback, blocked and forced uninstall with the proposed check-first repair, orphan after upgrade, the unreached Validate check |
| `entry_forms.rs` | Exit code and status visibility by entry form, with the function form derived from the generator's case-sensitive match |
| `redirect.rs` | Credentials on each redirect hop; iteration 4's single-URL proposal versus a per-hop rule |
| `native/` | Fixture modules (`wa`-`wg2`, plus a copy of iteration 4's `tmod`), the probe, a failing invoke class and the redirect-capable registry provider |

[TRUST.md](TRUST.md) states each representation boundary. Implementation proofs: 0.
