import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { HttpResponse, http } from "msw";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import App, { isWorkspaceShellRoute } from "./App";
import { MOCK_CURRENT_USER } from "./data/mock";
import {
  getPostAuthRedirect,
  rememberPostAuthRedirect,
} from "./lib/postAuthRedirect";
import { server } from "./test/msw/server";
import { _activeTestClients, createTestJwt } from "./test/render";

// Route tests mount the real socket provider; keep its transport local. Socket
// ticket acquisition and reconnection are exercised by App.wsTicket.test.tsx.
class RouteTestSocket {
  static CONNECTING = 0;
  static OPEN = 1;
  static CLOSED = 3;
  readyState = RouteTestSocket.CONNECTING;
  onopen = null;
  onclose = null;
  onerror = null;
  onmessage = null;
  send() {}
  close() {
    this.readyState = RouteTestSocket.CLOSED;
  }
}

beforeEach(() => vi.stubGlobal("WebSocket", RouteTestSocket));
afterEach(() => vi.unstubAllGlobals());

function renderAppAt(route: string) {
  window.history.pushState({}, "", route);
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });
  _activeTestClients.push(queryClient);

  return render(
    <QueryClientProvider client={queryClient}>
      <App />
    </QueryClientProvider>,
  );
}

describe("dedicated application root", () => {
  beforeEach(() => {
    sessionStorage.clear();
    server.use(
      http.get("/api/v1/workspaces/:workspaceId/provider-quota", () =>
        HttpResponse.json({ data: null }),
      ),
    );
  });
  afterEach(() => {
    window.history.pushState({}, "", "/");
    localStorage.clear();
    sessionStorage.clear();
  });

  it("opens login on the same origin without remembering root as a return destination", async () => {
    localStorage.setItem("g5_test_disable_auth", "1");
    const origin = window.location.origin;
    const historyLength = window.history.length;
    renderAppAt("/");
    expect(
      await screen.findByRole("link", { name: /continue with github/i }),
    ).toBeInTheDocument();
    expect(window.location.pathname).toBe("/login");
    expect(window.location.origin).toBe(origin);
    expect(window.history.length).toBe(historyLength + 1);
    expect(getPostAuthRedirect()).toBeNull();
  });

  it.each([
    ["org-g5labs", "/g5labs/home"],
    ["no-longer-a-membership", "/g5labs/home"],
  ])(
    "lands on the organization Home for active organization %s (g5.ui.sign-in-landing)",
    async (activeOrgId, destination) => {
      const user = {
        ...MOCK_CURRENT_USER,
        activeOrgId,
        // An active membership need not be first; a missing one falls back.
        orgs:
          activeOrgId === "org-g5labs"
            ? [...MOCK_CURRENT_USER.orgs].reverse()
            : MOCK_CURRENT_USER.orgs,
      };
      localStorage.setItem(
        "g5_auth_token",
        createTestJwt({ ...user, orgs: [], scopes: [] }, "org-g5labs"),
      );
      server.use(
        http.get("/api/v1/users/me", () => HttpResponse.json({ data: user })),
      );
      renderAppAt("/");
      await waitFor(() => expect(window.location.pathname).toBe(destination));
      expect(await screen.findByTestId("home-columns")).toBeInTheDocument();
    },
  );

  // Organizations are created only by platform operators
  // (g5.organization.creation): an account without one is asked to get an
  // invitation, never offered self-serve creation.
  it("sends an authenticated account without memberships to /no-organization", async () => {
    server.use(
      http.get("/api/v1/users/me", () =>
        HttpResponse.json({
          data: { ...MOCK_CURRENT_USER, activeOrgId: null, orgs: [] },
        }),
      ),
    );
    renderAppAt("/");
    expect(
      await screen.findByTestId("no-organization-email"),
    ).toHaveTextContent(MOCK_CURRENT_USER.email);
    expect(window.location.pathname).toBe("/no-organization");
    expect(
      screen.getByText(/ask an organization administrator for an invitation/i),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("dialog", { name: /create organization/i }),
    ).toBeNull();
  });

  it("signs out from /no-organization to use a different account", async () => {
    server.use(
      http.get("/api/v1/users/me", () =>
        HttpResponse.json({
          data: { ...MOCK_CURRENT_USER, activeOrgId: null, orgs: [] },
        }),
      ),
    );
    renderAppAt("/no-organization");
    fireEvent.click(
      await screen.findByTestId("no-organization-different-account"),
    );
    await waitFor(() => expect(window.location.pathname).toBe("/login"));
  });

  it("opens the organization when an account with a membership visits /no-organization", async () => {
    renderAppAt("/no-organization");
    await waitFor(() => expect(window.location.pathname).toBe("/g5labs/base"));
  });

  it("no longer routes /create-org to organization creation", async () => {
    // The retired segment is now just an organization slug nobody holds.
    renderAppAt("/create-org");
    expect(
      await screen.findByText(/do not have access to this organization/i),
    ).toBeInTheDocument();
    expect(window.location.pathname).toBe("/create-org/home");
    expect(
      screen.queryByRole("dialog", { name: /create organization/i }),
    ).toBeNull();
  });

  it("waits for session recovery before choosing login", async () => {
    let releaseRefresh!: () => void;
    const pendingRefresh = new Promise<void>((resolve) => {
      releaseRefresh = resolve;
    });
    server.use(
      http.post("/api/v1/auth/refresh", async () => {
        await pendingRefresh;
        return HttpResponse.json({}, { status: 401 });
      }),
    );
    renderAppAt("/");
    expect(
      screen.getByRole("status", { name: "Loading page" }),
    ).toBeInTheDocument();
    expect(window.location.pathname).toBe("/");
    expect(
      screen.queryByRole("link", { name: /continue with github/i }),
    ).toBeNull();
    releaseRefresh();
    await waitFor(() => expect(window.location.pathname).toBe("/login"));
  });

  it("resumes a preserved app URL, including its query and fragment", async () => {
    const destination = "/cli/authorize?userCode=5F2B-D227#approval";
    rememberPostAuthRedirect(destination);
    renderAppAt("/");
    expect(
      await screen.findByRole("heading", { name: "Authorize G5 CLI" }),
    ).toBeInTheDocument();
    expect(
      window.location.pathname + window.location.search + window.location.hash,
    ).toBe(destination);
  });

  it("keeps an earlier protected destination when root sends a signed-out visitor to login", async () => {
    localStorage.setItem("g5_test_disable_auth", "1");
    rememberPostAuthRedirect("/cli/authorize?userCode=5F2B-D227#approval");
    renderAppAt("/");
    await waitFor(() => expect(window.location.pathname).toBe("/login"));
    expect(getPostAuthRedirect()).toBe(
      "/cli/authorize?userCode=5F2B-D227#approval",
    );
  });
});

