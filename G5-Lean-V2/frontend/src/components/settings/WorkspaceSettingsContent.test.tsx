// =============================================================================
// WorkspaceSettingsContent — relocated settings body tests
// =============================================================================
// Covers the workspace/preference surfaces that moved out of the (now
// account-only) UserSettingsPanel flyout onto the full-page WorkspaceSettingsPage:
// theme selection, SCM connections, and org credentials. Backed by ontology
// [g5/client/settings-page D2 Hosted surfaces].
// =============================================================================

import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  createMockUser,
  getMockWorkspace,
  MOCK_ADMIN_SCOPES,
  MOCK_CURRENT_USER,
  MOCK_INSTANCE_INFO,
} from "../../test/fixtures";
import { server } from "../../test/msw/server";
import { resetMockScopedConfig } from "../../test/msw/handlers";
import { recordSettingChange } from "../../lib/telemetry";
import { renderApp } from "../../test/render";
import { WorkspaceSettingsContent } from "./WorkspaceSettingsContent";

// Value-carrying preference telemetry (operations
// g5.platform.client-telemetry): the page's native selects and toggles must
// reach it with the value they produced, not only as clicks.
vi.mock("../../lib/telemetry", () => ({ recordSettingChange: vi.fn() }));

// Provider-accounts kill-switch (external_agent_provider_contract D14,
// settings-page D2/D16): the Accounts section renders only while the
// `provider_accounts` instance feature is advertised. Default OFF here (four
// sections); gate-on tests flip the holder and stub the accounts list.
const providerAccountsGate = vi.hoisted(() => ({ enabled: false }));
vi.mock("../../lib/providerFlags", async (importOriginal) => {
  const actual =
    await importOriginal<typeof import("../../lib/providerFlags")>();
  return {
    ...actual,
    isProviderAccountsEnabled: () => providerAccountsGate.enabled,
  };
});

describe("personal Stop VM after merge default", () => {
  it("saves the personal default while workspace policy stays owner-only", async () => {
    let profile = { ...MOCK_CURRENT_USER, stopVmAfterMergeDefault: false };
    const patches: unknown[] = [];
    let finishSave!: () => void;
    const pending = new Promise<void>((resolve) => {
      finishSave = resolve;
    });
    server.use(
      http.get("/api/v1/users/me", () => HttpResponse.json({ data: profile })),
      http.get("/api/v1/workspaces/workspace-test", () =>
        HttpResponse.json({
          data: {
            ...getMockWorkspace("workspace-test"),
            ownerUserId: "another-owner",
          },
        }),
      ),
      http.patch("/api/v1/users/me", async ({ request }) => {
        const patch = (await request.json()) as {
          stopVmAfterMergeDefault: boolean;
        };
        patches.push(patch);
        await pending;
        profile = { ...profile, ...patch };
        return HttpResponse.json({ data: profile });
      }),
    );
    const { queryClient, unmount } = renderContent();
    const invalidate = vi.spyOn(queryClient, "invalidateQueries");
    const control = await screen.findByRole("switch", {
      name: "Stop VM after merge",
    });
    await waitFor(() => expect(control).toBeEnabled());
    expect(control).not.toBeChecked();
    expect(control).toHaveAccessibleDescription(
      "Default for all your workspaces. You can change it in Prepare to Ship.",
    );
    const execution = screen.getByRole("region", { name: "Execution policy" });
    expect(
      within(execution).getByRole("switch", { name: "Stop VM after merge" }),
    ).toBe(control);
    const idle = await screen.findByLabelText(
      "Workspace VM idle timeout in minutes",
    );
    expect(idle).toBeDisabled();
    expect(
      control.compareDocumentPosition(idle) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    await userEvent.click(control);
    await screen.findByText("Saving…");
    expect(control).toBeDisabled();
    expect(patches).toEqual([{ stopVmAfterMergeDefault: true }]);
    finishSave();
    await waitFor(() => expect(control).toBeChecked());
    await waitFor(() => expect(control).toBeEnabled());
    expect(invalidate).toHaveBeenCalledWith({
      queryKey: ["workspace-pull-requests"],
    });
    unmount();
    renderContent();
    await waitFor(() =>
      expect(
        screen.getByRole("switch", { name: "Stop VM after merge" }),
      ).toBeChecked(),
    );
    expect(await screen.findByText("On by default")).toBeInTheDocument();
  });

  it("retains the saved value after a failed save and allows retry", async () => {
    let profile = { ...MOCK_CURRENT_USER, stopVmAfterMergeDefault: false };
    let attempts = 0;
    server.use(
      http.get("/api/v1/users/me", () => HttpResponse.json({ data: profile })),
      http.patch("/api/v1/users/me", async ({ request }) => {
        attempts += 1;
        if (attempts === 1)
          return HttpResponse.json(
            { error: { message: "Save failed" } },
            { status: 500 },
          );
        const patch = (await request.json()) as {
          stopVmAfterMergeDefault: boolean;
        };
        profile = { ...profile, ...patch };
        return HttpResponse.json({ data: profile });
      }),
    );
    renderContent();
    const control = await screen.findByRole("switch", {
      name: "Stop VM after merge",
    });
    await waitFor(() => expect(control).toBeEnabled());
    await userEvent.click(control);
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Could not save. Try again.",
    );
    expect(control).not.toBeChecked();
    expect(control).toBeEnabled();
    await userEvent.click(control);
    await waitFor(() => expect(control).toBeChecked());
    expect(
      screen.queryByText("Could not save. Try again."),
    ).not.toBeInTheDocument();
    expect(attempts).toBe(2);
  });

  it("offers the stored personal default without a selected workspace", async () => {
    server.use(
      http.get("/api/v1/users/me", () =>
        HttpResponse.json({
          data: { ...MOCK_CURRENT_USER, stopVmAfterMergeDefault: true },
        }),
      ),
    );
    renderApp(<WorkspaceSettingsContent />, { route: "/g5labs/settings" });
    const control = await screen.findByRole("switch", {
      name: "Stop VM after merge",
    });
    await waitFor(() => expect(control).toBeEnabled());
    expect(control).toBeChecked();
    expect(
      screen.queryByRole("heading", { name: "Idle timeout" }),
    ).not.toBeInTheDocument();
    expect(screen.getByTestId("settings-index-execution")).toBeInTheDocument();
  });
});

