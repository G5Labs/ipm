// =============================================================================
// UserSettingsPanel — Sign Out Footer Tests
// =============================================================================
// Covers the persistent footer that invokes useAuth().logout() from inside
// the gear-icon flyout. Backed by ontology
// [g5/client/workspace-page/settings-panel I7. Sign Out control].
// =============================================================================

import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { afterEach, describe, expect, it, vi } from "vitest";

import {
  createMockUser,
  MOCK_ADMIN_SCOPES,
  MOCK_INSTANCE_INFO,
} from "../../test/fixtures";
import { server } from "../../test/msw/server";
import { renderApp } from "../../test/render";
import { UserSettingsPanel } from "./UserSettingsPanel";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function renderPanel() {
  const user = createMockUser({ scopes: MOCK_ADMIN_SCOPES });
  return renderApp(
    <UserSettingsPanel
      open={true}
      onClose={() => {}}
      workspaceId={"workspace-test"}
    />,
    { route: "/g5labs/base", user },
  );
}

function renderPanelAs(email: string) {
  const user = createMockUser({ email, scopes: MOCK_ADMIN_SCOPES });
  return renderApp(
    <UserSettingsPanel
      open={true}
      onClose={() => {}}
      workspaceId={"workspace-test"}
    />,
    { route: "/g5labs/base", user },
  );
}

// =============================================================================
// Tests
// =============================================================================

describe("UserSettingsPanel — Sign Out footer", () => {
  it("renders a Sign Out button regardless of active tab", async () => {
    renderPanel();

    // The footer is outside the tab content, so it renders on the default
    // (Organizations) tab without any tab switching.
    expect(await screen.findByTestId("settings-sign-out")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /sign out/i }),
    ).toBeInTheDocument();
  });

  it("invokes POST /api/v1/auth/logout and clears g5_auth_token when clicked", async () => {
    let logoutCalls = 0;
    server.use(
      http.post("/api/v1/auth/logout", () => {
        logoutCalls += 1;
        return new HttpResponse(null, { status: 204 });
      }),
    );

    renderPanel();

    expect(globalThis.localStorage.getItem("g5_auth_token")).not.toBeNull();

    const signOut = await screen.findByTestId("settings-sign-out");
    await userEvent.click(signOut);

    // The endpoint is hit and useAuth.logout()'s state-clear runs.
    // Navigation to /login is owned by the hook and covered by
    // useAuthContext.test.tsx; here we just verify the wiring.
    await waitFor(() => expect(logoutCalls).toBe(1));
    await waitFor(() =>
      expect(globalThis.localStorage.getItem("g5_auth_token")).toBeNull(),
    );
  });

  it("clears local auth state even when the server-side logout call fails", async () => {
    server.use(http.post("/api/v1/auth/logout", () => HttpResponse.error()));

    renderPanel();

    expect(globalThis.localStorage.getItem("g5_auth_token")).not.toBeNull();

    const signOut = await screen.findByTestId("settings-sign-out");
    await userEvent.click(signOut);

    await waitFor(() =>
      expect(globalThis.localStorage.getItem("g5_auth_token")).toBeNull(),
    );
  });
});