describe("App CLI authorization route", () => {
  afterEach(() => {
    window.history.pushState({}, "", "/");
    localStorage.clear();
  });

  it("preserves the CLI authorization URL when sign-in is required", async () => {
    localStorage.setItem("g5_test_disable_auth", "1");

    renderAppAt("/cli/authorize?userCode=5F2B-D227");

    await waitFor(() => {
      expect(getPostAuthRedirect()).toBe("/cli/authorize?userCode=5F2B-D227");
    });
  });

  it("approves the CLI device request from /cli/authorize", async () => {
    let requestBody: unknown;
    server.use(
      http.post("/api/v1/auth/cli/approve", async ({ request }) => {
        requestBody = await request.json();
        return HttpResponse.json({ data: { status: "approved" } });
      }),
    );

    renderAppAt("/cli/authorize?userCode=5F2B-D227");

    expect(
      await screen.findByRole("heading", { name: "Authorize G5 CLI" }),
    ).toBeInTheDocument();
    expect(screen.getByText("5F2B-D227")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /approve/i }));

    await waitFor(() => {
      expect(requestBody).toEqual({
        userCode: "5F2B-D227",
        approve: true,
      });
    });
    expect(
      await screen.findByText(
        "CLI authorization approved. Return to your terminal.",
      ),
    ).toBeInTheDocument();
  });
});

