// =============================================================================
// G5 Client — Shell route classification
// =============================================================================
// Which top-level route a pathname is on, for chrome that lives above the
// pages: the platform bug-report trigger (`App.tsx`) and the workspace tab
// strip (workbench g5.workbench.tabs). Pure functions over the pathname so
// both can share one classification without a dependency on `App.tsx`.
// =============================================================================

import { matchPath } from "react-router-dom";

/** The workspace page's routes under one workspace root path pattern. */
export interface WorkspaceRoutes {
  task: string;
  root: string;
  doc: string;
  file: string;
  intentLog: string;
  guide: string;
  issue: string;
  commit: string;
}

function workspaceRoutesUnder(root: string): WorkspaceRoutes {
  return {
    task: `${root}/task/:taskId`,
    root,
    doc: `${root}/doc/*`,
    file: `${root}/file/*`,
    intentLog: `${root}/intent-log`,
    guide: `${root}/guide/:guideId`,
    issue: `${root}/issue/:repositoryId/:number`,
    commit: `${root}/commit/:repoId/:sha`,
  };
}

/**
 * The workspace page's routes, by name: one registry for `App.tsx`'s route
 * registration, the page's `useMatch` calls and the shell classification
 * below. Content with an identity owns a path (g5.workbench.tabs).
 */
export const WORKSPACE_ROUTES: WorkspaceRoutes =
  workspaceRoutesUnder("/:org/:workspace");

/**
 * How a workspace route names its workspace: by name, or a personal
 * workspace by registry kind (personal_workspace g5.personal.routes) —
 * the caller's own at `/{org}/me`, a member's at `/{org}/users/{handle}`.
 */
export type WorkspaceRouteFamily = "workspace" | "me" | "user";

/** The same page routes under each family's root. */
export const WORKSPACE_ROUTE_FAMILIES: Record<
  WorkspaceRouteFamily,
  WorkspaceRoutes
> = {
  workspace: WORKSPACE_ROUTES,
  me: workspaceRoutesUnder("/:org/me"),
  user: workspaceRoutesUnder("/:org/users/:handle"),
};

export const WORKSPACE_SHELL_ROUTE_PATTERNS = Object.values(WORKSPACE_ROUTES);

/** Personal workspace route patterns, registered before the name routes. */
export const PERSONAL_SHELL_ROUTE_PATTERNS: ReadonlyArray<{
  family: "me" | "user";
  path: string;
}> = (["me", "user"] as const).flatMap((family) =>
  Object.values(WORKSPACE_ROUTE_FAMILIES[family]).map((path) => ({
    family,
    path,
  })),
);

/**
 * Route pattern of the organization fork picker ("+" in the tab strip): a
 * reserved segment under the organization, like `/:org/home`, so the URL
 * names the organization the picker is scoped to.
 */
export const FORK_PICKER_ROUTE = "/:org/fork";

/** Route of an organization's fork picker. */
export function forkPickerPath(orgSlug: string): string {
  return `/${orgSlug}/fork`;
}

/**
 * Static application routes in `App.tsx` that React Router ranks above
 * `/:org/:workspace`. Only an exact match is excluded: an organization may be
 * slugged `projects` or `account`, and `/projects/base` is then its workspace
 * page.
 */
const STATIC_ROUTES = [
  "/login",
  "/login/denied",
  "/landing",
  "/account/github",
  "/cli/authorize",
  "/forbidden",
  "/settings",
  "/projects",
  "/no-organization",
];

/** Workspace names that are organization-level client routes, never pages. */
const NON_WORKSPACE_SEGMENTS = new Set([
  "settings",
  "members",
  "workers",
  "home",
  "fork",
  "me",
  "users",
]);

export type ShellRoute =
  | { kind: "home"; orgSlug: string }
  | {
      kind: "workspace";
      orgSlug: string;
      /**
       * The route's workspace name; `me` for the caller's own personal
       * workspace, whose name the route does not carry.
       */
      workspaceName: string;
      /** Set on a personal workspace route (g5.personal.routes). */
      personal?: "me" | "user";
    }
  | { kind: "fork"; orgSlug: string }
  | { kind: "other" };

/** Classify a pathname into the shell routes the tab strip cares about. */
export function classifyShellRoute(pathname: string): ShellRoute {
  if (STATIC_ROUTES.some((path) => matchPath({ path, end: true }, pathname))) {
    return { kind: "other" };
  }
  const home = matchPath({ path: "/:org/home", end: true }, pathname);
  if (home?.params.org) return { kind: "home", orgSlug: home.params.org };
  const fork = matchPath({ path: FORK_PICKER_ROUTE, end: true }, pathname);
  if (fork?.params.org) return { kind: "fork", orgSlug: fork.params.org };
  for (const { family, path } of PERSONAL_SHELL_ROUTE_PATTERNS) {
    const match = matchPath({ path, end: true }, pathname);
    if (!match?.params.org) continue;
    if (family === "me") {
      return {
        kind: "workspace",
        orgSlug: match.params.org,
        workspaceName: "me",
        personal: "me",
      };
    }
    const handle = match.params.handle;
    if (!handle) return { kind: "other" };
    return {
      kind: "workspace",
      orgSlug: match.params.org,
      workspaceName: `users/${handle}`,
      personal: "user",
    };
  }
  for (const path of WORKSPACE_SHELL_ROUTE_PATTERNS) {
    const match = matchPath({ path, end: true }, pathname);
    if (!match) continue;
    const { org, workspace } = match.params as {
      org?: string;
      workspace?: string;
    };
    if (!org || !workspace || NON_WORKSPACE_SEGMENTS.has(workspace)) {
      return { kind: "other" };
    }
    let workspaceName = workspace;
    try {
      workspaceName = decodeURIComponent(workspace);
    } catch {
      // A malformed escape keeps the raw segment; the page reports it.
    }
    return { kind: "workspace", orgSlug: org, workspaceName };
  }
  return { kind: "other" };
}

/** True on a workspace page route (the platform report trigger's context). */
export function isWorkspaceShellRoute(pathname: string): boolean {
  return classifyShellRoute(pathname).kind === "workspace";
}
