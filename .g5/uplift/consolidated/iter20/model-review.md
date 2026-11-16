# Ordinary class ownership and orphan reader

This lane derives the selected `orphans -type CLS` output from physical class availability, normal parsed module declarations and naming fallback. It distinguishes a resource reference from a reference whose Module is an object, and distinguishes both from naming-convention inference. The checked behavior conforms; no new defect is asserted. Untyped mapped/system inventory noise remains inherited **P:F-UNIN-7**.

Product pin `571e04a19aba9f9b4bc0ea23fbe09182038f4e55`; parent baseline20 `b290e20eac078ff42a074fa62e65ef9e51f3171ade359885e4d0c03f4ae85d0e` (14217 members); prior19 seal `8374c347...` clean verified. Only the authorized model/cases/model-lane evidence and this note were written. Parent owns native execution, canonical statements, registers, source-map audit and integration. No product/native/harness/Docker/Git edits or jobs, evaluator-private reads or archive investigations were performed by this lane. Archive paths remain **UNASSESSED/UNPURSUED**.

## Predictions and producer correction

The earlier independent pre-body survey predicted that a free ordinary class would be visible, while a normal unrelated-name module declaring the exact class would omit it without removing its callable code. Existing inspection proofs cover stored-name/alias lookup, not inventory→ownership→visible-name subtraction. Deciding-source inspection additionally found a second pass inferring a home from the longest module/class prefix. Its no-resource module writer can change the final reader while explicit references and code remain unchanged. Later exact declaration should take precedence over inference even though both states omit the class.

Initial native `20261115T213339481447Z` exposed an important producer miss: inventory and CLI names end in lowercase `.cls`, while the observer initially compared uppercase `.CLS`. Its rejected presence bits were an observer-case error, not empty inventory. The raw session/fixture are retained by the parent. `FindHomeModule` checks the extension against uppercase `CLS` literally; it must not silently normalize that branch. The formal source was written with distinct Lower/Upper inputs after this calibration.

Corrected native `20261115T213633510810Z` records125 independent fields. Actual inventory includes both controlled lower-suffix classes throughout. Their payload methods each return1 throughout:

| State | Normal producer | Visible loose / named | Lower-input home loose / named |
| --- | --- | --- | --- |
| free | direct ordinary OBJ.Load compiles both |1 /1|none / none|
| claimed | module `iter20-claim` declares `Iter20Loose.Checks.CLS` |0 /1|iter20-claim / none|
| naming | empty module `iter20named`, no resources |0 /0|iter20-claim / iter20named|
| exact | module `iter20-exact` declares `Iter20Named.Loose.CLS` |0 /0|iter20-claim / iter20-exact|

Lowercase naming fallback returns resource name `Iter20Named.Loose.cls` and no reference. Direct uppercase input returns the same inferred module but resource `Iter20Named.PKG`, still with no reference. After exact declaration both inputs return the explicit module/reference; the resource output preserves each input's suffix case. Lowercase NonNull lookup matches the stored uppercase declaration under the selected SQL collation port. Native separately checks the reference's actual Module object, inferred home, returned reference/resource name, GetHomeModuleName, physical code/payload and actual CLI membership.

The naming module has zero ResourceReference rows. Native selected child membership counts are1 for each normal claimed reference. Model child names represent canonical lookup-equivalence membership, not raw OBJ.Load list/array byte equality. Actual reference IDs are opaque composite strings such as `2||63`; model ordinals0/1 identify only the selected logical references. No allocator or raw-ID equality is claimed.

## Model and source domain

`World` contains the selected physical code map, stored module names and resource rows. `ParsedModule` supplies valid parsed resources; `import_module` derives the module and each reference's owner relationship. The separate `document_loaded` writer derives physical code from a typed source and replaces child lookup names. Successful XML import/relationship save/OBJ.Load/compile and returned names remain provider premises; actual native fields qualify them. Generic XML/ObjectScript parsing, load failure/rollback and ownership collisions are unassessed.

`lookup_ref` first searches UniqueName and only then Children. The UniqueName query does not filter on Module existence: a null-owner unique row shadows a child match for that candidate. `owner_object` separately checks the owner/store object. Reference search visits exact class then ancestor package only for uppercase `CLS`; lowercase `.cls` repeats its exact name and bypasses that CLS-specific package/stub branch. SQL selected-name collation is modeled separately by `lookup_key`, leaving the extension decision intact. Ambiguous child-index matches, arbitrary SQL order/collation and resource null-name/custom-processor overrides are outside the domain.

The model is bounded to two-component normal nonstub unmapped source classes `Iter20Loose.Checks` and `Iter20Named.Loose`. Prefix candidates are derived from those selected identifiers, longest first, with a literal normalized-name port matching actual NameExists behavior. This is not a general identifier parser or datatype normalization proof. First pass exhausts resource candidates before naming fallback; returned reference state may persist even from an unowned row. Pure nullable, package, UniqueName-vs-Children and longest-prefix neighbors test those distinctions without injecting runtime rows. Their reachability under other ordinary workflows is not newly qualified by these native cases.

