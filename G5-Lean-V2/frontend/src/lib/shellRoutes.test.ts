import { describe, expect, it } from "vitest";

import { classifyShellRoute } from "./shellRoutes";

// Shell route classification shared by the bug-report trigger and the tab
// strip (workbench g5.workbench.tabs).
describe("classifyShellRoute", () => {
  it.each([
    ["/acme/home", { kind: "home", orgSlug: "acme" }],
    ["/acme/fork", { kind: "fork", orgSlug: "acme" }],
    [
      "/acme/demo",
      { kind: "workspace", orgSlug: "acme", workspaceName: "demo" },
    ],
    [
      "/acme/My%20Space/doc/readme",
      { kind: "workspace", orgSlug: "acme", workspaceName: "My Space" },
    ],
    [
      "/acme/demo/task/456",
      { kind: "workspace", orgSlug: "acme", workspaceName: "demo" },
    ],
    // Center surfaces with an identity (issue #561) stay shell routes.
    [
      "/acme/demo/task/123",
      { kind: "workspace", orgSlug: "acme", workspaceName: "demo" },
    ],
    [
      "/acme/demo/guide/getting-started",
      { kind: "workspace", orgSlug: "acme", workspaceName: "demo" },
    ],
    [
      "/acme/demo/issue/11/12",
      { kind: "workspace", orgSlug: "acme", workspaceName: "demo" },
    ],
    [
      "/acme/demo/commit/repo-1/abc123",
      { kind: "workspace", orgSlug: "acme", workspaceName: "demo" },
    ],
    ["/acme/fork/", { kind: "fork", orgSlug: "acme" }],
    // The picker lives under its organization; the bare path is no route.
    ["/fork", { kind: "other" }],
    ["/acme/fork/extra", { kind: "other" }],
    ["/account/github", { kind: "other" }],
    // An organization slugged like a static route still has workspace pages.
    [
      "/projects/base",
      { kind: "workspace", orgSlug: "projects", workspaceName: "base" },
    ],
    [
      "/account/base/doc/readme",
      { kind: "workspace", orgSlug: "account", workspaceName: "base" },
    ],
    // Personal workspace routes resolve by kind (g5.personal.routes).
    [
      "/acme/me",
      {
        kind: "workspace",
        orgSlug: "acme",
        workspaceName: "me",
        personal: "me",
      },
    ],
    [
      "/acme/me/file/notes.md",
      {
        kind: "workspace",
        orgSlug: "acme",
        workspaceName: "me",
        personal: "me",
      },
    ],
    [
      "/acme/users/ada",
      {
        kind: "workspace",
        orgSlug: "acme",
        workspaceName: "users/ada",
        personal: "user",
      },
    ],
    [
      "/acme/users/ada/task/7",
      {
        kind: "workspace",
        orgSlug: "acme",
        workspaceName: "users/ada",
        personal: "user",
      },
    ],
    // `users` alone is a reserved organization segment, not a workspace.
    ["/acme/users", { kind: "other" }],
    ["/no-organization", { kind: "other" }],
    ["/acme/settings", { kind: "other" }],
    ["/acme/members", { kind: "other" }],
    ["/acme/workers", { kind: "other" }],
    ["/login/denied", { kind: "other" }],
    ["/cli/authorize", { kind: "other" }],
    ["/projects", { kind: "other" }],
    ["/", { kind: "other" }],
  ])("classifies %s", (pathname, expected) => {
    expect(classifyShellRoute(pathname)).toEqual(expected);
  });
});
