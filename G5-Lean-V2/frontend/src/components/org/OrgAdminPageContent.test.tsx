// =============================================================================
// OrgAdminPageContent — org admin hub tests
// =============================================================================
// The hub collects every org configuration surface behind one center-panel
// page (`?view=admin`) and hosts the org settings administrators edit. Covers:
// the whole page is for admins only, the config links resolve to
// the canonical `/:org/settings/*` routes, the GitHub App link is
// feature-gated, and the active-org switch is offered only for a non-active
// org. The settings card has its own tests (OrgAdminSettingsCard.test.tsx).
// The Issues section works without the GitHub App: the switch, "Use G5
// issues" with a key from the slug, and a native domain's export and
// removal refusal (g5.issue.native.domain).
// =============================================================================

import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { Route, Routes } from "react-router-dom";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { IssueDomainDTO } from "../../lib/types";
import {
  issueConflictResponse,
  makeNativeIssueDomain,
} from "../../test/externalIssueFixtures";
import { createMockUser, MOCK_INSTANCE_INFO } from "../../test/fixtures";
import { server } from "../../test/msw/server";
import { renderApp } from "../../test/render";
import { resetMockScopedConfig } from "../../test/msw/handlers";
import { OrgAdminPageContent } from "./OrgAdminPageContent";

function renderAdmin(options?: {
  route?: string;
  user?: ReturnType<typeof createMockUser>;
}) {
  return renderApp(
    <Routes>
      <Route path="/:org/:workspace" element={<OrgAdminPageContent />} />
    </Routes>,
    {
      route: options?.route ?? "/g5labs/base?view=admin",
      user: options?.user ?? createMockUser(),
    },
  );
}

function memberUser(role: "member" | "viewer") {
  return createMockUser({
    scopes: [],
    orgs: [
      {
        orgId: "org-g5labs",
        orgName: "G5 Labs",
        orgSlug: "g5labs",
        role,
      },
    ],
  });
}

function serveInstanceFeatures(features: string[]) {
  server.use(
    http.get("/api/v1/instance/info", () =>
      HttpResponse.json({
        data: { ...MOCK_INSTANCE_INFO, features },
      }),
    ),
  );
}

