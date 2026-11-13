// One seeded workspace for #841's browser-only journeys, using the real app.
import ReactDOM from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import App from "../../src/App";
import "../../src/index.css";
import { installFetchInterceptor } from "../../src/mocks/fetch-interceptor";
import type { WorkspaceApprovalsStateDTO } from "../../src/lib/types";
import { seedMockWorkspaceApprovals } from "../../src/test/msw/handlers";
const workspaceId = "task-graph-rework";
const when = "2026-10-27T10:00:00Z";
const sha = (n: number) => n.toString(16).padStart(40, "0");
const heads = ["code", "docs"].map((repoId) => ({
  repoId,
  workspaceId,
  branchName: "feature",
  headCommit: sha(100),
}));
const definition = {
  id: "policy",
  latestVersion: 1,
  name: "Review policy",
  description: "",
  humans: [],
  agentType: "simple",
  agentPrompt: "Review branch changes.",
  invalidatesPrevious: false,
  requireAllReviewers: false,
  createdBy: { userId: "user-alex", displayName: "Alex Chen" },
  createdAt: when,
  firstCreatedBy: { userId: "user-alex", displayName: "Alex Chen" },
  firstCreatedAt: when,
};
const requests = ["report", "inspection"].map((id, index) => ({
  requestId: id,
  actorType: "agent",
  actorUserId: null,
  actor: null,
  requestedBy: { userId: "user-alex", displayName: "Alex Chen" },
  state: index ? "pending" : "settled",
  definitionVersion: 1,
  definitionName: definition.name,
  requestedAt: when,
  closedAt: index ? null : when,
  note: "Review the branch changes.",
  agentReviews: [
    {
      reviewId: id,
      taskId: null,
      sessionId: null,
      state: index ? "failed" : "approved",
      reportAvailable: !index,
      terminalReason: index ? "Comparison unavailable" : null,
      createdAt: when,
      startedAt: when,
      updatedAt: when,
    },
  ],
}));
seedMockWorkspaceApprovals(workspaceId, {
  requiredInto: [],
  attachmentHistory: [],
  outgoing: {
    targetWorkspaceId: "base",
    targetWorkspaceName: "Base",
    requirements: [
      {
        definition,
        state: "pending",
        approvers: [],
        callerCanDecide: false,
        requests,
        decisionLog: [
          {
            requestId: "report",
            agentReviewId: "report",
            actorType: "agent",
            actor: null,
            outcome: "approved",
            comment: null,
            definitionVersion: 1,
            superseded: false,
            reviewedGitHeads: heads,
            createdAt: when,
          },
        ],
      },
    ],
  },
} as WorkspaceApprovalsStateDTO);
installFetchInterceptor();
const fallback = window.fetch;
const count = (key: string) =>
  sessionStorage.setItem(
    key,
    String(Number(sessionStorage.getItem(key) ?? 0) + 1),
  );
