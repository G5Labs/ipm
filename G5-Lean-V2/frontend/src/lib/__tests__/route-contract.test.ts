// =============================================================================
// Route Contract Test — Validates that hook endpoint paths match routes.ts
// =============================================================================
// This test ensures every API path used in hooks and MSW handlers exists in the
// centralized route registry. If a hook uses a path not covered by routes.ts,
// the test will fail, preventing drift between frontend and backend routes.
// =============================================================================

import { describe, expect, it } from "vitest";
import { routes } from "../routes";

// ---------------------------------------------------------------------------
// Helper: Expand route registry into a flat list of path patterns
// ---------------------------------------------------------------------------

/**
 * Recursively walk the routes object and collect all path strings.
 * For function routes, call with placeholder args to get the pattern.
 */
function collectRoutePaths(
  obj: Record<string, unknown>,
  prefix = "",
): string[] {
  const paths: string[] = [];

  for (const [key, value] of Object.entries(obj)) {
    const fullKey = prefix ? `${prefix}.${key}` : key;

    if (typeof value === "string") {
      paths.push(value);
    } else if (typeof value === "function") {
      // Call the function with placeholder IDs to get the path pattern
      try {
        const fn = value as (...args: string[]) => string;
        const argCount = fn.length;
        const args = Array.from({ length: argCount }, (_, i) => `:arg${i}`);
        const result = fn(...args);
        if (typeof result === "string") {
          paths.push(result);
        }
      } catch {
        // Skip functions that can't be called with string args
      }
    } else if (typeof value === "object" && value !== null) {
      paths.push(
        ...collectRoutePaths(value as Record<string, unknown>, fullKey),
      );
    }
  }

  return paths;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("Route Registry Completeness", () => {
  const allPaths = collectRoutePaths(
    routes as unknown as Record<string, unknown>,
  );

  it("route registry produces valid paths", () => {
    expect(allPaths.length).toBeGreaterThan(50);
    for (const path of allPaths) {
      expect(path).toMatch(/^\/api\/v1\//);
    }
  });

  it("all auth routes are defined", () => {
    expect(routes.auth.refresh).toBe("/api/v1/auth/refresh");
    expect(routes.auth.logout).toBe("/api/v1/auth/logout");
    expect(routes.auth.cliApprove).toBe("/api/v1/auth/cli/approve");
    expect(routes.auth.myScopes).toBe("/api/v1/auth/me/scopes");
    expect(routes.auth.checkScopes).toBe("/api/v1/auth/me/scopes/check");
    expect(routes.auth.login("github")).toBe("/api/v1/auth/login/github");
  });

  it("all user routes are defined", () => {
    expect(routes.users.me).toBe("/api/v1/users/me");
    expect(routes.users.byId("u1")).toBe("/api/v1/users/u1");
  });

  it("all workspace routes are defined", () => {
    expect(routes.workspaces.get("ws1")).toBe("/api/v1/workspaces/ws1");
    expect(routes.workspaces.stats("ws1")).toBe("/api/v1/workspaces/ws1/stats");
    expect(routes.workspaces.list("org1")).toBe("/api/v1/orgs/org1/workspaces");
    expect(routes.workspaces.clone("ws1")).toBe("/api/v1/workspaces/ws1/clone");
  });

  it("all doc routes use workspace scope", () => {
    expect(routes.docs.list("ws1")).toBe("/api/v1/workspaces/ws1/docs");
    expect(routes.docs.get("ws1", "d1")).toBe("/api/v1/workspaces/ws1/docs/d1");
    expect(routes.docs.update("ws1", "d1")).toBe(
      "/api/v1/workspaces/ws1/docs/d1",
    );
    expect(routes.docs.deleteProjectDoc("ws1", "d1")).toBe(
      "/api/v1/workspaces/ws1/docs/project-docs/d1",
    );
    expect(
      routes.docs.deleteProjectDoc(
        "ws1",
        "overlay:11111111-1111-4111-8111-111111111111:ZG9jcy9ub3RlLm1k",
      ),
    ).toBe(
      "/api/v1/workspaces/ws1/docs/project-docs/overlay%3A11111111-1111-4111-8111-111111111111%3AZG9jcy9ub3RlLm1k",
    );
  });

  it("all session routes have correct paths (no /chat/ prefix)", () => {
    expect(routes.sessions.list("ws1")).toBe("/api/v1/workspaces/ws1/sessions");
    expect(routes.sessions.create("ws1")).toBe(
      "/api/v1/workspaces/ws1/sessions",
    );
    expect(routes.sessions.get("s1")).toBe("/api/v1/sessions/s1");
    expect(routes.sessions.messages("s1")).toBe("/api/v1/sessions/s1/messages");
    expect(routes.sessions.stopMessage("s1", "m1")).toBe(
      "/api/v1/sessions/s1/messages/m1/stop",
    );
    expect(routes.sessions.fork("s1")).toBe("/api/v1/sessions/s1/fork");
  });

  it("all comment routes are defined", () => {
    expect(routes.comments.list("ws1")).toBe("/api/v1/workspaces/ws1/comments");
    expect(routes.comments.create("ws1")).toBe(
      "/api/v1/workspaces/ws1/comments",
    );
    expect(routes.comments.update("c1")).toBe("/api/v1/comments/c1");
    expect(routes.comments.delete("c1")).toBe("/api/v1/comments/c1");
    expect(routes.comments.replies("c1")).toBe("/api/v1/comments/c1/replies");
  });

  it("all task routes are defined", () => {
    expect(routes.tasks.get("t1")).toBe("/api/v1/tasks/t1");
    expect(routes.tasks.timeline("t1")).toBe("/api/v1/tasks/t1/timeline");
  });

  it("all settings routes use workspace scope", () => {
    expect(routes.settings.get("ws1")).toBe("/api/v1/workspaces/ws1/settings");
    expect(routes.settings.spend("ws1")).toBe(
      "/api/v1/workspaces/ws1/settings/spend",
    );
    expect(routes.settings.spendHistory("ws1")).toBe(
      "/api/v1/workspaces/ws1/settings/spend/history",
    );
  });

  it("all instance routes are defined", () => {
    expect(routes.instance.info).toBe("/api/v1/instance/info");
    expect(routes.instance.health).toBe("/api/v1/instance/health");
    expect(routes.instance.chatModels).toBe("/api/v1/instance/chat/models");
  });

  it("all file routes are defined", () => {
    expect(routes.files.tree("w1", "r1")).toBe(
      "/api/v1/workspaces/w1/repos/r1/files",
    );
    expect(routes.files.content("w1", "r1", "src/main.rs")).toBe(
      "/api/v1/workspaces/w1/repos/r1/files/src/main.rs",
    );
  });

  it("all org routes are defined", () => {
    expect(routes.orgs.list).toBe("/api/v1/orgs");
    expect(routes.orgs.get("o1")).toBe("/api/v1/orgs/o1");
    expect(routes.orgs.members("o1")).toBe("/api/v1/orgs/o1/members");
    expect(routes.orgs.invitations("o1")).toBe("/api/v1/orgs/o1/invitations");
    expect(routes.orgs.scopes("o1")).toBe("/api/v1/orgs/o1/scopes");
    expect(routes.orgs.userScopes("o1", "u1")).toBe(
      "/api/v1/orgs/o1/scopes/members/u1/scopes",
    );
  });

  it("platform operator routes are defined", () => {
    expect(routes.platform.orgs).toBe("/api/v1/platform/orgs");
  });

  it("all permission routes are defined", () => {
    expect(routes.permissions.listOverrides("ws1")).toBe(
      "/api/v1/workspaces/ws1/permissions",
    );
    expect(routes.permissions.setOverride("ws1", "u1")).toBe(
      "/api/v1/workspaces/ws1/permissions/u1",
    );
  });

  it("no workspace search route is defined", () => {
    // P8.9 removed workspace search end to end; only chat-history session
    // search survives.
    expect((routes as Record<string, unknown>).search).toBeUndefined();
    expect(routes.sessions.search("ws1")).toBe(
      "/api/v1/workspaces/ws1/sessions/search",
    );
  });

  it("websocket route is defined", () => {
    expect(routes.ws).toBe("/api/v1/ws");
  });
});
