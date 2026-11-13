/**
 * Workspace landing (ui g5.workbench.landing.*, git g5.git.pr-description):
 * the summary line's destinations, the Workspace task with its linked issues
 * and readers, and the Implementation description for GitHub-published,
 * G5-only, merged and not-yet-created PRs.
 */
import { beforeEach, describe, expect, it } from "vitest";
import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { Route, Routes } from "react-router-dom";
import { renderApp } from "../../test/render";
import { server } from "../../test/msw/server";
import { createMockUser } from "../../test/fixtures";
import {
  makeNativeIssueDetail,
  makeNativeIssueLink,
  makeNativeIssueSummary,
  serveExternalIssuesFeature,
  serveNativeIssuesFeature,
} from "../../test/externalIssueFixtures";
import type {
  PullRequestDescriptionDTO,
  WorkspaceIssueLinkDTO,
  WorkspacePullRequestSummary,
} from "../../lib/types";
import { workspacePullRequestsQueryOptions } from "../../hooks/useWorkspace";
import { WorkspaceLanding } from "./WorkspaceLanding";
import { approvalSummary, type ApprovalSummary } from "../../lib/landingStatus";
import {
  approvalDefinition,
  approvalRequirement,
  approvalsState,
} from "../../test/approvalFixtures";
import { seedMockWorkspaceApprovals } from "../../test/msw/handlers";

const WS = "ws-landing";
const V1 = "/api/v1";
const ORG = "org-g5labs";

const SHA_API = "80e4b9a1111111111111111111111111111111aa";
const SHA_DOCS = "5813f5b2222222222222222222222222222222bb";

function overview() {
  return {
    workspace: {
      ownerUserId: "user-alex",
      targetWorkspaceId: "parent",
      phase: "be",
      readOnly: false,
    },
    pr: null,
    heads: [],
    baseHeads: null,
    parentHeads: null,
    pushedAt: {},
    repos: [
      { id: "repo-api", name: "G5-Lean", role: "code", rootPath: "code" },
      { id: "repo-docs", name: "docs", role: "docs", rootPath: "docs" },
    ],
    requests: [],
    decisions: [],
  };
}

function link(
  overrides: Partial<WorkspaceIssueLinkDTO> & {
    number: number;
    title: string;
    state: "open" | "closed";
    role: "primary" | "related";
    repo?: string;
  },
): WorkspaceIssueLinkDTO {
  const repo = overrides.repo ?? "G5Labs/G5-Lean";
  const displayKey = `${repo}#${overrides.number}`;
  return {
    provider: "github",
    id: `link-${overrides.number}`,
    workspaceId: WS,
    trackerId: "tracker",
    externalIssueId: String(overrides.number),
    displayKey,
    canonicalUrl: `https://github.com/${repo}/issues/${overrides.number}`,
    locator: { repositoryId: repo, number: overrides.number },
    role: overrides.role,
    linkedBy: "user-alex",
    linkedAt: "2026-10-01T00:00:00Z",
    issue: {
      provider: "github",
      trackerId: "tracker",
      domainId: "domain",
      externalIssueId: String(overrides.number),
      locator: { repositoryId: repo, number: overrides.number },
      displayKey,
      canonicalUrl: `https://github.com/${repo}/issues/${overrides.number}`,
      title: overrides.title,
      state: overrides.state,
      stateReason: overrides.state === "closed" ? "completed" : null,
      assignees: [],
      labels: [],
      updatedAt: "2026-10-01T00:00:00Z",
      contentTrusted: true,
      primaryWorkspace: null,
    },
    issueError: null,
  };
}

function issueDetail(number: number, title: string, body: string) {
  return {
    ...link({ number, title, state: "open", role: "primary" }).issue,
    body,
    author: null,
    createdAt: "2026-10-01T00:00:00Z",
    closedAt: null,
    commentCount: 0,
    comments: [],
    commentsTruncated: false,
    links: [],
  };
}

function pull(
  overrides: Partial<WorkspacePullRequestSummary> = {},
): WorkspacePullRequestSummary {
  return {
    id: "pr-1",
    sourceWorkspaceId: WS,
    sourceWorkspaceName: "issue-750",
    targetWorkspaceId: "parent",
    targetWorkspaceName: "Base",
    status: "open",
    localMergeStatus: "ready",
    providerStatus: "open",
    providerRepos: [],
    title: "Repository browsing and change selection",
    descriptionOverride: "Saved description",
    sourceHeads: [
      {
        repoId: "repo-api",
        workspaceId: WS,
        branchName: "b",
        headCommit: SHA_API,
      },
      {
        repoId: "repo-docs",
        workspaceId: WS,
        branchName: "b",
        headCommit: SHA_DOCS,
      },
    ],
    targetHeads: [],
    preflight: {} as WorkspacePullRequestSummary["preflight"],
    lastPreflightFingerprint: "",
    blockingMessage: null,
    createdAt: "2026-10-20T00:00:00Z",
    updatedAt: "2026-10-21T00:00:00Z",
    mergedAt: null,
    ...overrides,
  };
}

function providerRepo(
  repoId: string,
  name: string,
  number: number,
  providerStatus: "open" | "merged",
) {
  return {
    id: `prov-${repoId}`,
    repoId,
    provider: "github",
    providerRepoFullName: name,
    sourceRemoteBranch: "b",
    targetRemoteBranch: "main",
    providerPrNumber: number,
    providerPrUrl: `https://github.com/${name}/pull/${number}`,
    providerStatus,
    sourceHeadSha: null,
    targetHeadSha: null,
    providerHeadSha: null,
    providerBaseSha: null,
    lastError: null,
    lastSyncedAt: null,
    updatedAt: "2026-10-21T00:00:00Z",
  } as const;
}

