/**
 * GithubAppBindingStatusPageContent — organization GitHub App binding tab.
 *
 * Covered: recorded snapshot rendering, empty state and server-built
 * install/connect redirects, the live repository picker saving an exact
 * coverage set, disconnect, the callback outcome notice, and non-admin
 * gating.
 */

import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HttpResponse, http } from "msw";
import { Route, Routes } from "react-router-dom";
import { beforeEach, describe, expect, it } from "vitest";

import type {
  GithubAppBindingDTO,
  GithubAppBindingStatusResponseDTO,
  IssueDomainDTO,
} from "../../lib/types";
import {
  makeIssueDomain,
  serveOrgExternalIssues,
} from "../../test/externalIssueFixtures";
import {
  createMockUser,
  MOCK_ADMIN_SCOPES,
  MOCK_INSTANCE_INFO,
} from "../../test/fixtures";
import { server } from "../../test/msw/server";
import { renderApp } from "../../test/render";
import { GithubAppBindingStatusPageContent } from "./GithubAppBindingStatusPageContent";

const ORG_ID = "org-g5labs";
const ORG_SLUG = "g5labs";
const NOTICE = "Recorded binding — not live verified";

function makeBinding(
  overrides: Partial<GithubAppBindingDTO> = {},
): GithubAppBindingDTO {
  return {
    version: 4,
    disposition: "active",
    provenance: "coverage_change",
    installationId: 154535743,
    accountLogin: "acme",
    permissions: { issues: "write", metadata: "read" },
    recordedAt: "2026-09-20T14:25:00.000Z",
    coveredRepositories: [{ githubRepoId: 11, fullName: "acme/issues" }],
    ...overrides,
  };
}

function serveBinding(binding: GithubAppBindingDTO | null) {
  const response: GithubAppBindingStatusResponseDTO = {
    notice: NOTICE,
    binding,
  };
  server.use(
    http.get("/api/v1/orgs/:orgId/github-app/binding", () =>
      HttpResponse.json({ data: response }),
    ),
  );
}

function serveFeatures(features: string[]) {
  server.use(
    http.get("/api/v1/instance/info", () =>
      HttpResponse.json({ data: { ...MOCK_INSTANCE_INFO, features } }),
    ),
  );
}

function renderPage(role: "admin" | "member" = "admin", search = "") {
  return renderApp(
    <Routes>
      <Route
        path="/:org/settings/github-app"
        element={<GithubAppBindingStatusPageContent />}
      />
    </Routes>,
    {
      route: `/${ORG_SLUG}/settings/github-app${search}`,
      user: createMockUser({
        orgs: [{ orgId: ORG_ID, orgName: "G5 Labs", orgSlug: ORG_SLUG, role }],
        scopes: role === "member" ? ["workspaces:read"] : MOCK_ADMIN_SCOPES,
      }),
    },
  );
}

/** Capture top-level `location.href` assignments. */
async function captureNavigation(run: (redirects: string[]) => Promise<void>) {
  const realLocation = window.location;
  const redirects: string[] = [];
  const proxy = new Proxy(realLocation, {
    get(target, property, receiver) {
      const value = Reflect.get(target, property, receiver);
      return typeof value === "function" ? value.bind(target) : value;
    },
    set(target, property, value) {
      if (property === "href") {
        redirects.push(value as string);
        return true;
      }
      return Reflect.set(target, property, value);
    },
  });
  // @ts-expect-error jsdom allows replacing Location for navigation capture.
  delete window.location;
  // @ts-expect-error install a proxy that captures only href assignment.
  window.location = proxy;
  try {
    await run(redirects);
  } finally {
    // @ts-expect-error restore the real Location for later tests.
    delete window.location;
    // @ts-expect-error restore the real Location for later tests.
    window.location = realLocation;
  }
}

