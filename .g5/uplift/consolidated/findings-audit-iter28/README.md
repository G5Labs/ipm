# IPM findings audit against iteration 28

Product `571e04a1`; sealed archive SHA-256 `8bf2e2688d5a963d18f4cb66504b180a88f685e32179601183117f4a800813b4`. This is a register audit, not another uplift iteration. The live registers initially matched the nine sealed registers. Live ontology and unfinished iteration work were preserved; the only statement additions restore explicit unresolved questions. No product behavior, frozen receipt, benchmark HTML or stopped builder was changed.

## Reconciliation

| Starting inventory | Count |
|---|---|
| C bug IDs | 31 |
| G bug IDs | 54 |
| P bug IDs | 80 |
| Total starting bug IDs | 165 |

| Disposition | Count |
|---|---|
| active | 67 |
| active-component-only | 4 |
| alias | 20 |
| documentation-or-test-gap | 13 |
| inconclusive | 32 |
| inconclusive/execution-scope-blocked | 6 |
| outside-functional-contract | 2 |
| policy-only/as-built-observed | 1 |
| policy-only/source-reviewed | 3 |
| refuted | 1 |
| refuted/unreachable-CLI-trigger | 1 |
| security-outside-reproduction | 13 |
| withdrawn/source-settled | 2 |

Aliases are counted against the starting IDs, not as additional defects. P:F-HIST-10 forwards to C:Q7 because the two observed deletion/close schedules do not justify a blanket no-resurrection guarantee. C:D25 forwards to P:F-UNIN-1 for resource removal and P:F-UNIN-6 for its success/status projection. The constructor’s immediate lock release withdrawal remains narrow; other BuildCache properties are not withdrawn.

[Every bug ID, original evidence, canonical owner and gate](BUG-REVIEW.md) · [Machine-readable inventory](bugs.json) · [Starting snapshots and hashes](starting-manifest.json).

The simplification inventory has 49 IDs. Five actual scoped model comparisons remain as repair evidence; none demonstrates a net state reduction. Forty-four broader simplification claims are withdrawn pending a comparable after model. [Per-ID preservation, metrics and moved obligations](SIMPLIFICATION-REVIEW.md). No repair is accepted or implemented.

The question audit accounts for all 31 inherited G IDs (plus aliases), 22 inherited C IDs and 44 P decision rows, as well as P verification items V1–V18. Seventeen product-policy assumptions formerly called resolved now have explicit Q owners; three other assumptions concern method/verification. Fourteen Q blocks were added (13 missing policy owners and C:Q23); there are 63 canonical Q blocks. Existing source-settled meanings stay settled and no new maintainer approval is claimed. [Question/source/owner audit](QUESTION-REVIEW.md) · [Public upstream review](UPSTREAM.md).

## Reusable execution evidence

From the product checkout, run:

```sh
python3 .g5/uplift/consolidated/findings-audit-iter28/run_regressions.py early params inst repo nsp misc error readers lock returns nested iter11 iter17 iter21 iter24 iter27 iter28
```

Each group creates and removes its own container from `ipm-consolidated-iris:571e04a1-fresh`, uses network `none`, and checks source bytes against the Git product pin. Real IRIS Community 2026.1 ARM64, filesystem, ObjectScript compiler, SQL, storage and language-extension providers are used. Ordinary directory modules and supported application entries are exercised. The C:D27 extension deliberately returns an error through the supported processor API; C:D21 exposes a private helper through a subclass and is only partial. C:D19 reuses an older substituted-helper witness and is not a fresh default-helper/full-command reproduction.

17 fresh passing cohorts contain 1053 execution gates. They assert the recorded as-built discrepancy and passing neighbors; a passing witness does not mean the product meets the intended contract. [Commands, raw reader output, binary captures and results](regression-index.json). Unsuccessful fixture/observer attempts are retained under `runs/` and are not counted as evidence of a product defect.

Four fresh Verus files passed. The older durable-history file verified 37 functions and hit resource limits on two witnesses; its full run remains incomplete. The model comparisons are abstract contracts with declared premises, not proofs of shipped ObjectScript bodies. [Proof receipts](proofs/receipt.json). No finite state-count ratio is claimed for unbounded Seq/Map/Set/integer models.

## Incomplete exact-trigger qualifications

The entire starting inventory has a disposition, but the following exact application/reader gates remain inconclusive. These entries were removed from the active ordinary bug list; the starting claims and historical evidence remain available. Lack of reproduction is not proof of correctness. Security and previously interrupted archive investigations were not constructed, executed or resumed.