describe("UserSettingsPanel — Account (Organizations)", () => {
  it("renders the Organizations content directly with no tab bar", async () => {
    renderPanel();

    expect(await screen.findByText("Organizations")).toBeInTheDocument();
    // The Account flyout is a single view — the former tab bar is gone.
    expect(screen.queryByRole("tab")).not.toBeInTheDocument();
  });

  it("shows exactly Open Home and Show Admin Page per org", async () => {
    renderPanel();

    // Open Home → the org's default landing page (g5.ui.sign-in-landing).
    expect(
      (await screen.findAllByTestId("org-open-home-link")).map((link) =>
        link.getAttribute("href"),
      ),
    ).toEqual(["/g5labs/home", "/acme/home"]);

    // Show Admin Page → the org Admin hub (base workspace, `?view=admin`).
    expect(
      screen
        .getAllByTestId("org-show-admin-link")
        .map((link) => link.getAttribute("href")),
    ).toEqual(["/g5labs/base?view=admin", "/acme/base?view=admin"]);

    // The former per-org cluster (Members / Approvals / GitHub App / Invite)
    // moved to the Admin page and is gone from the flyout.
    expect(
      screen.queryByTestId("org-manage-members-link"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("org-manage-approvals-link"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("org-manage-github-app-link"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("org-open-invite-dialog"),
    ).not.toBeInTheDocument();
  });

  it("offers Show Admin Page only for orgs the user administers", async () => {
    const user = createMockUser({
      orgs: [
        {
          orgId: "org-g5labs",
          orgName: "G5 Labs",
          orgSlug: "g5labs",
          role: "member",
        },
        {
          orgId: "org-acme",
          orgName: "Acme Corp",
          orgSlug: "acme",
          role: "admin",
        },
        { orgId: "org-beta", orgName: "Beta", orgSlug: "beta", role: "viewer" },
      ],
    });
    renderApp(
      <UserSettingsPanel
        open={true}
        onClose={() => {}}
        workspaceId={"workspace-test"}
      />,
      { route: "/g5labs/base", user },
    );

    // Every org keeps Open Home; only the administered org links its Admin page.
    expect(await screen.findAllByTestId("org-open-home-link")).toHaveLength(3);
    expect(
      screen
        .getAllByTestId("org-show-admin-link")
        .map((link) => link.getAttribute("href")),
    ).toEqual(["/acme/base?view=admin"]);
  });

  // Only platform operators create organizations (g5.organization.creation):
  // the flyout's former Create Organization entry, which a client-side
  // @g5labs.ai check only hid, is gone for every account.
  describe("no self-serve organization creation", () => {
    afterEach(() => {
      vi.unstubAllEnvs();
    });

    it.each([
      ["production", "Staff@G5Labs.AI"],
      ["production", "outsider@example.com"],
      ["development", "outsider@example.com"],
    ])("offers no Create Organization entry (%s, %s)", async (mode, email) => {
      vi.stubEnv("MODE", mode);
      renderPanelAs(email);

      expect(await screen.findByText("Organizations")).toBeInTheDocument();
      expect(await screen.findAllByTestId("org-name")).not.toHaveLength(0);
      expect(
        screen.queryByRole("button", { name: /create organization/i }),
      ).not.toBeInTheDocument();
      expect(
        screen.queryByTestId("user-settings-create-org"),
      ).not.toBeInTheDocument();
    });
  });

  it("keeps the per-org links in self-hosted mode", async () => {
    server.use(
      http.get("/api/v1/instance/info", () =>
        HttpResponse.json({
          data: {
            ...MOCK_INSTANCE_INFO,
            features: ["chat", "oauth"],
            selfHosted: true,
          },
        }),
      ),
    );

    renderPanel();

    expect(await screen.findByText("Organizations")).toBeInTheDocument();
    expect(await screen.findAllByTestId("org-name")).not.toHaveLength(0);
    // Open Home and Show Admin Page are not cloud-gated, so they stay in
    // self-hosted mode.
    expect(
      await screen.findAllByTestId("org-show-admin-link"),
    ).not.toHaveLength(0);
    expect(await screen.findAllByTestId("org-open-home-link")).not.toHaveLength(
      0,
    );
  });

  it("no longer renders SCM Connections or Org Credentials (moved to the settings page)", async () => {
    renderPanel();

    // Ensure the Organizations content has rendered before asserting absence.
    await screen.findByText("Organizations");

    expect(screen.queryByText("SCM Connections")).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("settings-scm-connections"),
    ).not.toBeInTheDocument();
    expect(screen.queryByText("Org Credentials")).not.toBeInTheDocument();
    expect(screen.queryByTestId("theme-selector")).not.toBeInTheDocument();
  });
});
