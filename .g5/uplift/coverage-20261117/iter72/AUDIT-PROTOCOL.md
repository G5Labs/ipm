# Iteration72 rule-4a re-audit protocol

Every obligation graded `full` at `fca3ed0f` gets one ledger entry. Seven auditors (G1-G7, by page group) applied this protocol, and the builder re-judged a sample of each group.

## Questions per full obligation

Open each cited declaration (and the deciding product code when the model's fidelity is in doubt).

1. **Deciding value.** Which model expression produces the outcome the obligation observes? Is it computed from the product state and inputs the obligation names?
   - Computed: a function of the named state or inputs that mirrors the product's deciding branch, loop or guard.
   - Written down (rule 4a, so partial): a constant; a per-kind or per-case outcome table that summarises emergent product behaviour (transaction scope, a loop or guard, a sequence of calls); a fixture-fixed field; a parameter that is the decision itself (for example `succeeded: bool`, `rolled_back: bool`, `circular_error` that no branch sets).
   - A literal table or constant in the product's own deciding code (an extension-to-folder `$case`, a fixed default string) may be transcribed. Cite the product line. Its entries are the computation.
2. **Cases exercised.** Does a theorem, neighbour assertion or rejected control exercise every case the obligation states, on both sides of each contrast? A spec case never exercised is partial.
3. **Mutation.** Would removing or altering the deciding product branch as modelled make a cited theorem, neighbour assertion or recorded control fail? If the model has no counterpart of the product's deciding branch, there is nothing to mutate and the obligation is partial.
4. **Scope.** If the obligation claims a general rule and the model only asserts fixture instances, keep full only when the instances cover every case the obligation states. Mark it `instance_scoped`.

## Support kinds for a kept full

- `mutation-checked`: the auditor ran a mutation of the deciding computation with `iter72/mutate.py` and it was killed. A mutation that survives is a finding. Either the obligation becomes partial, or the entry explains why the surviving mutant is not a meaningful change.
- `control`: an existing recorded control (playbook `controls/*.json`, crate `control-NN` features or review mutants, a consolidated iteration's attempts or controls) mutates the deciding computation and is recorded as rejected. Cite it.
- `inspection`: judged by reading only. Say why no mutation was run (for example a crate needing cargo, or a run longer than 60 s).

## Ledger

`iter72/audit/<page-slug>.json`:

```json
{"page": ".g5/ontology/features/x.g5.md", "auditor": "G3",
 "entries": [{"id": "x.y#1", "verdict": "full" | "partial",
   "support": "mutation-checked" | "control" | "inspection",
   "deciding": "<model decl> computes <value> from <inputs>",
   "product": "src/cls/IPM/...cls:NN (when opened)",
   "mutations": ["G3-m04"], "control": "<path and control name>",
   "instance_scoped": false,
   "gap": "<missing distinction> (required when verdict is partial)",
   "note": "<short>"}]}
```

Mutations are logged by `mutate.py` to `iter72/mutations/<group>.jsonl`. Scratch copies are under `/tmp/ipmcov72-<group>/`.