function enableProviderAccounts() {
  providerAccountsGate.enabled = true;
  server.use(
    http.get("/api/v1/orgs/:orgId/provider-accounts", () =>
      HttpResponse.json({ data: [] }),
    ),
  );
}

const USER_SETTINGS_STORAGE_KEY = "g5-user-settings";

beforeEach(() => {
  window.localStorage.removeItem(USER_SETTINGS_STORAGE_KEY);
  providerAccountsGate.enabled = false;
  vi.mocked(recordSettingChange).mockClear();
});

afterEach(() => {
  window.localStorage.removeItem(USER_SETTINGS_STORAGE_KEY);
});

function renderContent(user = createMockUser({ scopes: MOCK_ADMIN_SCOPES })) {
  return renderApp(
    <WorkspaceSettingsContent workspaceId={"workspace-test"} />,
    { route: "/g5labs/settings", user },
  );
}

describe("workspace configuration ownership", () => {
  it.each([true, false])(
    "requires current ownership even for an org admin (owner=%s)",
    async (isOwner) => {
      resetMockScopedConfig({
        workspace: { settings: { TEAM_ENV: "production" } },
      });
      server.use(
        http.get("/api/v1/workspaces/workspace-test", () =>
          HttpResponse.json({
            data: {
              ...getMockWorkspace("workspace-test"),
              ownerUserId: isOwner ? "user-alex" : "another-owner",
            },
          }),
        ),
      );
      enableProviderAccounts();
      renderContent();
      const idleTimeout = await screen.findByLabelText(
        "Workspace VM idle timeout in minutes",
      );
      await waitFor(() =>
        isOwner
          ? expect(idleTimeout).toBeEnabled()
          : expect(idleTimeout).toBeDisabled(),
      );
      await userEvent.click(
        await screen.findByTestId("runtime-env-override-TEAM_ENV"),
      );
      const scopes = screen.getByTestId("runtime-env-scope-TEAM_ENV");
      expect(
        within(scopes)
          .getAllByRole("option")
          .map((option) => option.getAttribute("value")),
      ).toEqual(["workspace", "user"]);
      const workspaceOption = within(scopes).getByRole("option", {
        name: "Workspace",
      });
      if (isOwner) expect(workspaceOption).toBeEnabled();
      else expect(workspaceOption).toBeDisabled();
      // Personal configuration stays editable for every reader.
      expect(within(scopes).getByRole("option", { name: "You" })).toBeEnabled();

      await userEvent.click(screen.getByTestId("provider-account-add"));
      await userEvent.click(
        screen.getByTestId("provider-account-scope-select"),
      );
      expect(
        screen.queryByTestId("provider-account-scope-select-option-org"),
      ).toBeNull();
      const accountWorkspaceOption = await screen.findByTestId(
        "provider-account-scope-select-option-workspace",
      );
      if (isOwner) expect(accountWorkspaceOption).toBeEnabled();
      else expect(accountWorkspaceOption).toBeDisabled();
      expect(screen.queryByText("API keys")).toBeNull();
    },
  );
});