describe("retained approval report route", () => {
  afterEach(() => {
    window.history.pushState({}, "", "/");
    localStorage.clear();
    sessionStorage.clear();
  });
  it("requires sign-in and preserves the exact report destination", async () => {
    localStorage.setItem("g5_test_disable_auth", "1");
    renderAppAt("/g5labs/review-reports/report/workspace/exact-report");
    await waitFor(() =>
      expect(getPostAuthRedirect()).toBe(
        "/g5labs/review-reports/report/workspace/exact-report",
      ),
    );
    expect(
      screen.queryByTestId("agent-review-report-view"),
    ).not.toBeInTheDocument();
  });
  it("opens an authenticated report URL directly without a workspace shell", async () => {
    server.use(
      http.get(
        "/api/v1/workspaces/workspace/approvals/reviews/exact-report/report",
        () => HttpResponse.json({ data: { document: "# Retained findings" } }),
      ),
    );
    renderAppAt("/g5labs/review-reports/report/workspace/exact-report");
    expect(
      await screen.findByRole("heading", { name: "Retained findings" }),
    ).toBeVisible();
    expect(
      screen.queryByTestId("workspace-approvals-requirement"),
    ).not.toBeInTheDocument();
  });
});

describe("workspace shell route matching", () => {
  it.each([
    ["/acme/demo", true],
    ["/acme/demo/spec/architecture", false],
    ["/acme/demo/task/456", true],
    ["/acme/demo/issues/bug-123", false],
    ["/landing/extra/fallback", false],
    ["/unknown/extra/fallback", false],
    ["/login/denied", false],
    ["/cli/authorize", false],
    ["/acme/settings", false],
    ["/acme/members", false],
    ["/acme/workers", false],
    ["/acme/home", false],
  ])("classifies %s as workspace route: %s", (pathname, expected) => {
    expect(isWorkspaceShellRoute(pathname)).toBe(expected);
  });
});

