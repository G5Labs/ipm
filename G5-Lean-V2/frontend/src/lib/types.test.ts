import { describe, expect, it } from "vitest";

import type {
  PendingSyncFile,
  WorkspacePullRequestProviderRepoSummary,
  WorkspacePullRequestSummary,
} from "./types";
import {
  githubPublications,
  isIsolatedPendingSyncDocFile,
  workspacePullRequestProviderOutcome,
} from "./types";

describe("isIsolatedPendingSyncDocFile", () => {
  const WORKSPACE_ID = "019edbc0-8c0c-7791-8b9c-be69dd0e8d0d";
  const OTHER_WORKSPACE_ID = "019edbc0-ffff-7791-8b9c-000000000000";

  const docRow = (path: string): PendingSyncFile => ({
    id: `doc:repo-1:${path}`,
    targetKind: "doc",
    path,
    repoId: "repo-1",
    repoName: "docs",
    branchName: "feat/docs",
    headCommit: "a".repeat(40),
    lastDownliftedCommitSha: null,
    targetDocRef: "doc-ref",
  });

  it("accepts supported doc files inside the workspace's Project Docs folder", () => {
    expect(
      isIsolatedPendingSyncDocFile(
        docRow(`docs/${WORKSPACE_ID}/a.md`),
        WORKSPACE_ID,
      ),
    ).toBe(true);
    expect(
      isIsolatedPendingSyncDocFile(
        docRow(`docs/${WORKSPACE_ID}/sub/dir/b.html`),
        WORKSPACE_ID,
      ),
    ).toBe(true);
    expect(
      isIsolatedPendingSyncDocFile(
        docRow(`docs/${WORKSPACE_ID}/c.htm`),
        WORKSPACE_ID,
      ),
    ).toBe(true);
    expect(
      isIsolatedPendingSyncDocFile(
        docRow(`docs/${WORKSPACE_ID}/session.jsonl`),
        WORKSPACE_ID,
      ),
    ).toBe(true);
    // Case-insensitive workspace-id comparison (defensive lowercase).
    expect(
      isIsolatedPendingSyncDocFile(
        docRow(`docs/${WORKSPACE_ID}/a.md`),
        WORKSPACE_ID.toUpperCase(),
      ),
    ).toBe(true);
  });

  it("rejects other workspaces' folders, root docs paths, and non-doc rows", () => {
    expect(
      isIsolatedPendingSyncDocFile(
        docRow(`docs/${OTHER_WORKSPACE_ID}/a.md`),
        WORKSPACE_ID,
      ),
    ).toBe(false);
    expect(
      isIsolatedPendingSyncDocFile(docRow("docs/root-note.md"), WORKSPACE_ID),
    ).toBe(false);
    // Prefix must match with the trailing slash: `docs/<ws>.md` is a file.
    expect(
      isIsolatedPendingSyncDocFile(
        docRow(`docs/${WORKSPACE_ID}.md`),
        WORKSPACE_ID,
      ),
    ).toBe(false);
    expect(
      isIsolatedPendingSyncDocFile(
        docRow(`views/${WORKSPACE_ID}/a.md`),
        WORKSPACE_ID,
      ),
    ).toBe(false);
    expect(
      isIsolatedPendingSyncDocFile(
        docRow(`docs/${WORKSPACE_ID}/notes.txt`),
        WORKSPACE_ID,
      ),
    ).toBe(false);
    expect(
      isIsolatedPendingSyncDocFile(
        docRow(
          `docs/${WORKSPACE_ID}/sync/sync-auth--aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa/review/code_review.md`,
        ),
        WORKSPACE_ID,
      ),
    ).toBe(false);
    expect(
      isIsolatedPendingSyncDocFile(
        docRow(
          `docs/${WORKSPACE_ID}/sync/sync-auth--aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa/merge/run-1_merge_report.md`,
        ),
        WORKSPACE_ID,
      ),
    ).toBe(false);
    expect(
      isIsolatedPendingSyncDocFile(
        docRow(
          `docs/${WORKSPACE_ID}/sync/sync-auth--aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa/future-report.jsonl`,
        ),
        WORKSPACE_ID,
      ),
    ).toBe(false);
    expect(
      isIsolatedPendingSyncDocFile(
        docRow(`docs/${WORKSPACE_ID}/code_review.md`),
        WORKSPACE_ID,
      ),
    ).toBe(false);
    expect(
      isIsolatedPendingSyncDocFile(
        docRow(`docs/${WORKSPACE_ID}/merge/run-1_merge_report.md`),
        WORKSPACE_ID,
      ),
    ).toBe(false);
    expect(
      isIsolatedPendingSyncDocFile(
        docRow(`docs/${WORKSPACE_ID}/sync-notes.md`),
        WORKSPACE_ID,
      ),
    ).toBe(true);
    expect(
      isIsolatedPendingSyncDocFile(
        docRow(`docs/${WORKSPACE_ID}/archive/run-1_merge_report.md`),
        WORKSPACE_ID,
      ),
    ).toBe(true);
    expect(
      isIsolatedPendingSyncDocFile(
        docRow(`docs/${WORKSPACE_ID}/merge/nested/run-1_merge_report.md`),
        WORKSPACE_ID,
      ),
    ).toBe(true);
    expect(
      isIsolatedPendingSyncDocFile(
        {
          ...docRow(`docs/${WORKSPACE_ID}/a.md`),
          targetDocRef: undefined,
        },
        WORKSPACE_ID,
      ),
    ).toBe(false);
    expect(
      isIsolatedPendingSyncDocFile(
        {
          ...docRow(`docs/${WORKSPACE_ID}/a.md`),
          targetKind: "ontology",
          targetNodeId: "node-1",
        } as unknown as PendingSyncFile,
        WORKSPACE_ID,
      ),
    ).toBe(false);
  });
});