describe("OrgAdminPageContent", () => {
  it("lists the sheet's sections in the index and links the sub-pages", async () => {
    serveInstanceFeatures(["chat", "github_app", "provider_accounts"]);
    server.use(
      http.get("/api/v1/orgs/:orgId/provider-accounts", () =>
        HttpResponse.json({ data: [] }),
      ),
    );
    renderAdmin();

    await screen.findByText("G5 Labs · Organization settings · Admin");
    const index = screen.getByTestId("settings-index");
    await waitFor(() =>
      expect(
        within(index)
          .getAllByRole("button")
          .map((button) => button.textContent),
      ).toEqual([
        "Organization",
        "Workspaces and hosts",
        "Workbench actions",
        "Defaults",
        "Accounts",
        "API keys",
      ]),
    );
    expect(
      within(index)
        .getAllByRole("link")
        .map((link) => link.textContent),
    ).toEqual([
      "Members",
      "Approval definitions",
      "Workers",
      "GitHub App",
      "Settings",
    ]);
  });

  it("lists no Accounts section while provider accounts are off", async () => {
    serveInstanceFeatures(["chat"]);
    renderAdmin();

    await screen.findByText("G5 Labs · Organization settings · Admin");
    await screen.findByTestId("settings-index-api-keys");
    expect(screen.queryByTestId("settings-index-accounts")).toBeNull();
    expect(
      screen.queryByRole("heading", { level: 2, name: "Accounts" }),
    ).toBeNull();
  });

  it("hosts org runtime settings, secrets and API keys behind the admin gate", async () => {
    const writes: Record<string, unknown>[] = [];
    resetMockScopedConfig({
      org: { settings: { ORG_REGION: "eu" }, secretKeys: ["ORG_TOKEN"] },
    });
    server.use(
      http.put("/api/v1/orgs/org-g5labs/config", async ({ request }) => {
        writes.push((await request.json()) as Record<string, unknown>);
        return HttpResponse.json({ data: {} });
      }),
    );
    renderAdmin();
    expect(
      await screen.findByTestId("org-admin-runtime-config"),
    ).toBeInTheDocument();
    expect((await screen.findAllByText("API keys")).length).toBeGreaterThan(0);
    await userEvent.click(
      await screen.findByTestId("runtime-env-override-ORG_REGION"),
    );
    expect(
      within(screen.getByTestId("runtime-env-scope-ORG_REGION"))
        .getAllByRole("option")
        .map((o) => o.getAttribute("value")),
    ).toEqual(["org"]);
    fireEvent.change(screen.getByTestId("runtime-env-text-input-ORG_REGION"), {
      target: { value: "us" },
    });
    await userEvent.click(screen.getByTestId("runtime-env-apply-ORG_REGION"));
    await userEvent.click(screen.getByTestId("runtime-env-override-ORG_TOKEN"));
    fireEvent.change(screen.getByTestId("runtime-env-secret-input-ORG_TOKEN"), {
      target: { value: "rotated-org-secret" },
    });
    await userEvent.click(screen.getByTestId("runtime-env-apply-ORG_TOKEN"));
    await userEvent.click(screen.getByTestId("runtime-env-save"));
    await waitFor(() =>
      expect(writes).toEqual([
        {
          settings: { ORG_REGION: "us" },
          setSecrets: { ORG_TOKEN: "rotated-org-secret" },
        },
      ]),
    );
  });

  it("links each configuration surface to its canonical org route", async () => {
    serveInstanceFeatures(["chat", "github_app"]);
    renderAdmin();

    // The not-found gate shares the page's test id while auth hydrates, so
    // wait for the resolved org's heading, not the container.
    expect(
      await screen.findByText("G5 Labs · Organization settings · Admin"),
    ).toBeInTheDocument();

    expect(screen.getByTestId("org-admin-link-members")).toHaveAttribute(
      "href",
      "/g5labs/settings/members",
    );
    expect(screen.getByTestId("org-admin-link-approvals")).toHaveAttribute(
      "href",
      "/g5labs/settings/approvals",
    );
    expect(screen.getByTestId("org-admin-link-workers")).toHaveAttribute(
      "href",
      "/g5labs/settings/workers",
    );
    // The instance features load independently of the user.
    expect(
      await screen.findByTestId("org-admin-link-github-app"),
    ).toHaveAttribute("href", "/g5labs/settings/github-app");
    expect(screen.getByTestId("org-admin-link-settings")).toHaveAttribute(
      "href",
      "/g5labs/settings",
    );
  });

  it("hides the GitHub App link when the instance lacks the feature", async () => {
    serveInstanceFeatures(["chat"]);
    renderAdmin();

    // Wait for the resolved org, not the shared container test id (see above).
    // The other surfaces remain linked.
    expect(
      await screen.findByTestId("org-admin-link-members"),
    ).toBeInTheDocument();
    expect(
      screen.queryByTestId("org-admin-link-github-app"),
    ).not.toBeInTheDocument();
  });

  it("hosts the organization settings card for an Admin", async () => {
    renderAdmin();

    expect(
      await screen.findByTestId("org-admin-settings-card"),
    ).toBeInTheDocument();
  });

  it("offers the Invite action to an admin and opens the dialog", async () => {
    renderAdmin();

    const invite = await screen.findByTestId("org-admin-invite");
    await userEvent.click(invite);

    // InviteDialog renders its heading once opened.
    expect(
      await screen.findByRole("heading", { name: /invite/i }),
    ).toBeInTheDocument();
  });

  it.each(["member", "viewer"] as const)(
    "shows only an Administrators-only notice to a %s",
    async (role) => {
      let settingsRequested = false;
      server.use(
        http.get("/api/v1/orgs/:orgId/admin-settings", () => {
          settingsRequested = true;
          return HttpResponse.json({
            data: { defaultIdleTimeoutSecs: 28800, maxRunningVms: 10 },
          });
        }),
      );
      renderAdmin({ user: memberUser(role) });

      expect(
        await screen.findByTestId("org-admin-forbidden"),
      ).toHaveTextContent(/administrators only/i);
      expect(screen.queryByTestId("org-admin-invite")).not.toBeInTheDocument();
      expect(
        screen.queryByTestId("org-admin-config-section"),
      ).not.toBeInTheDocument();
      expect(
        screen.queryByTestId("org-admin-settings-card"),
      ).not.toBeInTheDocument();
      expect(screen.queryByTestId("org-admin-runtime-config")).toBeNull();
      expect(screen.queryByText("Create API key")).toBeNull();
      expect(settingsRequested).toBe(false);
    },
  );

  it("decides by the route org's role, not the selected org's admin scopes", async () => {
    // Admin scopes describe the JWT's selected org (g5labs, admin); the route
    // org (acme) is one where the user is only a member.
    renderAdmin({
      route: "/acme/base?view=admin",
      user: createMockUser({
        orgs: [
          {
            orgId: "org-g5labs",
            orgName: "G5 Labs",
            orgSlug: "g5labs",
            role: "admin",
          },
          {
            orgId: "org-acme",
            orgName: "Acme Corp",
            orgSlug: "acme",
            role: "member",
          },
        ],
      }),
    });

    expect(
      await screen.findByTestId("org-admin-forbidden"),
    ).toBeInTheDocument();
  });

  it("offers an active-org switch only when the org is not active", async () => {
    // activeOrgId defaults to the first org (org-g5labs); open the hub for the
    // second org (acme) so it is not the active org.
    renderAdmin({ route: "/acme/base?view=admin" });

    expect(
      await screen.findByTestId("org-admin-switch-active"),
    ).toBeInTheDocument();
  });

  it("marks the active org without a switch control", async () => {
    renderAdmin({ route: "/g5labs/base?view=admin" });

    // The not-found gate shares the page's test id while auth hydrates, so
    // wait for the active-org text itself, not the container.
    expect(
      await screen.findByText(/this organization is active/i),
    ).toBeInTheDocument();
    expect(
      screen.queryByTestId("org-admin-switch-active"),
    ).not.toBeInTheDocument();
  });
});