// bug-report-affordance#D3 / layout_and_navigation#D6: the shell fallback bug
// trigger is always mounted above the modal/overlay layer, including on
// workspace routes, so it stays reachable when a modal (z-50) covers the
// top-bar icon. Regression guard for the App wiring that previously suppressed
// the fallback on workspace routes (showShellTrigger={!workspaceRoute}).
describe("App platform bug-report fallback reachability", () => {
  afterEach(() => {
    window.history.pushState({}, "", "/");
    localStorage.clear();
  });

  // A valid (unexpired) JWT keeps RequireAuth from redirecting off the
  // workspace route, so `isWorkspaceShellRoute` stays true and the fallback's
  // route gate is exercised for real. MSW returns MOCK_CURRENT_USER for
  // /users/me when test-auth is not disabled.
  function testJwt(): string {
    const now = Math.floor(Date.now() / 1000);
    const header = btoa(JSON.stringify({ alg: "HS256", typ: "JWT" }));
    const payload = btoa(
      JSON.stringify({
        sub: "user-alex",
        email: "alex@g5labs.dev",
        orgId: "org-acme",
        role: "admin",
        scopes: [],
        iat: now,
        exp: now + 3600,
      }),
    );
    return `${header}.${payload}.test-signature`;
  }

  function withFeatures(features: string[]) {
    localStorage.setItem("g5_auth_token", testJwt());
    // Tracks whether the app actually fetched instance info, so a test can gate
    // on the feature query resolving rather than on a signal (like the URL) that
    // is already true before the request even fires.
    const instanceInfoRequested = { current: false };
    server.use(
      http.get("/api/v1/instance/info", () => {
        instanceInfoRequested.current = true;
        return HttpResponse.json({
          data: {
            version: "0.1.0",
            features,
            authProviders: ["github"],
            maxUploadSizeMb: 100,
            selfHosted: false,
            devLoginEnabled: true,
            motd: null,
          },
        });
      }),
    );
    return { instanceInfoRequested };
  }

  // platform_reports g5.quality.platform-reports.trigger: the chrome carries
  // the trigger on a workspace route; the floating fallback stays only where
  // no chrome trigger mounts.
  it("leaves the floating trigger out once the workspace chrome carries one", async () => {
    withFeatures(["platform_issue_reporting", "chat"]);

    renderAppAt("/org-g5labs/test-workspace");

    expect(
      await screen.findByTestId("platform-bug-report-topbar-button"),
    ).toBeInTheDocument();
    expect(window.location.pathname).toBe("/org-g5labs/test-workspace");
    expect(screen.queryByTestId("platform-bug-report-button")).toBeNull();
  });

  // Issue #701: the auth and workspace loading screens hold the chrome
  // trigger's place, so the fallback never flashes before the top bar mounts.
  it("never shows the floating trigger while a workspace route loads", async () => {
    withFeatures(["platform_issue_reporting", "chat"]);
    let floatingSeen = false;
    // Inspect each added node, so a trigger added and removed within one
    // mutation batch still counts.
    const floatingSelector = '[data-testid="platform-bug-report-button"]';
    const observer = new MutationObserver((records) => {
      for (const record of records) {
        for (const node of record.addedNodes) {
          if (
            node instanceof Element &&
            (node.matches(floatingSelector) ||
              node.querySelector(floatingSelector) !== null)
          ) {
            floatingSeen = true;
          }
        }
      }
    });
    observer.observe(document.body, { childList: true, subtree: true });

    try {
      renderAppAt("/org-g5labs/test-workspace");
      expect(
        await screen.findByTestId("platform-bug-report-topbar-button"),
      ).toBeInTheDocument();
    } finally {
      observer.disconnect();
    }
    expect(floatingSeen).toBe(false);
  });

  it("shows the floating trigger when the workspace fails to load", async () => {
    withFeatures(["platform_issue_reporting", "chat"]);
    server.use(
      http.get("/api/v1/workspaces/:id", () =>
        HttpResponse.json({ error: "boom" }, { status: 500 }),
      ),
    );

    renderAppAt("/org-g5labs/test-workspace");

    expect(
      await screen.findByText("Failed to load workspace", undefined, {
        timeout: 5_000,
      }),
    ).toBeInTheDocument();
    expect(
      screen.getByTestId("platform-bug-report-button"),
    ).toBeInTheDocument();
  });

  it("keeps the floating trigger on a page without a chrome trigger", async () => {
    withFeatures(["platform_issue_reporting", "chat"]);

    renderAppAt("/settings");

    expect(
      await screen.findByTestId("platform-bug-report-button"),
    ).toBeInTheDocument();
    // Settings is organization-scoped; the route lands under the org.
    expect(window.location.pathname).toMatch(/\/settings$/);
    expect(
      screen.queryByTestId("platform-bug-report-topbar-button"),
    ).toBeNull();
  });

  it("still fails closed (no trigger) when the feature is off on a workspace route", async () => {
    const { instanceInfoRequested } = withFeatures(["chat"]);

    renderAppAt("/org-g5labs/test-workspace");

    // Gate on the instance-info request actually resolving (waitFor flushes the
    // resolved query into the provider), so the absence assertion reflects the
    // fail-closed feature gate evaluated against loaded data — not a still-
    // pending request that would hide the trigger for the wrong reason.
    await waitFor(() => expect(instanceInfoRequested.current).toBe(true));
    expect(screen.queryByTestId("platform-bug-report-button")).toBeNull();
    expect(
      screen.queryByTestId("platform-bug-report-topbar-button"),
    ).toBeNull();
  });
});

