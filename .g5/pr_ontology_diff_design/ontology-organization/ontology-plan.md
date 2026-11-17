# Organizing the IPM ontology

Start at the [IPM overview](../../ontology/root.g5.md). The organization follows the questions a package author, operator or maintainer brings to IPM. The owner delegated the choice of categories in the [request](../../chat_intent/ontology-organization/request.md).

## Where a rule belongs

| Reader’s question | Owner | Example |
|---|---|---|
| What must several commands agree on? | A shared contract under `concepts/` | A module’s current supplier can change. A finalized install record keeps the source used for that action. [Data policy](../../ontology/concepts/data-policy.g5.md#history.fields.source-snapshot) |
| What happens when I work on a package? | A workflow under `features/` | Reinstall requests the installed version and searches eligible repositories again. [Reinstall selection](../../ontology/features/install.g5.md#reinstall.selection) |
| What can I configure for an installation or namespace? | An operator task under `operations/` | An operator can share IPM code and independently request shared repository settings. [Enabling namespaces](../../ontology/operations/namespaces.g5.md#namespace.enable) |
| Which mechanism produces an outcome that several workflows depend on? | A design rule under `architecture/` | The same phase runner supplies prerequisites, initializes preload code and dispatches callbacks. [Lifecycle execution](../../ontology/architecture/lifecycle.g5.md#lifecycle.chain) |

“Module” is vocabulary. Identity and ownership belong to the data policy. Editing a manifest, deploying compiled code and installing a web application each have their own outcomes and exceptions. “Repository” likewise spans three decisions. The operator chooses the available sources, a shared rule selects an eligible supplier, and a provider delivers the bytes. [Manifest editing](../../ontology/features/manifest.g5.md#module.identity.document) · [Repository management](../../ontology/operations/repositories.g5.md) · [Source selection](../../ontology/concepts/repositories.g5.md#repo.search-order) · [Repository providers](../../ontology/architecture/package-sources.g5.md)

## Reading a commitment in context

Each page introduces its vocabulary, then uses headings for distinct questions. A statement takes children when a later rule qualifies that specific promise. A failed developer-mode load is one such case. Its outcome depends on which commit was reached, and failure before the first commit has its own rule. An unrelated question about the caller’s final status starts a new section. [Load outcomes](../../ontology/features/load.g5.md#load.outcome) · [Command status](../../ontology/features/load.g5.md#load.status)

The code and models distinguish outcomes that an outline by component would obscure. A normal load can return an error after the final commit. Tests can produce a report before returning a failure, and a nested run can put the wrong run in the exported file. The shared policies keep those qualifications beside the promise. An owner can review a credential-forwarding decision without implicitly endorsing the pinned provider’s current behavior. [Load implementation](../../../src/cls/IPM/Utils/Module.cls#L1152) · [Load contract](../../Verus_verification/crates/ipm-load-history/src/contract.rs) · [Test outcome model](../../Verus_verification/consolidated/iter4/test_outcome.rs) · [Test report identity](../../ontology/concepts/testing-policy.g5.md#build.test-output.run-identity) · [Registry credentials](../../ontology/concepts/download-security.g5.md#repo.remote.download)

## Acceptance

- A reader can reach a package workflow, a shared policy, an operator task or an architecture decision directly from the overview. The G5 viewer exposes each section’s independent statements when children are collapsed.
- Each original stable ID has one owning statement. Its earlier address still leads to that rule. The current findings and questions link directly to their owners, and the original model associations and verification scope remain attached.
- The harness can parse the ontology and its findings. Open questions retain their dispositions and proof claims retain their recorded scope.

## Review results

The comparison baseline is the IPM uplift at `5fb9f43cb`. Each of its 263 statements occurs once in the reorganized ontology. A comparison keyed by stable ID matched the original kind, full statement text and full per-statement metadata after resolving relative Markdown links to their original evidence or statement IDs. This includes examples, exceptions, working assumptions, parent and child references, provenance and verification qualifications.

Section headings group the statements for reading. The original statement hierarchy remains available through the links in each block’s metadata. Earlier file addresses that no longer own statements link to their current owners.

The G5 viewer resolved all 263 original statement addresses. With children collapsed, 159 statements are visible across the 27 owning pages, compared with 37 across the original 19 pages. All 137 associations with the 88 Verus manifests resolve. The findings check passed, and the ontology checker reported zero structural issues and the same 44 writing warnings as the baseline.