describe("WorkspaceSettingsContent — preferences", () => {
  it("renders theme controls and suppresses the sync launch selector", async () => {
    renderContent();

    expect(await screen.findByTestId("theme-selector")).toBeInTheDocument();
    expect(screen.getByTestId("mode-toggle")).toBeInTheDocument();
  });
});

describe("WorkspaceSettingsContent — layout preferences (settings-page D12)", () => {
  function storedSettings(): Record<string, unknown> {
    return JSON.parse(
      window.localStorage.getItem(USER_SETTINGS_STORAGE_KEY) ?? "{}",
    );
  }

  it("renders the Layout controls with their defaults", async () => {
    renderContent();

    const select = (await screen.findByTestId(
      "sidebar-display-mode-select",
    )) as HTMLSelectElement;
    expect(select.value).toBe("collapse-to-rail");
    const optionLabels = Array.from(select.options).map(
      (option) => option.textContent,
    );
    expect(optionLabels).toEqual([
      "Full sidebar",
      "Full sidebar, icon rail when collapsed",
      "Expand on hover",
      "Icon rail only",
    ]);
  });

  it("reports select and toggle changes to telemetry with their values", async () => {
    renderContent();
    const user = userEvent.setup();
    const select = await screen.findByTestId("sidebar-display-mode-select");
    await user.selectOptions(select, "rail");
    expect(recordSettingChange).toHaveBeenCalledWith(
      "sidebarDisplayMode",
      "rail",
      "collapse-to-rail",
    );

    const modeToggle = within(screen.getByTestId("mode-toggle")).getByRole(
      "switch",
    );
    await user.click(modeToggle);
    expect(recordSettingChange).toHaveBeenCalledWith(
      "themeMode",
      "light",
      "dark",
    );
  });

  it("persists the sidebar display mode through user settings", async () => {
    renderContent();

    const select = await screen.findByTestId("sidebar-display-mode-select");
    await userEvent.selectOptions(select, "rail");

    expect((select as HTMLSelectElement).value).toBe("rail");
    expect(storedSettings().sidebarDisplayMode).toBe("rail");
  });

  it("offers Rows per Home tile with 5 by default and persists a larger size (g5.ui.home-paging)", async () => {
    renderContent();

    const select = (await screen.findByTestId(
      "home-rows-per-tile-select",
    )) as HTMLSelectElement;
    expect(select.value).toBe("5");
    expect([...select.options].map((option) => option.value)).toEqual([
      "5",
      "10",
      "20",
      "50",
    ]);
    await userEvent.selectOptions(select, "20");
    expect(select.value).toBe("20");
    expect(storedSettings().homeRowsPerTile).toBe(20);
  });

  it("reflects persisted values on load", async () => {
    window.localStorage.setItem(
      USER_SETTINGS_STORAGE_KEY,
      JSON.stringify({ sidebarDisplayMode: "expanded" }),
    );
    renderContent();

    const select = (await screen.findByTestId(
      "sidebar-display-mode-select",
    )) as HTMLSelectElement;
    expect(select.value).toBe("expanded");
  });

  it("normalizes an invalid stored sidebar mode back to the default", async () => {
    window.localStorage.setItem(
      USER_SETTINGS_STORAGE_KEY,
      JSON.stringify({ sidebarDisplayMode: "sideways" }),
    );
    renderContent();

    const select = (await screen.findByTestId(
      "sidebar-display-mode-select",
    )) as HTMLSelectElement;
    expect(select.value).toBe("collapse-to-rail");
  });

  it("hides workspace tabs by default and persists the toggle (g5.workspace.ui.center-tabs)", async () => {
    renderContent();

    const toggle = await screen.findByRole("switch", {
      name: "Show workspace tabs",
    });
    expect(toggle).toHaveAttribute("aria-checked", "false");

    await userEvent.click(toggle);

    expect(toggle).toHaveAttribute("aria-checked", "true");
    expect(storedSettings().showTabBar).toBe(true);
  });

  it("normalizes a non-boolean stored tab visibility back to hidden", async () => {
    window.localStorage.setItem(
      USER_SETTINGS_STORAGE_KEY,
      JSON.stringify({ showTabBar: "yes" }),
    );
    renderContent();

    expect(
      await screen.findByRole("switch", { name: "Show workspace tabs" }),
    ).toHaveAttribute("aria-checked", "false");
  });

  it("renders the subscription usage window control defaulting to 7d (D8)", async () => {
    renderContent();

    const select = (await screen.findByTestId(
      "subscription-usage-window-select",
    )) as HTMLSelectElement;
    expect(select.value).toBe("7d");
    const optionLabels = Array.from(select.options).map(
      (option) => option.textContent,
    );
    expect(optionLabels).toEqual([
      "7-day weekly limit",
      "5-hour window",
      "Never display",
    ]);
  });

  it("persists a Never-display subscription usage window through user settings", async () => {
    renderContent();

    const select = await screen.findByTestId(
      "subscription-usage-window-select",
    );
    await userEvent.selectOptions(select, "off");

    expect((select as HTMLSelectElement).value).toBe("off");
    expect(storedSettings().subscriptionUsageWindow).toBe("off");
  });
});