const commits = Array.from({ length: 60 }, (_, i) => ({
  sha: sha(100 - i),
  summary: `Branch change ${60 - i}`,
  authorName: "Author",
  committedAt: 1700000000 + i,
  parents: [sha(99 - i)],
}));
const menu = () => {
  const mode = sessionStorage.getItem("issue931-bases");
  const workspaceBase = mode === "combined" ? sha(30) : sha(10);
  return {
    generation: 0,
    fingerprint: `capture-${localStorage.getItem("issue841-head") ?? "0"}`,
    private: false,
    repos: ["code", "docs"].map((repoId) => ({
      repoId,
      name: repoId === "code" ? "G5-Lean" : "docs",
      role: repoId,
      base: sha(30),
      workspaceBase,
      parent: sha(30),
      parentName: "main",
      pushed: sha(100),
      commits: repoId === "code" ? commits : [commits[0]],
      truncated: false,
      error: null,
      live: null,
      comparison: {
        mergeBase: sha(30),
        mergeBaseCommit: {
          ...commits[0],
          sha: sha(30),
          summary: "Shared ancestor",
        },
        parentMerges: [],
        workspaceBase: {
          ...commits[0],
          sha: workspaceBase,
          summary: "Original fork",
        },
        workspaceBaseInHistory: true,
        sinceWorkspaceBaseCount: mode === "combined" ? 0 : 2,
      },
      ...(mode === "base"
        ? {
            base: null,
            workspaceBase: null,
            parent: null,
            unpushedOnly: true,
            commits: [],
            comparison: null,
          }
        : {}),
    })),
  };
};
const patches = [
  { repoId: "code", path: "src/app.ts", kind: "code" },
  { repoId: "code", path: ".g5/ontology/app.g5.md", kind: "ontology" },
  { repoId: "docs", path: "docs/plan.md", kind: "docs" },
].map((file) => ({
  ...file,
  repoName: file.repoId === "code" ? "G5-Lean" : "docs",
  fileKey: file.path,
  label: "Pushed changes vs merge base",
  before: sha(30),
  after: sha(100),
  file: {
    path: file.path,
    oldPath: null,
    change: "modified",
    diff: {
      status: "text",
      additions: 1,
      deletions: 0,
      hunks: [
        {
          startLine: 1,
          header: "@@ -0,0 +1 @@",
          lines: [
            {
              type: "addition",
              newLine: 1,
              content:
                file.kind === "code"
                  ? `export const value = "${"wide ".repeat(250)}";`
                  : "Updated branch content",
            },
          ],
        },
      ],
    },
  },
}));
const snapshot = { patches, fileCount: 3, private: false };
const provider = {
  id: "provider",
  repoId: "code",
  provider: "github",
  providerRepoFullName: "G5Labs/G5-Lean",
  sourceRemoteBranch: "feature",
  targetRemoteBranch: "main",
  providerPrNumber: 841,
  providerPrUrl: "https://github.com/G5Labs/G5-Lean/pull/841",
  providerStatus: "open",
  sourceHeadSha: sha(100),
  targetHeadSha: sha(30),
  providerHeadSha: sha(100),
  providerBaseSha: sha(30),
  lastError: null,
  lastSyncedAt: when,
  updatedAt: when,
};
const pull = {
  id: "pr",
  sourceWorkspaceId: workspaceId,
  sourceWorkspaceName: "Task Graph Rework",
  targetWorkspaceId: "base",
  targetWorkspaceName: "Base",
  status: "open",
  localMergeStatus: "ready",
  providerStatus: "open",
  providerRepos: [provider],
  title: "Workspace landing and review fixes",
  descriptionOverride:
    "Minimal changes to workspace navigation, history, and approval readers.",
  sourceHeads: heads,
  targetHeads: [],
  preflight: {},
  lastPreflightFingerprint: "",
  blockingMessage: null,
  createdAt: when,
  updatedAt: when,
  mergedAt: null,
};
window.fetch = async (input, init) => {
  const request = new Request(
    input instanceof Request ? input : new URL(String(input), location.origin),
    init,
  );
  const url = new URL(request.url);
  const path = url.pathname;
  const ok = (data: unknown) => Response.json({ data });
  if (path.endsWith("/source-control/menu")) {
    count("issue841-menu-reads");
    return ok(menu());
  }
  if (path.endsWith("/source-control/checkpoint")) return ok({ generation: 0 });
  if (
    path.endsWith("/source-control/pushed") ||
    path.endsWith("/source-control/apply")
  ) {
    count("issue841-diff-reads");
    if (path.endsWith("/source-control/apply"))
      sessionStorage.setItem("issue931-last-apply", await request.text());
    return ok(snapshot);
  }
  if (path.endsWith("/source-control/history")) {
    count("issue841-history-reads");
    const body = await request.json();
    if (sessionStorage.getItem("issue841-large-history")) {
      const start = Number(body.cursor?.split(":")[1] ?? 0);
      return ok({
        commits: Array.from({ length: 50 }, (_, offset) => ({
          ...commits[0],
          sha: sha(10_000 + start + offset),
          summary: `Historical commit ${String(start + offset).padStart(4, "0")}`,
        })),
        nextCursor: start + 50 < 1000 ? `large:${start + 50}` : null,
      });
    }
    return ok({
      commits: [
        {
          ...commits[0],
          sha: sha(body.cursor ? 21 : 22),
          summary: body.cursor
            ? "Older parent change"
            : "Imported parent change",
        },
      ],
      nextCursor: body.cursor ? null : sha(22),
    });
  }
  if (path.endsWith("/review-flow"))
    return ok({
      workspace: {
        ownerUserId: "user-alex",
        targetWorkspaceId: "base",
        phase: "ie",
        readOnly: false,
      },
      pr: pull,
      heads,
      baseHeads: [],
      parentHeads: [],
      pushedAt: {},
      repos: [
        { id: "code", name: "G5-Lean", role: "code", rootPath: "code" },
        { id: "docs", name: "docs", role: "docs", rootPath: "docs" },
      ],
      requests: [],
      decisions: [],
    });
  if (path.endsWith("/review-flow/changes"))
    return ok({
      beforeHeads: [],
      afterHeads: heads,
      baselineAvailable: true,
      files: patches.map((p) => ({
        repoId: p.repoId,
        path: p.path,
        status: "modified",
        kind: p.kind,
        additions: 1,
        deletions: 0,
      })),
      file: null,
    });
  if (path.endsWith("/pull-requests")) return ok([pull]);
  if (path.endsWith("/pull-requests/pr/description"))
    return ok({
      pullRequestId: "pr",
      description: pull.descriptionOverride,
      descriptionSource: "override",
      descriptionHtml: null,
      repositories: [],
    });
  if (path.match(/\/approvals\/reviews\/[^/]+\/report$/)) {
    const inspection = path.includes("/inspection/");
    return ok({
      reviewId: inspection ? "inspection" : "report",
      requestId: inspection ? "inspection" : "report",
      request: {
        note: "Review the branch changes.",
        requestedBy: "Alex Chen",
        requestedAt: when,
      },
      definition: { agent_prompt: "Review branch changes." },
      assignment: inspection
        ? null
        : {
            resolution: {
              schemaVersion: 4,
              assignedAt: when,
              repositoryNames: { code: "G5-Lean", docs: "docs" },
              sourceHeads: heads,
              targetHeads: heads,
              workspaceBaseHeads: heads,
              mergeBase: {
                repositories: heads.map((h) => ({
                  repoId: h.repoId,
                  repoName: h.repoId,
                  gitSha: sha(30),
                })),
              },
            },
          },
      failureStage: inspection ? "capture" : null,
      failureCause: inspection ? "Comparison unavailable" : null,
      sha256: "a".repeat(64),
      document: inspection
        ? null
        : "---\nschemaVersion: 4\nassignedAt: 2026-10-27T10:00:00Z\nsubmittedAt: 2026-10-27T10:01:00Z\n---\n\n# Review report\n\nReviewed the captured branch changes.\n\n" +
          "Readable report evidence.\n\n".repeat(80),
    });
  }
  if (path.endsWith("/transcript"))
    return ok({ items: [], nextCursor: null, eventsTrimmed: false });
  return fallback(input, init);
};
ReactDOM.createRoot(document.getElementById("root")!).render(
  <QueryClientProvider
    client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
  >
    <App />
  </QueryClientProvider>,
);