describe("GithubAppBindingStatusPageContent", () => {
  beforeEach(() => {
    serveFeatures([...MOCK_INSTANCE_INFO.features, "github_app"]);
    serveBinding(null);
  });

  it("explains when the deployment has no GitHub App configured", async () => {
    let requested = false;
    serveFeatures(
      MOCK_INSTANCE_INFO.features.filter((f) => f !== "github_app"),
    );
    server.use(
      http.get("/api/v1/orgs/:orgId/github-app/binding", () => {
        requested = true;
        return HttpResponse.json({ data: { notice: NOTICE, binding: null } });
      }),
    );
    renderPage();

    expect(
      await screen.findByTestId("github-app-binding-unavailable"),
    ).toBeInTheDocument();
    expect(requested).toBe(false);
  });

  it("shows the recorded binding, its coverage, Issues permission and the non-live notice", async () => {
    serveBinding(makeBinding());
    renderPage();

    expect(
      await screen.findByTestId("github-app-binding-card"),
    ).toBeInTheDocument();
    expect(screen.getByTestId("github-app-binding-notice")).toHaveTextContent(
      NOTICE,
    );
    expect(
      screen.getByTestId("github-app-binding-disposition"),
    ).toHaveTextContent("Connected");
    expect(
      screen.getByTestId("github-app-binding-installation"),
    ).toHaveTextContent("154535743");
    expect(
      screen.getByTestId("github-app-binding-issues-permission"),
    ).toHaveTextContent("Read and write");
    expect(screen.getByText("acme/issues")).toBeInTheDocument();
  });

  it("starts install and connect flows at server-built URLs without showing their state", async () => {
    const requested: string[] = [];
    server.use(
      http.post("/api/v1/orgs/:orgId/github-app/connection-attempt", () => {
        requested.push("install");
        return HttpResponse.json({
          data: {
            redirectUrl:
              "https://github.com/apps/g5/installations/new?state=install-opaque",
          },
        });
      }),
      http.post("/api/v1/orgs/:orgId/github-app/reconnect-attempt", () => {
        requested.push("reconnect");
        return HttpResponse.json({
          data: {
            redirectUrl:
              "https://github.com/login/oauth/authorize?state=reconnect-opaque",
          },
        });
      }),
    );
    renderPage();
    const user = userEvent.setup();

    expect(
      await screen.findByTestId("github-app-binding-empty"),
    ).toHaveTextContent("GitHub App not connected");
    await captureNavigation(async (redirects) => {
      await user.click(
        screen.getByRole("button", { name: "Install GitHub App" }),
      );
      await waitFor(() => expect(redirects).toHaveLength(1));
      await user.click(
        screen.getByRole("button", { name: "Connect existing installation" }),
      );
      await waitFor(() => expect(redirects).toHaveLength(2));
      expect(requested).toEqual(["install", "reconnect"]);
      expect(redirects[0]).toContain("state=install-opaque");
      expect(redirects[1]).toContain("state=reconnect-opaque");
    });
    expect(screen.queryByText(/opaque/)).not.toBeInTheDocument();
  });

  it("lists the installation's repositories and saves the exact selection", async () => {
    serveBinding(makeBinding());
    let saved: unknown = null;
    server.use(
      http.get("/api/v1/orgs/:orgId/github-app/binding/repositories", () =>
        HttpResponse.json({
          data: {
            bindingVersion: 4,
            repositories: [
              {
                githubRepoId: 12,
                fullName: "acme/api",
                private: true,
                covered: false,
              },
              {
                githubRepoId: 11,
                fullName: "acme/issues",
                private: true,
                covered: true,
              },
              {
                githubRepoId: 13,
                fullName: "acme/site",
                private: false,
                covered: false,
              },
            ],
          },
        }),
      ),
      http.put(
        "/api/v1/orgs/:orgId/github-app/binding/coverage",
        async ({ request }) => {
          saved = await request.json();
          return HttpResponse.json({
            data: {
              notice: NOTICE,
              binding: makeBinding({
                version: 5,
                coveredRepositories: [
                  { githubRepoId: 12, fullName: "acme/api" },
                ],
              }),
            },
          });
        },
      ),
    );
    renderPage();
    const user = userEvent.setup();

    await user.click(
      await screen.findByRole("button", { name: /Choose repositories/ }),
    );
    const manager = await screen.findByTestId("github-app-coverage-manager");
    const issues = within(manager).getByRole("checkbox", {
      name: "acme/issues",
    });
    const api = within(manager).getByRole("checkbox", { name: "acme/api" });
    expect(issues).toBeChecked();
    expect(api).not.toBeChecked();
    expect(within(manager).getByText("Public")).toBeInTheDocument();
    const save = screen.getByTestId("github-app-save-coverage");
    expect(save).toBeDisabled();

    await user.click(issues);
    await user.click(api);
    expect(save).toBeEnabled();
    await user.click(save);

    await waitFor(() =>
      expect(saved).toEqual({ bindingVersion: 4, githubRepoIds: [12] }),
    );
  });

  it("disconnects the binding at its current version after confirmation", async () => {
    serveBinding(makeBinding());
    let disconnected: unknown = null;
    server.use(
      http.post(
        "/api/v1/orgs/:orgId/github-app/binding/disconnect",
        async ({ request }) => {
          disconnected = await request.json();
          return HttpResponse.json({
            data: {
              notice: NOTICE,
              binding: makeBinding({
                version: 5,
                disposition: "disconnected",
                provenance: "disconnect",
                coveredRepositories: [],
              }),
            },
          });
        },
      ),
    );
    renderPage();
    const user = userEvent.setup();

    await user.click(await screen.findByTestId("github-app-disconnect"));
    expect(disconnected).toBeNull();
    await user.click(screen.getByTestId("github-app-confirm-disconnect"));
    await waitFor(() => expect(disconnected).toEqual({ bindingVersion: 4 }));
  });

  it("refreshes the active binding from GitHub at its current version", async () => {
    serveBinding(makeBinding());
    let refreshed: unknown = null;
    server.use(
      http.post(
        "/api/v1/orgs/:orgId/github-app/binding/refresh",
        async ({ request }) => {
          refreshed = await request.json();
          return HttpResponse.json({
            data: {
              notice: NOTICE,
              binding: makeBinding({
                version: 5,
                provenance: "installation_refresh",
              }),
            },
          });
        },
      ),
    );
    renderPage();
    const user = userEvent.setup();

    await user.click(await screen.findByTestId("github-app-refresh"));
    await waitFor(() => expect(refreshed).toEqual({ bindingVersion: 4 }));
  });

  it("offers reconnection for a disconnected binding", async () => {
    serveBinding(
      makeBinding({ disposition: "disconnected", coveredRepositories: [] }),
    );
    renderPage();

    expect(
      await screen.findByTestId("github-app-binding-disposition"),
    ).toHaveTextContent("Disconnected");
    expect(
      screen.getByRole("button", { name: "Connect existing installation" }),
    ).toBeInTheDocument();
    expect(
      screen.queryByTestId("github-app-disconnect"),
    ).not.toBeInTheDocument();
    expect(screen.queryByTestId("github-app-refresh")).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /Choose repositories/ }),
    ).not.toBeInTheDocument();
  });

  it("reports a failed GitHub callback", async () => {
    renderPage("admin", "?githubAppConnection=failed");
    expect(
      await screen.findByTestId("github-app-connection-outcome"),
    ).toHaveTextContent("could not be verified");
  });

  it("shows members an access notice without requesting the binding", async () => {
    let requested = false;
    server.use(
      http.get("/api/v1/orgs/:orgId/github-app/binding", () => {
        requested = true;
        return HttpResponse.json({ data: { notice: NOTICE, binding: null } });
      }),
    );
    renderPage("member");

    expect(
      await screen.findByTestId("github-app-binding-access-denied"),
    ).toBeInTheDocument();
    expect(requested).toBe(false);
  });
});