describe("App login providers", () => {
  afterEach(() => {
    window.history.pushState({}, "", "/");
    localStorage.clear();
  });

  it("shows only OAuth providers reported by instance info", async () => {
    server.use(
      http.get("/api/v1/instance/info", () =>
        HttpResponse.json({
          data: {
            version: "0.1.0",
            features: ["oauth"],
            authProviders: ["github"],
            maxUploadSizeMb: 100,
            selfHosted: false,
            devLoginEnabled: false,
            motd: null,
          },
        }),
      ),
      http.post("/api/v1/auth/refresh", () =>
        HttpResponse.json({}, { status: 401 }),
      ),
    );

    renderAppAt("/login");

    expect(
      await screen.findByRole("link", { name: /continue with github/i }),
    ).toHaveAttribute("href", "/api/v1/auth/login/github");
    expect(
      screen.queryByRole("link", { name: /continue with google/i }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("link", { name: /continue with gitlab/i }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("link", { name: /continue with microsoft/i }),
    ).not.toBeInTheDocument();
  });

  it.each([
    ["login-dev-login", "demo@g5labs.ai"],
    ["login-dev-login-member", "member@g5labs.ai"],
    ["login-dev-login-viewer", "viewer@g5labs.ai"],
  ])("offers dev personas and signs in with %s", async (testId, email) => {
    const requestedEmails: string[] = [];
    const refreshBodies: unknown[] = [];
    server.use(
      http.get("/api/v1/instance/info", () =>
        HttpResponse.json({
          data: {
            version: "0.1.0",
            features: ["oauth"],
            authProviders: ["github"],
            maxUploadSizeMb: 100,
            selfHosted: false,
            devLoginEnabled: true,
            motd: null,
          },
        }),
      ),
      http.post("/api/v1/auth/test-token", async ({ request }) => {
        const body = (await request.json()) as { email: string };
        requestedEmails.push(body.email);
        return HttpResponse.json({
          data: { jwt: "fixture-jwt", refreshToken: "fixture-refresh" },
        });
      }),
      http.post("/api/v1/auth/refresh", async ({ request }) => {
        if (!request.headers.get("content-type"))
          return HttpResponse.json({}, { status: 401 });
        refreshBodies.push(await request.json());
        return HttpResponse.json({ data: { jwt: "dev-jwt" } });
      }),
    );

    renderAppAt("/login");

    expect(await screen.findByTestId("login-dev-login")).toHaveTextContent(
      "Dev Login as Alex (Admin)",
    );
    const memberButton = screen.getByTestId("login-dev-login-member");
    expect(memberButton).toHaveTextContent("Dev Login as Jordan (Member)");
    expect(screen.getByTestId("login-dev-login-viewer")).toHaveTextContent(
      "Dev Login as Vera (Viewer)",
    );

    fireEvent.click(screen.getByTestId(testId));

    await waitFor(() => {
      expect(localStorage.getItem("g5_auth_email")).toBe(email);
    });
    expect(requestedEmails).toEqual([email]);
    expect(refreshBodies).toEqual([{ refreshToken: "fixture-refresh" }]);
    expect(localStorage.getItem("g5_auth_token")).toBe("dev-jwt");
  });

  it("links the footer Terms of Service and Privacy Policy to g5labs.ai", async () => {
    server.use(
      http.get("/api/v1/instance/info", () =>
        HttpResponse.json({
          data: {
            version: "0.1.0",
            features: ["oauth"],
            authProviders: ["github"],
            maxUploadSizeMb: 100,
            selfHosted: false,
            devLoginEnabled: false,
            motd: null,
          },
        }),
      ),
      http.post("/api/v1/auth/refresh", () =>
        HttpResponse.json({}, { status: 401 }),
      ),
    );

    renderAppAt("/login");

    const terms = await screen.findByRole("link", {
      name: /terms of service/i,
    });
    expect(terms).toHaveAttribute("href", "https://g5labs.ai/terms");
    expect(terms).toHaveAttribute("target", "_blank");
    expect(terms).toHaveAttribute("rel", "noopener noreferrer");

    const privacy = screen.getByRole("link", { name: /privacy policy/i });
    expect(privacy).toHaveAttribute("href", "https://g5labs.ai/privacy");
    expect(privacy).toHaveAttribute("target", "_blank");
    expect(privacy).toHaveAttribute("rel", "noopener noreferrer");
  });

  it("shows the pre-production banner for self-hosted installs", async () => {
    server.use(
      http.get("/api/v1/instance/info", () =>
        HttpResponse.json({
          data: {
            version: "0.1.0",
            features: ["oauth"],
            authProviders: ["github"],
            maxUploadSizeMb: 100,
            selfHosted: true,
            devLoginEnabled: false,
            motd: null,
          },
        }),
      ),
      http.post("/api/v1/auth/refresh", () =>
        HttpResponse.json({}, { status: 401 }),
      ),
    );

    renderAppAt("/login");

    expect(
      await screen.findByRole("status", {
        name: /pre production deployment/i,
      }),
    ).toBeInTheDocument();
  });
});

describe("App authenticated login default", () => {
  afterEach(() => {
    window.history.pushState({}, "", "/");
    localStorage.clear();
    sessionStorage.clear();
  });

  it.each([false, true])(
    "offers a shared GitHub connection after login only when disconnected (connected=%s)",
    async (connected) => {
      server.use(
        http.get("/api/v1/instance/info", () =>
          HttpResponse.json({
            data: {
              version: "0.1.0",
              features: ["github_app"],
              authProviders: ["github"],
              maxUploadSizeMb: 100,
              selfHosted: false,
              devLoginEnabled: false,
              motd: null,
            },
          }),
        ),
        http.get("/api/v1/github-app/user-connection", () =>
          HttpResponse.json({
            data: {
              connected,
              githubLogin: connected ? "octocat" : null,
              offerAfterLogin: !connected,
            },
          }),
        ),
      );
      renderAppAt("/login");
      if (connected) {
        await waitFor(() =>
          expect(window.location.pathname).toBe("/g5labs/home"),
        );
      } else {
        expect(
          await screen.findByRole("button", { name: "Connect GitHub" }),
        ).toBeInTheDocument();
        expect(
          screen.getByRole("link", { name: "Continue without connecting" }),
        ).toHaveAttribute("href", "/g5labs/home");
      }
    },
  );

  // g5.settings.github-connection: the server decides the after-login offer for
  // the organization the user is entering (connection, role, whether that
  // organization uses GitHub); the landing asks only then and never waits on a
  // failed answer.
  function instanceWithGithubApp() {
    return http.get("/api/v1/instance/info", () =>
      HttpResponse.json({
        data: {
          version: "0.1.0",
          features: ["github_app"],
          authProviders: ["github"],
          maxUploadSizeMb: 100,
          selfHosted: false,
          devLoginEnabled: false,
          motd: null,
        },
      }),
    );
  }

  function disconnectedGithub(offerAfterLogin: boolean, asked: string[]) {
    return http.get("/api/v1/github-app/user-connection", ({ request }) => {
      const orgId = new URL(request.url).searchParams.get("afterLoginOrgId");
      if (orgId) asked.push(orgId);
      return HttpResponse.json({
        data: {
          connected: false,
          githubLogin: null,
          ...(orgId ? { offerAfterLogin } : {}),
        },
      });
    });
  }

  it("asks about the entered organization and offers the connection when the server does", async () => {
    const asked: string[] = [];
    server.use(instanceWithGithubApp(), disconnectedGithub(true, asked));
    renderAppAt("/login");
    expect(
      await screen.findByRole("button", { name: "Connect GitHub" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("link", { name: "Continue without connecting" }),
    ).toHaveAttribute("href", "/g5labs/home");
    expect(asked).toContain("org-g5labs");
  });

  it("continues to the destination when the server does not offer the connection", async () => {
    const asked: string[] = [];
    server.use(instanceWithGithubApp(), disconnectedGithub(false, asked));
    renderAppAt("/login");
    await waitFor(() => expect(window.location.pathname).toBe("/g5labs/home"));
    expect(asked).toEqual(["org-g5labs"]);
    expect(
      screen.queryByRole("button", { name: "Connect GitHub" }),
    ).not.toBeInTheDocument();
  });

  it("continues to the destination when the offer cannot be read", async () => {
    server.use(
      instanceWithGithubApp(),
      http.get("/api/v1/github-app/user-connection", () =>
        HttpResponse.json(
          { error: { code: "INTERNAL", message: "unavailable" } },
          { status: 500 },
        ),
      ),
    );
    renderAppAt("/login");
    await waitFor(() => expect(window.location.pathname).toBe("/g5labs/home"));
    expect(
      screen.queryByRole("button", { name: "Connect GitHub" }),
    ).not.toBeInTheDocument();
  });

  it("does not ask or offer the connection when the user enters no organization", async () => {
    const asked: string[] = [];
    server.use(
      instanceWithGithubApp(),
      disconnectedGithub(true, asked),
      http.get("/api/v1/users/me", () =>
        HttpResponse.json({
          data: { ...MOCK_CURRENT_USER, activeOrgId: null, orgs: [] },
        }),
      ),
    );
    renderAppAt("/login");
    await waitFor(() =>
      expect(window.location.pathname).toBe("/no-organization"),
    );
    expect(asked).toEqual([]);
    expect(
      screen.queryByRole("button", { name: "Connect GitHub" }),
    ).not.toBeInTheDocument();
  });

  it.each([false, true])(
    "resumes a requested connection page and its nested destination after login (connected=%s)",
    async (connected) => {
      server.use(
        http.get("/api/v1/instance/info", () =>
          HttpResponse.json({
            data: {
              version: "0.1.0",
              features: ["github_app"],
              authProviders: ["github"],
              maxUploadSizeMb: 100,
              selfHosted: false,
              devLoginEnabled: false,
              motd: null,
            },
          }),
        ),
        http.get("/api/v1/github-app/user-connection", () =>
          HttpResponse.json({
            data: {
              connected,
              githubLogin: connected ? "octocat" : null,
            },
          }),
        ),
      );
      const destination = "/acme/work?view=activity";
      const connectionPath = `/account/github?returnTo=${encodeURIComponent(destination)}`;
      rememberPostAuthRedirect(connectionPath);
      renderAppAt("/login");
      expect(
        await screen.findByRole("heading", { name: "Account settings" }),
      ).toBeInTheDocument();
      expect(
        await screen.findByRole("button", {
          name: connected ? "Reconnect GitHub" : "Connect GitHub",
        }),
      ).toBeInTheDocument();
      expect(window.location.pathname + window.location.search).toBe(
        connectionPath,
      );
      expect(
        screen.getByRole("link", { name: "Continue to G5" }),
      ).toHaveAttribute("href", destination);
    },
  );

  it("lands on the active organization's Home (g5.ui.sign-in-landing)", async () => {
    sessionStorage.clear();
    server.use(
      http.get("/api/v1/instance/info", () =>
        HttpResponse.json({
          data: {
            version: "0.1.0",
            features: [],
            authProviders: ["github"],
            maxUploadSizeMb: 100,
            selfHosted: false,
            devLoginEnabled: false,
            motd: null,
          },
        }),
      ),
    );

    renderAppAt("/login");

    await waitFor(() => {
      expect(window.location.pathname).toBe("/g5labs/home");
    });
  });
});

// Issue #1517: the /login/denied page names the rejected address (from the
// short-lived server-set g5_login_denied cookie) and offers a different-account
// action, instead of the old context-free message.
describe("App login denied page", () => {
  afterEach(() => {
    window.history.pushState({}, "", "/");
    localStorage.clear();
    // Best-effort cookie teardown for both paths the test may have used.
    document.cookie = "g5_login_denied=; Path=/login; Max-Age=0";
    document.cookie = "g5_login_denied=; Max-Age=0";
  });

  it("names the rejected email from the g5_login_denied cookie and clears it", async () => {
    localStorage.setItem("g5_test_disable_auth", "1");
    document.cookie = `g5_login_denied=${encodeURIComponent(
      "alice@corp.com",
    )}; Path=/login`;

    renderAppAt("/login/denied");

    const named = await screen.findByTestId("login-denied-email");
    expect(named).toHaveTextContent("alice@corp.com");
    expect(
      screen.getByTestId("login-denied-different-account"),
    ).toBeInTheDocument();

    // The context cookie is single-use: it is cleared after being read.
    await waitFor(() => {
      expect(document.cookie).not.toContain("g5_login_denied=alice");
    });
  });

  it("renders escaped text, not markup, for the rejected address", async () => {
    localStorage.setItem("g5_test_disable_auth", "1");
    // A hostile value must never be interpreted as HTML.
    document.cookie = `g5_login_denied=${encodeURIComponent(
      "<img src=x onerror=alert(1)>@evil.com",
    )}; Path=/login`;

    renderAppAt("/login/denied");

    const named = await screen.findByTestId("login-denied-email");
    // Rendered as literal text (React escaping); no <img> element is created.
    expect(named).toHaveTextContent("<img src=x onerror=alert(1)>@evil.com");
    expect(named.querySelector("img")).toBeNull();
  });

  it("falls back to the generic message when no denied cookie is present", async () => {
    localStorage.setItem("g5_test_disable_auth", "1");

    renderAppAt("/login/denied");

    expect(
      await screen.findByText(
        /has no G5 invitation or organization membership/i,
      ),
    ).toBeInTheDocument();
    expect(screen.queryByTestId("login-denied-email")).not.toBeInTheDocument();
  });

  it("navigates to /login when 'sign in with a different account' is activated", async () => {
    localStorage.setItem("g5_test_disable_auth", "1");
    document.cookie = `g5_login_denied=${encodeURIComponent(
      "alice@corp.com",
    )}; Path=/login`;

    renderAppAt("/login/denied");

    // Wait until the page (and its different-account action) has rendered
    // before stubbing navigation, so React Router still reads the real
    // location during the initial render.
    const differentAccount = await screen.findByTestId(
      "login-denied-different-account",
    );

    // jsdom does not implement navigation, so capture the assignment the
    // handler makes to `window.location.href`. A Proxy forwards every other
    // access (pathname, etc.) to the real Location so routing stays intact.
    const realLocation = window.location;
    let navigatedTo: string | null = null;
    const locationProxy = new Proxy(realLocation, {
      get(target, prop, receiver) {
        const value = Reflect.get(target, prop, receiver);
        return typeof value === "function" ? value.bind(target) : value;
      },
      set(target, prop, value) {
        if (prop === "href") {
          navigatedTo = value as string;
          return true;
        }
        return Reflect.set(target, prop, value);
      },
    });
    // @ts-expect-error jsdom allows deleting window.location for test stubbing.
    delete window.location;
    // @ts-expect-error reinstall the proxy in place of the real Location.
    window.location = locationProxy;

    try {
      fireEvent.click(differentAccount);
      expect(navigatedTo).toBe("/login");
    } finally {
      // @ts-expect-error restore the real Location for later tests.
      delete window.location;
      // @ts-expect-error restore the real Location for later tests.
      window.location = realLocation;
    }
  });
});

// Exercise the production route table, including its RequireAuth and Home destination.
describe("organization-root Home default", () => {
  beforeEach(() => {
    vi.mocked(window.matchMedia).mockImplementation((query: string) => ({
      matches: query.includes("min-width: 1024px"),
      media: query,
      onchange: null,
      addListener: vi.fn(),
      removeListener: vi.fn(),
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      dispatchEvent: vi.fn(),
    }));
    server.use(
      http.get("/api/v1/orgs/:orgId/home/attention", () =>
        HttpResponse.json({ data: { items: [], nextCursor: null } }),
      ),
      http.get("/api/v1/orgs/:orgId/workspaces/open-question-metrics", () =>
        HttpResponse.json({ data: [] }),
      ),
    );
  });
  afterEach(() => {
    window.history.pushState({}, "", "/");
    localStorage.clear();
    sessionStorage.clear();
  });

  it.each(["/g5labs", "/g5labs/", "/g5labs?keep=1#section"])(
    "opens same-origin Home from %s even without the UI v3 flag",
    async (route) => {
      const historyLength = window.history.length;
      const origin = window.location.origin;
      renderAppAt(route);
      await waitFor(() =>
        expect(window.location.pathname).toBe("/g5labs/home"),
      );
      expect(window.location.origin).toBe(origin);
      expect(window.history.length).toBe(historyLength + 1);
      if (route.includes("?")) {
        expect(window.location.search).toBe("?keep=1");
        expect(window.location.hash).toBe("#section");
      }
      expect(await screen.findByTestId("home-columns")).toBeInTheDocument();
    },
  );

  it("preserves the requested root through login before resolving Home", async () => {
    localStorage.setItem("g5_test_disable_auth", "1");
    renderAppAt("/g5labs?keep=1#section");
    await waitFor(() => expect(window.location.pathname).toBe("/login"));
    expect(getPostAuthRedirect()).toBe("/g5labs?keep=1#section");
  });

  it("keeps explicit child workspace destinations unchanged", async () => {
    renderAppAt("/g5labs/task-graph-rework");
    expect(await screen.findByTestId("center-panel")).toBeInTheDocument();
    expect(window.location.pathname).toBe("/g5labs/task-graph-rework");
  });

  it("retains the Home organization-access refusal", async () => {
    renderAppAt("/not-a-membership");
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "You do not have access to this organization.",
    );
    expect(window.location.pathname).toBe("/not-a-membership/home");
  });
});
