# Iteration 33: list-dependents report from dependency graphs

This is lane 2 of batch 2, from the 2026-11-16 Linux run. Product `571e04a1` is unchanged. The owners are [inspection](../../../ontology/features/inspection.g5.md), which has only the summary line "`list-dependents` lists modules that depend on a given module", and [deps.installed](../../../ontology/features/dependencies.g5.md#deps.installed) for the resolution consequence. Links: [Model](../../../Verus_verification/consolidated/iter33/dependents.rs), [cases and controls](../../../Verus_verification/consolidated/iter33/cases.json), [bindings](bindings.json), [ledger](working-ledger.json). Not committed; the main thread integrates.

## Predictions and exposure

The owner-based predictions were written before any body was read. The source predictions were added after reading ListDependents, PrintTree, GetDependentsList, GetDependentsAsTree, InvertDependencies and BuildAllDependencyGraphs, and before the first native run. The tree omission was not predicted from source. It was localized by a raw `GetDependentsAsTree` and graph dump (`attempts/explore2-tree-dump.log`).

## Fixture

The fixture uses directory modules `i33a` 2.0.0, `i33b`→a `>=1.0.0`, `i33c`→b, `i33d`→a `>=2.0.0`, `i33e` (no dependencies) and `i33g`→c. Filesystem repository `r33` holds a 1.0.0, b 1.0.0 and the repository-only `i33f`→a. The readers parse the printed flat lines (name, version, `(installed)` or `@ repo`, and warning lines), the printed tree lines (node count, presence, column, line, version), and the installed versions.

| Case | Observed | Owner prediction | Source prediction |
|---|---|---|---|
| `list-dependents i33a` | b, c, d, g (transitive) | dependents listed | same |
| `i33a 1.0.0` / `i33a 2.0.0` | none / b, c, d, g (version resolved in each dependent's graph) | not stated | same |
| `-t i33a` | a, then b, then c at depth 2; d and g absent | b and c shown | full inverted tree (miss) |
| `i33b` / `-t i33b` | c, g / b then c; g absent | c, g | flat same; tree miss |
| `i33d`, `-t i33d`, `i33e` | nothing | nothing | same |
| `-r r33 i33a` / `-r r33 i33a 2.0.0` | b and f `@ r33`; installed c, d, g absent / none | not stated | same |
| `uninstall i33a -force`, then `i33a` | every root is a warning: "Could not find satisfactory version of i33a ... Required by: i33b: >=1.0.0; i33d: >=2.0.0" | not stated | not predicted |
| `uninstall i33d -force`, then `i33a` / `i33a 1.0.0` | b, c, g listed; they also match 1.0.0 (resolved from r33) | not stated | same |

A directory downgrade from i33a 2.0.0 to 1.0.0 was refused ("Cannot downgrade"). That planned broken-requirement setup was therefore replaced by `uninstall -force`, as recorded in `attempts/explore1.log`.

## Evidence

- **Formal:** `dependents.rs` verified 13 units, 0 errors. Seven semantic controls were rejected:
  - tree includes roots
  - flat report lists direct dependents only
  - version filter compares the root version
  - repository mode uses installed roots
  - installed requirements ignored
  - failed graph still listed
  - three-character padding per level
- **Native:** [receipt](native/20261116T171635420146Z/observations.json) has 524 fields, every one equal to its pre-run gate. The source pin matched, and the run used image `sha256:7da4480b…` with network none.
- **Replay:** 471 fields were asserted against the model ([receipt](replay/receipt.json)). The opposite values of `l0t.count.i33d`, `l0.list.i33g` and `l2.warn.i33b` were rejected.
- **Superseded run:** run 1 (`native/20261116T171554601881Z`) passed but flipped trivial fields. It is kept in `attempts/run1-trivial-flips/`. Exploratory logs are not counted.
- **Command:** from the repository root, `python3 .g5/Verus_verification/consolidated/iter33/run.py` (23 s).

## Reach candidates

- **Newly reached:** `Main.cls::{ListDependents, PrintTree}` and `Utils/Module.cls::{GetDependentsAsTree, InvertDependencies, BuildAllDependencyGraphs}`.
- **Already linked, gaining a role:** `Utils/Module.cls::GetDependentsList`, `Storage/Module.cls::BuildDependencyGraph` (parent-edge recording) and `Storage/Module.cls::ProcessSingleDependencyIterative` (installed requirements at the reader).

## Limits

- Each fixture module declares at most one dependency. Diamonds, cycles, sibling order inside a tree, the SQL order of roots, prerelease, snapshot and partial versions, and HTTP/ORAS repositories are unassessed.
- Tree line-index replay holds for chain-shaped trees only.
- SemanticVersion comparison is a provider premise.
- No ObjectScript body is verified directly.

## Discrepancies

1. **The `-t` tree omits every module that depends on the target as a graph root.** That covers all direct dependents, and every top-level dependent, which appears only if it is an intermediate in another root's graph. The command help says "show as a tree (rather than a flattened list)" and gives no hint that roots disappear. The flat and tree forms disagree on the same graph: b, c, d, g versus b, c. This is a reachable defect (Medium: visible misbehavior in a report). The cause is that BuildDependencyGraph records no parent edge for a root's direct dependency, and InvertDependencies relies on those edges.
2. **With the target uninstalled, one stricter installed requirement turns every dependent into a warning.** This follows from deps.installed (iteration31), so no new defect is proposed. The report then cannot answer "what depends on X" at all, which the owner may want stated.

## Proposed owner text

- **`inspection.g5.md`, new D statement `inspect.list-dependents`:** "`list-dependents <module> [<version>]` lists every installed module whose dependency graph contains the module, directly or through other modules, with the dependent's own version. A version argument keeps only dependents whose graph resolves the module to that version. With `-r <repos>`, it lists repository manifests instead and ignores installed modules. A dependent whose graph cannot be resolved is printed as a warning instead of a dependent."
  - Its metadata should read `agent`, `user-facing-behavior` and `Verus-scoped`, with Model: [dependents.rs](../../Verus_verification/consolidated/iter33/dependents.rs) `listed`, `warned`, `flat_neighbors`, `repo_neighbors`, `removed_target_neighbors`.
- **Added bullet:** "As built, `-t` prints only the module and the modules between it and a top-level dependent; it never prints a module that depends on it as the top of a graph, so direct dependents are missing (defect C:D-next)." The model is `tree_omits_roots`.
- **Proposed register entry (C register):** "`list-dependents -t` leaves out every top-level and direct dependent", Severity Medium, Decision Open, Found by source review plus iteration33 model `tree_omits_roots`, control `tree-includes-roots`.
  - Reproduced by E2E: receipt `iter33/native/20261116T171635420146Z`. `list-dependents -t i33a` prints a, b, c while the flat form lists b, c, d, g. The passing neighbour is the flat report, and `-t i33d` (no dependents) prints nothing. A repair would record the root as the parent of each direct dependency, or seed the inversion with each graph root, and it is unchecked.
- **Optional deps.installed bullet:** "The same check makes `list-dependents` report every dependent as a warning when the module is absent and the namespace's requirements on it cannot all be met by any offer."
