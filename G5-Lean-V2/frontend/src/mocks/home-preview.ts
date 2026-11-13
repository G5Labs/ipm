// =============================================================================
// Mock mode — Home data for the frontend-only preview
// =============================================================================
// The `frontend-only` preview profile runs the real client with this mock API
// (VITE_MOCK=true). The unit-test fixture (`test/msw/handlers`) answers the
// organization Home's reads with nothing: no issue domains, no GitHub
// connection, no attention, compute or open-question routes. So the
// alignment Home showed four error or empty tiles, and the Issues and tasks
// tile (and its header layout) could not be judged in a preview.
//
// These handlers answer those reads in mock mode only, ahead of the fixture:
// a connected GitHub account, two issue domains with open issues, attention
// items, running VMs and open-question counts for the fixture's workspaces.
// Search narrows by typed terms and `label:`, Create and Link change the
// in-memory issue list (a created issue opens on its own page), each domain
// has a label catalog that the picker, the catalog editor and an issue
// page's label changes use, and a reload starts over. Unit tests never load this
// file; they keep the fixture's empty defaults.
// =============================================================================

import { http, HttpResponse } from "msw";

import type {
  CreateIssueInput,
  ExternalIssueLabelDTO,
  IssueLabelChangeDTO,
  IssueWriteTargetDTO,
  LabelChangeOutcomeDTO,
  LabelWriteDTO,
  CreateWorkspaceIssueLinkInput,
  ExternalIssueDetailDTO,
  ExternalIssueSummaryDTO,
  ExternalIssueUserDTO,
  WorkspaceActivityDTO,
  HomeAttentionItem,
  IssueCreationDTO,
  IssueDomainDTO,
  RunningComputeDTO,
  WorkspaceIssueLinkDTO,
  WorkspaceOpenQuestionMetrics,
} from "../lib/types";
import { MOCK_WORKSPACES } from "../data/mock";
import { MOCK_WORKSPACE_ACTIVITY } from "../test/fixtures";

/** The caller's connected GitHub login ("Assigned to me" matches it). */
export const MOCK_GITHUB_LOGIN = "alexchen";

const MINUTE = 60_000;
const ago = (minutes: number) =>
  new Date(Date.now() - minutes * MINUTE).toISOString();

const user = (login: string): ExternalIssueUserDTO => ({
  login,
  avatarUrl: `https://api.dicebear.com/7.x/initials/svg?seed=${login}`,
});

const DOMAINS: IssueDomainDTO[] = [
  {
    id: "domain-platform",
    trackerId: "tracker-github",
    provider: "github",
    containerKind: "github_repository",
    externalContainerId: "880001",
    displayKey: "g5labs/platform",
    isDefaultCreateTarget: true,
    contentTrusted: true,
    commentsEnabled: true,
    available: true,
    unavailableReason: null,
    createdAt: "2026-09-01T09:00:00Z",
  },
  {
    id: "domain-docs",
    trackerId: "tracker-github",
    provider: "github",
    containerKind: "github_repository",
    externalContainerId: "880002",
    displayKey: "g5labs/docs",
    isDefaultCreateTarget: false,
    contentTrusted: false,
    commentsEnabled: false,
    available: true,
    unavailableReason: null,
    createdAt: "2026-09-01T09:05:00Z",
  },
];

const LABEL_COLORS: Record<string, string> = {
  bug: "d73a4a",
  enhancement: "a2eeef",
  ui: "7057ff",
  docs: "0075ca",
  "good first issue": "7057ff",
  performance: "fbca04",
};

const LABEL_DESCRIPTIONS: Record<string, string> = {
  bug: "Something isn't working",
  enhancement: "New feature or request",
  docs: "Improvements or additions to documentation",
  "good first issue": "Good for newcomers",
};

/** Each domain's label catalog, by domain id. */
function seedCatalogs(): Record<string, ExternalIssueLabelDTO[]> {
  const catalog = (names: string[], base: number) =>
    names.map((name, index) => ({
      id: String(base + index),
      name,
      color: LABEL_COLORS[name] ?? null,
      description: LABEL_DESCRIPTIONS[name] ?? "",
    }));
  return {
    "domain-platform": catalog(
      ["bug", "enhancement", "good first issue", "performance", "ui"],
      5001,
    ),
    "domain-docs": catalog(["docs", "good first issue"], 6001),
  };
}