const basePullRequest = {
  id: "pr-1",
  sourceWorkspaceId: "source",
  sourceWorkspaceName: "source",
  targetWorkspaceId: "base",
  targetWorkspaceName: "base",
  status: "open",
  localMergeStatus: "ready",
  providerStatus: "open",
  title: "source into base",
  sourceHeads: [],
  targetHeads: [],
  preflight: {},
  lastPreflightFingerprint: "fingerprint",
  blockingMessage: null,
  createdAt: "2026-07-08T00:00:00Z",
  updatedAt: "2026-07-08T00:00:00Z",
  mergedAt: null,
} as WorkspacePullRequestSummary;

describe("workspacePullRequestProviderOutcome", () => {
  it("distinguishes all-skipped and mixed provider PR outcomes", () => {
    expect(
      workspacePullRequestProviderOutcome({
        ...basePullRequest,
        providerStatus: "action_required",
        providerRepos: [
          {
            id: "provider-code",
            repoId: "repo-code",
            provider: "github",
            providerRepoFullName: "G5Labs/G5",
            sourceRemoteBranch: "g5/source",
            targetRemoteBranch: "main",
            providerPrNumber: null,
            providerPrUrl: null,
            providerStatus: "action_required",
            sourceHeadSha: "source-tip",
            targetHeadSha: "base-tip",
            providerHeadSha: null,
            providerBaseSha: null,
            lastError: "GitHub target branch moved.",
            lastSyncedAt: null,
            updatedAt: "2026-07-08T00:00:00Z",
          },
        ],
      }),
    ).toEqual({
      hasSkippedRepos: true,
      allReposSkipped: true,
      someReposSkipped: false,
    });

    expect(
      workspacePullRequestProviderOutcome({
        ...basePullRequest,
        providerStatus: "partial",
        providerRepos: [
          {
            id: "provider-docs",
            repoId: "repo-docs",
            provider: "github",
            providerRepoFullName: "G5Labs/G5_Docs",
            sourceRemoteBranch: "g5/source",
            targetRemoteBranch: "main",
            providerPrNumber: 42,
            providerPrUrl: "https://github.com/G5Labs/G5_Docs/pull/42",
            providerStatus: "open",
            sourceHeadSha: "docs-source-tip",
            targetHeadSha: "docs-base-tip",
            providerHeadSha: "docs-source-tip",
            providerBaseSha: "docs-base-tip",
            lastError: null,
            lastSyncedAt: "2026-07-08T00:00:00Z",
            updatedAt: "2026-07-08T00:00:00Z",
          },
          {
            id: "provider-code",
            repoId: "repo-code",
            provider: "github",
            providerRepoFullName: "G5Labs/G5",
            sourceRemoteBranch: "g5/source",
            targetRemoteBranch: "main",
            providerPrNumber: null,
            providerPrUrl: null,
            providerStatus: "action_required",
            sourceHeadSha: "code-source-tip",
            targetHeadSha: "code-base-tip",
            providerHeadSha: null,
            providerBaseSha: null,
            lastError: "GitHub target branch moved.",
            lastSyncedAt: null,
            updatedAt: "2026-07-08T00:00:00Z",
          },
        ],
      }),
    ).toEqual({
      hasSkippedRepos: true,
      allReposSkipped: false,
      someReposSkipped: true,
    });
  });
});

describe("githubPublications", () => {
  // g5.git.pr-adapter: GitHub publication facts come from the adapter's
  // GitHub repository pull requests, never local or unpublished ones.
  const record = {
    id: "row",
    repoId: "api",
    provider: "github",
    providerRepoFullName: "acme/api",
    sourceRemoteBranch: "feature",
    targetRemoteBranch: "main",
    providerPrNumber: 42,
    providerPrUrl: "https://github.com/acme/api/pull/42",
    providerStatus: "open",
  } as unknown as WorkspacePullRequestProviderRepoSummary;

  it("reads the GitHub records of the repository pull requests", () => {
    const publications = githubPublications({
      providerRepos: [{ ...record, providerRepoFullName: "stale/raw" }],
      repositoryPullRequests: [
        {
          repoId: "api",
          repository: "acme/api",
          adapter: "github",
          number: 42,
          url: record.providerPrUrl,
          state: "OPEN",
          headRefOid: "a",
          baseRefOid: "b",
          github: record,
        },
        {
          repoId: "docs",
          repository: "docs",
          adapter: "g5",
          number: null,
          url: null,
          state: "OPEN",
          headRefOid: "c",
          baseRefOid: "d",
        },
        {
          repoId: "web",
          repository: "acme/web",
          adapter: "github",
          number: null,
          url: null,
          state: null,
          headRefOid: "e",
          baseRefOid: "f",
        },
      ],
    });
    expect(publications).toEqual([record]);
  });

  it("reads raw records only from a server without repository pull requests", () => {
    expect(githubPublications({ providerRepos: [record] })).toEqual([record]);
  });
});