describe("OrgAdminPageContent — Issues", () => {
  const DOMAINS_URL = "/api/v1/orgs/org-g5labs/issue-domains";

  function serveDomains(initial: IssueDomainDTO[]) {
    let domains = initial;
    server.use(
      http.get(DOMAINS_URL, () => HttpResponse.json({ data: { domains } })),
    );
    return (next: IssueDomainDTO[]) => {
      domains = next;
    };
  }

  afterEach(() => {
    vi.restoreAllMocks();
    resetMockScopedConfig();
  });

  it("has issues on by default and offers G5 issues on a deployment without the GitHub App", async () => {
    const user = userEvent.setup();
    serveInstanceFeatures([...MOCK_INSTANCE_INFO.features, "issues"]);
    // An organization that never touched the switch: issues are on
    // (g5.issue.native.domain); its native domain was removed here, so the
    // section offers G5 issues again.
    resetMockScopedConfig();
    const setDomains = serveDomains([]);
    let posted: unknown = null;
    server.use(
      http.post(DOMAINS_URL, async ({ request }) => {
        posted = await request.json();
        const created = makeNativeIssueDomain({ displayKey: "G5LABS" });
        setDomains([created]);
        return HttpResponse.json({ data: created }, { status: 201 });
      }),
    );
    renderAdmin();

    const section = await screen.findByTestId("org-admin-issues-section");
    await waitFor(() =>
      expect(
        within(section).getByTestId("external-issues-switch"),
      ).toBeChecked(),
    );

    const form = await within(section).findByTestId("issue-domain-native-form");
    // The key comes from the slug; native content starts trusted.
    expect(within(form).getByTestId("issue-domain-native-key")).toHaveValue(
      "G5LABS",
    );
    expect(
      within(form).getByRole("checkbox", { name: "Trust G5 issue content" }),
    ).toBeChecked();
    // No GitHub App: no GitHub repository can be added here.
    expect(
      within(section).queryByTestId("issue-domain-needs-binding"),
    ).not.toBeInTheDocument();
    expect(
      within(section).queryByTestId("issue-domain-add-form"),
    ).not.toBeInTheDocument();

    await user.click(within(form).getByTestId("issue-domain-native-add"));
    await waitFor(() =>
      expect(posted).toEqual({
        provider: "g5",
        key: "G5LABS",
        contentTrusted: true,
      }),
    );
    const row = await within(section).findByTestId("issue-domain-domain-g5");
    expect(within(row).getByTestId("issue-domain-kind")).toHaveTextContent(
      "G5 issues",
    );
    // Native comments never leave G5: no G5-comments switch.
    expect(
      within(row).queryByTestId("issue-domain-comments-toggle"),
    ).not.toBeInTheDocument();
    expect(within(row).getByTestId("issue-domain-export")).toBeVisible();
    // One native tracker per organization.
    expect(
      within(section).queryByTestId("issue-domain-native-form"),
    ).not.toBeInTheDocument();
  });

  it("exports a native domain and explains why one with issues cannot be removed", async () => {
    const user = userEvent.setup();
    serveInstanceFeatures([...MOCK_INSTANCE_INFO.features, "issues"]);
    resetMockScopedConfig({
      org: { settings: { external_issues_enabled: true } },
    });
    serveDomains([makeNativeIssueDomain()]);
    let exported = false;
    server.use(
      http.get(`${DOMAINS_URL}/domain-g5/export`, () => {
        exported = true;
        return HttpResponse.json({ issues: [] });
      }),
      http.delete(`${DOMAINS_URL}/domain-g5`, () =>
        issueConflictResponse("domain_has_issues"),
      ),
    );
    const createObjectURL = vi.fn(() => "blob:export");
    Object.assign(URL, { createObjectURL, revokeObjectURL: vi.fn() });
    const click = vi
      .spyOn(HTMLAnchorElement.prototype, "click")
      .mockImplementation(() => {});
    renderAdmin();

    const row = await screen.findByTestId("issue-domain-domain-g5");
    await user.click(within(row).getByTestId("issue-domain-export"));
    await waitFor(() => expect(click).toHaveBeenCalled());
    expect(exported).toBe(true);
    expect(createObjectURL).toHaveBeenCalled();

    await user.click(within(row).getByTestId("issue-domain-remove"));
    await user.click(
      within(row).getByTestId("issue-domain-remove-confirm-button"),
    );
    expect(await screen.findByTestId("issue-domain-error")).toHaveTextContent(
      "This issue tracker still has issues, so it cannot be removed.",
    );
  });
});
