// =============================================================================
// API Route Registry — Single source of truth for all backend API paths
// =============================================================================
// Generated from g5-api-routes/src/routes/mod.rs router configuration.
// Every hook and MSW handler should reference these paths.
//
// When the Rust backend changes a route, update it HERE and the compiler
// will surface every call-site that needs attention.
// =============================================================================

import { NATIVE_ISSUE_REPOSITORY_ID } from "./issuePage";
import type { ExternalIssueLocatorDTO } from "./types";

const V1 = "/api/v1";

/** An issue of either tracker by its locator. */
function issuePath(orgId: string, locator: ExternalIssueLocatorDTO): string {
  return locator.repositoryId === NATIVE_ISSUE_REPOSITORY_ID
    ? `${V1}/orgs/${orgId}/issues/g5/${locator.number}`
    : `${V1}/orgs/${orgId}/issues/github/${encodeURIComponent(locator.repositoryId)}/${locator.number}`;
}

export const routes = {
  // A person's own GitHub connection (GitHub App user authorization).
  githubApp: {
    userConnection: `${V1}/github-app/user-connection`,
    userConnectionDisconnect: `${V1}/github-app/user-connection/disconnect`,
  },
  // ---------------------------------------------------------------------------
  // Auth
  // ---------------------------------------------------------------------------
  auth: {
    login: (provider: string) => `${V1}/auth/login/${provider}`,
    callback: (provider: string) => `${V1}/auth/callback/${provider}`,
    refresh: `${V1}/auth/refresh`,
    logout: `${V1}/auth/logout`,
    testToken: `${V1}/auth/test-token`,
    cliApprove: `${V1}/auth/cli/approve`,
    myScopes: `${V1}/auth/me/scopes`,
    checkScopes: `${V1}/auth/me/scopes/check`,
  },

  // ---------------------------------------------------------------------------
  // Users
  // ---------------------------------------------------------------------------
  users: {
    me: `${V1}/users/me`,
    byId: (id: string) => `${V1}/users/${id}`,
  },

  // ---------------------------------------------------------------------------
  // Platform operator console (g5.organization.platform-operator)
  // ---------------------------------------------------------------------------
  platform: {
    orgs: `${V1}/platform/orgs`,
  },

  // ---------------------------------------------------------------------------
  // Orgs
  // ---------------------------------------------------------------------------
  orgs: {
    list: `${V1}/orgs`,
    get: (orgId: string) => `${V1}/orgs/${orgId}`,
    update: (orgId: string) => `${V1}/orgs/${orgId}`,
    memberShutdown: (orgId: string, userId: string) =>
      `${V1}/orgs/${orgId}/members/${userId}/shutdown`,
    deactivateMember: (orgId: string, userId: string) =>
      `${V1}/orgs/${orgId}/members/${userId}/deactivate`,
    reactivateMember: (orgId: string, userId: string) =>
      `${V1}/orgs/${orgId}/members/${userId}/reactivate`,
    members: (orgId: string) => `${V1}/orgs/${orgId}/members`,
    inviteMember: (orgId: string) => `${V1}/orgs/${orgId}/members`,
    updateMember: (orgId: string, userId: string) =>
      `${V1}/orgs/${orgId}/members/${userId}`,
    removeMember: (orgId: string, userId: string) =>
      `${V1}/orgs/${orgId}/members/${userId}`,
    invitations: (orgId: string) => `${V1}/orgs/${orgId}/invitations`,
    resendInvitation: (orgId: string, invId: string) =>
      `${V1}/orgs/${orgId}/invitations/${invId}/resend`,
    revokeInvitation: (orgId: string, invId: string) =>
      `${V1}/orgs/${orgId}/invitations/${invId}`,
    scopes: (orgId: string) => `${V1}/orgs/${orgId}/scopes`,
    userScopes: (orgId: string, userId: string) =>
      `${V1}/orgs/${orgId}/scopes/members/${userId}/scopes`,
    repos: (orgId: string) => `${V1}/orgs/${orgId}/repos`,
    workspaces: (orgId: string) => `${V1}/orgs/${orgId}/workspaces`,
    workerHosts: (orgId: string) => `${V1}/orgs/${orgId}/worker-hosts`,
    provisionWorkerHost: (orgId: string) => `${V1}/orgs/${orgId}/worker-hosts`,
    workerHostImagePolicy: (orgId: string) =>
      `${V1}/orgs/${orgId}/worker-hosts/image-policy`,
    deallocateWorkerHost: (orgId: string, hostId: string) =>
      `${V1}/orgs/${orgId}/worker-hosts/${hostId}`,
    refreshWorkerHostRuntimeLabels: (orgId: string, hostId: string) =>
      `${V1}/orgs/${orgId}/worker-hosts/${hostId}/runtime-labels/refresh`,
    // Org approval-definition library (B6 governance PR-1).
    approvalDefinitions: (orgId: string) =>
      `${V1}/orgs/${orgId}/approval-definitions`,
    publishApprovalDefinitionVersion: (orgId: string, definitionId: string) =>
      `${V1}/orgs/${orgId}/approval-definitions/${definitionId}/versions`,
    homeAttention: (orgId: string) => `${V1}/orgs/${orgId}/home/attention`,
    // Base re-verification status (g5.reverification.status).
    homeBaseReverification: (orgId: string) =>
      `${V1}/orgs/${orgId}/home/base-reverification`,
    computeRunning: (orgId: string) => `${V1}/orgs/${orgId}/compute/running`,
    // GitHub App organization binding (routes/github_app.rs).
    githubAppBinding: (orgId: string) =>
      `${V1}/orgs/${orgId}/github-app/binding`,
    githubAppConnectionAttempt: (orgId: string) =>
      `${V1}/orgs/${orgId}/github-app/connection-attempt`,
    githubAppReconnectAttempt: (orgId: string) =>
      `${V1}/orgs/${orgId}/github-app/reconnect-attempt`,
    githubAppRepositories: (orgId: string) =>
      `${V1}/orgs/${orgId}/github-app/binding/repositories`,
    githubAppCoverage: (orgId: string) =>
      `${V1}/orgs/${orgId}/github-app/binding/coverage`,
    githubAppRefresh: (orgId: string) =>
      `${V1}/orgs/${orgId}/github-app/binding/refresh`,
    githubAppDisconnect: (orgId: string) =>
      `${V1}/orgs/${orgId}/github-app/binding/disconnect`,
    githubAppManagementUrl: (orgId: string) =>
      `${V1}/orgs/${orgId}/github-app/binding/management-url`,
    // Issue domains (GitHub repositories, the native G5 tracker) and
    // org-scoped issue reads.
    issueDomains: (orgId: string) => `${V1}/orgs/${orgId}/issue-domains`,
    issueDomain: (orgId: string, domainId: string) =>
      `${V1}/orgs/${orgId}/issue-domains/${domainId}`,
    // An administrator's JSON export of the native tracker's issues.
    issueDomainExport: (orgId: string, domainId: string) =>
      `${V1}/orgs/${orgId}/issue-domains/${domainId}/export`,
    // A domain's label catalog (g5.issue.labels.contract).
    issueDomainLabels: (orgId: string, domainId: string) =>
      `${V1}/orgs/${orgId}/issue-domains/${domainId}/labels`,
    issueDomainLabel: (orgId: string, domainId: string, labelId: string) =>
      `${V1}/orgs/${orgId}/issue-domains/${domainId}/labels/${encodeURIComponent(labelId)}`,
    issueSearch: (orgId: string) => `${V1}/orgs/${orgId}/issues/search`,
    issueResolve: (orgId: string) => `${V1}/orgs/${orgId}/issues/resolve`,
    // Issue creation in the organization's default creation target.
    issueCreateTarget: (orgId: string) =>
      `${V1}/orgs/${orgId}/issues/create-target`,
    issues: (orgId: string) => `${V1}/orgs/${orgId}/issues`,
    githubIssue: (orgId: string, repositoryId: string, number: number) =>
      `${V1}/orgs/${orgId}/issues/github/${encodeURIComponent(repositoryId)}/${number}`,
    nativeIssue: (orgId: string, number: number) =>
      `${V1}/orgs/${orgId}/issues/g5/${number}`,
    // A G5 issue's image attachments (g5.issue.native.attachments).
    nativeIssueAttachments: (orgId: string, number: number) =>
      `${V1}/orgs/${orgId}/issues/g5/${number}/attachments`,
    nativeIssueAttachment: (
      orgId: string,
      number: number,
      attachmentId: string,
    ) =>
      `${V1}/orgs/${orgId}/issues/g5/${number}/attachments/${encodeURIComponent(attachmentId)}`,
    // An issue of either tracker, by locator (`repositoryId` "g5" for a
    // native one): read, edit, state and comments from its page
    // (g5.issue.native.page); a GitHub issue's write target.
    issue: issuePath,
    issueWriteTarget: (orgId: string, locator: ExternalIssueLocatorDTO) =>
      `${issuePath(orgId, locator)}/write-target`,
    issueState: (orgId: string, locator: ExternalIssueLocatorDTO) =>
      `${issuePath(orgId, locator)}/state`,
    issueComments: (orgId: string, locator: ExternalIssueLocatorDTO) =>
      `${issuePath(orgId, locator)}/comments`,
    issueComment: (
      orgId: string,
      locator: ExternalIssueLocatorDTO,
      commentId: string,
    ) =>
      `${issuePath(orgId, locator)}/comments/${encodeURIComponent(commentId)}`,
    // Labels added to and removed from an issue.
    issueLabels: (orgId: string, locator: ExternalIssueLocatorDTO) =>
      `${issuePath(orgId, locator)}/labels`,
  },

  // ---------------------------------------------------------------------------
  // Workspace approvals (B6 governance PR-2)
  // ---------------------------------------------------------------------------
  workspaceApprovals: {
    state: (workspaceId: string) => `${V1}/workspaces/${workspaceId}/approvals`,
    attach: (workspaceId: string) =>
      `${V1}/workspaces/${workspaceId}/approvals/requirements`,
    retire: (workspaceId: string, definitionId: string) =>
      `${V1}/workspaces/${workspaceId}/approvals/requirements/${definitionId}/retire`,
    decide: (workspaceId: string) =>
      `${V1}/workspaces/${workspaceId}/approvals/decisions`,
    requestHumans: (workspaceId: string) =>
      `${V1}/workspaces/${workspaceId}/approvals/requests`,
    cancelRequest: (workspaceId: string, requestId: string) =>
      `${V1}/workspaces/${workspaceId}/approvals/requests/${requestId}/cancel`,
    requestAgentReview: (workspaceId: string) =>
      `${V1}/workspaces/${workspaceId}/approvals/reviews`,
  },

  // ---------------------------------------------------------------------------
  // Review workspaces (reviewer-owned fork of a workspace under review)
  // ---------------------------------------------------------------------------
  workspaceReview: {
    context: (workspaceId: string) => `${V1}/workspaces/${workspaceId}/review`,
    start: (workspaceId: string) =>
      `${V1}/workspaces/${workspaceId}/review/start`,
    publish: (reviewWorkspaceId: string) =>
      `${V1}/workspaces/${reviewWorkspaceId}/review/publish`,
    fileSuggestions: (workspaceId: string) =>
      `${V1}/workspaces/${workspaceId}/review/file-suggestions`,
    discardChanges: (reviewWorkspaceId: string) =>
      `${V1}/workspaces/${reviewWorkspaceId}/review/changes/discard`,
    publications: (workspaceId: string) =>
      `${V1}/workspaces/${workspaceId}/review/publications`,
    suggestions: (workspaceId: string) =>
      `${V1}/workspaces/${workspaceId}/review/suggestions`,
    suggestionCounts: (workspaceId: string) =>
      `${V1}/workspaces/${workspaceId}/review/suggestions/counts`,
    suggestionAction: (
      workspaceId: string,
      suggestionId: string,
      action: "accept" | "reject" | "resolve" | "withdraw",
    ) =>
      `${V1}/workspaces/${workspaceId}/review/suggestions/${suggestionId}/${action}`,
    suggestionReplies: (workspaceId: string, suggestionId: string) =>
      `${V1}/workspaces/${workspaceId}/review/suggestions/${suggestionId}/replies`,
    preview: (reviewWorkspaceId: string) =>
      `${V1}/workspaces/${reviewWorkspaceId}/review/preview`,
    artifact: (workspaceId: string, suggestionId: string) =>
      `${V1}/workspaces/${workspaceId}/review/suggestions/${suggestionId}/artifact`,
  },

  // ---------------------------------------------------------------------------
  // Invitations
  // ---------------------------------------------------------------------------
  invitations: {
    accept: (id: string) => `${V1}/invitations/${id}/accept`,
    decline: (id: string) => `${V1}/invitations/${id}/decline`,
  },

  // ---------------------------------------------------------------------------
  // Workspaces
  // ---------------------------------------------------------------------------
  workspaces: {
    list: (orgId: string) => `${V1}/orgs/${orgId}/workspaces`,
    get: (id: string) => `${V1}/workspaces/${id}`,
    changes: (id: string) => `${V1}/workspaces/${id}/changes`,
    changesAction: (
      id: string,
      action: "push" | "update" | "switch-back" | "commit",
    ) => `${V1}/workspaces/${id}/changes/${action}`,
    update: (id: string) => `${V1}/workspaces/${id}`,
    delete: (id: string) => `${V1}/workspaces/${id}/delete`,
    stats: (id: string) => `${V1}/workspaces/${id}/stats`,
    branches: (id: string) => `${V1}/workspaces/${id}/branches`,
    sync: (id: string) => `${V1}/workspaces/${id}/sync`,
    merge: (id: string) => `${V1}/workspaces/${id}/merge`,
    rebaseMerge: (id: string) => `${V1}/workspaces/${id}/rebase-merge`,
    clone: (id: string) => `${V1}/workspaces/${id}/clone`,
    sourceDownload: (id: string) => `${V1}/workspaces/${id}/source-download`,
    // External issue links (primary first; unlink never touches the issue).
    issueLinks: (id: string) => `${V1}/workspaces/${id}/issue-links`,
    activity: (id: string) => `${V1}/workspaces/${id}/activity`,
    activityCurrent: (id: string) => `${V1}/workspaces/${id}/activity/current`,
    activityComments: (id: string) =>
      `${V1}/workspaces/${id}/activity/comments`,
    activityGithubComment: (id: string, commentId: string) =>
      `${V1}/workspaces/${id}/activity/comments/github/${encodeURIComponent(commentId)}`,
    activityNativeComment: (id: string, commentId: string) =>
      `${V1}/workspaces/${id}/activity/comments/g5/${encodeURIComponent(commentId)}`,
    unlinkIssueLink: (id: string, linkId: string) =>
      `${V1}/workspaces/${id}/issue-links/${linkId}/unlink`,
    // Close (as completed) or reopen a linked issue (GitHub: as the
    // caller's own account).
    issueLinkState: (id: string, linkId: string) =>
      `${V1}/workspaces/${id}/issue-links/${linkId}/state`,
  },

  // ---------------------------------------------------------------------------
  // Docs  (nested under workspaces/:workspaceId/docs)
  // ---------------------------------------------------------------------------
  docs: {
    list: (wsId: string) => `${V1}/workspaces/${wsId}/docs`,
    create: (wsId: string) => `${V1}/workspaces/${wsId}/docs`,
    get: (wsId: string, docId: string) =>
      `${V1}/workspaces/${wsId}/docs/${docId}`,
    update: (wsId: string, docId: string) =>
      `${V1}/workspaces/${wsId}/docs/${docId}`,
    updateProjectDoc: (wsId: string, docId: string) =>
      `${V1}/workspaces/${wsId}/docs/project-docs/${encodeURIComponent(docId)}`,
    deleteProjectDoc: (wsId: string, docId: string) =>
      `${V1}/workspaces/${wsId}/docs/project-docs/${encodeURIComponent(docId)}`,
    // Retained view docs delete through the doc-scoped route (`delete_view`
    // server-side); the project-docs route rejects view types.
    deleteViewDoc: (wsId: string, docId: string) =>
      `${V1}/workspaces/${wsId}/docs/${docId}`,
  },

  // ---------------------------------------------------------------------------
  // Tasks
  // ---------------------------------------------------------------------------
  tasks: {
    get: (id: string) => `${V1}/tasks/${id}`,
    timeline: (id: string) => `${V1}/tasks/${id}/timeline`,
  },

  // ---------------------------------------------------------------------------
  // Comments
  // ---------------------------------------------------------------------------
  comments: {
    list: (wsId: string) => `${V1}/workspaces/${wsId}/comments`,
    create: (wsId: string) => `${V1}/workspaces/${wsId}/comments`,
    update: (id: string) => `${V1}/comments/${id}`,
    delete: (id: string) => `${V1}/comments/${id}`,
    replies: (id: string) => `${V1}/comments/${id}/replies`,
  },

  // ---------------------------------------------------------------------------
  // Chat Sessions
  // ---------------------------------------------------------------------------
  sessions: {
    list: (wsId: string) => `${V1}/workspaces/${wsId}/sessions`,
    create: (wsId: string) => `${V1}/workspaces/${wsId}/sessions`,
    search: (wsId: string) => `${V1}/workspaces/${wsId}/sessions/search`,
    get: (id: string) => `${V1}/sessions/${id}`,
    messages: (id: string) => `${V1}/sessions/${id}/messages`,
    stopMessage: (id: string, mid: string) =>
      `${V1}/sessions/${id}/messages/${mid}/stop`,
    liveTail: (id: string) => `${V1}/sessions/${id}/live-tail`,
    fork: (id: string) => `${V1}/sessions/${id}/fork`,
  },

  // ---------------------------------------------------------------------------
  // Settings & Spend
  // ---------------------------------------------------------------------------
  settings: {
    get: (wsId: string) => `${V1}/workspaces/${wsId}/settings`,
    update: (wsId: string) => `${V1}/workspaces/${wsId}/settings`,
    spend: (wsId: string) => `${V1}/workspaces/${wsId}/settings/spend`,
    spendHistory: (wsId: string) =>
      `${V1}/workspaces/${wsId}/settings/spend/history`,
  },

  // ---------------------------------------------------------------------------
  // Files
  // ---------------------------------------------------------------------------
  files: {
    tree: (workspaceId: string, repoId: string) =>
      `${V1}/workspaces/${workspaceId}/repos/${repoId}/files`,
    content: (workspaceId: string, repoId: string, path: string) =>
      `${V1}/workspaces/${workspaceId}/repos/${repoId}/files/${path}`,
    writeEntry: (workspaceId: string, repoId: string, path: string) =>
      `${V1}/workspaces/${workspaceId}/repos/${repoId}/files/${path}`,
    deleteEntry: (workspaceId: string, repoId: string, path: string) =>
      `${V1}/workspaces/${workspaceId}/repos/${repoId}/files/${path}`,
  },

  // ---------------------------------------------------------------------------
  // Permissions
  // ---------------------------------------------------------------------------
  permissions: {
    listOverrides: (wsId: string) => `${V1}/workspaces/${wsId}/permissions`,
    setOverride: (wsId: string, userId: string) =>
      `${V1}/workspaces/${wsId}/permissions/${userId}`,
    removeOverride: (wsId: string, userId: string) =>
      `${V1}/workspaces/${wsId}/permissions/${userId}`,
  },

  // ---------------------------------------------------------------------------
  // SCM
  // ---------------------------------------------------------------------------
  scm: {
    connect: (provider: string) => `${V1}/scm/${provider}/connect`,
    callback: (provider: string) => `${V1}/scm/${provider}/callback`,
    disconnect: (provider: string) => `${V1}/scm/${provider}/disconnect`,
    repos: (provider: string) => `${V1}/scm/${provider}/repos`,
    importRepos: (provider: string) => `${V1}/scm/${provider}/repos/import`,
    orgs: (provider: string) => `${V1}/scm/${provider}/orgs`,
  },

  // ---------------------------------------------------------------------------
  // Instance
  // ---------------------------------------------------------------------------
  instance: {
    info: `${V1}/instance/info`,
    health: `${V1}/instance/health`,
    chatModels: `${V1}/instance/chat/models`,
  },

  // ---------------------------------------------------------------------------
  // Platform bug reporting
  // ---------------------------------------------------------------------------
  platformIssues: {
    create: `${V1}/platform/issues`,
  },

  // ---------------------------------------------------------------------------
  // Resolve
  // ---------------------------------------------------------------------------
  resolve: `${V1}/resolve`,

  // ---------------------------------------------------------------------------
  // Telemetry (UI interaction ingest, issue #1552)
  // ---------------------------------------------------------------------------
  telemetry: {
    uiEvents: `${V1}/telemetry/ui-events`,
  },

  // ---------------------------------------------------------------------------
  // ---------------------------------------------------------------------------
  // WebSocket
  // ---------------------------------------------------------------------------
  ws: `${V1}/ws`,
} as const;