function issue(
  domain: IssueDomainDTO,
  number: number,
  title: string,
  options: {
    minutesAgo: number;
    assignees?: string[];
    labels?: string[];
    primaryWorkspaceId?: string;
  },
): ExternalIssueSummaryDTO {
  const primary = options.primaryWorkspaceId
    ? MOCK_WORKSPACES.find((w) => w.id === options.primaryWorkspaceId)
    : undefined;
  return {
    provider: "github",
    trackerId: domain.trackerId,
    domainId: domain.id,
    externalIssueId: `${domain.externalContainerId}${number}`,
    locator: { repositoryId: domain.externalContainerId, number },
    displayKey: `${domain.displayKey}#${number}`,
    canonicalUrl: `https://github.com/${domain.displayKey}/issues/${number}`,
    title,
    state: "open",
    stateReason: null,
    assignees: (options.assignees ?? []).map(user),
    labels: (options.labels ?? []).map((name) => ({
      name,
      color: LABEL_COLORS[name] ?? null,
    })),
    updatedAt: ago(options.minutesAgo),
    contentTrusted: domain.contentTrusted,
    primaryWorkspace: primary
      ? { visible: true, workspaceId: primary.id, name: primary.name }
      : null,
  };
}

function seedIssues(): ExternalIssueSummaryDTO[] {
  const [platform, docs] = DOMAINS;
  return [
    issue(platform, 742, "Home tile header actions break onto a new line", {
      minutesAgo: 12,
      assignees: [MOCK_GITHUB_LOGIN],
      labels: ["bug", "ui"],
      primaryWorkspaceId: "task-graph-rework",
    }),
    issue(platform, 739, "Show approval gates on the Workspaces tile", {
      minutesAgo: 55,
      assignees: ["jordanlee"],
      labels: ["enhancement", "ui"],
    }),
    issue(
      platform,
      731,
      "Preview dialog should keep the profile name while closing",
      { minutesAgo: 190, labels: ["bug"] },
    ),
    issue(platform, 728, "Task queue stalls after a VM restart", {
      minutesAgo: 320,
      assignees: [MOCK_GITHUB_LOGIN, "samrivera"],
      labels: ["bug", "performance"],
    }),
    issue(docs, 118, "Document the frontend-only preview profile", {
      minutesAgo: 600,
      labels: ["docs", "good first issue"],
    }),
    issue(platform, 720, "Search issues across every configured repository", {
      minutesAgo: 1_400,
      assignees: ["jordanlee"],
      labels: ["enhancement"],
    }),
    issue(docs, 112, "Getting Started guide still shows the classic header", {
      minutesAgo: 2_900,
      labels: ["docs"],
    }),
    issue(platform, 711, "Running compute tile: show the slot size", {
      minutesAgo: 4_300,
      assignees: [MOCK_GITHUB_LOGIN],
      labels: ["ui"],
    }),
    issue(platform, 705, "Faster workspace fork for large repositories", {
      minutesAgo: 8_600,
      labels: ["performance"],
    }),
    issue(platform, 699, "Keyboard navigation for the attention list", {
      minutesAgo: 12_000,
      labels: ["ui", "good first issue"],
    }),
  ];
}

interface MockLink {
  workspaceId: string;
  externalIssueId: string;
  role: "primary" | "related";
  linkedAt: string;
}

/** The seeded primaries, as links the workspace Issues panel lists. */
function seedLinks(rows: ExternalIssueSummaryDTO[]): MockLink[] {
  return rows.flatMap((row) =>
    row.primaryWorkspace?.visible
      ? [
          {
            workspaceId: row.primaryWorkspace.workspaceId,
            externalIssueId: row.externalIssueId,
            role: "primary" as const,
            linkedAt: row.updatedAt,
          },
        ]
      : [],
  );
}

let issues = seedIssues();
let links = seedLinks(issues);
let nextIssueNumber = 743;
let catalogs = seedCatalogs();
let nextLabelId = 7001;

/** Restore the seeded issues, links and label catalogs (tests). */
export function resetHomePreviewData(): void {
  issues = seedIssues();
  links = seedLinks(issues);
  nextIssueNumber = 743;
  catalogs = seedCatalogs();
  nextLabelId = 7001;
}

