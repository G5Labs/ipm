# Iteration 22 consolidated Verus runbook

Work from the IPM repository root at product pin `571e04a19aba9f9b4bc0ea23fbe09182038f4e55`. The product is unchanged. [Handoff](../../../uplift/consolidated/iter22/HANDOFF.md), [trust](TRUST.md), [bindings](../../../uplift/consolidated/iter22/bindings.json) and the [complete project map](../../../uplift/consolidated/iter22/project-map.json) distinguish source locators from semantic scope and executable-body verification.

Maintain each link in its owning manifest/iteration binding, then regenerate the complete map. It preserves legacy and nested boundary authority scopes, evidence and partial withdrawals, discovers unregistered inputs and reconciles canonical model targets. Earlier seals stay immutable. Public scoring guide `8442346f...` applies; evaluator-owned private workflow/member cases and seeds were not searched or read.

```sh
python3 .g5/uplift/consolidated/iter22/project_map.py
python3 .g5/uplift/consolidated/iter22/project_map.py --check
python3 .g5/uplift/consolidated/iter22/project_map.py --formal reinstall.rs::installed_request
python3 .g5/uplift/consolidated/iter22/project_map.py --member 'Main.cls::Reinstall'
python3 .g5/Verus_verification/consolidated/iter22/run.py
python3 .g5/Verus_verification/consolidated/iter22/run.py --no-native
python3 .g5/uplift/consolidated/iter22/validate.py /absolute/path/to/ipm-iter22/baseline/members.json
python3 .g5/Verus_verification/consolidated/iter22/freshness.py check
```

Forward lookup gives exact pinned declarations, ranges, hashes and roles for deriving a request from the installed record. Reverse lookup lists every registered formal claim affecting the caller. Full positive verification precedes focused direct-definition wrong-rule controls, unchanged-source positive neighbors, native observation replay and true-opposite field controls. Run/model-lane receipts give exact counts and raw verdicts. Direct ObjectScript executable-body proofs: zero.

The native fixture reuses the exact 16/18 module, class and root literals in a new private container; artifacts remain under iteration 22 and runtime copy is `/tmp/iter16`. Alpha offers versions 1 and 2, beta offers 1. Qualified beta 1 is followed by unqualified reinstall selecting older alpha 1; disabling alpha selects beta 1. Deleting both definitions leaves files/current code present, but default/dev reinstall refuses. Missing installed name refuses before delegation. Four successful history snapshots and original command strings are independently selected by watermarks; two delegated failures each create one failed row, exposed through public filtered `history find`. Early missing-name creates none. Logical record labels are not native IDs.

Normal filesystem/cache/parser/compile/save/logging/SQL providers and stable sources are required. No dependencies or successful developer/current-version-2 loads are newly qualified; snapshot flags are source configuration only. Failed history fields beyond count/action/failure and summary error are unassessed. Failed detail-status encoding remains a separate ordinary reader lead. An observer printed encoded status bytes as UTF-8 and failed; exact inputs are retained, partial bytes were not saved by the wrapper, and corrected Boolean output was re-executed.

Freshness hashes all production files/additions and transitive formal/tool/config/map/native inputs. It separates theorem reuse from product correspondence and demonstrates disposable source/formal/new-writer changes without executing them. Archive-path behavior remains unassessed and unpursued, including investigation, reproduction and probing. Only exact permitted source methods were inspected in this iteration.
