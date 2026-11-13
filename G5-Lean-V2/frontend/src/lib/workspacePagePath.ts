// =============================================================================
// G5 Client — Workspace page route
// =============================================================================
// One home for the `/:org/:workspace` path every surface links to: the
// switcher's row hrefs, review landing, the workspace tab strip and the fork
// picker. Workspace names are Git branch names and may contain `/`, so the
// name is one encoded path segment (react-router decodes the param).
// =============================================================================

export function workspacePagePath(
  orgSlug: string,
  workspaceName: string,
): string {
  return `/${orgSlug}/${encodeURIComponent(workspaceName)}`;
}

/** The caller's own personal workspace (g5.personal.routes). */
export function personalWorkspaceMePath(orgSlug: string): string {
  return `/${orgSlug}/me`;
}

/**
 * A member's personal workspace by handle (g5.personal.routes). The route
 * resolves by registry kind; it reads like the `users/<handle>` name.
 */
export function personalWorkspaceUserPath(
  orgSlug: string,
  handle: string,
): string {
  return `/${orgSlug}/users/${encodeURIComponent(handle)}`;
}

/** Direct task identity; independent of any list page or conversation. */
export function taskRoutePath(taskId: string): string {
  return `/task/${encodeURIComponent(taskId)}`;
}