const sameName = (left: string, right: string) =>
  left.toLowerCase() === right.toLowerCase();

function catalogOf(domainId: string): ExternalIssueLabelDTO[] {
  return [...(catalogs[domainId] ?? [])].sort((a, b) =>
    a.name.localeCompare(b.name),
  );
}

/** An issue's label as issues show it: the catalog's name and colour. */
function issueLabel(label: ExternalIssueLabelDTO): ExternalIssueLabelDTO {
  return { name: label.name, color: label.color };
}

/** Every issue of a domain with one label replaced (or, `null`, dropped). */
function relabelIssues(
  domainId: string,
  name: string,
  next: ExternalIssueLabelDTO | null,
) {
  issues = issues.map((row) =>
    row.domainId !== domainId
      ? row
      : {
          ...row,
          labels: row.labels.flatMap((label) =>
            sameName(label.name, name)
              ? next
                ? [issueLabel(next)]
                : []
              : [label],
          ),
        },
  );
}

function labelError(status: number, code: string, message: string, extra = {}) {
  return HttpResponse.json(
    {
      error: {
        code: status === 409 ? "CONFLICT" : "VALIDATION_ERROR",
        message,
        details: { code, error: message, field: "labels", ...extra },
        requestId: "",
      },
    },
    { status },
  );
}