const LONG_BODY =
  "Files browses complete authorized repository trees.\n\nThe [scope amendment](https://example.com/amend) adds file-type emphasis.";

let descriptionCalls = 0;

/** What the server resolves for the default PR's saved description. */
const SAVED_DESCRIPTION: PullRequestDescriptionDTO = {
  pullRequestId: "pr-1",
  description: "Saved description",
  descriptionSource: "saved",
  descriptionHtml: null,
  repositories: [],
};

function serve({
  links = [] as WorkspaceIssueLinkDTO[],
  pulls = [] as WorkspacePullRequestSummary[],
  description = SAVED_DESCRIPTION as PullRequestDescriptionDTO,
  changes = [
    { repoId: "repo-api", path: "src/a.ts", kind: "code" },
    { repoId: "repo-api", path: "src/b.ts", kind: "code" },
    { repoId: "repo-docs", path: "README.md", kind: "docs" },
    { repoId: "repo-api", path: ".g5/ontology/root.g5.md", kind: "ontology" },
  ],
} = {}) {
  descriptionCalls = 0;
  serveExternalIssuesFeature();
  server.use(
    http.get(`${V1}/workspaces/${WS}/review-flow`, () =>
      HttpResponse.json({ data: overview() }),
    ),
    http.get(`${V1}/workspaces/${WS}/review-flow/changes`, () =>
      HttpResponse.json({
        data: {
          files: changes,
          baselineAvailable: true,
          afterHeads: pull().sourceHeads,
        },
      }),
    ),
    http.get(`${V1}/workspaces/${WS}/issue-links`, () =>
      HttpResponse.json({ data: { links } }),
    ),
    http.get(`${V1}/workspaces/${WS}/activity`, () =>
      HttpResponse.json({
        data: {
          primaryIssue: null,
          entries: [],
          earlierCursor: null,
          composer: null,
        },
      }),
    ),
    http.get(`${V1}/workspaces/${WS}/pull-requests`, () =>
      HttpResponse.json({ data: pulls }),
    ),
    http.get(`${V1}/workspaces/${WS}/pull-requests/:id/description`, () => {
      descriptionCalls += 1;
      return HttpResponse.json({ data: description });
    }),
    http.get(`${V1}/orgs/${ORG}/issues/github/:repo/:number`, ({ params }) => {
      const number = Number(params.number);
      return HttpResponse.json({
        data:
          number === 750
            ? issueDetail(
                750,
                "Simplify workspace browsing",
                `Primary body.\n\n${LONG_BODY}`,
              )
            : issueDetail(number, `Issue ${number}`, `Body of issue ${number}`),
      });
    }),
  );
}

function renderLanding(
  approvals: ApprovalSummary = {
    kind: "none-required",
    text: "No approvals required",
  },
) {
  return renderApp(
    <Routes>
      <Route
        path="/:org/:workspace"
        element={
          <WorkspaceLanding
            workspaceId={WS}
            workspaceName="workspace-browsing"
            description={LONG_BODY}
            docsRepoId={null}
            approvals={approvals}
          />
        }
      />
    </Routes>,
    { route: "/g5labs/workspace-browsing", user: createMockUser() },
  );
}

beforeEach(() => {
  descriptionCalls = 0;
});

describe("summary line", () => {
  it("omits redundant category zeros only when the total is known to be zero", async () => {
    serve({ changes: [] });
    renderLanding();
    await waitFor(() =>
      expect(screen.getByTestId("landing-summary-changed")).toHaveTextContent(
        "0 changed files",
      ),
    );
    expect(screen.getByTestId("landing-summary-changed")).toHaveAttribute(
      "href",
      expect.stringContaining("?view=source-control"),
    );
    for (const kind of ["code", "ontology", "docs"])
      expect(screen.queryByTestId(`landing-summary-${kind}`)).toBeNull();
  });

  it("does not treat unavailable change counts as zero", async () => {
    serve();
    server.use(
      http.get(`${V1}/workspaces/${WS}/review-flow/changes`, () =>
        HttpResponse.json({ data: { files: [], baselineAvailable: false } }),
      ),
    );
    renderLanding();
    await screen.findByText("No recorded comparison baseline is available.");
    expect(screen.getByTestId("landing-summary-changed")).toHaveTextContent(
      "Changed files",
    );
    expect(screen.getByTestId("landing-summary")).not.toHaveTextContent(
      "0 changed files",
    );
    for (const kind of ["code", "ontology", "docs"])
      expect(screen.getByTestId(`landing-summary-${kind}`)).toBeVisible();
  });

  it("reads approvals as requirement counts in the same linked place (g5.workbench.landing.summary)", async () => {
    serve();
    renderLanding(
      approvalSummary(
        approvalsState([
          approvalRequirement({ state: "approved" }),
          approvalRequirement({ state: "approved" }),
          approvalRequirement({ state: "rejected" }),
          approvalRequirement({ state: "pending" }),
        ]),
        "ok",
      ),
    );
    const summary = await screen.findByTestId("landing-summary");
    const approvals = within(summary).getByTestId("landing-summary-approvals");
    expect(approvals).toHaveTextContent(
      /^Approvals\s*· 2 approved, 1 rejected, 1 pending$/,
    );
    expect(approvals.getAttribute("href")).toMatch(/\?view=approvals$/);
    // It still leads the line, before the changed-file counts.
    expect(summary.firstElementChild).toBe(approvals);
  });

  it("links approvals and every count to its existing destination", async () => {
    serve();
    renderLanding();
    const summary = await screen.findByTestId("landing-summary");
    await waitFor(() =>
      expect(
        within(summary).getByTestId("landing-summary-changed"),
      ).toHaveTextContent("4 changed files"),
    );
    for (const [id, text, view] of [
      ["landing-summary-approvals", "No approvals required", "approvals"],
      ["landing-summary-code", "Code 2", "source-control"],
      ["landing-summary-ontology", "Ontology 1", "ontology"],
      ["landing-summary-docs", "Docs 1", "docs"],
    ]) {
      const item = within(summary).getByTestId(id);
      expect(item).toHaveTextContent(text);
      expect(item.getAttribute("href")).toMatch(new RegExp(`\\?view=${view}$`));
    }
    const total = within(summary).getByTestId("landing-summary-changed");
    const target = new URL(total.getAttribute("href")!, "https://g5.test");
    expect(target.searchParams.get("view")).toBe("source-control");
    expect(target.searchParams.has("scope")).toBe(false);
    expect(target.searchParams.has("heads")).toBe(false);
  });
});

