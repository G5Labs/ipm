# Iteration 35: `list-installed` flat, sorted and tree reports

Batch 3 lane, product `571e04a1` unchanged. Not sealed or committed; the main thread integrates. [Model](../../../Verus_verification/consolidated/iter35/listing.rs), [cases and controls](../../../Verus_verification/consolidated/iter35/cases.json), [bindings](bindings.json), [ledger with predictions](working-ledger.json), owner [inspect](../../../ontology/features/inspection.g5.md#inspect).

## Gap

`ListInstalled`, `SortList`, `DisplayModules`, `GetListModule` and `AccumulateTreeRecursive` had no binding. The owner says only that `list-installed` lists installed modules, optionally as a tree, with Python packages or sorted.

## Predictions and observations

The owner predictions were written before any body was read. Source predictions were added afterwards.

| Case | Observed | Owner predicted | Source predicted |
|---|---|---|---|
| l0 `list-installed` | `IPM (zpm)` first, then by lowercase label; `Zeta35 (i35n)` last although its name is `i35n`; `(DeveloperMode)` on i35e | each module once with version | same as observed |
| l1 `-s v` | zpm, i35pb `1.0.0-rc.10`, i35pa `1.0.0-rc.2`, ..., i35y `1.9.0`, i35x `1.10.0` | ascending version | rc.10 before rc.2 (text) |
| l2 `-s n-d` | zpm still first, then descending label | descending | IPM pinned |
| l3 `list-installed i35b*` | only i35b | not stated | LIKE on name |
| l4 `-showsource` | root printed for every module | not stated | same |
| t0, t1 `-tree` (with `-s v`) | roots c, d, e, n, pa, pb, x, y, zpm; b under c, a under b (depth 2) and again under d (depth 1); 12 rows; `-s` ignored | dependents above dependencies | same |
| t2 after `uninstall i35a -force` | a printed twice as `[missing]`; a not a root | not stated | same |
| l5 flat after removal | a absent, others shift | absent | same |

Exploration run 1 misparsed `IPM (zpm)` and tree depth. Only the reader changed; both logs are kept under `attempts/`.

## Evidence

- Formal: `listing.rs` 13 verified, 0 errors. Six controls were each rejected: `version-prerelease-semantic`, `name-sort-internal-name`, `ipm-not-pinned`, `tree-all-roots`, `tree-missing-hidden` and `minor-compared-as-text`.
- Native: [receipt](native/20261116T173038793473Z/observations.json), 441 fields, all equal to pre-run gates, network none, source matched the pin.
- Replay: 368 fields asserted in Verus ([receipt](replay/receipt.json)). Flips of `l1.pos.i35pb`, `t2.version.i35a` and `l0.pos.i35n` were each rejected. The search (l3) and root (l4) fields are gated but not replayed.
- Command: `python3 .g5/Verus_verification/consolidated/iter35/run.py` from the repository root (231 s, mostly replay compute).

## Reach candidates

Newly reached: `Main.cls::{ListInstalled, SortList, DisplayModules, GetListModule, AccumulateTreeRecursive}`. Gaining this role: `GetListModules` and `PrintTree`. That is 5 new plus 2 strengthened, 7 distinct.

## Limits

Labels are ASCII, and each fixture module declares at most one dependency, so sibling order is not modelled. Not assessed: date sort (install times tie within a second), `-python` (it needs installed Python libraries; not exercised), `-showupstream`, `-description`, `-repository` and namespace patterns (`GetListNamespace`). IRIS subscript collation and zero-padding are premises. No ObjectScript body is verified directly.

## Discrepancies

- `-s v` (help: "Sort ... version") orders prereleases of the same version as text, so ascending `1.0.0-rc.10` comes before `1.0.0-rc.2`. This is contrary to [version.order](../../../ontology/concepts/versions.g5.md#version.order). It is display-only, so Low.
- Name sorting uses the displayed label, so a module with an external name sorts by that name. The owner is silent and the help says "by name". This is recorded as a fact, not a defect.

## Proposed owner text

- **`inspection.g5.md`, new D statement `inspect.list-installed`:** "`list-installed` prints installed modules with their versions, IPM first, then by displayed name, where a module with an external name shows as `External (name)`. A developer-mode module is marked `(DeveloperMode)`, and `-showsource` adds each module's root. `-s` sorts by name, version or date, ascending or descending, and IPM stays first. `-tree` ignores `-s`: it starts from installed modules no other installed module depends on, in name order, and prints each dependency under every module that needs it, marking an uninstalled one `[missing]`."
  - Bullet: "As built, `-s version` compares prereleases of the same version as text, so `1.0.0-rc.10` sorts before `1.0.0-rc.2` (defect C:D-next)."
  - Metadata: `agent`, `user-facing-behavior`. Model: [listing.rs](../../Verus_verification/consolidated/iter35/listing.rs) `flat_pos`, `sort_neighbors`, `tree`, `tree_neighbors`.
- **C register entry:** "`list-installed -s version` orders prereleases of one version as text", Severity Low, Decision Open. Found by: source review of `Main.cls::SortList` and iteration35 model `sort_neighbors` with control `version-prerelease-semantic`. Reproduced by: E2E reproduced. Receipt `iter35/native/20261116T173038793473Z`: `list-installed -s v` lists i35pb `1.0.0-rc.10` at position 2 and i35pa `1.0.0-rc.2` at position 3. The passing neighbour is i35y `1.9.0` before i35x `1.10.0`. A repair would compare prerelease identifiers by `SemanticVersion` order; it is unchecked.