const normalizeColor = (color: string | undefined) =>
  color?.trim().replace(/^#/, "").toLowerCase();

function linkDto(link: MockLink): WorkspaceIssueLinkDTO | null {
  const row = issues.find((r) => r.externalIssueId === link.externalIssueId);
  if (!row) return null;
  return {
    id: `link-${link.workspaceId}-${row.externalIssueId}`,
    workspaceId: link.workspaceId,
    trackerId: row.trackerId,
    externalIssueId: row.externalIssueId,
    displayKey: row.displayKey,
    canonicalUrl: row.canonicalUrl,
    locator: row.locator,
    role: link.role,
    linkedBy: "user-alex",
    linkedAt: link.linkedAt,
    issue: row,
    issueError: null,
  };
}

/**
 * The fixture's approximation of GitHub's issue search: free terms must all
 * appear in the title, `label:` (quoted or not) must match a label, other
 * qualifiers are ignored.
 */
export function searchMockIssues(
  all: readonly ExternalIssueSummaryDTO[],
  q: string,
  assignee: string | null,
): ExternalIssueSummaryDTO[] {
  const tokens = q.match(/(?:[^\s"]+:"[^"]*"|"[^"]*"|\S+)/g) ?? [];
  const terms: string[] = [];
  const labels: string[] = [];
  for (const token of tokens) {
    const qualifier = /^([a-z-]+):(.*)$/i.exec(token);
    if (qualifier) {
      if (qualifier[1].toLowerCase() === "label") {
        labels.push(qualifier[2].replace(/^"|"$/g, "").toLowerCase());
      }
      continue;
    }
    terms.push(token.replace(/^"|"$/g, "").toLowerCase());
  }
  return all.filter((row) => {
    if (assignee === "me") {
      if (!row.assignees.some((a) => a.login === MOCK_GITHUB_LOGIN)) {
        return false;
      }
    } else if (assignee === "none" && row.assignees.length > 0) {
      return false;
    }
    const title = row.title.toLowerCase();
    if (!terms.every((t) => title.includes(t))) return false;
    const names = row.labels.map((l) => l.name.toLowerCase());
    return labels.every((l) => names.includes(l));
  });
}

function attentionItems(): HomeAttentionItem[] {
  const byId = (id: string) =>
    MOCK_WORKSPACES.find((w) => w.id === id)?.name ?? id;
  return [
    {
      id: "attention-1",
      kind: "approval",
      workspaceId: "task-graph-rework",
      workspaceName: byId("task-graph-rework"),
      sourceId: "approval-se",
      displayName: "System Engineering sign-off",
      actionableAt: ago(20),
    },
    {
      id: "attention-2",
      kind: "open_question",
      workspaceId: "auth-refactor",
      workspaceName: byId("auth-refactor"),
      sourceId: "questions-auth",
      openQuestionCount: 3,
      actionableAt: ago(140),
    },
    {
      id: "attention-3",
      kind: "approval",
      workspaceId: "new-feature-idea",
      workspaceName: byId("new-feature-idea"),
      sourceId: "approval-be",
      displayName: "Business Engineering review",
      actionableAt: ago(1_500),
    },
    {
      id: "attention-4",
      kind: "open_question",
      workspaceId: "task-graph-rework",
      workspaceName: byId("task-graph-rework"),
      sourceId: "questions-graph",
      openQuestionCount: 1,
      actionableAt: ago(3_100),
    },
  ];
}

function runningCompute(): RunningComputeDTO[] {
  return [
    {
      workspaceId: "base",
      workspaceName: "base",
      status: "running",
      startedAt: ago(95),
      lastActivityAt: ago(2),
      ownerUserId: "user-alex",
      ownerDisplayName: "Alex Chen",
      hostId: "host-1",
      slotCpus: 4,
      slotMemMib: 16_384,
      instanceType: "c3-standard-8",
    },
    {
      workspaceId: "task-graph-rework",
      workspaceName: "Task Graph Rework",
      status: "running",
      startedAt: ago(30),
      lastActivityAt: ago(1),
      ownerUserId: "user-alex",
      ownerDisplayName: "Alex Chen",
      hostId: "host-1",
      slotCpus: 4,
      slotMemMib: 16_384,
      instanceType: "c3-standard-8",
    },
    {
      workspaceId: "auth-refactor",
      workspaceName: "Auth Refactor",
      status: "starting",
      startedAt: null,
      lastActivityAt: null,
      ownerUserId: "user-alex",
      ownerDisplayName: "Alex Chen",
      hostId: null,
      slotCpus: null,
      slotMemMib: null,
      instanceType: null,
    },
  ];
}

const OPEN_QUESTIONS: Record<string, number> = {
  "task-graph-rework": 1,
  "auth-refactor": 3,
};

function badRequest(message: string) {
  return HttpResponse.json(
    { error: { code: "BAD_REQUEST", message, requestId: "" } },
    { status: 400 },
  );
}

export const homePreviewHandlers = [
  http.get("/api/v1/github-app/user-connection", () =>
    HttpResponse.json({
      data: { connected: true, githubLogin: MOCK_GITHUB_LOGIN },
    }),
  ),

  http.get("/api/v1/orgs/:orgId/issue-domains", () =>
    HttpResponse.json({ data: { domains: DOMAINS } }),
  ),

  http.get("/api/v1/orgs/:orgId/issues/search", ({ request }) => {
    const url = new URL(request.url);
    const state = url.searchParams.get("state") ?? "open";
    const rows =
      state === "closed"
        ? []
        : searchMockIssues(
            issues,
            url.searchParams.get("q") ?? "",
            url.searchParams.get("assignee"),
          );
    return HttpResponse.json({
      data: {
        results: [...rows].sort((a, b) =>
          b.updatedAt.localeCompare(a.updatedAt),
        ),
        domainErrors: [],
        domainCount: DOMAINS.length,
        truncated: false,
      },
    });
  }),

  http.get("/api/v1/orgs/:orgId/issues/create-target", () => {
    const target = DOMAINS[0];
    return HttpResponse.json({
      data: {
        state: "ready",
        destination: {
          domainId: target.id,
          trackerId: target.trackerId,
          repositoryId: target.externalContainerId,
          displayKey: target.displayKey,
          contentTrusted: target.contentTrusted,
        },
        githubLogin: MOCK_GITHUB_LOGIN,
        assignableUsers: [MOCK_GITHUB_LOGIN, "jordanlee", "samrivera"].map(
          user,
        ),
        error: null,
      },
    });
  }),

  http.get(
    "/api/v1/orgs/:orgId/issues/github/:repositoryId/:number",
    ({ params }) => {
      const row = issues.find(
        (r) =>
          r.locator.repositoryId === params.repositoryId &&
          r.locator.number === Number(params.number),
      );
      if (!row) {
        return HttpResponse.json(
          {
            error: {
              code: "NOT_FOUND",
              message: "Issue not found",
              details: { issueError: "not_found" },
              requestId: "",
            },
          },
          { status: 404 },
        );
      }
      const primary =
        row.primaryWorkspace?.visible === true ? row.primaryWorkspace : null;
      return HttpResponse.json({
        data: {
          ...row,
          body: "Seen in the frontend-only preview's demo data.",
          author: user(row.assignees[0]?.login ?? MOCK_GITHUB_LOGIN),
          createdAt: row.updatedAt,
          closedAt: null,
          commentCount: 0,
          comments: [],
          commentsTruncated: false,
          links: primary
            ? [
                {
                  workspaceId: primary.workspaceId,
                  workspaceName: primary.name,
                  role: "primary",
                },
              ]
            : [],
        } satisfies ExternalIssueDetailDTO,
      });
    },
  ),

  http.post("/api/v1/orgs/:orgId/issues", async ({ request }) => {
    const input = (await request.json()) as CreateIssueInput;
    const target = DOMAINS.find((d) => d.id === input.domainId) ?? DOMAINS[0];
    const destination = {
      domainId: target.id,
      trackerId: target.trackerId,
      repositoryId: target.externalContainerId,
      displayKey: target.displayKey,
      contentTrusted: target.contentTrusted,
    };
    const assignees = input.assignees.map((a) =>
      a === "@me" ? MOCK_GITHUB_LOGIN : a,
    );
    const requested = input.labels ?? [];
    const unknown = requested.filter(
      (name) =>
        !catalogOf(target.id).some((label) => sameName(label.name, name)),
    );
    if (unknown.length > 0) {
      return labelError(
        422,
        "unknown_label",
        `${unknown.join(", ")} not in ${target.displayKey}'s labels`,
      );
    }
    const base: Omit<IssueCreationDTO, "state" | "sent" | "issue"> = {
      idempotencyKey: input.idempotencyKey,
      destination,
      githubLogin: MOCK_GITHUB_LOGIN,
      title: input.title,
      payload: input.body,
      assignees,
      message: null,
      parent: null,
      labels: requested,
    };
    if (input.checkOnly) {
      return HttpResponse.json({
        data: { ...base, state: "not_found", sent: false, issue: null },
      });
    }
    const created = {
      ...issue(target, nextIssueNumber++, input.title, {
        minutesAgo: 0,
        assignees,
      }),
      labels: catalogOf(target.id)
        .filter((label) => requested.some((name) => sameName(name, label.name)))
        .map(issueLabel),
    };
    issues = [created, ...issues];
    return HttpResponse.json({
      data: {
        ...base,
        state: "created",
        sent: true,
        issue: {
          externalIssueId: created.externalIssueId,
          repositoryId: created.locator.repositoryId,
          number: created.locator.number,
          displayKey: created.displayKey,
          title: created.title,
          url: created.canonicalUrl,
        },
      } satisfies IssueCreationDTO,
    });
  }),

  http.post(
    "/api/v1/workspaces/:workspaceId/issue-links",
    async ({ params, request }) => {
      const input = (await request.json()) as CreateWorkspaceIssueLinkInput;
      const workspaceId = String(params.workspaceId);
      const workspace = MOCK_WORKSPACES.find((w) => w.id === workspaceId);
      const target = issues.find(
        (row) => row.displayKey === input.ref || row.canonicalUrl === input.ref,
      );
      if (!workspace || !target) return badRequest("Unknown issue reference");
      const linked: ExternalIssueSummaryDTO =
        input.role === "primary"
          ? {
              ...target,
              primaryWorkspace: {
                visible: true,
                workspaceId,
                name: workspace.name,
              },
            }
          : target;
      issues = issues.map((row) => (row === target ? linked : row));
      // One link per workspace and issue; a new primary replaces the old one.
      const link: MockLink = {
        workspaceId,
        externalIssueId: target.externalIssueId,
        role: input.role,
        linkedAt: new Date().toISOString(),
      };
      links = [
        ...links.filter(
          (l) =>
            l.externalIssueId !== target.externalIssueId ||
            (l.workspaceId !== workspaceId &&
              !(input.role === "primary" && l.role === "primary")),
        ),
        link,
      ];
      return HttpResponse.json({ data: linkDto(link) });
    },
  ),

  // The workspace page names its primary issue from the activity read.
  http.get("/api/v1/workspaces/:workspaceId/activity", ({ params }) => {
    const primary = links
      .filter((l) => l.workspaceId === params.workspaceId)
      .map((l) => (l.role === "primary" ? linkDto(l) : null))
      .find((l): l is WorkspaceIssueLinkDTO => l !== null);
    return HttpResponse.json({
      data: {
        ...MOCK_WORKSPACE_ACTIVITY,
        primaryIssue: primary
          ? {
              linkId: primary.id,
              displayKey: primary.displayKey,
              canonicalUrl: primary.canonicalUrl,
              locator: primary.locator,
              contentTrusted: primary.issue?.contentTrusted ?? true,
            }
          : MOCK_WORKSPACE_ACTIVITY.primaryIssue,
      } satisfies WorkspaceActivityDTO,
    });
  }),

  http.get("/api/v1/workspaces/:workspaceId/issue-links", ({ params }) =>
    HttpResponse.json({
      data: {
        links: links
          .filter((l) => l.workspaceId === params.workspaceId)
          .map(linkDto)
          .filter((l): l is WorkspaceIssueLinkDTO => l !== null),
      },
    }),
  ),

  // Label catalogs (g5.issue.labels).
  http.get(
    "/api/v1/orgs/:orgId/issue-domains/:domainId/labels",
    ({ params }) => {
      const domain = DOMAINS.find((d) => d.id === params.domainId);
      if (!domain) return badRequest("Unknown issue domain");
      return HttpResponse.json({
        data: {
          domainId: domain.id,
          displayKey: domain.displayKey,
          labels: catalogOf(domain.id),
          nextPage: null,
        },
      });
    },
  ),

  http.post(
    "/api/v1/orgs/:orgId/issue-domains/:domainId/labels",
    async ({ params, request }) => {
      const domainId = String(params.domainId);
      const input = (await request.json()) as {
        name: string;
        color?: string;
        description?: string;
      };
      const name = input.name.trim();
      const existing = catalogOf(domainId).find((l) => sameName(l.name, name));
      if (existing) {
        return labelError(
          409,
          "label_exists",
          `a label named ${existing.name} already exists`,
          { label: existing },
        );
      }
      const label: ExternalIssueLabelDTO = {
        id: String(nextLabelId++),
        name,
        color: normalizeColor(input.color) ?? "ededed",
        description: input.description?.trim() ?? "",
      };
      catalogs[domainId] = [...(catalogs[domainId] ?? []), label];
      return HttpResponse.json(
        {
          data: {
            outcome: "applied",
            label,
            githubLogin: MOCK_GITHUB_LOGIN,
            message: null,
          } satisfies LabelWriteDTO,
        },
        { status: 201 },
      );
    },
  ),

  http.patch(
    "/api/v1/orgs/:orgId/issue-domains/:domainId/labels/:labelId",
    async ({ params, request }) => {
      const domainId = String(params.domainId);
      const input = (await request.json()) as {
        name?: string;
        color?: string;
        description?: string;
      };
      const current = catalogOf(domainId).find((l) => l.id === params.labelId);
      if (!current) return badRequest("Label not found");
      const name = input.name?.trim() ?? current.name;
      const clash = catalogOf(domainId).find(
        (l) => l.id !== current.id && sameName(l.name, name),
      );
      if (clash) {
        return labelError(
          409,
          "label_exists",
          `a label named ${clash.name} already exists`,
          { label: clash },
        );
      }
      const label: ExternalIssueLabelDTO = {
        ...current,
        name,
        color: normalizeColor(input.color) ?? current.color,
        description: input.description?.trim() ?? current.description,
      };
      catalogs[domainId] = (catalogs[domainId] ?? []).map((l) =>
        l.id === current.id ? label : l,
      );
      relabelIssues(domainId, current.name, label);
      return HttpResponse.json({
        data: {
          outcome: "applied",
          label,
          githubLogin: MOCK_GITHUB_LOGIN,
          message: null,
        } satisfies LabelWriteDTO,
      });
    },
  ),

  http.delete(
    "/api/v1/orgs/:orgId/issue-domains/:domainId/labels/:labelId",
    ({ params }) => {
      const domainId = String(params.domainId);
      const current = catalogOf(domainId).find((l) => l.id === params.labelId);
      if (!current) return badRequest("Label not found");
      catalogs[domainId] = (catalogs[domainId] ?? []).filter(
        (l) => l.id !== current.id,
      );
      relabelIssues(domainId, current.name, null);
      return HttpResponse.json({
        data: {
          outcome: "applied",
          label: current,
          githubLogin: MOCK_GITHUB_LOGIN,
          message: null,
        } satisfies LabelWriteDTO,
      });
    },
  ),

  http.get(
    "/api/v1/orgs/:orgId/issues/github/:repositoryId/:number/write-target",
    ({ params }) => {
      const row = issues.find(
        (r) =>
          r.locator.repositoryId === params.repositoryId &&
          r.locator.number === Number(params.number),
      );
      const domain = DOMAINS.find((d) => d.id === row?.domainId);
      if (!row || !domain) return badRequest("Issue not found");
      return HttpResponse.json({
        data: {
          state: domain.commentsEnabled ? "ready" : "comments_disabled",
          githubLogin: MOCK_GITHUB_LOGIN,
          contentTrusted: domain.contentTrusted,
          displayKey: row.displayKey,
          destination: {
            trackerId: row.trackerId,
            externalIssueId: row.externalIssueId,
          },
          assignableUsers: [MOCK_GITHUB_LOGIN, "jordanlee", "samrivera"].map(
            user,
          ),
          error: null,
        } satisfies IssueWriteTargetDTO,
      });
    },
  ),

  http.post(
    "/api/v1/orgs/:orgId/issues/github/:repositoryId/:number/labels",
    async ({ params, request }) => {
      const row = issues.find(
        (r) =>
          r.locator.repositoryId === params.repositoryId &&
          r.locator.number === Number(params.number),
      );
      if (!row) return badRequest("Issue not found");
      const input = (await request.json()) as {
        addLabels: string[];
        removeLabels: string[];
      };
      const catalog = catalogOf(row.domainId);
      const unknown = input.addLabels.filter(
        (name) => !catalog.some((label) => sameName(label.name, name)),
      );
      if (unknown.length > 0) {
        return labelError(
          422,
          "unknown_label",
          `${unknown.join(", ")} not in the domain's labels`,
        );
      }
      const kept = row.labels.filter(
        (label) =>
          !input.removeLabels.some((name) => sameName(name, label.name)),
      );
      const added = catalog
        .filter(
          (label) =>
            input.addLabels.some((name) => sameName(name, label.name)) &&
            !kept.some((have) => sameName(have.name, label.name)),
        )
        .map(issueLabel);
      const updated = {
        ...row,
        labels: [...kept, ...added],
        updatedAt: new Date().toISOString(),
      };
      issues = issues.map((r) => (r === row ? updated : r));
      const changes: LabelChangeOutcomeDTO[] = [
        ...input.addLabels.map((label) => ({
          label,
          change: "add" as const,
          outcome: "applied" as const,
        })),
        ...input.removeLabels.map((label) => ({
          label,
          change: "remove" as const,
          outcome: "applied" as const,
        })),
      ];
      return HttpResponse.json({
        data: {
          outcome: "applied",
          changes,
          issue: updated,
          githubLogin: MOCK_GITHUB_LOGIN,
          message: null,
        } satisfies IssueLabelChangeDTO,
      });
    },
  ),

  http.get("/api/v1/orgs/:orgId/home/attention", ({ request }) => {
    const url = new URL(request.url);
    const kind = url.searchParams.get("kind");
    const order = url.searchParams.get("order") ?? "oldest";
    const limit = Number(url.searchParams.get("limit") ?? "20") || 20;
    const rows = attentionItems()
      .filter((item) => !kind || item.kind === kind)
      .sort((a, b) =>
        order === "newest"
          ? b.actionableAt.localeCompare(a.actionableAt)
          : a.actionableAt.localeCompare(b.actionableAt),
      );
    return HttpResponse.json({
      data: { items: rows.slice(0, limit), nextCursor: null },
    });
  }),

  http.get("/api/v1/orgs/:orgId/compute/running", () =>
    HttpResponse.json({ data: runningCompute() }),
  ),

  http.get(
    "/api/v1/orgs/:orgId/workspaces/open-question-metrics",
    ({ request }) => {
      const ids = (new URL(request.url).searchParams.get("workspaceIds") ?? "")
        .split(",")
        .filter(Boolean);
      return HttpResponse.json({
        data: ids.map(
          (workspaceId): WorkspaceOpenQuestionMetrics => ({
            workspaceId,
            documentOpenQuestions: OPEN_QUESTIONS[workspaceId] ?? 0,
            openQuestionsTotal: OPEN_QUESTIONS[workspaceId] ?? 0,
            projectDocSources: [],
          }),
        ),
      });
    },
  ),
];