it("keeps commit history out of the implementation summary and reader", async () => {
  serve({ pulls: [pull()] });
  server.use(
    http.get(`${V1}/workspaces/${WS}/review-flow`, () =>
      HttpResponse.json({
        data: { ...overview(), pr: pull(), heads: pull().sourceHeads },
      }),
    ),
  );
  renderLanding();
  const section = await screen.findByTestId("landing-implementation");
  await within(section).findByTestId("workspace-read-implementation");
  expect(within(section).queryByText("Commit History")).toBeNull();
  await userEvent.click(
    within(section).getByTestId("workspace-read-implementation"),
  );
  const reader = await screen.findByRole("dialog", {
    name: "Implementation description",
  });
  expect(within(reader).queryByText("Commit History")).toBeNull();
  expect(within(reader).getAllByTestId("landing-repository-row")).toHaveLength(
    2,
  );
  expect(reader).toHaveTextContent(SHA_API.slice(0, 7));
  expect(reader).toHaveTextContent(SHA_DOCS.slice(0, 7));
});

describe("Workspace task", () => {
  it("shows the primary issue's title row and collapsed linked issues without duplicating it", async () => {
    serve({
      links: [
        link({
          number: 750,
          title: "Simplify workspace browsing",
          state: "open",
          role: "primary",
        }),
        link({
          number: 769,
          title: "Change history blows up",
          state: "closed",
          role: "related",
        }),
        link({
          number: 793,
          title: "Unify file editing",
          state: "open",
          role: "related",
        }),
      ],
    });
    renderLanding();
    const task = await screen.findByTestId("landing-task");
    await waitFor(() =>
      expect(within(task).getByTestId("landing-task-title")).toHaveTextContent(
        "Simplify workspace browsing",
      ),
    );
    expect(within(task).getByTestId("landing-task-status")).toHaveTextContent(
      "Open",
    );
    const github = within(task).getByTestId("landing-task-github");
    expect(github).toHaveTextContent("GitHub issue #750");
    expect(github).toHaveAttribute(
      "href",
      "https://github.com/G5Labs/G5-Lean/issues/750",
    );
    expect(task).not.toHaveTextContent("Primary issue");
    expect(task).not.toHaveTextContent("Related issues");

    const toggle = within(task).getByTestId("landing-linked-issues-toggle");
    expect(toggle).toHaveTextContent("Linked issues (2)");
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(within(task).queryByTestId("landing-linked-issue-row")).toBeNull();
    await userEvent.click(toggle);
    const rows = within(task).getAllByTestId("landing-linked-issue-row");
    expect(rows).toHaveLength(2);
    expect(rows[0]).toHaveTextContent("Change history blows up");
    expect(rows[0]).toHaveTextContent("Closed");
    expect(rows[0]).not.toHaveTextContent(/completed/i);
    expect(rows[0]).toHaveTextContent("GitHub issue #769");
    // Read more stays at the bottom, below the expanded list.
    const readMore = within(task).getByTestId(
      "workspace-read-task-description",
    );
    expect(
      rows[1].compareDocumentPosition(readMore) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it("opens the full task then the expanded list in a reader with one close control", async () => {
    serve({
      links: [
        link({
          number: 750,
          title: "Simplify workspace browsing",
          state: "open",
          role: "primary",
        }),
        link({
          number: 769,
          title: "Change history blows up",
          state: "closed",
          role: "related",
        }),
      ],
    });
    renderLanding();
    const task = await screen.findByTestId("landing-task");
    const preview = await within(task).findByTestId("landing-task-preview");
    await userEvent.click(
      within(task).getByTestId("workspace-read-task-description"),
    );
    const dialog = screen.getByRole("dialog", { name: "Workspace task" });
    const content = within(dialog).getByRole("region", {
      name: "Full task description",
    });
    // The preview is the reader's own rendering: identical opening and links.
    expect(content.innerHTML.startsWith(preview.innerHTML.slice(0, 200))).toBe(
      true,
    );
    expect(
      within(content).getByRole("link", { name: "scope amendment" }),
    ).toHaveAttribute("href", "https://example.com/amend");
    // The linked list is expanded in the reader although collapsed on the landing.
    expect(
      within(dialog).getAllByTestId("landing-reader-linked-issue-row"),
    ).toHaveLength(1);
    expect(
      within(dialog).getAllByRole("button", { name: "Close" }),
    ).toHaveLength(1);

    // A linked issue opens its own description and identity.
    await userEvent.click(
      within(dialog).getByTestId("landing-reader-linked-issue-open"),
    );
    const issueReader = await screen.findByRole("dialog", {
      name: "Linked issue",
    });
    expect(
      await within(issueReader).findByText("Body of issue 769"),
    ).toBeVisible();
    expect(within(issueReader).queryByText("Primary body.")).toBeNull();
    expect(
      within(issueReader).queryByTestId("landing-reader-linked-issue-row"),
    ).toBeNull();
  });

  it("qualifies issue numbers only when several repositories make them ambiguous", async () => {
    serve({
      links: [
        link({
          number: 750,
          title: "Simplify workspace browsing",
          state: "open",
          role: "primary",
        }),
        link({
          number: 12,
          title: "Docs follow-up",
          state: "open",
          role: "related",
          repo: "G5Labs/docs",
        }),
      ],
    });
    renderLanding();
    const task = await screen.findByTestId("landing-task");
    expect(
      await within(task).findByTestId("landing-task-github"),
    ).toHaveTextContent("GitHub issue G5-Lean#750");
    await userEvent.click(
      within(task).getByTestId("landing-linked-issues-toggle"),
    );
    expect(
      within(task).getByTestId("landing-linked-issue-github"),
    ).toHaveTextContent("GitHub issue docs#12");
  });

  it("shows the native task without inventing an issue", async () => {
    serve();
    renderLanding();
    const task = await screen.findByTestId("landing-task");
    await waitFor(() =>
      expect(within(task).getByTestId("landing-task-title")).toHaveTextContent(
        "workspace-browsing",
      ),
    );
    expect(
      await within(task).findByTestId("landing-task-preview"),
    ).toHaveTextContent("Files browses complete authorized repository trees.");
    expect(within(task).queryByTestId("landing-task-status")).toBeNull();
    expect(within(task).queryByTestId("landing-task-github")).toBeNull();
    expect(
      within(task).queryByTestId("landing-linked-issues-toggle"),
    ).toBeNull();
  });
});

describe("Workspace task with G5 issue attachments", () => {
  it("loads the primary and a linked G5 issue's images through the API, never by their page address (g5.issue.native.attachments)", async () => {
    serve({
      links: [
        makeNativeIssueLink({ role: "primary" }),
        makeNativeIssueLink({
          id: "link-13",
          role: "related",
          issue: makeNativeIssueSummary({
            locator: { repositoryId: "g5", number: 13 },
            title: "Linked native issue",
          }),
        }),
      ],
    });
    serveNativeIssuesFeature();
    const reads: string[] = [];
    server.use(
      http.get(`${V1}/orgs/${ORG}/issues/g5/:number`, ({ params }) => {
        const number = Number(params.number);
        const page = `http://localhost:3000/g5labs/issues/g5/${number}`;
        return HttpResponse.json({
          data: makeNativeIssueDetail({
            locator: { repositoryId: "g5", number },
            body: `See\n\n![shot-${number}.png](${page}/attachments/${number}0)`,
          }),
        });
      }),
      http.get(
        `${V1}/orgs/${ORG}/issues/g5/:number/attachments/:id`,
        ({ params }) => {
          reads.push(`${String(params.number)}/${String(params.id)}`);
          return new HttpResponse(new Uint8Array([137, 80, 78, 71]), {
            headers: { "Content-Type": "image/png" },
          });
        },
      ),
    );
    renderLanding();
    const task = await screen.findByTestId("landing-task");
    const preview = await within(task).findByTestId("landing-task-preview");
    const image = await within(preview).findByTestId("native-attachment-image");
    expect(image).toHaveAttribute("alt", "shot-12.png");
    await waitFor(() =>
      expect(image).toHaveAttribute("src", expect.stringMatching(/^blob:/)),
    );

    await userEvent.click(
      within(task).getByTestId("landing-linked-issues-toggle"),
    );
    await userEvent.click(
      within(task).getByTestId("landing-linked-issue-open"),
    );
    const reader = await screen.findByTestId("landing-linked-issue-reader");
    expect(
      await within(reader).findByTestId("native-attachment-image"),
    ).toHaveAttribute("alt", "shot-13.png");

    await waitFor(() => expect(reads.sort()).toEqual(["12/120", "13/130"]));
    for (const element of document.querySelectorAll("img")) {
      expect(element.getAttribute("src") ?? "").not.toContain("/attachments/");
    }
  });
});

describe("Implementation description", () => {
  it.each([null, "", "  \n  "])(
    "distinguishes a failed canonical read from an absent description (override %j)",
    async (descriptionOverride) => {
      serve({ pulls: [pull({ descriptionOverride })] });
      server.use(
        http.get(`${V1}/workspaces/${WS}/pull-requests/:id/description`, () =>
          HttpResponse.json(
            { error: "Temporarily unavailable" },
            { status: 503 },
          ),
        ),
      );
      renderLanding();
      const implementation = await screen.findByTestId(
        "landing-implementation",
      );
      expect(
        await within(implementation).findByTestId(
          "landing-implementation-description-error",
        ),
      ).toHaveTextContent("The description could not be loaded.");
      expect(
        within(implementation).queryByTestId(
          "landing-implementation-no-description",
        ),
      ).toBeNull();
      await userEvent.click(
        within(implementation).getByTestId("workspace-read-implementation"),
      );
      const reader = await screen.findByRole("dialog", {
        name: "Implementation description",
      });
      expect(
        within(reader).getByTestId("landing-implementation-description-error"),
      ).toHaveTextContent("The description could not be loaded.");
      expect(
        within(reader).queryByTestId("landing-implementation-no-description"),
      ).toBeNull();
    },
  );

  it("shows the saved description, marked as possibly stale, when the canonical read fails", async () => {
    serve({ pulls: [pull()] });
    server.use(
      http.get(`${V1}/workspaces/${WS}/pull-requests/:id/description`, () =>
        HttpResponse.json(
          { error: "Temporarily unavailable" },
          { status: 503 },
        ),
      ),
    );
    renderLanding();
    const implementation = await screen.findByTestId("landing-implementation");
    expect(
      await within(implementation).findByText("Saved description"),
    ).toBeVisible();
    expect(
      within(implementation).getByTestId(
        "landing-implementation-description-stale",
      ),
    ).toHaveTextContent("may be out of date");
    expect(
      within(implementation).queryByTestId(
        "landing-implementation-description-error",
      ),
    ).toBeNull();
  });

  it("keeps the loaded implementation after a background PR refresh fails", async () => {
    serve({ pulls: [pull()] });
    const { queryClient } = renderLanding();
    const implementation = await screen.findByTestId("landing-implementation");
    await within(implementation).findByText(
      "Repository browsing and change selection",
    );
    server.use(
      http.get(`${V1}/workspaces/${WS}/pull-requests`, () =>
        HttpResponse.json(
          { error: "Temporarily unavailable" },
          { status: 503 },
        ),
      ),
    );
    const { queryKey } = workspacePullRequestsQueryOptions(WS, "source", "all");
    await act(async () => {
      await queryClient.refetchQueries({ queryKey });
    });
    expect(queryClient.getQueryState(queryKey)?.status).toBe("error");
    expect(implementation).toHaveTextContent(
      "Repository browsing and change selection",
    );
    expect(
      within(implementation).getAllByTestId("landing-repository-row"),
    ).toHaveLength(2);
    expect(
      within(implementation).getByTestId("workspace-read-implementation"),
    ).toBeVisible();
  });

  it("shows a retry when the initial PR read fails", async () => {
    serve();
    server.use(
      http.get(`${V1}/workspaces/${WS}/pull-requests`, () =>
        HttpResponse.json(
          { error: "Temporarily unavailable" },
          { status: 503 },
        ),
      ),
    );
    renderLanding();
    const implementation = await screen.findByTestId("landing-implementation");
    expect(await within(implementation).findByRole("alert")).toHaveTextContent(
      "Could not load the implementation.",
    );
    expect(
      within(implementation).getByRole("button", { name: "Retry" }),
    ).toBeEnabled();
  });

  it("before the first PR says only that there is no description", async () => {
    serve();
    renderLanding();
    const section = await screen.findByTestId("landing-implementation");
    expect(
      await within(section).findByTestId("landing-implementation-empty"),
    ).toHaveTextContent("No implementation description yet.");
    expect(within(section).queryByRole("button")).toBeNull();
    expect(section).not.toHaveTextContent(
      /Create PR|Update PR|Submit PR|Open|Draft/,
    );
    expect(descriptionCalls).toBe(0);
  });

  it("shows a changed member's local repository PR and marks unchanged members", async () => {
    // g5.git.pr-repository: a local repository PR is named by its G5 PR's
    // number, written PR###### (g5.git.pr-local-number), and shares its
    // lifecycle; an unchanged member has none.
    serve({
      pulls: [
        pull({
          number: "PR000007",
          repositoryPullRequests: [
            {
              repoId: "repo-api",
              repository: "G5-Lean",
              adapter: "g5",
              number: null,
              url: null,
              state: "OPEN",
              headRefOid: SHA_API,
              baseRefOid: SHA_DOCS,
            },
          ],
        }),
      ],
    });
    renderLanding();
    const section = await screen.findByTestId("landing-implementation");
    const rows = await within(section).findAllByTestId(
      "landing-repository-row",
    );
    expect(rows).toHaveLength(2);
    expect(
      within(rows[0]).getByTestId("landing-repository-g5"),
    ).toHaveTextContent(/^PR000007$/);
    expect(
      within(rows[0]).getByTestId("landing-repository-status"),
    ).toHaveTextContent("Open");
    expect(
      within(rows[1]).getByTestId("landing-repository-unchanged"),
    ).toHaveTextContent("Unchanged");
    expect(
      within(rows[1]).queryByTestId("landing-repository-status"),
    ).toBeNull();
    expect(section).not.toHaveTextContent(/GitHub PR|G5 PR|#7/);
  });

  it("shows a local repository PR whose member only the preflight evidence lists", async () => {
    // The server lists such a member's repository PR (g5.git.pr-repository);
    // the landing shows the same rows as g5d.
    serve({
      pulls: [
        pull({
          number: "PR000007",
          sourceHeads: [
            {
              repoId: "repo-api",
              workspaceId: WS,
              branchName: "b",
              headCommit: SHA_API,
            },
          ],
          repositoryPullRequests: [
            {
              repoId: "repo-docs",
              repository: "docs",
              adapter: "g5",
              number: null,
              url: null,
              state: "OPEN",
              headRefOid: SHA_DOCS,
              baseRefOid: SHA_API,
            },
          ],
        }),
      ],
    });
    renderLanding();
    const section = await screen.findByTestId("landing-implementation");
    const rows = await within(section).findAllByTestId(
      "landing-repository-row",
    );
    expect(rows).toHaveLength(2);
    expect(rows[1]).toHaveTextContent("docs");
    expect(
      within(rows[1]).getByTestId("landing-repository-g5"),
    ).toHaveTextContent(/^PR000007$/);
    expect(
      within(rows[1]).getByTestId("landing-repository-status"),
    ).toHaveTextContent("Open");
  });

  it("shows G5-only rows with SHAs and no provider metadata", async () => {
    serve({
      pulls: [pull()],
      description: {
        pullRequestId: "pr-1",
        description: LONG_BODY,
        descriptionSource: "saved",
        descriptionHtml: null,
        repositories: [],
      },
    });
    renderLanding();
    const section = await screen.findByTestId("landing-implementation");
    expect(
      await within(section).findByTestId("landing-implementation-status"),
    ).toHaveTextContent("Open");
    const rows = await within(section).findAllByTestId(
      "landing-repository-row",
    );
    expect(rows.map((r) => r.textContent)).toEqual([
      expect.stringContaining("G5-Lean"),
      expect.stringContaining("docs"),
    ]);
    expect(rows[0]).toHaveTextContent(SHA_API.slice(0, 7));
    expect(
      within(rows[0]).getByLabelText(`Commit ${SHA_API}`),
    ).toBeInTheDocument();
    expect(
      within(section).queryByTestId("landing-repository-github"),
    ).toBeNull();
    expect(
      within(section).queryByTestId("landing-repository-status"),
    ).toBeNull();
    expect(
      within(section).queryByTestId("landing-implementation-divergence"),
    ).toBeNull();
    expect(section).not.toHaveTextContent(/GitHub|Update PR|PR submitted/);
    await waitFor(() =>
      expect(
        within(section).getByTestId("landing-implementation-preview"),
      ).toHaveTextContent(
        "Files browses complete authorized repository trees.",
      ),
    );
  });

  it("keeps the G5 status and each provider status distinct and shows matches without tabs", async () => {
    serve({
      pulls: [
        pull({
          providerRepos: [
            providerRepo("repo-api", "G5Labs/G5-Lean", 754, "open"),
            providerRepo("repo-docs", "G5Labs/docs", 128, "open"),
          ],
        }),
      ],
      description: {
        pullRequestId: "pr-1",
        description: LONG_BODY,
        descriptionSource: "saved",
        descriptionHtml:
          '<p>Rendered by GitHub with <a href="https://github.com/G5Labs/G5-Lean/issues/769">G5Labs/G5-Lean#769</a>.</p>',
        repositories: [
          {
            repoId: "repo-api",
            repository: "G5Labs/G5-Lean",
            number: 754,
            url: null,
            lifecycle: "open",
            comparison: "matches",
            body: "x",
            bodyHtml: "<p>x</p>",
            error: null,
          },
          {
            repoId: "repo-docs",
            repository: "G5Labs/docs",
            number: 128,
            url: null,
            lifecycle: "merged",
            comparison: "matches",
            body: "y",
            bodyHtml: "<p>y</p>",
            error: null,
          },
        ],
      },
    });
    renderLanding();
    const section = await screen.findByTestId("landing-implementation");
    await waitFor(() =>
      expect(
        within(section)
          .getAllByTestId("landing-repository-status")
          .map((s) => s.textContent),
      ).toEqual(["Open", "Merged"]),
    );
    expect(
      within(section).getByTestId("landing-implementation-status"),
    ).toHaveTextContent("Open");
    const links = within(section).getAllByTestId("landing-repository-github");
    expect(links.map((l) => l.textContent)).toEqual([
      "GitHub PR #754",
      "GitHub PR #128",
    ]);
    const rendered = within(section).getByRole("link", {
      name: "G5Labs/G5-Lean#769",
    });
    expect(rendered).toHaveAttribute("target", "_blank");
    expect(
      within(section).queryByTestId("landing-implementation-divergence"),
    ).toBeNull();

    await userEvent.click(
      within(section).getByTestId("workspace-read-implementation"),
    );
    const dialog = screen.getByRole("dialog", {
      name: "Implementation description",
    });
    expect(
      within(dialog).getByTestId("landing-description-comparison"),
    ).toHaveTextContent("Matches GitHub");
    expect(within(dialog).queryByRole("tablist")).toBeNull();
    expect(
      within(dialog).getAllByTestId("landing-repository-row"),
    ).toHaveLength(2);
    expect(
      within(dialog).getAllByRole("button", { name: "Close" }),
    ).toHaveLength(1);
  });

  it("names a difference and offers each source without replacing G5 content", async () => {
    serve({
      pulls: [
        pull({
          providerRepos: [
            providerRepo("repo-api", "G5Labs/G5-Lean", 754, "open"),
          ],
        }),
      ],
      description: {
        pullRequestId: "pr-1",
        description: "Saved description",
        descriptionSource: "saved",
        descriptionHtml: null,
        repositories: [
          {
            repoId: "repo-api",
            repository: "G5Labs/G5-Lean",
            number: 754,
            url: null,
            lifecycle: "draft",
            comparison: "differs",
            body: "Edited",
            bodyHtml: "<p>Additional note on GitHub</p>",
            error: null,
          },
        ],
      },
    });
    renderLanding();
    const section = await screen.findByTestId("landing-implementation");
    expect(
      await within(section).findByTestId("landing-implementation-divergence"),
    ).toHaveTextContent("GitHub description differs.");
    expect(
      within(section).getByTestId("landing-repository-status"),
    ).toHaveTextContent("Draft");
    expect(
      within(section).getByTestId("landing-implementation-preview"),
    ).toHaveTextContent("Saved description");
    await userEvent.click(
      within(section).getByTestId("workspace-read-implementation"),
    );
    const dialog = screen.getByRole("dialog", {
      name: "Implementation description",
    });
    const tabs = within(dialog).getAllByRole("tab");
    expect(tabs.map((t) => t.textContent)).toEqual([
      "G5 Description",
      "GitHub · G5-Lean #754",
    ]);
    expect(within(dialog).getByText("Saved description")).toBeVisible();
    await userEvent.click(tabs[1]);
    expect(within(dialog).getByText("Additional note on GitHub")).toBeVisible();
    // Arrow keys move the selection between sources; the panel follows.
    await userEvent.keyboard("{ArrowLeft}");
    expect(tabs[0]).toHaveAttribute("aria-selected", "true");
    expect(tabs[0]).toHaveFocus();
    expect(within(dialog).getByRole("tabpanel")).toHaveAttribute(
      "aria-labelledby",
      tabs[0].id,
    );
    expect(within(dialog).getByText("Saved description")).toBeVisible();
    await userEvent.keyboard("{End}");
    expect(tabs[1]).toHaveAttribute("aria-selected", "true");
    // The repository rows stay the same across sources.
    expect(
      within(dialog).getAllByTestId("landing-repository-row"),
    ).toHaveLength(2);
  });

  it("keeps a merged PR expanded and an unreadable provider row Unavailable", async () => {
    serve({
      pulls: [
        pull({
          status: "merged",
          providerRepos: [
            providerRepo("repo-api", "G5Labs/G5-Lean", 754, "open"),
          ],
        }),
        pull({
          id: "pr-old",
          status: "closed",
          title: "Older",
          createdAt: "2026-09-01T00:00:00Z",
        }),
      ],
      description: {
        pullRequestId: "pr-1",
        description: "Saved description",
        descriptionSource: "saved",
        descriptionHtml: null,
        repositories: [
          {
            repoId: "repo-api",
            repository: "G5Labs/G5-Lean",
            number: 754,
            url: null,
            lifecycle: null,
            comparison: "unavailable",
            body: null,
            bodyHtml: null,
            error: "GitHub could not be read",
          },
        ],
      },
    });
    renderLanding();
    const section = await screen.findByTestId("landing-implementation");
    expect(
      await within(section).findByTestId("landing-implementation-status"),
    ).toHaveTextContent("Merged");
    expect(
      within(section).getByTestId("landing-implementation-title"),
    ).toHaveTextContent("Repository browsing and change selection");
    await waitFor(() =>
      expect(
        within(section).getByTestId("landing-implementation-divergence"),
      ).toHaveAttribute("data-state", "unavailable"),
    );
    // The recorded open observation stands in for the failed read.
    expect(
      within(section).getByTestId("landing-repository-status"),
    ).toHaveTextContent("Open");
    expect(
      within(section).getByTestId("landing-implementation-preview"),
    ).toHaveTextContent("Saved description");
    expect(section).not.toHaveTextContent("Matches GitHub");
  });
});

it("marks an incomplete total once and names unavailable repositories without marking categories", async () => {
  serve();
  server.use(
    http.get(`${V1}/workspaces/${WS}/review-flow/changes`, () =>
      HttpResponse.json({
        data: {
          baselineAvailable: true,
          files: [
            {
              repoId: "repo-api",
              path: "app.ts",
              kind: "code",
              status: "modified",
              additions: 1,
              deletions: 0,
            },
          ],
          afterHeads: [],
          beforeHeads: [],
          file: null,
          unavailableRepositories: [
            {
              repoId: "repo-docs",
              repoName: "docs",
              reason: "Parent comparison unavailable",
            },
          ],
        },
      }),
    ),
  );
  renderLanding();
  await waitFor(() =>
    expect(screen.getByTestId("landing-summary-changed")).toHaveTextContent(
      "1+ changed files",
    ),
  );
  expect(screen.getByTestId("landing-summary-code")).toHaveTextContent(
    "Code 1",
  );
  expect(screen.getByTestId("landing-summary-code")).not.toHaveTextContent("+");
  await userEvent.click(
    screen.getByLabelText("Unavailable repository comparisons"),
  );
  expect(screen.getByText("docs: Parent comparison unavailable")).toBeVisible();
  expect(screen.queryByTestId("landing-summary-parent-advanced")).toBeNull();
});

describe("personal approval banner (g5.governance.approval.personal-requests)", () => {
  const pendingFor = (name: string, requester = "Maya Chen") =>
    approvalRequirement({
      definition: approvalDefinition({ id: name, name }),
      callerCanDecide: true,
      requests: [
        {
          requestId: `request-${name}`,
          actorType: "human",
          actorUserId: "user-alex",
          actor: { userId: "user-alex", displayName: "Alex Chen" },
          requestedBy: { userId: "user-maya", displayName: requester },
          state: "pending",
          definitionVersion: 1,
          definitionName: name,
          requestedAt: "2026-11-04T09:00:00Z",
          closedAt: null,
          agentReviews: [],
        },
      ],
    });

  it("leads the landing with the count, the first three names and Go to Approvals", async () => {
    serve();
    seedMockWorkspaceApprovals(
      WS,
      approvalsState([
        pendingFor("Security"),
        // Satisfied by someone else, yet still requested from this viewer.
        { ...pendingFor("Architecture"), state: "approved" },
        pendingFor("UX"),
        pendingFor("Docs"),
        approvalRequirement({
          definition: approvalDefinition({ id: "other", name: "Other" }),
        }),
      ]),
    );
    renderLanding();
    const banner = await screen.findByRole("region", {
      name: "Your requested approvals",
    });
    expect(
      within(banner).getByRole("heading", {
        name: "4 approvals requested from you",
      }),
    ).toBeVisible();
    expect(banner).toHaveTextContent("Maya Chen requested your review.");
    const names = within(
      within(banner).getByTestId("personal-approvals-names"),
    ).getAllByRole("link");
    expect(names.map((link) => link.textContent)).toEqual([
      "Architecture",
      "Docs",
      "Security",
      "+1 more",
    ]);
    for (const link of [
      ...names,
      within(banner).getByRole("link", { name: "Go to Approvals" }),
    ])
      expect(link.getAttribute("href")).toMatch(/\?view=approvals$/);
    // It sits above the existing summary line.
    const summary = screen.getByTestId("landing-summary");
    expect(
      banner.compareDocumentPosition(summary) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it("says when requests cannot be read, and is absent for a known empty set", async () => {
    serve();
    server.use(
      http.get(`${V1}/workspaces/${WS}/approvals`, () =>
        HttpResponse.json({ error: "Unavailable" }, { status: 503 }),
      ),
    );
    renderLanding();
    const banner = await screen.findByTestId("personal-approvals-banner");
    await waitFor(() =>
      expect(banner).toHaveAttribute("data-state", "unavailable"),
    );
    expect(banner).toHaveTextContent("Couldn’t load your approval requests.");
    expect(within(banner).getByRole("button", { name: "Retry" })).toBeVisible();

    server.resetHandlers();
    serve();
    seedMockWorkspaceApprovals(
      WS,
      approvalsState([approvalRequirement({ callerCanDecide: false })]),
    );
    await userEvent.click(
      within(banner).getByRole("button", { name: "Retry" }),
    );
    await waitFor(() =>
      expect(screen.queryByTestId("personal-approvals-banner")).toBeNull(),
    );
  });

  it("is absent where the workspace offers no Approvals destination", async () => {
    serve();
    seedMockWorkspaceApprovals(WS, approvalsState([pendingFor("Security")]));
    renderApp(
      <Routes>
        <Route
          path="/:org/:workspace"
          element={
            <WorkspaceLanding
              workspaceId={WS}
              workspaceName="workspace-browsing"
              description={LONG_BODY}
              docsRepoId={null}
              approvals={null}
            />
          }
        />
      </Routes>,
      { route: "/g5labs/workspace-browsing", user: createMockUser() },
    );
    await screen.findByTestId("landing-task");
    expect(screen.queryByTestId("personal-approvals-banner")).toBeNull();
  });
});

describe("Primary Issue and Work Description (g5.workbench.landing.task)", () => {
  it("renames the two sections and keeps Primary Issue open while Work Description is empty", async () => {
    serve();
    renderLanding();
    const task = await screen.findByRole("region", { name: "Primary Issue" });
    const work = screen.getByRole("region", { name: "Work Description" });
    expect(
      await within(work).findByTestId("landing-implementation-empty"),
    ).toHaveTextContent("No implementation description yet.");
    const toggle = within(task).getByRole("button", { name: "Primary Issue" });
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    expect(within(task).getByTestId("landing-task-title")).toBeVisible();
    expect(within(work).queryByRole("button")).toBeNull();
    expect(
      screen.queryByText(/Workspace task|Implementation description/),
    ).toBeNull();
  });

  it("starts collapsed beside a PR, toggles by keyboard and keeps the choice across refresh", async () => {
    serve({ pulls: [pull()] });
    const view = renderLanding();
    const task = await screen.findByRole("region", { name: "Primary Issue" });
    const toggle = within(task).getByRole("button", { name: "Primary Issue" });
    await waitFor(() =>
      expect(toggle).toHaveAttribute("aria-expanded", "false"),
    );
    expect(within(task).getByTestId("landing-task-title")).not.toBeVisible();
    expect(
      await within(
        screen.getByRole("region", { name: "Work Description" }),
      ).findByTestId("landing-implementation-title"),
    ).toBeVisible();

    // Let the first description read finish so the refresh below is a new one.
    await waitFor(() => expect(descriptionCalls).toBeGreaterThan(0));
    await waitFor(() =>
      expect(
        within(
          screen.getByRole("region", { name: "Work Description" }),
        ).queryByText(/Loading/),
      ).toBeNull(),
    );

    toggle.focus();
    await userEvent.keyboard("{Enter}");
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    expect(within(task).getByTestId("landing-task-title")).toBeVisible();
    const callsBefore = descriptionCalls;
    await act(() => view.queryClient.invalidateQueries());
    await waitFor(() => expect(descriptionCalls).toBeGreaterThan(callsBefore));
    expect(toggle).toHaveAttribute("aria-expanded", "true");
  });

  it("does not count a loading or failed PR read as Work Description content", async () => {
    serve({ pulls: [pull()] });
    server.use(
      http.get(`${V1}/workspaces/${WS}/pull-requests`, () =>
        HttpResponse.json({ error: "Unavailable" }, { status: 503 }),
      ),
    );
    renderLanding();
    const task = await screen.findByRole("region", { name: "Primary Issue" });
    const toggle = within(task).getByRole("button", { name: "Primary Issue" });
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    await within(
      screen.getByRole("region", { name: "Work Description" }),
    ).findByTestId("review-implementation-retry");
    expect(toggle).toHaveAttribute("aria-expanded", "true");
  });
});