// Workbench header settings (ui g5.workbench.header-styles, g5.ui.guided-toggle):
// the one shell offers the style selects unconditionally; the Guided switch
// exists only while the instance advertises `guided_journey`.
describe("WorkspaceSettingsContent — workbench header settings", () => {
  function advertise(...features: string[]) {
    server.use(
      http.get("/api/v1/instance/info", () =>
        HttpResponse.json({
          data: {
            ...MOCK_INSTANCE_INFO,
            features: [...MOCK_INSTANCE_INFO.features, ...features],
          },
        }),
      ),
    );
  }

  function optionValues(select: HTMLSelectElement): string[] {
    return Array.from(select.options).map((option) => option.value);
  }

  function storedSettings(): Record<string, unknown> {
    return JSON.parse(
      window.localStorage.getItem(USER_SETTINGS_STORAGE_KEY) ?? "{}",
    );
  }

  it("offers the Guided experience toggle only with the guided journey and persists it", async () => {
    const first = renderContent();
    await screen.findByTestId("action-bar-style-select");
    expect(screen.queryByTestId("guided-journey-setting")).toBeNull();
    first.unmount();

    advertise("guided_journey");
    renderContent();
    const setting = await screen.findByTestId("guided-journey-setting");
    const toggle = within(setting).getByRole("switch", {
      name: "Show the guided experience",
    });
    // Off by default (g5.ui.guided-toggle, owner request 2026-11-09).
    expect(toggle).toHaveAttribute("aria-checked", "false");
    await userEvent.click(toggle);
    expect(toggle).toHaveAttribute("aria-checked", "true");
    expect(storedSettings().showGuidedJourney).toBe(true);
  });

  it("offers no Workspace panels select: the workbench is the one shell", async () => {
    renderContent();
    await screen.findByTestId("phase-bar-style-select");
    expect(screen.queryByTestId("workspace-layout-select")).toBeNull();
    expect(screen.queryByText("Workspace panels")).toBeNull();
  });

  it("offers the header style selects, defaulting to minimal / dynamic, and persists them", async () => {
    renderContent();
    const phaseBar = (await screen.findByTestId(
      "phase-bar-style-select",
    )) as HTMLSelectElement;
    const actionBar = screen.getByTestId(
      "action-bar-style-select",
    ) as HTMLSelectElement;
    expect(phaseBar.value).toBe("minimal");
    expect(optionValues(phaseBar)).toEqual(["extended", "minimal", "vertical"]);
    expect(actionBar.value).toBe("dynamic");
    expect(optionValues(actionBar)).toEqual([
      "extended-icons",
      "minimal-icons",
      "extended-text",
      "simplified",
      "dynamic",
    ]);
    expect(
      actionBar.querySelector('option[value="dynamic"]')?.textContent,
    ).toBe("Dynamic — icons shrink to fit, then scroll");

    await userEvent.selectOptions(phaseBar, "vertical");
    await userEvent.selectOptions(actionBar, "minimal-icons");
    expect(phaseBar.value).toBe("vertical");
    expect(actionBar.value).toBe("minimal-icons");
    expect(storedSettings().phaseBarStyle).toBe("vertical");
    expect(storedSettings().actionBarStyle).toBe("minimal-icons");
  });

  it("offers the chat bubble style and count, defaulting to 3D / 5, and persists them (g5.workspace.ui.chat-bubble-stack)", async () => {
    renderContent();
    const style = (await screen.findByTestId(
      "chat-bubble-style-select",
    )) as HTMLSelectElement;
    const limit = screen.getByTestId(
      "chat-bubble-limit-input",
    ) as HTMLInputElement;
    expect(style.value).toBe("3d");
    expect(optionValues(style)).toEqual(["off", "still", "animated", "3d"]);
    expect(limit.value).toBe("5");
    expect(limit).toHaveAttribute("min", "1");
    expect(limit).toHaveAttribute("max", "50");
    expect(screen.queryByTestId("chat-bubble-stack-select")).toBeNull();

    await userEvent.selectOptions(style, "animated");
    fireEvent.change(limit, { target: { value: "12" } });
    expect(style.value).toBe("animated");
    expect(limit.value).toBe("12");
    expect(storedSettings().chatBubbleStyle).toBe("animated");
    expect(storedSettings().chatBubbleLimit).toBe(12);

    // Out of range stays where it was.
    fireEvent.change(limit, { target: { value: "0" } });
    expect(storedSettings().chatBubbleLimit).toBe(12);
  });
});

