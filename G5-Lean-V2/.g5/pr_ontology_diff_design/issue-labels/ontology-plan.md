# Issue labels

Intent: the owner's [G5-Lean#1055](https://github.com/G5Labs/G5-Lean/issues/1055), whose
body is the reviewed design this plan records and refines for implementation. It extends the
[native tracker design](../issue-681-native-g5-issue-tracker/ontology-plan.md) and the
[`g5d issue edit` design](../issue-808-g5d-issue-edit/ontology-plan.md). The owner asked to
work on the issue (2026-11-03), which authorizes the implementation.

## Ontology changes

| Block | Change | Summary |
|---|---|---|
| [`g5.issue.labels`](../../ontology/features/issues.g5.md#g5.issue.labels) | New | Labels classify issues in every domain: catalog, tagging, creation, reads, search, UI and `g5d`, export; no taxonomy or automation. |
| [`.contract`](../../ontology/features/issues.g5.md#g5.issue.labels.contract) | New | One common label and operation set through the issue service; the adapter matches names; deltas, central validation, pagination. |
| [`.native`](../../ontology/features/issues.g5.md#g5.issue.labels.native) | New | Native catalog and join, stable ids, locking order, one transaction per delta, events with the name at the time. |
| [`.github`](../../ontology/features/issues.g5.md#g5.issue.labels.github) | New | GitHub stays authoritative; existing write guards; per-change outcomes; creation drops reported; create reconciled by name. |
| [`.surfaces`](../../ontology/features/issues.g5.md#g5.issue.labels.surfaces) | New | One picker, chips, catalog editor; `g5d issue label …`, `--add-label/--remove-label`, `--label`. |
| [`g5.issue.native`](../../ontology/features/issues.g5.md#g5.issue.native), [`.scope`](../../ontology/features/issues.g5.md#g5.issue.native.scope) | Amended | Labels leave the retired/out-of-scope lists; the retired local-issue label data stays dropped. |
| [`g5.issue.native.adapter`](../../ontology/features/issues.g5.md#g5.issue.native.adapter) | Amended | Label catalog and label changes are adapter content operations. |
| [`g5.external-issues.creation`](../../ontology/features/issues.g5.md#g5.external-issues.creation), [`.edit`](../../ontology/features/issues.g5.md#g5.external-issues.edit), [`.cli`](../../ontology/features/issues.g5.md#g5.external-issues.cli) | Amended | Labels selectable on creation and changed by explicit deltas, no longer out of scope. |
| [`g5.issue.native.search`](../../ontology/features/issues.g5.md#g5.issue.native.search), [`.page`](../../ontology/features/issues.g5.md#g5.issue.native.page), [`.migration`](../../ontology/features/issues.g5.md#g5.issue.native.migration) | Amended | `label:`/`no:label`; labels editable on the page; export carries the catalog. |

## Governing ontology

- [`g5.issue.native.adapter`](../../ontology/features/issues.g5.md#g5.issue.native.adapter):
  one issue service with provider adapters; surfaces never branch on the provider.
- [`g5.issue.native.writes`](../../ontology/features/issues.g5.md#g5.issue.native.writes):
  member write permission, the agent as its owner, viewers read, no external guards natively.
- [`g5.external-issues.creation.admission`](../../ontology/features/issues.g5.md#g5.external-issues.creation.admission)
  and [`g5.issue.comment-authority`](../../ontology/features/issues.g5.md#g5.issue.comment-authority):
  the GitHub write guards every label write reuses.
- [`g5.external-issues.creation.recovery`](../../ontology/features/issues.g5.md#g5.external-issues.creation.recovery):
  creation's marker protocol, which labels selected at creation ride on.
- [`g5.external-issues.cli.reads`](../../ontology/features/issues.g5.md#g5.external-issues.cli.reads):
  the agent reads the configured domains only, while its owner is a member.

## Design

```text
list_labels(domain, page)                    -> labels, nextPage?
create_label(domain, name, color?, desc?)    -> label | conflict(existing)
edit_label(domain, label, {name?, color?, description?}) -> label | conflict | not_found
delete_label(domain, label)                  -> applied | not_found | unknown
change_issue_labels(issue, add[], remove[])  -> outcome, per-change outcomes, observed labels?
create_issue(existing fields, labels[])      -> existing creation result (+ labels not applied)
```

**Owners.** The issue service (`services/git/issue_labels.rs` for the common shapes and
validation, the shared `*_as` operations beside `edit_issue_as`) resolves the domain or issue,
the caller's authority and the provider, then calls the adapter. The native adapter
(`native_issues/labels.rs`) owns its catalog and assignments in Postgres; the GitHub adapter
(`g5-scm` label transport) owns GitHub's protocol and guards. A label is `{id, name, color,
description}`: `id` is an opaque string (GitHub's numeric label id, or the native id), unique
in its domain only; summaries keep `name` and `color` and gain the other two. Callers name
labels by name for tagging and by id or name for catalog edits; the adapter does the matching,
so no caller encodes a GitHub id or database key. No second label service, no cross-domain
identity, no synchronization.

**Validation (central, before any provider).** Name: trimmed, non-empty, one line, at most 50
characters (GitHub's limit). Description: at most 100 characters (GitHub's limit); empty
clears. Color: six hex digits, with or without `#`, stored lowercase without `#`. A change
names at most 20 labels in total, never the same label twice or in both lists, and at least
one; creation selects at most 20. A catalog page holds 100 labels (GitHub's page maximum);
`nextPage` says more exist. GitHub catalog id lookups scan at most 10 pages (1,000 labels) and
otherwise refuse with a pointer to name the label instead.

**Deltas, not sets.** Tagging sends additions and removals, never a replacement set, so a
concurrent change to another label survives. Changing priority removes the old label and adds
the new one in one request; there is no exclusivity and no atomic multi-label guarantee across
providers. Catalog edits are explicit last-writer-wins writes.

**Native.** Migration `154_native_issue_labels.sql`: `native_issue_labels` (org, domain,
identity id, name, color, description, times; unique lower-cased name per domain) and
`native_issue_label_assignments` (org, domain, issue, label), with composite foreign keys
through `(org_id, domain_id, id)` on both sides so an assignment cannot cross organizations or
domains, tenant RLS, and assignments deleted with their issue or label. Events gain `labeled`
and `unlabeled` with `label_name`, the name at the time. Lock order is labels before issues:
tagging locks the named labels `FOR SHARE` (resolving names case-insensitively, refusing
unknown ones), then the issue `FOR UPDATE`, inserts or deletes assignments, and writes events
and the issue's update time only for actual changes; deleting a label locks it `FOR UPDATE`,
then the issues carrying it, records `unlabeled` on each and removes the assignments with the
label, in one transaction. Rename changes only the catalog row. Creation resolves and attaches
labels in the creating transaction, and the key replay compares the label set as it compares
assignees. A create whose name exists returns that label when it matches the request
(an idempotent retry) and is otherwise a conflict carrying the existing label. Export adds the
catalog, each issue's labels and the label events.

**GitHub.** Reads (catalog, by-name lookups) use the domain's installation token; writes use
the person's connected account, or the owner's for the agent, after `admit_person_write`
(comments switch, non-public now, connection) and a secret screen of supplied names and
descriptions; the UI shows untrusted-domain payloads before sending. No redirect is followed.
Tagging: every added name must exist (`GET …/labels/{name}`), since GitHub's add creates a
missing label; one `POST …/issues/{n}/labels` adds them, then one `DELETE …/labels/{name}` per
removal (404 means already absent). A refusal or unknown answer stops the sequence; the rest
are `unattempted`. A refusal before anything applied is returned as the error. The issue is
read again for the observed labels; a failed read leaves the known outcomes. Creation sends
`labels` in its one `POST`; GitHub silently drops labels the author may not set, so labels
missing from the answer are reported (`labelsNotApplied`, exit 1 in `g5d`) and repaired by an
explicit delta. A marker replay reports the issue as found and never re-applies labels. A
label create GitHub refuses as existing is reconciled by reading that label, as natively; an
unknown create, edit or delete is reported unknown, to be re-read before retrying. Only a 404
saying "Label does not exist" reads as already absent; a bare 404 (a deleted or moved issue)
is a refusal.

**Fields and labels in one edit.** `g5d issue edit` may change both. Every check of both runs
before any write: natively both change in one transaction (labels first, keeping the lock
order); on GitHub every named label is looked up, the field edit (all checks before its one
write) is made, then the label changes report their own outcomes.

**Residual risks (accepted).** On GitHub, a label deleted between G5's existence check and the
add is recreated by GitHub with a default colour; the check narrows but cannot close that
window. GitHub catalog edits are unconditional. These follow from GitHub's API.

## Surfaces

- UI: one `IssueLabelPicker` (creation dialog and issue page), the existing chips with colours,
  and a catalog editor per domain reached from the picker ("Manage labels") and the Issues
  settings. Disabled reasons come from the same write target; partial and unknown outcomes are
  shown with what applied. Membership changes refresh every cached copy of the issue; catalog
  edits refresh the catalog and issue displays.
- `g5d issue label list|create|edit|delete [--domain KEY|owner/name]` (the creation domain
  when omitted and unambiguous), `g5d issue edit --add-label/--remove-label`,
  `g5d issue create --label`, and `g5d issue search --label` (adds `label:"…"`). Deleting
  needs `label delete`; no edit deletes a catalog label.

## Acceptance

Native-only deployment with no GitHub client: catalog CRUD, rename/delete with attached
issues, creation and replay with labels, tagging, `label:`/`no:label` search, Activity and
export; tenant and viewer isolation; agent attribution; racing additions/removals and catalog
deletion. GitHub mocked transport: guards, pagination, refusal, timeout and partial results.
UI picker/search/cache behaviour and CLI command and JSON integration. Run the related API,
provider, frontend and CLI suites.
