# Lock-file graph and writer boundary

The selection ledger was frozen after canonical `deps.lockfile` and product `LockFile` tests were read, before the deciding graph and writer bodies. See native/lockfile/selection.json. No private evaluator was consulted. The canonical owner already promises resolved versions and omitted credentials, but does not specify duplicate-name insertion, flat-list cursor order or serialized key order. The inherited thin-surfaces model covers status/file survival, not this producer-to-JSON content path.

## Sampled workflows and predictions

- Product `Test09Module2DependenciesSameTransient` (K): the common A dependency should occur once before C/E. The actual graph produces A,B,C,E. Versions in saved JSON should match installed module records.
- Product `Test10ComplexNestedDependencies` (L): the graph should produce A,B,C,D,E,F with no duplicate A/B/E entries. Sibling order was unanswered by the canonical owner before source inspection.
- A new reversed-name filesystem fixture: aa-parent depends on zz-leaf. The actual flat producer should return zz-leaf,aa-parent. The JSON array-of-objects provider's key traversal is a separate boundary; natural Name traversal predicts aa-parent,zz-leaf. This serialized-order prediction is unconfirmed until the parent's native run. No current lock-file installer exists, so an artifact-order difference would not establish a live install-order failure.
- Cursor challenge: aa depends on cc, while bb/cc/dd have no dependencies. The flat producer continues its cursor after visits and must produce bb,cc,dd,aa, rather than restarting a smallest-ready sort after removing cc.
- Direct writer challenge: append a duplicate A reference with forged version 999.0.0 and repository name. CreateLockFileForModule must re-open the installed A by Name, retain one named dependency and serialize the installed version/repository. This helper is separate from the ordinary CLI graph producer, which already has unique named nodes.
- Direct repository export: use synthetic credential values with an actual persisted remote definition and no HTTP request. The subclass's named JSON mapping should retain its URL and omit Username/Password/Token fields. GetRepo itself returns the live definition, including credentials; it is not the sanitizing operation. Arbitrary secrets embedded in allowed URL text are outside this whitelist claim.

## Source roles

| Member | Role and formal relation |
|---|---|
| Utils/Module.cls::LoadNewModule | Sets top-level LockFileModule and calls LoadDependencies. Existing Root/timing defects remain separate. |
| Storage/Module.cls::BuildDependencyGraph | Resolved provider graph has module-name keys and dependent-name edges. Generic resolution, versions and raw list parsing remain inherited/conditional boundaries. |
| Utils/Module.cls::LoadDependencies | Constructs the inverted graph, flattens it, loads dependencies and invokes the lock writer only for the named top-level module. |
| Utils/Module.cls::ConstructInvertedDependencyGraph | `invert` reverses dependency-to-dependent edges into each dependent's remaining requirements. The unrelated private InvertDependencies is not credited. |
| Utils/Module.cls::GetFlatDependencyListFromInvertedDependencyGraph | `after`, `next`, `blocked`, `remove`, `walk`: ordered cursor traversal, ready-node selection and destructive removal. Finite fuel reports incomplete remainder explicitly. |
| General/LockFile.cls::CreateLockFileForModule | `refs`, `add_dependency`, `write_dependencies`, `build`: use only each flat reference Name to re-open installed modules; capture exact stored version/repository and direct requirement expressions; SetAt under module Name. |
| General/LockFile.cls::AddRepositoryToLockFile | `add_repo`: keep one repository by its name. |
| General/LockFile.cls::GetRepo | Repository lookup succeeds only for a defined name; the returned object remains credential-bearing. |
| Repo/Filesystem/Definition.cls XData LockFileMapping | Filesystem projection: type, sort order, readOnly, root, depth. |
| Repo/Remote/Definition.cls XData LockFileMapping | Remote projection: type, sort order, readOnly, URL. |
| Repo/Oras/Definition.cls XData LockFileMapping | Source/formal-only projection extends remote fields with orasNamespace. No ORAS request/native install is claimed. |
| %JSON.Adaptor and array collection provider | `json_keys` consumes the provider's natural Name order; encoding and concrete order need native qualification. |
| Native LockProbe::Read/CountInstalledVersions | Re-open the real saved JSON, retain raw JSON and actual flat names, compare dependency keys/order/version/repository with installed records. |

These are scoped behavioral bindings, not executable-body proofs or whole-method coverage.

## Native isolation and known boundaries

LockProbe copies the product lock-test tree under `/tmp/iter12/lockfile/product/` and removes only copied preexisting module-lock.json files before installation, so an old expected file cannot hide a failed write. It resets repository definitions inside the fresh private container, creates the two product filesystem roots and performs full `install ... -create-lockfile` commands for K and L. The reversed-name install uses a separate tiny filesystem repository. The duplicate/helper and credential/helper files use explicit fresh populated Root folders without altering the product checkout or persisted installed module Root.

The parent must run Namespace before LockProbe because the latter resets private repository state. No network service is required. The existing empty-Root relative-write defect is preserved and is not repaired by these successful filesystem-install or populated-Root cases. A missing or stale file is charged as a failed native expectation, not replaced by fixture contents. Repository schema serialization, JSON parsing, file saves, name collation and graph resolution are provider premises qualified only by the sampled observations.

## Formal checks

The Verus model contains no assume/admit/external_body. The initial use of Seq.contains inside proof computation exceeded the computation limit; its failed source/log are retained. It was replaced by an explicit recursive membership function over the same input domain, after which the baseline verified 15 units with zero errors. This is a proof-layout correction, not a strengthened premise.

Fourteen semantic controls change edge polarity, dependency admission, cursor continuation, emitted flat names, selected version, selected repository, direct requirements, dependency name, duplicate JSON output, JSON traversal, top-level repository inclusion, password-field exclusion, URL preservation or missing-repository refusal. Controls using proof computation are rejected with Verus's `evaluates to false` diagnostic; compile errors/timeouts/resource limits do not qualify. Exact sources and receipts live under native/lockfile/proofs. The parent owns final integrated verification, native results and clean-seal replay.

## Integrated confirmation

The final parent native run confirmed all sampled fields, including reversed serialized name order. Full positive replay and all14 lock-file controls pass the integrated gate. Reverse lookup identifies this ordering witness as inherited P:F-DEP-2; no new defect or question is registered. Writer/consumer scope and field-mask limits above remain in force.