// One sheet, one scroll, an index on the left (ui g5.ui.settings-page):
// the sections in page order, Accounts second when offered, and the index
// listing the same sections.
describe("WorkspaceSettingsContent — section layout", () => {
  function heading(name: string) {
    return screen.findByRole("heading", { level: 2, name });
  }

  async function headingOrder(names: string[]) {
    const elements = await Promise.all(names.map((name) => heading(name)));
    for (let i = 1; i < elements.length; i++) {
      expect(
        elements[i - 1].compareDocumentPosition(elements[i]) &
          Node.DOCUMENT_POSITION_FOLLOWING,
        `${names[i - 1]} before ${names[i]}`,
      ).toBeTruthy();
    }
  }

  it("renders Appearance, Defaults, Editor, Access and Execution policy while provider accounts are off", async () => {
    renderContent();

    await headingOrder([
      "Appearance",
      "Defaults",
      "Editor",
      "Access",
      "Notifications",
      "Execution policy",
    ]);
    expect(screen.queryByText("Provider runtime")).not.toBeInTheDocument();
    expect(screen.queryByText("Preferences")).not.toBeInTheDocument();
    expect(
      screen.queryByRole("heading", { level: 2, name: "Accounts" }),
    ).not.toBeInTheDocument();
    expect(screen.queryByTestId("provider-accounts-section")).toBeNull();
  });

  it("renders Accounts second while provider accounts are on", async () => {
    enableProviderAccounts();
    renderContent();

    await headingOrder([
      "Appearance",
      "Accounts",
      "Defaults",
      "Editor",
      "Access",
      "Notifications",
      "Execution policy",
    ]);
    expect(
      await screen.findByTestId("provider-accounts-section"),
    ).toBeInTheDocument();
  });

  it("lists the same sections, in the same order, in the index", async () => {
    enableProviderAccounts();
    renderContent();

    await heading("Accounts");
    await heading("Execution policy");
    const index = screen.getByTestId("settings-index");
    expect(
      within(index)
        .getAllByRole("button")
        .map((button) => button.textContent),
    ).toEqual([
      "Appearance",
      "Accounts",
      "Defaults",
      "Editor",
      "Access",
      "Notifications",
      "Execution policy",
    ]);
    expect(within(index).getByText("Your settings")).toBeInTheDocument();
  });

  it("offers the Admin page from the index to an owner or admin only", async () => {
    renderContent();
    expect(await screen.findByTestId("settings-admin-link")).toHaveAttribute(
      "href",
      "/g5labs/base?view=admin",
    );
    expect(screen.getByTestId("settings-priority-note")).toBeInTheDocument();
  });

  it("hides the Admin link from a member", async () => {
    renderContent(
      createMockUser({
        scopes: [],
        orgs: [
          {
            orgId: "org-g5labs",
            orgName: "G5 Labs",
            orgSlug: "g5labs",
            role: "member",
          },
        ],
      }),
    );
    await heading("Access");
    expect(screen.queryByTestId("settings-admin-link")).toBeNull();
    // The precedence note holds for everyone (org → workspace → user).
    expect(screen.getByTestId("settings-priority-note")).toHaveTextContent(
      "Your settings on this page take priority over the organization settings.",
    );
  });
});