describe("GithubAppBindingStatusPageContent — Issue domain section", () => {
  const DOMAINS_URL = `/api/v1/orgs/${ORG_ID}/issue-domains`;

  /** Serve a mutable domain list; returns a setter for later responses. */
  function serveDomains(initial: IssueDomainDTO[]) {
    let domains = initial;
    server.use(
      http.get(DOMAINS_URL, () => HttpResponse.json({ data: { domains } })),
    );
    return (next: IssueDomainDTO[]) => {
      domains = next;
    };
  }

  beforeEach(() => {
    serveFeatures([
      ...MOCK_INSTANCE_INFO.features,
      "github_app",
      "external_issues",
    ]);
    serveOrgExternalIssues(true);
    serveBinding(
      makeBinding({
        coveredRepositories: [
          { githubRepoId: 11, fullName: "acme/issues" },
          { githubRepoId: 12, fullName: "acme/api" },
        ],
      }),
    );
  });

  /** Serve the organization switch from mutable state a PUT updates. */
  function serveMutableSwitch(initial: boolean) {
    let enabled = initial;
    const updates: unknown[] = [];
    const body = () => ({
      data: {
        scope: "org",
        settings: { external_issues_enabled: enabled },
        secretKeys: [],
        effectiveSettings: { external_issues_enabled: enabled },
        effectiveSecretKeys: [],
      },
    });
    server.use(
      http.get("/api/v1/orgs/:orgId/config", () => HttpResponse.json(body())),
      http.put("/api/v1/orgs/:orgId/config", async ({ request }) => {
        const update = (await request.json()) as {
          settings?: Record<string, unknown>;
        };
        updates.push(update);
        enabled = update.settings?.external_issues_enabled === true;
        return HttpResponse.json(body());
      }),
    );
    return updates;
  }

  it("turning the switch on shows the issue domain settings", async () => {
    const updates = serveMutableSwitch(false);
    serveDomains([]);
    renderPage();

    const toggle = await screen.findByTestId("external-issues-switch");
    await waitFor(() => expect(toggle).toBeEnabled());
    expect(toggle).not.toBeChecked();
    // Off with nothing to remove: no domain section.
    expect(
      screen.queryByTestId("issue-domain-section"),
    ).not.toBeInTheDocument();

    fireEvent.click(toggle);
    await waitFor(() => expect(updates).toHaveLength(1));
    expect(updates[0]).toEqual({ settings: { external_issues_enabled: true } });
    await waitFor(() =>
      expect(screen.getByTestId("external-issues-switch")).toBeChecked(),
    );
    expect(
      await screen.findByTestId("issue-domain-add-form"),
    ).toBeInTheDocument();
    expect(
      screen.queryByTestId("issue-domain-remove-only"),
    ).not.toBeInTheDocument();
  });

  it("keeps existing domains removable while the organization has it off", async () => {
    serveOrgExternalIssues(false);
    serveDomains([makeIssueDomain({ id: "d1", displayKey: "acme/issues" })]);
    const deleted: string[] = [];
    server.use(
      http.delete(`${DOMAINS_URL}/:domainId`, ({ params }) => {
        deleted.push(String(params.domainId));
        return new HttpResponse(null, { status: 204 });
      }),
    );
    renderPage();

    const row = await screen.findByTestId("issue-domain-d1");
    expect(screen.getByTestId("issue-domain-remove-only")).toBeInTheDocument();
    expect(
      within(row).queryByTestId("issue-domain-trusted-toggle"),
    ).not.toBeInTheDocument();
    expect(
      within(row).queryByTestId("issue-domain-comments-toggle"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("issue-domain-add-form"),
    ).not.toBeInTheDocument();

    fireEvent.click(within(row).getByTestId("issue-domain-remove"));
    fireEvent.click(screen.getByTestId("issue-domain-remove-confirm-button"));
    await waitFor(() => expect(deleted).toEqual(["d1"]));
  });

  it.each([
    ["without a binding", () => serveBinding(null)],
    [
      "when the binding cannot be loaded",
      () =>
        server.use(
          http.get("/api/v1/orgs/:orgId/github-app/binding", () =>
            HttpResponse.json(
              { error: { code: "INTERNAL", message: "boom" } },
              { status: 500 },
            ),
          ),
        ),
    ],
  ])("shows the organization switch %s", async (_label, serve) => {
    serve();
    renderPage();

    expect(
      await screen.findByTestId("external-issues-switch"),
    ).toBeInTheDocument();
  });

  it("is absent while the external issues feature is off", async () => {
    let requested = false;
    serveFeatures([...MOCK_INSTANCE_INFO.features, "github_app"]);
    server.use(
      http.get(DOMAINS_URL, () => {
        requested = true;
        return HttpResponse.json({ data: { domains: [] } });
      }),
    );
    renderPage();

    expect(
      await screen.findByTestId("github-app-binding-card"),
    ).toBeInTheDocument();
    expect(
      screen.queryByTestId("issue-domain-section"),
    ).not.toBeInTheDocument();
    expect(requested).toBe(false);
  });

  it("is not shown to organization members", async () => {
    let requested = false;
    server.use(
      http.get(DOMAINS_URL, () => {
        requested = true;
        return HttpResponse.json({ data: { domains: [] } });
      }),
    );
    renderPage("member");

    expect(
      await screen.findByTestId("github-app-binding-access-denied"),
    ).toBeInTheDocument();
    expect(
      screen.queryByTestId("issue-domain-section"),
    ).not.toBeInTheDocument();
    expect(requested).toBe(false);
  });

  it("lists domains with availability and the reason a domain is unavailable", async () => {
    serveDomains([
      makeIssueDomain({ id: "d1", displayKey: "acme/issues" }),
      makeIssueDomain({
        id: "d2",
        displayKey: "acme/old",
        externalContainerId: "99",
        isDefaultCreateTarget: false,
        contentTrusted: true,
        available: false,
        unavailableReason: "repository_uncovered",
      }),
    ]);
    renderPage();

    const available = await screen.findByTestId("issue-domain-d1");
    expect(
      within(available).getByTestId("issue-domain-availability"),
    ).toHaveTextContent("Available");
    expect(available).toHaveTextContent("Default");
    expect(
      within(available).getByRole("switch", { name: "Trusted content" }),
    ).not.toBeChecked();

    const unavailable = screen.getByTestId("issue-domain-d2");
    expect(
      within(unavailable).getByTestId("issue-domain-availability"),
    ).toHaveTextContent("Unavailable");
    expect(
      within(unavailable).getByTestId("issue-domain-unavailable-reason"),
    ).toHaveTextContent(
      "The GitHub App binding no longer covers this repository.",
    );
    expect(
      within(unavailable).getByRole("switch", { name: "Trusted content" }),
    ).toBeChecked();
  });

  it("adds a covered repository as an untrusted domain by default", async () => {
    const setDomains = serveDomains([
      makeIssueDomain({ id: "d1", externalContainerId: "11" }),
    ]);
    let posted: unknown = null;
    server.use(
      http.post(DOMAINS_URL, async ({ request }) => {
        posted = await request.json();
        const created = makeIssueDomain({
          id: "d2",
          externalContainerId: "12",
          displayKey: "acme/api",
          isDefaultCreateTarget: false,
        });
        setDomains([
          makeIssueDomain({ id: "d1", externalContainerId: "11" }),
          created,
        ]);
        return HttpResponse.json({ data: created }, { status: 201 });
      }),
    );
    renderPage();
    const user = userEvent.setup();

    const select = await screen.findByRole("combobox", {
      name: "Covered repository",
    });
    // acme/issues (id 11) is already a domain, so only acme/api is offered.
    expect(
      within(select)
        .getAllByRole("option")
        .map((option) => option.textContent),
    ).toEqual(["Choose a repository…", "acme/api"]);
    expect(
      screen.getByRole("checkbox", { name: "Trust issue content" }),
    ).not.toBeChecked();
    expect(screen.getByTestId("issue-domain-add")).toBeDisabled();

    await user.selectOptions(select, "12");
    await user.click(screen.getByTestId("issue-domain-add"));

    await waitFor(() =>
      expect(posted).toEqual({ githubRepoId: "12", contentTrusted: false }),
    );
    expect(await screen.findByTestId("issue-domain-d2")).toBeInTheDocument();
    expect(
      await screen.findByTestId("issue-domain-no-candidates"),
    ).toHaveTextContent("Every covered repository is already an issue domain.");
  });

  it("explains a duplicate or ineligible domain", async () => {
    serveDomains([]);
    server.use(
      http.post(DOMAINS_URL, () =>
        HttpResponse.json(
          {
            error: {
              code: "CONFLICT",
              message: "Issue domain exists",
              details: {},
              requestId: "",
            },
          },
          { status: 409 },
        ),
      ),
    );
    renderPage();
    const user = userEvent.setup();

    await user.selectOptions(
      await screen.findByRole("combobox", { name: "Covered repository" }),
      "11",
    );
    await user.click(
      screen.getByRole("checkbox", { name: "Trust issue content" }),
    );
    await user.click(screen.getByTestId("issue-domain-add"));

    expect(await screen.findByTestId("issue-domain-error")).toHaveTextContent(
      "This repository is already an issue domain.",
    );
  });

  it("toggles trusted content with PATCH", async () => {
    const setDomains = serveDomains([makeIssueDomain({ id: "d1" })]);
    const patched: unknown[] = [];
    server.use(
      http.patch(`${DOMAINS_URL}/:domainId`, async ({ request, params }) => {
        patched.push({ id: params.domainId, body: await request.json() });
        const updated = makeIssueDomain({ id: "d1", contentTrusted: true });
        setDomains([updated]);
        return HttpResponse.json({ data: updated });
      }),
    );
    renderPage();
    const user = userEvent.setup();

    const toggle = within(
      await screen.findByTestId("issue-domain-d1"),
    ).getByRole("switch", { name: "Trusted content" });
    await user.click(toggle);

    await waitFor(() =>
      expect(patched).toEqual([{ id: "d1", body: { contentTrusted: true } }]),
    );
    await waitFor(() =>
      expect(
        within(screen.getByTestId("issue-domain-d1")).getByRole("switch", {
          name: "Trusted content",
        }),
      ).toBeChecked(),
    );
  });

  it("enables G5 comments on a domain with PATCH", async () => {
    const setDomains = serveDomains([makeIssueDomain({ id: "d1" })]);
    const patched: unknown[] = [];
    server.use(
      http.patch(`${DOMAINS_URL}/:domainId`, async ({ request, params }) => {
        patched.push({ id: params.domainId, body: await request.json() });
        const updated = makeIssueDomain({ id: "d1", commentsEnabled: true });
        setDomains([updated]);
        return HttpResponse.json({ data: updated });
      }),
    );
    renderPage();
    const user = userEvent.setup();

    const toggle = within(
      await screen.findByTestId("issue-domain-d1"),
    ).getByRole("switch", { name: "Allow G5 comments and issue creation" });
    expect(toggle).not.toBeChecked();
    await user.click(toggle);

    await waitFor(() =>
      expect(patched).toEqual([{ id: "d1", body: { commentsEnabled: true } }]),
    );
    await waitFor(() =>
      expect(
        within(screen.getByTestId("issue-domain-d1")).getByRole("switch", {
          name: "Allow G5 comments and issue creation",
        }),
      ).toBeChecked(),
    );
  });

  it("removes a domain with DELETE only after confirmation", async () => {
    const setDomains = serveDomains([makeIssueDomain({ id: "d1" })]);
    const deleted: string[] = [];
    server.use(
      http.delete(`${DOMAINS_URL}/:domainId`, ({ params }) => {
        deleted.push(String(params.domainId));
        setDomains([]);
        return new HttpResponse(null, { status: 204 });
      }),
    );
    renderPage();
    const user = userEvent.setup();

    const row = await screen.findByTestId("issue-domain-d1");
    await user.click(
      within(row).getByRole("button", {
        name: "Remove issue domain acme/api",
      }),
    );
    expect(
      within(row).getByTestId("issue-domain-remove-confirm"),
    ).toHaveTextContent("Existing workspace links are kept.");
    expect(deleted).toEqual([]);
    await user.click(
      within(row).getByTestId("issue-domain-remove-confirm-button"),
    );

    await waitFor(() => expect(deleted).toEqual(["d1"]));
    expect(await screen.findByTestId("issue-domain-empty")).toBeInTheDocument();
  });
});
