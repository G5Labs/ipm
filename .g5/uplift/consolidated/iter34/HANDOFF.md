# Iteration 34: filesystem repository version order and the install choice

Batch 3, product `571e04a1` unchanged. [Model](../../../Verus_verification/consolidated/iter34/versions.rs), [cases and controls](../../../Verus_verification/consolidated/iter34/cases.json), [bindings](bindings.json), [ledger with predictions](working-ledger.json). Owners: [version.order](../../../ontology/concepts/versions.g5.md#version.order) and [repo.search-order](../../../ontology/concepts/repositories.g5.md#repo.search-order).

## Gap

Iteration21 bound the filesystem cache query but left order, ranges, prereleases and multiple rows conditional. Iteration31 relied on the premise that a filesystem repository lists candidates highest first. The producer of that order (`SemanticVersion.Index` and `Find`, 21 unbound members) and the consumer's re-sort were unbound.

## Predictions and observations

| Case | Observed | Owner predicted | Source predicted |
|---|---|---|---|
| q1/q2 cache query, releases 1.9.0, 1.10.0 (+2.0.0-beta.1 with the prerelease parameter) | 1.10.0 before 1.9.0; beta first when included | semantic order | same |
| q3/q5 cache query, rc.2/rc.10 and 2.0.0/2.0.0-beta.1 | rc.10 first; 2.0.0 first | same | same |
| a2/a6 `install i34b` (repository created by `repo -fs`) | 2.0.0-beta.1 | 1.10.0 if prereleases are skipped | first: filtered by the repository flag. Refuted: `repo -fs` stores prereleases on. Second: beta |
| a3/a7 `install i34b 1.x` | 1.10.0 | 1.10.0 | 1.10.0 |
| a4/a5 exact `2.0.0-beta.1` | installed | installed | first: not found (refuted); second: installed |
| c1 `install i34c` | 1.0.0-rc.2 | rc.10 | second round: rc.2 (text order) |
| d1 `install i34d` | 2.0.0-beta.1 | 2.0.0 | second round: beta |
| d2 `install i34d 2.0.0` | 2.0.0 | 2.0.0 | 2.0.0 |
| a8 `install i34b >=1.0` | not found | (C:D6) | (C:D6) |

Exploratory run 1 refuted two source predictions. New repositories default to prereleases and snapshots on, and Main.Install consumes `moduleSqlToList`'s re-sorted first row, not the index order. Both were re-predicted before exploratory run 3, which added module i34d. The owner statement version.range already says the prerelease skip applies only to repositories that do not offer prereleases, so a2/a6 are as owned. a8 is the inherited C:D6 partial-version defect and is gated but not replayed.

## Evidence

- Formal: `versions.rs` 12 verified, 0 errors. The generic lemmas are `releases_agree` (any two releases: index, SQL and semantic order agree, which keeps iteration31's premise for releases) and `release_versus_own_prerelease` (any release and its own prerelease: semantic and index put the release first, SQL puts the prerelease first). Six controls were each rejected: `sql-prerelease-semantic`, `sql-release-first`, `index-numbers-as-text`, `index-release-marker-low`, `last-row-kept` and `prerelease-flag-ignored`.
- Native: [receipt](native/20261116T172512256458Z/observations.json), 53 fields, all gated, network none, source bytes matched the pin.
- Replay: 36 fields in Verus ([receipt](replay/receipt.json)). Flips of `d1.version`, `c1.version` and `q5.pos.7` were each rejected.
- Command: `python3 .g5/Verus_verification/consolidated/iter34/run.py` (17 s).

## Findings

- [C:D39](../BUGS.md#c-d39), High, E2E reproduced: a filesystem repository install picks a prerelease over its own release and orders prerelease numbers as text. Discovered in this search.

## Reach

[Project map](project-map.json): 433/1216 (iteration33: 425, none lost). Newly reached: `Index.GetIndexGlobalNode`, `Index.Insert`, `Find.PreviousItem`, `Find.ContainsItem`, `Find.%OnNew`, `Find.MatchesExecute`, `Find.MatchesFetch` and `SemanticVersion.FromListBuild`. Strengthened with this role: `Cache.OrderedMatchesExecute/Fetch`, `Repo/Utils.moduleSqlToList`, `Filesystem/PackageService.ListModules`, `Manager.SearchRepositoriesForModule` and `Main.Install`. That gives 8 new plus 6 strengthened, 14 distinct.

## Limits

No build metadata or snapshots. Words are single-character codes. IRIS subscript collation and the SQL ordering of the version columns are native-qualified premises fixed by observed rows. `Index.Delete` was not exercised. ORAS and remote repositories use other listing code and are not covered. No ObjectScript body is verified directly.
