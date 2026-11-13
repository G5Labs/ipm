// =============================================================================
// Issue test fixtures (GitHub and native G5 issue domains, summaries,
// details, workspace links) and MSW helpers for the issue features.
// =============================================================================

import { HttpResponse, http } from "msw";

import type {
  CommentResultDTO,
  ExternalIssueCommentDTO,
  ExternalIssueDetailDTO,
  GithubCommentResultDTO,
  IssueEditDTO,
  IssueWriteTargetDTO,
  ExternalIssueSummaryDTO,
  IssueCreateTargetDTO,
  IssueCreationDTO,
  IssueDomainDTO,
  IssueLabelChangeDTO,
  IssueStateChangeDTO,
  LabelPageDTO,
  WorkspaceIssueLinkDTO,
} from "../lib/types";
import { MOCK_INSTANCE_INFO } from "./fixtures";
import { server } from "./msw/server";

export function makeIssueDomain(
  overrides: Partial<IssueDomainDTO> = {},
): IssueDomainDTO {
  return {
    id: "domain-1",
    trackerId: "tracker-1",
    provider: "github",
    containerKind: "github_repository",
    externalContainerId: "11",
    displayKey: "acme/api",
    isDefaultCreateTarget: true,
    contentTrusted: false,
    commentsEnabled: false,
    available: true,
    unavailableReason: null,
    createdAt: "2026-09-20T12:00:00Z",
    ...overrides,
  };
}

export function makeIssueSummary(
  overrides: Partial<ExternalIssueSummaryDTO> = {},
): ExternalIssueSummaryDTO {
  const number = overrides.locator?.number ?? 12;
  return {
    provider: "github",
    trackerId: "tracker-1",
    domainId: "domain-1",
    externalIssueId: `9000${number}`,
    locator: { repositoryId: "11", number },
    displayKey: `acme/api#${number}`,
    canonicalUrl: `https://github.com/acme/api/issues/${number}`,
    title: "Search returns stale results",
    state: "open",
    stateReason: null,
    assignees: [{ login: "octocat", avatarUrl: null }],
    labels: [{ name: "bug", color: "d73a4a" }],
    updatedAt: "2026-09-24T12:00:00Z",
    contentTrusted: false,
    primaryWorkspace: null,
    ...overrides,
  };
}

export function makeIssueDetail(
  overrides: Partial<ExternalIssueDetailDTO> = {},
): ExternalIssueDetailDTO {
  return {
    ...makeIssueSummary(overrides),
    body: "The search index is **stale**.",
    author: { login: "reporter", avatarUrl: null },
    createdAt: "2026-09-20T12:00:00Z",
    closedAt: null,
    commentCount: 2,
    comments: [
      {
        id: "c1",
        author: { login: "first", avatarUrl: null },
        body: "First comment",
        createdAt: "2026-09-21T12:00:00Z",
        updatedAt: "2026-09-21T12:00:00Z",
        url: "https://github.com/acme/api/issues/12#issuecomment-1",
      },
      {
        id: "c2",
        author: { login: "second", avatarUrl: null },
        body: "Second comment",
        createdAt: "2026-09-22T12:00:00Z",
        updatedAt: "2026-09-22T12:00:00Z",
        url: "https://github.com/acme/api/issues/12#issuecomment-2",
      },
    ],
    commentsTruncated: false,
    links: [],
    ...overrides,
  };
}

export function makeIssueLink(
  overrides: Partial<WorkspaceIssueLinkDTO> = {},
): WorkspaceIssueLinkDTO {
  const issue =
    overrides.issue === undefined ? makeIssueSummary() : overrides.issue;
  const locator = overrides.locator ??
    issue?.locator ?? { repositoryId: "11", number: 12 };
  return {
    id: "link-1",
    workspaceId: "ws-1",
    trackerId: "tracker-1",
    externalIssueId: issue?.externalIssueId ?? "900012",
    displayKey: `acme/api#${locator.number}`,
    canonicalUrl: `https://github.com/acme/api/issues/${locator.number}`,
    locator,
    role: "primary",
    linkedBy: "user-1",
    linkedAt: "2026-09-23T12:00:00Z",
    issue,
    issueError: null,
    ...overrides,
  };
}

/** A ready creation target: `acme/api` (repository 11), trusted, as `alex-gh`. */
export function makeIssueCreateTarget(
  overrides: Partial<IssueCreateTargetDTO> = {},
): IssueCreateTargetDTO {
  return {
    state: "ready",
    destination: {
      domainId: "domain-1",
      trackerId: "tracker-1",
      repositoryId: "11",
      displayKey: "acme/api",
      contentTrusted: true,
    },
    githubLogin: "alex-gh",
    assignableUsers: [],
    error: null,
    ...overrides,
  };
}