`inventory` is the complete provider projection for these two controlled ordinary classes. It does not represent all namespace inventory, counts, item order, other resource types, mapped/system/generated/stub filtering or arbitrary namespace arguments. `execute` filters those provided names by actual home-object absence. `fetch` and `drain` preserve the selected row stream; normal consumption reaches first AtEnd and closes the temporary cursor. `printed` projects Main's actual name lines. Fresh successful single-cursor use is required; overlapping/abandoned/stale cursors, after-end restart and error paths are unassessed. Pure cursor ordering is conditional on its provider input, not a portable inventory ordering claim.

No existing model is imported merely to inflate reuse. Earlier inspection/ownership contracts remain unchanged and valid within their own domains; this adds the missing final-reader relation. Parent separately corrected its initial `Document.Load` locator to the actual `Document.OnLoad`, retaining that miss. Locator presence does not establish whole-member coverage.

## Source roles

| Role | Narrow correspondence |
| --- | --- |
| parsed resource/owner writer | `Storage.Module.Resources:36`, XML complete child relationship; ordinary module import supplies the declared resource and owning module |
| physical/children writer | `Default.Document.OnLoad:398-416` calls OBJ.Load and passes returned children; `ResourceReference.UpdateChildrenFromString:242-253` clears, sets and saves Children |
| exact index before child index | `ResourceReference.NonNullResourceNameExists:336-340`; no Module-is-not-null filter |
| object-owned resource search | `ExtensionBase.Utils.FindHomeModule:85-104,112-114`, exact CLS/ancestor PKG only in uppercase extension branch; normal source/stub exclusion is a provider domain |
| naming inference and returned fields | `FindHomeModule:106-114`, longest class/module prefix after resource pass; returning a home does not create a reference |
| module-name final reader | `GetHomeModuleName:32-35` projects only an object home name |
| inventory and filtering | `Utils.Module.OrphanedResourcesExecute:2151-2165`, actual `*.CLS` StudioOpenDialog call and absence-of-home-object decision |
| cursor and user output | `OrphanedResourcesFetch:2181-2185`, `Close:2193-2195`, `Main.ListOrphans:3270-3275` prints returned Names |

These are hand-translated specification contracts plus scoped native/source correspondence. They are not mechanical ObjectScript/framework/provider body proofs.

## Proofs, controls and retained attempts

The complete final candidate verifies16 units,0 errors, with no inherited model units. Seven focused unchanged-source witnesses each verify exactly one unit. Twenty-four local mutants reject semantically with no resource/time/recursion-limit credit. Each selected witness unfolds definitions directly and calls no proof lemmas/broadcasts, local or imported. Controls use `--verify-root --verify-function <selector> --multiple-errors 0`; the whole positive baseline uses default scope/options.

Fetch advancement has its own witness that never calls recursive `drain`: a nonadvancing-fetch mutant cannot supply an unverified recursive drain equation to its own control. Other controls distinguish nullable owner, UniqueName/Children priority, explicit/package/naming priority, extension/collation behavior, returned reference retention, false empty-module claims, child replacement, physical code/payload, orphan filtering and cursor output/closure.

Initial positive source/log pairs retain the unproved Upper naming-resource fact. Staged assertions exposed missing literal module-name distinctness; only byte-literal inequalities are computed, never full Maps/Worlds. Baseline3 then passed14 units. A first control runner attempt passed the then15-unit baseline and six focused positives, but failed compiling its empty inherited stub because vstd was omitted. That harness failure and exact source/config/receipt are retained under `attempt1-empty-import`, without control credit. The corrected zero-import stub verifies0 units.

First110-alias replay failed with16 verified/one failed proof unit on empty-resource versus nonempty-name facts, with no limits. Exact generated source/log/receipt/cases are retained under `attempt2-replay`. Only bounded literal nonempty/distinctness and empty-resource definition facts were added to replay support; ownership/inventory/writer rules and already-clean control inputs stayed unchanged. The exact control cases snapshot is retained under `controls/config.json`.

Final replay verifies **17 units,0 errors**, covering110 modeled aliases plus15 gated qualifications against all125 native fields. Its first three aliases are actual visibility Booleans `free-visible-loose=1`, `claimed-visible-loose=0`, `naming-visible-named=0`. Direct formal suite wall42.117s; final replay12.185s. The control-input digest is unchanged by replay-only support: `62c1899445ba62fd0b26085d12467ba375d3e7bcd16d041736f8ab5db5921854`.

Frozen model SHA256: `1220936bf5bc70914bbb589706084715506dff7a6fdbb04875dbed7a74dd9860`; final cases SHA256: `fd75cdd67623db00483c349c0ee2b0d322bf581e42ecfac5371c1469f7851494`. Native observation SHA256: `a93cfe06f7a710de89564d77f8da99c1dc23cff6c15edeffe6fb6a8d1530e1d5`. Exact receipts are `model-lane/controls/receipt.json`, `replay-receipt.json` and `final-summary.json`. Native execution belongs to parent and used actual image `sha256:8db58b93cf76245f6cc9cfc16aaa9627391535eab6350134ecc1bbf8bcf8b82f`. No new trusted declarations or ObjectScript implementation proofs were added.

No repository-wide convergence follows. The source-only extension/package neighbor warrants its own ordinary producer/native qualification before any defect or general package-policy claim.