describe("WorkspaceSettingsContent — Access section", () => {
  it("keeps SCM connections and removes organization API key controls", async () => {
    renderContent();
    expect(await screen.findByText("SCM connections")).toBeInTheDocument();
    expect(screen.queryByText("API keys")).toBeNull();
    expect(screen.queryByText("Create API key")).toBeNull();
  });

  it("does not render worker registration tokens on the settings page", async () => {
    renderContent();

    await screen.findByText("SCM connections");
    expect(
      screen.queryByText("Worker registration tokens"),
    ).not.toBeInTheDocument();
    expect(screen.queryByText("Create worker token")).not.toBeInTheDocument();
  });

  it("shows Connect GitHub when GitHub is not already connected", async () => {
    const user = {
      ...createMockUser({ scopes: MOCK_ADMIN_SCOPES }),
      scmConnections: [],
    };

    renderApp(<WorkspaceSettingsContent workspaceId={"workspace-test"} />, {
      route: "/g5labs/settings",
      user,
    });

    expect(
      await screen.findByRole("button", { name: /connect github/i }),
    ).toBeInTheDocument();
  });

  it("deletes an existing SCM connection and refreshes the panel", async () => {
    const confirmSpy = vi
      .spyOn(window, "confirm")
      .mockImplementation(() => true);
    let deleteCalls = 0;
    const user = createMockUser({
      scopes: MOCK_ADMIN_SCOPES,
      scmConnections: [
        {
          provider: "github",
          providerUsername: "alexchen",
          scopes: ["repo", "read:org"],
          hasInstallation: true,
          connectedAt: "2024-01-15T10:00:00Z",
        },
      ],
    } as Partial<ReturnType<typeof createMockUser>>);

    server.use(
      http.delete("/api/v1/scm/github/disconnect", () => {
        deleteCalls += 1;
        globalThis.localStorage.setItem(
          "g5_test_current_user",
          JSON.stringify({ ...user, scmConnections: [] }),
        );
        return new HttpResponse(null, { status: 204 });
      }),
    );

    renderApp(<WorkspaceSettingsContent workspaceId={"workspace-test"} />, {
      route: "/g5labs/settings",
      user,
    });

    const deleteGithub = await screen.findByRole("button", {
      name: /delete github scm connection/i,
    });
    await userEvent.click(deleteGithub);

    await waitFor(() => expect(deleteCalls).toBe(1));
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: /connect github/i }),
      ).toBeInTheDocument(),
    );
    expect(confirmSpy).toHaveBeenCalledWith(
      "Delete your github SCM connection?",
    );

    confirmSpy.mockRestore();
  });
});