/** A created issue `acme/api#77` in the ready target. */
export function makeIssueCreation(
  overrides: Partial<IssueCreationDTO> = {},
): IssueCreationDTO {
  const title = overrides.title ?? "Write the release notes";
  return {
    state: "created",
    sent: true,
    issue: {
      externalIssueId: "900077",
      repositoryId: "11",
      number: 77,
      displayKey: "acme/api#77",
      title,
      url: "https://github.com/acme/api/issues/77",
    },
    idempotencyKey: "key",
    destination: makeIssueCreateTarget().destination!,
    githubLogin: "alex-gh",
    title,
    payload: "<!-- g5-issue abc -->",
    assignees: [],
    message: null,
    parent: null,
    labels: [],
    ...overrides,
  };
}

/** GitHub applied a close or reopen; the issue as it answered. */
export function makeIssueStateChange(
  state: "closed" | "open",
  issue: Partial<ExternalIssueSummaryDTO> = {},
): IssueStateChangeDTO {
  return {
    requested: state,
    outcome: "applied",
    issue: makeIssueSummary({
      ...issue,
      state,
      stateReason: state === "closed" ? "completed" : "reopened",
    }),
    githubLogin: "alex-gh",
    message: null,
  };
}

/**
 * A ready write target for `acme/api#12` as `alex-gh`, untrusted, with
 * `octocat` and `hubot` assignable.
 */
export function makeIssueWriteTarget(
  overrides: Partial<IssueWriteTargetDTO> = {},
): IssueWriteTargetDTO {
  return {
    state: "ready",
    githubLogin: "alex-gh",
    contentTrusted: false,
    displayKey: "acme/api#12",
    destination: { trackerId: "tracker-1", externalIssueId: "900012" },
    assignableUsers: [
      { login: "octocat", avatarUrl: null },
      { login: "hubot", avatarUrl: null },
    ],
    error: null,
    ...overrides,
  };
}

/** An applied GitHub issue edit as `alex-gh`. */
export function makeIssueEdit(
  overrides: Partial<IssueEditDTO> = {},
): IssueEditDTO {
  return {
    outcome: "applied",
    issue: makeIssueSummary(),
    githubLogin: "alex-gh",
    message: null,
    ...overrides,
  };
}

/** A GitHub comment GitHub confirmed, on `acme/api#12` as `alex-gh`. */
export function makeGithubCommentResult(
  overrides: Partial<GithubCommentResultDTO> = {},
): GithubCommentResultDTO {
  return {
    id: "pub-1",
    provider: "github",
    state: "published",
    kind: "comment",
    origin: "person",
    destination: "issue",
    displayKey: "acme/api#12",
    githubLogin: "alex-gh",
    body: "Looks right.",
    payload: "Looks right.\n\n<!-- g5-comment pub-1 -->",
    externalCommentUrl: "https://github.com/acme/api/issues/12#issuecomment-9",
    error: null,
    createdAt: "2026-10-20T12:00:00Z",
    publishedAt: "2026-10-20T12:00:01Z",
    message: null,
    ...overrides,
  };
}

/** A stored native comment, as the native comment routes answer. */
export function makeNativeCommentResult(
  comment: ExternalIssueCommentDTO,
  kind: "comment" | "edit" = "comment",
): CommentResultDTO {
  return {
    id: `g5:${comment.id}`,
    provider: "g5",
    state: "published",
    kind,
    origin: "person",
    destination: "issue",
    displayKey: "ACME-12",
    githubLogin: null,
    body: comment.body,
    payload: comment.body,
    externalCommentUrl: comment.url,
    error: null,
    createdAt: comment.createdAt,
    publishedAt: comment.updatedAt,
    comment,
  };
}

/** A typed conflict refusal (`details.code`), as the write routes answer. */
export function issueConflictResponse(code: string, message = "refused") {
  return HttpResponse.json(
    {
      error: { code: "CONFLICT", message, details: { code }, requestId: "" },
    },
    { status: 409 },
  );
}

/** The viewer's GitHub App connection, as `useGithubConnection` reads it. */
export function serveGithubConnection(connected: boolean, login = "alex-gh") {
  server.use(
    http.get("/api/v1/github-app/user-connection", () =>
      HttpResponse.json({
        data: { connected, githubLogin: connected ? login : null },
      }),
    ),
  );
}

