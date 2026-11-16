# Independent source-first inventory review

The pinned census reproduces 1,216 members. Iteration 1's accepted reach is 174 distinct members: 165 direct and nine shared-contract-only. This review classifies 75 of the 1,042 members outside that reach, across nine purposive strata. It also challenges the previously represented publication entry point. It does not award representation or proof credit.

| Outside accepted reach | Members |
|---|---:|
| Material decisions inspected in source | 44 |
| Shared helpers or delegation inspected in source | 27 |
| Justified exclusions from separate deep modeling | 4 |
| Still unclassified | 967 |
| Total | 1,042 |

The four exclusions are constant interface methods (`IPublishService.IsAvailable`, its default `CheckUnpublishEnabled`, filesystem `Supports`, and test `SupportsSync`). They remain in the 1,216 denominator. An interface constant cannot prove the network or operation contract of an overriding implementation. Delegation classification similarly does not imply that the delegated contract is covered.

Selection used source, local operator documentation, changelog and pinned history. It deliberately sampled integrity and destructive operations, then followed their actual providers and final readers. It is not a random sample, and no global estimate of materiality follows from its proportions. Each reviewed row records a concrete observed distinction, valid neighbor, provider or reader, and remaining work. The review did not inspect formal model bodies or the independent evaluator's fresh cases. Iteration 1 disposition IDs were used only to establish the gap frame.

## Consequential leads

1. **Selecting an ORAS tag depends on provider order.** `GetLatestTag` returns the last `get_tags` entry without sorting. In contrast, `GetAllTags` calls descending `SortOCITags`. The former's comment says highest numbered version. A `Pull` with no explicit tag consumes the former path. A provider that supplies `[2.0.0, 1.0.0]` selects `1.0.0`; reversing the list selects `2.0.0`. This is a source counterexample to the comment absent a provider-order premise, not a confirmed production registry defect. Inspect or exercise the actual disposable provider before assuming an order guarantee.

2. **Artifact validation uses the first tar-media layer.** `VerifyModuleHashAndSize` skips other media types, then immediately returns the first matching media type's size-and-digest comparison. If that first tar layer mismatches and a later one matches, the helper returns false. `Pull` separately chooses the first `*.tgz` filesystem result. The model must distinguish those two selection operations. Treat unique-layer/unique-file assumptions as external premises until validated; hashing a selected stream does not prove arbitrary manifest selection.

3. **Publication can leave external effects after a command error.** `PushOras` validates and pushes one tag at a time. A later invalid tag or provider failure follows possible earlier publication. `DeleteTags` similarly deletes sequentially. `PublishModule` can append a temporary-directory cleanup failure after successful push. `Lifecycle.Base.%Publish` calls the remote provider before optional local `NewVersion` export. A postpublication local failure does not establish registry rollback. That challenges any broad interpretation of the existing `%Publish` credit; this review has not inspected the old model's narrower assertion and therefore does not withdraw it automatically.

4. **Remote and ORAS publishing managers have different cache behavior.** Remote `CheckServiceCache` returns a previously available client without rereading the repository's `Enabled` state or probing it. A previously unavailable name stays suppressed for that manager instance. ORAS manager rereads and probes each time. The Remote source comment says Enabled is not cached, but that applies only within the uncached branch. `General.Singleton` stores an instance by process and namespace and removes its record on close; callers must keep its lifetime explicit. A same-instance programmatic sequence can expose the difference. A shell may shorten that lifetime, so do not claim persistence across shell calls without tracing retained references.

5. **A forced uninstall's traversal state differs from the final module store.** `UninstallAll` removes a failed module from its dependency traversal graph when forced, allowing attempts on its dependencies. It retains an error status, so bookkeeping progress is not evidence that the module was actually removed. IPM itself is skipped. Test final installed rows and the aggregate status independently.

6. **Summary and test readers have distinct boundaries.** `GetDirectDependents` only reports direct unscoped dependencies. `GetSection` omits scoped resources and test processors. `GetModuleRoot` resolves a blank store value to `dataDir/ipm`, while `GetAll` may display blank. `TestsShouldRun` uses raw prefixes; `OwnsClassName` uses a dot boundary. `RelPathToClassName` converts a `.cls` outside the declared directory despite its comment, but its caller may later reject it through class ownership. These are source distinctions or suspected mismatches, not confirmed extra test execution.

The parent implementer received these source leads before candidate sealing. They belong to the repair/discovery queue, not the separately frozen evaluator panel. Their presence in the fresh panel, if any, cannot retroactively make them blind cases.

## Reproduction and scope

Run from the IPM root:

```sh
python3 .g5/uplift/consolidated/iter2/inventory/triage.py
```

`inventory.json` records the pinned path, exact member span and SHA-256 for every census member. `baseline-reach.json` preserves the 174-member baseline union and the exact 1,042-member complement. `review-notes.tsv` holds the manual review; `reviewed-source.txt` collects the corresponding pinned source slices. `summary.json` gives counts, exclusions, selection reasons and evidence hashes. The script confirms member-set equality with the frozen iteration 1 disposition list. It neither edits that list nor invokes a grader.

Unreviewed families remain open, including most CLI parsing, datatype validation, extension callbacks, archive extraction, resource processors, lifecycle/namespace provisioning, storage writers, Studio import/export and concurrency. Whole-source inventory is not whole-source semantic assurance. Additional caller and dynamic-dispatch paths can change consequences beyond these inspected slices. No native observation or ObjectScript implementation proof is claimed by this review. Owner priorities, disputed policy and human readability remain unmeasured. Reviewer tokens and cost are unknown.
