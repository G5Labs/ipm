# Repo

<a id="g5.repo.ownership"></a>
[I · g5.repo.ownership]

Each repository belongs to an organization, has a name and stores its complete Git history. An organization may have several repositories.

<a id="g5.repo.management"></a>
[I · g5.repo.management]

An organization can add and remove repositories.

An organization administrator removes a repository. Removal is refused while any workspace includes the repository, Base included, and no workspace can gain it while its removal is under way, so the [docs repository](#g5.repo.docs), which every workspace includes, stays while the organization has workspaces.

<a id="g5.repo.durability"></a>
[I · g5.repo.durability]

Repositories must withstand G5 infrastructure failures and should be backed up.

<a id="g5.repo.storage-tenancy"></a>
[D · g5.repo.storage-tenancy]

Repository storage keeps each organization's repositories, with their review artifacts and locks, under that organization's own directory, and names each repository by its id there (`<org>/<repo>`). A repository id paired with the wrong organization names a directory that does not exist.

<a id="g5.repo.storage-guard"></a>
[D · g5.repo.storage-guard]

Only the repository store's owner reaches [repository storage](#g5.repo.storage-tenancy), and only for an organization the caller is scoped to: a tenant transaction's organizations, or the organization a verified workspace or host credential carries. It refuses any other organization before building a path, so storage protects [tenancy](organizations.g5.md#g5.organization.tenancy) by itself rather than relying on every caller having first read the repository through row-level security. Tests go through the same check, and a static audit keeps every other module from naming the storage or building paths under it.

<a id="g5.repo.hosted-url"></a>
[D · g5.repo.hosted-url]

A G5-hosted repository, docs or code, records `g5-local://<org-slug>/<repo-slug>` as its URL, and the URL follows an organization rename. It names no disk path, and storage never reads it. A provider-less repository that a workspace tracks as a remote keeps its remote URL.

<a id="g5.repo.git-interface"></a>
[I · g5.repo.git-interface]

Serve repositories through the standard Git interface.

<a id="g5.repo.permissions"></a>
[I · g5.repo.permissions]

Only authorized organization users may read a repository. Read authority is per repository: a member who may read any workspace that includes the repository may fetch all of it, every workspace branch and flush snapshot included, so no branch is more private than the repository that holds it. Because every active member reads every ordinary [workspace](workspaces.g5.md#g5.workspace.audience), this matches workspace read and normally means organization membership. Only authorized workspaces and the central API may write to it, and each workspace writes only its own branches.

<a id="g5.repo.external-binding"></a>
[I · g5.repo.external-binding]

A repository may connect to an external repository.

<a id="g5.repo.docs"></a>
[I · g5.repo.docs]

Each organization has a docs repository for alignment documents. Every workspace includes it.

## Metadata

### g5.repo.ownership

- human-provided
- Source: [Ben, line 43](../../../docs/ben_new_ontology.md#L43).

### g5.repo.management

- human-provided
- Source: [Ben, line 44](../../../docs/ben_new_ontology.md#L44).
- [RepositoryRemoval](../../uplift/ben-ontology/reviews/workspace-git-full.md) checks safe admission and a completed cleanup path for a proposed removal flow that refuses required or in-use repositories and rechecks dependencies under lock. The public removal feature is missing.
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 28 (GT-cell-49).

### g5.repo.durability

- human-provided
- Source: [Ben, line 45](../../../docs/ben_new_ontology.md#L45).
- [BackupBoundary](../../uplift/ben-ontology/reviews/workspace-git-full.md) checks that stopping writers and saving SQL and Git together preserves required references through restore; SQL-only or unfenced capture can fail. Actual backups, writer shutdown and restore drills remain unverified.

### g5.repo.storage-tenancy

- human-verified
- Source: the owner's 2026-11-04 decisions on [G5-Lean#1079](https://github.com/G5Labs/G5-Lean/issues/1079#decisions): a one-off, stop-everything move for QA with orphan directories reported and left in place; organization deletion out of scope. Modelled on the attachment store of G5-Lean#1053 (`g5.issue.native.attachment-store`).
- Assumption: a repository never changes organization (`repos.org_id` is immutable through composite foreign keys) and no operation spans organizations, so `<org>/<repo>` is stable and no repository refers to another's directory.
- Implemented: `g5_storage::RepoKey`; the one-off QA move `scripts/migrate-repo-layout.py`, whose docstring is the runbook.

### g5.repo.storage-guard

- human-verified
- Source: the owner's 2026-11-04 decisions on [G5-Lean#1079](https://github.com/G5Labs/G5-Lean/issues/1079#decisions): a strict guard with a sole holder, no unchecked keys in tests and a static audit.
- Implemented: `services/git/repo_store.rs`; the Git route resolves its path only after authorization; [tenancy tests](../../../backend/g5-api/tests/repo_storage_tenancy.rs) and the [static audit](../../../backend/g5-api/tests/repo_storage_tenancy_guard.rs).

### g5.repo.hosted-url

- human-verified
- Source: the owner's 2026-11-04 decision on [G5-Lean#1079](https://github.com/G5Labs/G5-Lean/issues/1079#decisions) to give every G5-hosted repository a scoped URL; following renames was added in review because organization slugs can change.
- Implemented: `org_service::local_repo_git_url`; migration `150_scoped_local_repo_urls.sql` rewrites existing rows.

### g5.repo.git-interface

- human-provided
- Source: [Ben, line 46](../../../docs/ben_new_ontology.md#L46).

### g5.repo.permissions

- human-provided
- security
- Source: [Ben, line 47](../../../docs/ben_new_ontology.md#L47).
- Per-repository read is the owner's 2026-10-10 decision on [issue 423](../../chat_intent/issue-423-git-read-boundary/request.md) ("if one person is allowed to work on the repo, it simply means that this person can read anything in the repo"); its former home, `g5.security.mvp-exposure.objects`, was dropped in the 2026-10-13 rebuild (`5c66d337a`). The match with workspace read follows the owner's decision 5 of 2026-10-30 ([request](../../chat_intent/security-invariants/request.md#decisions-the-owner-has-made-2026-10-30)).

### g5.repo.external-binding

- human-provided
- Source: [Ben, line 48](../../../docs/ben_new_ontology.md#L48).

### g5.repo.docs

- human-provided
- Source: [Ben, line 49](../../../docs/ben_new_ontology.md#L49).