/** Serve organizations whose own external-issues switch is on (or off). */
export function serveOrgExternalIssues(enabled = true) {
  server.use(
    http.get("/api/v1/orgs/:orgId/config", () =>
      HttpResponse.json({
        data: {
          scope: "org",
          settings: { external_issues_enabled: enabled },
          secretKeys: [],
          effectiveSettings: { external_issues_enabled: enabled },
          effectiveSecretKeys: [],
        },
      }),
    ),
  );
}

/**
 * Advertise `external_issues` (plus any extra features) from instance info,
 * and serve organizations with external issues turned on.
 */
export function serveExternalIssuesFeature(extra: string[] = []) {
  serveOrgExternalIssues(true);
  server.use(
    http.get("/api/v1/instance/info", () =>
      HttpResponse.json({
        data: {
          ...MOCK_INSTANCE_INFO,
          features: [
            ...MOCK_INSTANCE_INFO.features,
            "external_issues",
            ...extra,
          ],
        },
      }),
    ),
  );
}

/**
 * Fail the first instance-info request, then advertise `external_issues`.
 * Returns the number of requests made so far.
 */
export function serveInstanceInfoFailingOnce(): () => number {
  let calls = 0;
  server.use(
    http.get("/api/v1/instance/info", () => {
      calls += 1;
      if (calls === 1) {
        return HttpResponse.json(
          { error: { code: "INTERNAL", message: "instance info unavailable" } },
          { status: 503 },
        );
      }
      return HttpResponse.json({
        data: {
          ...MOCK_INSTANCE_INFO,
          features: [...MOCK_INSTANCE_INFO.features, "external_issues"],
        },
      });
    }),
  );
  return () => calls;
}

/** A typed issue error response body. */
export function issueErrorResponse(
  status: number,
  issueError: string,
  message = "GitHub request failed",
  code = "ISSUE_ERROR",
) {
  return HttpResponse.json(
    { error: { code, message, details: { issueError }, requestId: "" } },
    { status },
  );
}

// ---------------------------------------------------------------------------
// Native G5 issues (g5.issue.native)
// ---------------------------------------------------------------------------

/** The organization's native tracker, key `ACME`, trusted. */
export function makeNativeIssueDomain(
  overrides: Partial<IssueDomainDTO> = {},
): IssueDomainDTO {
  return makeIssueDomain({
    id: "domain-g5",
    trackerId: "tracker-g5",
    provider: "g5",
    containerKind: "g5_project",
    externalContainerId: "1",
    displayKey: "ACME",
    contentTrusted: true,
    commentsEnabled: false,
    ...overrides,
  });
}

/** Alex Chen, the mock signed-in user, as a native issue user. */
export const NATIVE_ALEX = {
  login: "alex@g5labs.dev",
  avatarUrl: null,
  userId: "user-alex",
  displayName: "Alex Chen",
};

/** Maya Patel, another member, as a native issue user. */
export const NATIVE_MAYA = {
  login: "maya@g5labs.dev",
  avatarUrl: null,
  userId: "user-maya",
  displayName: "Maya Patel",
};

/** A native issue `ACME-<number>` (default 12). */
export function makeNativeIssueSummary(
  overrides: Partial<ExternalIssueSummaryDTO> = {},
): ExternalIssueSummaryDTO {
  const number = overrides.locator?.number ?? 12;
  return makeIssueSummary({
    provider: "g5",
    trackerId: "tracker-g5",
    domainId: "domain-g5",
    externalIssueId: `${700 + number}`,
    locator: { repositoryId: "g5", number },
    displayKey: `ACME-${number}`,
    canonicalUrl: `http://localhost:3000/g5labs/issues/g5/${number}`,
    title: "Native search is slow",
    assignees: [NATIVE_MAYA],
    labels: [],
    updatedAt: "2026-10-20T12:00:00.123456Z",
    contentTrusted: true,
    ...overrides,
  });
}