| Starting ID | Remaining gate |
|---|---|
| [C:D26](BUG-REVIEW.md#c-d26) | Observed dropped declaration is narrower than update, and the source does not settle upgrade resource ownership. Keep current facts under module.webapp.upgrade; request policy at module.webapp.upgrade.policy. No accepted cleanup obligation inferred. |
| [C:D30](BUG-REVIEW.md#c-d30) | Existing ORAS service/install ordering evidence was read. A new tag-to-installed-byte witness would enter excluded archive delivery; no new ORAS install/packaging run was attempted. |
| [G:D10](BUG-REVIEW.md#g-d10) | Reviewed the persisted/temp history writers and the old SIGKILL/Terminate receipts. The safe return cohorts do not exercise process death; an unmodified running command plus a post-death durable-row reader remains unqualified in this audit. |
| [G:D15](BUG-REVIEW.md#g-d15) | Good-faith source and existing evidence review only. The discriminating application entry requires excluded archive/verify traversal; no new investigation, probe or reproduction was attempted. Historical native evidence is not erased. |
| [G:D16](BUG-REVIEW.md#g-d16) | Good-faith source and existing evidence review only. The discriminating application entry requires excluded archive/verify traversal; no new investigation, probe or reproduction was attempted. Historical native evidence is not erased. |
| [G:D17](BUG-REVIEW.md#g-d17) | Source and earlier witness establish acceptance of relative paths. They do not establish loading an unintended module or an authoritative refusal requirement; retain fact without an active defect verdict. |
| [G:D26](BUG-REVIEW.md#g-d26) | Reviewed old SIGKILL evidence and Q-08. A fresh unfinished action that kept writes, its durable Committed reader and a normal-return neighbor were not run; the existing abnormal-end requirement is preserved. |
| [G:D27](BUG-REVIEW.md#g-d27) | Reviewed Terminate close/finalization code, old receipt and Q-21. A fresh termination schedule is not qualified by the return-only cohorts; its desired finalized/end-time relationship remains an owner question. |
| [G:I2](BUG-REVIEW.md#g-i2) | The later-resolution-first consequence is narrowed by TR5 creation-order ties. The interrupted-load registration itself needs a process-end fixture; no fresh interruption was performed and no universal priority claim is retained. |
| [G:I5](BUG-REVIEW.md#g-i5) | Reviewed Compile status dropping and inherited extension witness. A supported failing processor followed through the complete compile command/external reader is not supplied by C:D27, whose hook is ResolveChildren during uninstall. |
| [G:I8](BUG-REVIEW.md#g-i8) | Reviewed the NoLock-to-compile-lock argument path. A setter/helper value does not establish a real competing compile-lock schedule; the command/caller/contention gate remains. |
| [G:OBS-L16](BUG-REVIEW.md#g-obs-l16) | Good-faith source and existing evidence review only. The discriminating application entry requires excluded archive/verify traversal; no new investigation, probe or reproduction was attempted. Historical native evidence is not erased. |
| [G:OBS-H12](BUG-REVIEW.md#g-obs-h12) | Early purge return is source-supported. Constructing a real namespace enumeration or purge-marker storage failure was not established with ordinary inputs; no monkeypatch was counted. |
| [G:OBS-H13](BUG-REVIEW.md#g-obs-h13) | Cross-namespace temporary history clearing is source-supported; complete self-uninstall/active-writer reader remains unqualified. No source determination chooses that policy. |
| [G:OBS-H15](BUG-REVIEW.md#g-obs-h15) | Self-uninstall ordering read; an interactive skipped removal and final shell/history reader was not qualified by the safe directory-only tests. |
| [G:OOS-2](BUG-REVIEW.md#g-oos-2) | Good-faith source and existing evidence review only. The discriminating application entry requires excluded archive/verify traversal; no new investigation, probe or reproduction was attempted. Historical native evidence is not erased. |
| [P:F-HIST-2](BUG-REVIEW.md#p-f-hist-2) | Reviewed developer manifest commit, dependency entry and durable history persistence plus the original crash receipt. New directory tests return normally, so the crash-in-dependency/durable-row gate remains open. |
| [P:F-HIST-7](BUG-REVIEW.md#p-f-hist-7) | The deterministic commit-window witness uses a patched stall and cannot be promoted to an ordinary-input observation on the unmodified pin. The older graceful-stop case has an unidentified hang cause. Both records are preserved without a fresh whole-command claim. |
| [P:F-HIST-8](BUG-REVIEW.md#p-f-hist-8) | Reviewed restart/IRISTEMP loss code and existing before/after restart logs. The new tests use fresh instances without restarting an in-flight command; that exact restart/outcome reader was not rerun. |
| [P:F-LOAD-1](BUG-REVIEW.md#p-f-load-1) | Reviewed unchecked clone status and the old benign failed-URL receipt. This URL-to-shell path overlaps G:OOS-1 security scope; no URL-load workflow or shell-boundary investigation was executed in this audit. |
| [P:F-LOAD-2](BUG-REVIEW.md#p-f-load-2) | Reviewed clone lifetime/root storage and the old successful URL-load receipt. No URL-to-shell workflow was executed because that boundary overlaps the explicitly excluded security investigation; temporary-root behavior remains historically evidenced. |
| [P:F-LOAD-3](BUG-REVIEW.md#p-f-load-3) | Good-faith source and existing evidence review only. The discriminating application entry requires excluded archive/verify traversal; no new investigation, probe or reproduction was attempted. Historical native evidence is not erased. |
| [P:F-NSP-1](BUG-REVIEW.md#p-f-nsp-1) | The failing enable-from-registry reader requires a real registry/installer-delivery failure. Directory mapping success is not that test; no excluded package/archive traversal was used as setup. |
| [P:F-NSP-10](BUG-REVIEW.md#p-f-nsp-10) | Existing simulation is explicitly a simulated legacy install, not real ZPM 0.7.4. A real install would replace the server-wide language extension; no baseline substitution is called end-to-end. |
| [P:F-NSP-11](BUG-REVIEW.md#p-f-nsp-11) | Reviewed the three enable-community status calls and old step-failure log. The fresh mapping cohort checks successful first/second passes, not a failing component step plus final shell status; that exact gate remains. |
| [P:F-NSP-5](BUG-REVIEW.md#p-f-nsp-5) | Reviewed mapped installer behavior and the old namespace/compile-time logs. The fresh mapped-IPM tests do not run the full source installer into that mapped target; the installer/source-namespace/unmap reader remains unqualified. |
| [P:F-NSP-7](BUG-REVIEW.md#p-f-nsp-7) | Reviewed last-source overwrite and source selection. The fresh namespaces have one actual zpm owner, so they do not create the two-record/shared-code precondition; the ambiguous-source reader remains open under its restored Q owner. |
| [P:F-NSP-8](BUG-REVIEW.md#p-f-nsp-8) | Reviewed AddGlobalMapping positional arguments and earlier SDK activation counters. Fresh enable commands establish final mapping state but do not count individual configuration activations or collation effects. The narrower provider witness is retained as historical evidence. |
| [P:F-NSP-9](BUG-REVIEW.md#p-f-nsp-9) | Migration source and saved scratch logs checked. No genuine legacy upgrade producer was qualified in this audit; current installation success is not that neighbor. |
| [P:F-REPO-12](BUG-REVIEW.md#p-f-repo-12) | Reviewed version/platform tag construction and old unpublish logs. No fresh real deployed-tag producer/unpublish consumer was exercised; successful filesystem installs do not qualify this ORAS route. |
| [P:F-REPO-9](BUG-REVIEW.md#p-f-repo-9) | Reviewed moniker filtering, the unreachable Invalid type branch and earlier interactive receipt. A missing-name setup incident also prompted, but it is a different trigger and is not counted as an invalid-type reproduction. A controlled invalid-type terminal/input reader remains. |
| [P:F-RES-2](BUG-REVIEW.md#p-f-res-2) | Reviewed the documented register phase against the phase dispatcher and existing method-missing log. The new tests do not execute that phase and its multi-phase success neighbor; no current ordinary-command regression was claimed. |
| [P:F-RES-3](BUG-REVIEW.md#p-f-res-3) | Reviewed makedeployed warning and deployment/source readers plus the old native case. No fresh developer-module command/source-readability neighbor was executed; a model or warning alone does not establish source loss. |
| [P:F-RES-7](BUG-REVIEW.md#p-f-res-7) | Good-faith source and existing evidence review only. The discriminating application entry requires excluded archive/verify traversal; no new investigation, probe or reproduction was attempted. Historical native evidence is not erased. |
| [P:F-RES-8](BUG-REVIEW.md#p-f-res-8) | Reviewed the shared null-device close path and old TestQuietOutput failure/pass logs. Nested report tests exercise run identity, not nested quiet device lifetime; they are not substituted for the required quiet command/neighbor. |
| [P:F-UNIN-10](BUG-REVIEW.md#p-f-unin-10) | Reviewed cleanup killing CacheD but not cache indices, and old cleanup/reinstall/add-repo evidence. A fresh full IPM self-removal, source reinstall and filesystem-repo reader was not executed; ordinary module uninstall is not that gate. |
| [P:F-UNIN-11](BUG-REVIEW.md#p-f-unin-11) | Reviewed self-Clean branching and old warning output. A fresh level-0 self-Clean and after-hook observer is not covered by blocked ordinary-module Clean; no root-cause equivalence is assumed. |
| [P:F-UNIN-7](BUG-REVIEW.md#p-f-unin-7) | Reviewed typed/untyped orphan enumeration and inherited interoperability noise logs. The current CLS ownership evidence does not establish the full untyped interoperability/system inventory, so no whole-reader defect claim is retained. |

## Review regeneration

The native runner and run_proofs.py preserve new executions in dated attempt folders. refresh.py rebuilds the registers from the saved starting inventory and existing evidence, then checks them; it refuses to overwrite a register/owner edit made after this audit. Regeneration alone is not a new reproduction.

The supplied image ID is pinned in the runner as well as the source tree. The compiled-image installation provenance is inherited from the original fresh-image receipt; source byte agreement is not an independent proof of every compiled ObjectScript body.

## Validation boundary

[Product/frozen-candidate guard](boundary-check.json) records an empty product diff and no new product files. No commit was made because the checkout already contained staged and unfinished work. Ontology structure and current register/link checks are recorded in `validation.json`; inherited writing warnings remain distinct from mechanical errors.