/** A native issue's detail: one comment by Alex, one by Maya. */
export function makeNativeIssueDetail(
  overrides: Partial<ExternalIssueDetailDTO> = {},
): ExternalIssueDetailDTO {
  const number = overrides.locator?.number ?? 12;
  const page = `http://localhost:3000/g5labs/issues/g5/${number}`;
  return {
    ...makeNativeIssueSummary(overrides),
    body: "Native search takes **seconds**.",
    author: NATIVE_MAYA,
    createdAt: "2026-10-19T12:00:00.123456Z",
    closedAt: null,
    commentCount: 2,
    comments: [
      {
        id: "41",
        author: NATIVE_ALEX,
        body: "I can reproduce it.",
        createdAt: "2026-10-19T13:00:00.123456Z",
        updatedAt: "2026-10-19T13:00:00.123456Z",
        url: `${page}#comment-41`,
      },
      {
        id: "42",
        author: NATIVE_MAYA,
        body: "Looking into the index.",
        createdAt: "2026-10-19T14:00:00.123456Z",
        updatedAt: "2026-10-19T14:00:00.123456Z",
        url: `${page}#comment-42`,
      },
    ],
    commentsTruncated: false,
    links: [],
    ...overrides,
  };
}

/** A workspace link to native issue `ACME-<number>`. */
export function makeNativeIssueLink(
  overrides: Partial<WorkspaceIssueLinkDTO> = {},
): WorkspaceIssueLinkDTO {
  const issue =
    overrides.issue === undefined ? makeNativeIssueSummary() : overrides.issue;
  const locator = overrides.locator ??
    issue?.locator ?? { repositoryId: "g5", number: 12 };
  return makeIssueLink({
    provider: "g5",
    trackerId: "tracker-g5",
    externalIssueId: issue?.externalIssueId ?? "712",
    displayKey: `ACME-${locator.number}`,
    canonicalUrl: `http://localhost:3000/g5labs/issues/g5/${locator.number}`,
    locator,
    issue,
    ...overrides,
  });
}

/** A ready native creation target `ACME`, with Alex and Maya assignable. */
export function makeNativeCreateTarget(
  overrides: Partial<IssueCreateTargetDTO> = {},
): IssueCreateTargetDTO {
  return {
    state: "ready",
    destination: {
      provider: "g5",
      domainId: "domain-g5",
      trackerId: "tracker-g5",
      repositoryId: "g5",
      displayKey: "ACME",
      contentTrusted: true,
    },
    githubLogin: null,
    assignableUsers: [NATIVE_ALEX, NATIVE_MAYA],
    error: null,
    ...overrides,
  };
}

/**
 * Advertise only the always-on `issues` feature (no GitHub App), and serve
 * organizations with issues turned on.
 */
export function serveNativeIssuesFeature(extra: string[] = []) {
  serveOrgExternalIssues(true);
  server.use(
    http.get("/api/v1/instance/info", () =>
      HttpResponse.json({
        data: {
          ...MOCK_INSTANCE_INFO,
          features: [...MOCK_INSTANCE_INFO.features, "issues", ...extra],
        },
      }),
    ),
  );
}

// ---------------------------------------------------------------------------
// Labels (g5.issue.labels)
// ---------------------------------------------------------------------------

/** `bug`, `needs triage` and `wip` in `acme/api` (domain-1). */
export function makeLabelPage(
  overrides: Partial<LabelPageDTO> = {},
): LabelPageDTO {
  return {
    domainId: "domain-1",
    displayKey: "acme/api",
    labels: [
      { id: "101", name: "bug", color: "d73a4a", description: "Broken" },
      { id: "102", name: "needs triage", color: "fbca04", description: "" },
      { id: "103", name: "wip", color: null, description: "" },
    ],
    nextPage: null,
    ...overrides,
  };
}

/** Serve one page of a domain's label catalog; returns the requests seen. */
export function serveLabelCatalog(
  domainId = "domain-1",
  page: LabelPageDTO = makeLabelPage({ domainId }),
): () => number {
  let reads = 0;
  server.use(
    http.get(`/api/v1/orgs/:orgId/issue-domains/${domainId}/labels`, () => {
      reads += 1;
      return HttpResponse.json({ data: page });
    }),
  );
  return () => reads;
}

/** An applied label change; the issue as re-read. */
export function makeIssueLabelChange(
  overrides: Partial<IssueLabelChangeDTO> = {},
): IssueLabelChangeDTO {
  return {
    outcome: "applied",
    changes: [],
    issue: makeIssueSummary(),
    githubLogin: "alex-gh",
    message: null,
    ...overrides,
  };
}

/** A typed validation refusal (422 `details.code`), as the label routes answer. */
export function labelValidationResponse(code: string, error: string) {
  return HttpResponse.json(
    {
      error: {
        code: "VALIDATION_ERROR",
        message: error,
        details: { code, error, field: "labels" },
        requestId: "",
      },
    },
    { status: 422 },
  );
}
