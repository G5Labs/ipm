// =============================================================================
// RuntimeEnvConfig (Defaults card) tests — settings-page D19 (layout: default
// agent, per-agent default model + account + resolution line, custom
// variables; no built-in provider rows, no subscription checkbox), D5
// (custom-variable editor semantics), D6 (default agent select), D8 (standard
// Codex availability), D9 (custom secret hints), D17 (account
// select per row); contract D10/D11/D14.
// =============================================================================

import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { useState } from "react";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";

import { server } from "../../test/msw/server";
import { MOCK_INSTANCE_INFO } from "../../data/mock";
import { renderApp } from "../../test/render";
import { RuntimeEnvConfig } from "./RuntimeEnvConfig";
import { CLAUDE_CHAT_DEFAULT_MODEL } from "../../lib/claudeModels";
import { CODEX_CHAT_DEFAULT_MODEL } from "../../lib/codexModels";
import type {
  AsyncState,
  PickerHarness,
  ProviderAccountDTO,
  ScopedConfig,
} from "../../lib/types";

// The provider-accounts kill-switch resolves through providerFlags over the
// instance-info query. Tests control it via this holder; accounts are OFF
// by default. Codex itself is always available.
const gates = vi.hoisted(() => ({ providerAccounts: false }));
vi.mock("../../lib/providerFlags", async (importOriginal) => {
  const actual =
    await importOriginal<typeof import("../../lib/providerFlags")>();
  return {
    ...actual,
    isProviderAccountsEnabled: () => gates.providerAccounts,
  };
});

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const HIDDEN_PROVIDER_KEYS = [
  "ANTHROPIC_AUTH_TOKEN",
  "CLAUDE_CODE_OAUTH_TOKEN",
  "ANTHROPIC_BASE_URL",
  "ANTHROPIC_MODEL",
  "CLAUDE_CODE_SUBAGENT_MODEL",
  "OPENROUTER_API_KEY",
  "OPENAI_API_KEY",
  "OPENAI_BASE_URL",
  "OPENAI_MODEL",
] as const;

function scopedConfig(
  scope: ScopedConfig["scope"],
  settings: Record<string, unknown> = {},
  secretKeys: string[] = [],
  secretKeyHints: Record<string, string> = {},
): ScopedConfig {
  return {
    scope,
    settings,
    secretKeys,
    secretKeyHints,
    // effective* are server-computed; the component does NOT rely on them for
    // provenance, so simple mirrors are fine for these tests.
    effectiveSettings: settings,
    effectiveSecretKeys: secretKeys,
    effectiveSecretKeyHints: secretKeyHints,
    updatedBy: null,
    updatedAt: null,
  };
}

function ok(data: ScopedConfig): AsyncState<ScopedConfig> {
  return { status: "ok", data };
}

function account(overrides: Partial<ProviderAccountDTO>): ProviderAccountDTO {
  return {
    id: "acc-1",
    orgId: "org-1",
    name: "Personal Max",
    kind: "claude_subscription",
    family: "claude",
    scope: "user",
    workspaceId: null,
    userId: "user-1",
    baseUrl: null,
    hasSecret: true,
    secretHint: "sk-ant-oat01",
    createdBy: "user-1",
    createdAt: "2026-07-02T10:00:00Z",
    updatedAt: "2026-07-02T10:00:00Z",
    usage: null,
    ...overrides,
  };
}

const CLAUDE_ACCOUNT = account({});
const CODEX_ACCOUNT = account({
  id: "acc-3",
  name: "ChatGPT Pro",
  kind: "chatgpt_subscription",
  family: "codex",
  hasSecret: false,
  secretHint: null,
});

function installAccounts(accounts: ProviderAccountDTO[]) {
  server.use(
    http.get("/api/v1/orgs/org-1/provider-accounts", () =>
      HttpResponse.json({ data: accounts }),
    ),
  );
}

interface RenderOpts {
  context?: "workspace" | "organization";
  org?: ScopedConfig;
  workspace?: ScopedConfig;
  user?: ScopedConfig;
  workspaceId?: string | null;
  updateOrg?: ReturnType<typeof vi.fn>;
  updateWorkspace?: ReturnType<typeof vi.fn>;
  updateUser?: ReturnType<typeof vi.fn>;
  canEditOrg?: boolean;
  canEditWorkspace?: boolean;
  defaultAgentModels?: Partial<Record<PickerHarness, string>>;
  onDefaultAgentModelChange?: ReturnType<typeof vi.fn>;
  showScopedConfig?: boolean;
}

function renderConfig(opts: RenderOpts = {}) {
  const updateOrg = opts.updateOrg ?? vi.fn().mockResolvedValue(undefined);
  const updateWorkspace =
    opts.updateWorkspace ?? vi.fn().mockResolvedValue(undefined);
  const updateUser = opts.updateUser ?? vi.fn().mockResolvedValue(undefined);

  const result = renderApp(
    <RuntimeEnvConfig
      context={opts.context}
      orgId="org-1"
      workspaceId={opts.workspaceId ?? "ws-1"}
      org={ok(opts.org ?? scopedConfig("org"))}
      workspace={ok(opts.workspace ?? scopedConfig("workspace"))}
      user={ok(opts.user ?? scopedConfig("user"))}
      updateOrg={updateOrg}
      updateWorkspace={updateWorkspace}
      updateUser={updateUser}
      canEditOrg={opts.canEditOrg ?? true}
      canEditWorkspace={opts.canEditWorkspace ?? true}
      defaultAgentModels={opts.defaultAgentModels}
      onDefaultAgentModelChange={opts.onDefaultAgentModelChange}
      showScopedConfig={opts.showScopedConfig}
    />,
  );
  return { ...result, updateOrg, updateWorkspace, updateUser };
}

/** Render with the model rows on (the app always supplies the callback). */
function renderDefaults(opts: RenderOpts = {}) {
  return renderConfig({
    defaultAgentModels: {
      claude_code: CLAUDE_CHAT_DEFAULT_MODEL,
      codex: CODEX_CHAT_DEFAULT_MODEL,
    },
    onDefaultAgentModelChange: vi.fn(),
    ...opts,
  });
}

async function openThemedSelect(testId: string) {
  await userEvent.click(await screen.findByTestId(testId));
  return screen.findByRole("listbox");
}

// ---------------------------------------------------------------------------
// Layout (D19)
// ---------------------------------------------------------------------------

describe("RuntimeEnvConfig — Defaults layout", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("renders the default agent select, a Claude Code row and a Codex row, and the custom-variable editor", () => {
    renderDefaults();

    const defaultAgent = screen.getByTestId("runtime-env-default-agent");
    expect(within(defaultAgent).getByText("Default agent")).toBeInTheDocument();
    expect(
      screen.getByTestId("runtime-env-default-agent-select"),
    ).toBeInTheDocument();

    const claudeRow = screen.getByTestId(
      "runtime-env-default-model-claude_code",
    );
    expect(within(claudeRow).getByText("Claude Code")).toBeInTheDocument();
    expect(within(claudeRow).getByText("Default model")).toBeInTheDocument();
    expect(
      within(claudeRow).getByTestId(
        "runtime-env-default-model-select-claude_code",
      ),
    ).toBeInTheDocument();

    const codexRow = screen.getByTestId("runtime-env-default-model-codex");
    expect(within(codexRow).getByText("Codex")).toBeInTheDocument();
    expect(
      within(codexRow).getByTestId("runtime-env-default-model-select-codex"),
    ).toBeInTheDocument();

    // Order: default agent, Claude Code, Codex, then Other variables.
    expect(
      defaultAgent.compareDocumentPosition(claudeRow) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(
      claudeRow.compareDocumentPosition(codexRow) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    const other = screen.getByText("Other variables");
    expect(
      codexRow.compareDocumentPosition(other) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(screen.getByTestId("runtime-env-add-var")).toHaveTextContent(
      "Add custom variable",
    );
    expect(screen.getByTestId("runtime-env-save")).toBeInTheDocument();
  });

  it("renders no built-in provider variable row and no subscription checkbox even when every scope carries those keys", () => {
    renderDefaults({
      org: scopedConfig(
        "org",
        {
          ANTHROPIC_BASE_URL: "https://openrouter.ai/api",
          ANTHROPIC_MODEL: "anthropic/claude-opus-4.8",
          CLAUDE_CODE_SUBAGENT_MODEL: "anthropic/claude-haiku-4.5",
          OPENAI_BASE_URL: "https://api.openai.com/v1",
          OPENAI_MODEL: "gpt-5.6-sol",
          codex_subscription_enabled: false,
          claude_account_id: "acc-1",
          codex_account_id: "acc-3",
        },
        ["ANTHROPIC_AUTH_TOKEN", "OPENROUTER_API_KEY", "OPENAI_API_KEY"],
        { ANTHROPIC_AUTH_TOKEN: "sk-or-v1-012" },
      ),
      workspace: scopedConfig("workspace", {}, ["CLAUDE_CODE_OAUTH_TOKEN"]),
      user: scopedConfig("user", { codex_subscription_enabled: true }),
    });

    for (const key of HIDDEN_PROVIDER_KEYS) {
      expect(screen.queryByTestId(`runtime-env-row-${key}`)).toBeNull();
      expect(screen.queryByText(key)).toBeNull();
    }
    for (const key of [
      "codex_subscription_enabled",
      "claude_account_id",
      "codex_account_id",
      "default_agent_provider",
    ]) {
      expect(screen.queryByTestId(`runtime-env-row-${key}`)).toBeNull();
    }
    expect(screen.queryByRole("checkbox")).toBeNull();
    expect(screen.queryByTestId("runtime-env-codex-subscription")).toBeNull();
    expect(screen.queryByText(/ChatGPT Paid Tier/)).toBeNull();
    // The card stays clean (only the Save bar is dirty-tracked), so nothing
    // is staged for those keys.
    expect(screen.getByTestId("runtime-env-save")).toBeDisabled();
    expect(screen.getByTestId("runtime-env-status")).toHaveTextContent(
      "All changes saved",
    );
  });

  it("keeps custom variables editable while provider keys are present", () => {
    renderDefaults({
      org: scopedConfig(
        "org",
        { GH_TOKEN_HOST: "github.com", ANTHROPIC_MODEL: "claude-opus-4-8" },
        ["GH_TOKEN", "ANTHROPIC_AUTH_TOKEN"],
        { GH_TOKEN: "ghp_0123456789" },
      ),
    });

    const setting = screen.getByTestId("runtime-env-row-GH_TOKEN_HOST");
    expect(within(setting).getByText("github.com")).toBeInTheDocument();
    expect(within(setting).getByText("Org")).toBeInTheDocument();
    const secret = screen.getByTestId("runtime-env-row-GH_TOKEN");
    expect(within(secret).getByText("secret")).toBeInTheDocument();
    expect(
      within(secret).getByTestId("runtime-env-hint-GH_TOKEN"),
    ).toHaveTextContent("ghp_0123456789…");
    expect(screen.queryByTestId("runtime-env-row-ANTHROPIC_MODEL")).toBeNull();
    expect(
      screen.queryByTestId("runtime-env-row-ANTHROPIC_AUTH_TOKEN"),
    ).toBeNull();
  });

  it("hides the default agent, account, and custom-variable controls but keeps the model selects when showScopedConfig is false", () => {
    gates.providerAccounts = true;
    installAccounts([CLAUDE_ACCOUNT]);
    renderDefaults({ showScopedConfig: false });

    expect(
      screen.getByTestId("runtime-env-default-model-select-claude_code"),
    ).toBeInTheDocument();
    expect(
      screen.getByTestId("runtime-env-default-model-select-codex"),
    ).toBeInTheDocument();
    expect(screen.queryByTestId("runtime-env-default-agent")).toBeNull();
    expect(
      screen.queryByTestId("provider-account-assignment-claude_code"),
    ).toBeNull();
    expect(screen.queryByText("Other variables")).toBeNull();
    expect(screen.queryByTestId("runtime-env-save")).toBeNull();
    // No resolution line exists in either gate state (settings-page D19).
    expect(
      screen.queryByTestId(
        "provider-account-resolution-unavailable-claude_code",
      ),
    ).toBeNull();
    expect(
      screen.queryByTestId("provider-account-resolution-claude_code"),
    ).toBeNull();
  });

  it("keeps the model selects usable while the scoped config loads", () => {
    const onDefaultAgentModelChange = vi.fn();
    renderApp(
      <RuntimeEnvConfig
        orgId="org-1"
        workspaceId="ws-1"
        org={{ status: "loading" }}
        workspace={{ status: "loading" }}
        user={{ status: "loading" }}
        updateOrg={vi.fn()}
        updateWorkspace={vi.fn()}
        updateUser={vi.fn()}
        canEditOrg
        canEditWorkspace
        defaultAgentModels={{
          claude_code: "anthropic/claude-opus-4.8",
          codex: CODEX_CHAT_DEFAULT_MODEL,
        }}
        onDefaultAgentModelChange={onDefaultAgentModelChange}
        showScopedConfig
      />,
    );

    expect(screen.getByText("Loading configuration…")).toBeInTheDocument();
    const claudeSelect = screen.getByTestId(
      "runtime-env-default-model-select-claude_code",
    ) as HTMLSelectElement;
    const codexSelect = screen.getByTestId(
      "runtime-env-default-model-select-codex",
    ) as HTMLSelectElement;
    expect(claudeSelect.disabled).toBe(false);
    expect(codexSelect.disabled).toBe(false);
    expect(screen.queryByTestId("runtime-env-default-agent")).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// Default model selects (D5 preference semantics)
// ---------------------------------------------------------------------------

describe("RuntimeEnvConfig — default model selects", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("hides the model rows unless onDefaultAgentModelChange is provided and reports each provider key", () => {
    const { unmount } = renderConfig();
    expect(
      screen.queryByTestId("runtime-env-default-model-claude_code"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("runtime-env-default-model-codex"),
    ).not.toBeInTheDocument();
    unmount();

    const onDefaultAgentModelChange = vi.fn();
    renderConfig({
      defaultAgentModels: {
        claude_code: "anthropic/claude-opus-4.8",
        codex: CODEX_CHAT_DEFAULT_MODEL,
      },
      onDefaultAgentModelChange,
    });

    const claudeSelect = screen.getByTestId(
      "runtime-env-default-model-select-claude_code",
    ) as HTMLSelectElement;
    expect(claudeSelect.value).toBe("anthropic/claude-opus-4.8");
    fireEvent.change(claudeSelect, { target: { value: "claude-sonnet-4-6" } });
    expect(onDefaultAgentModelChange).toHaveBeenCalledWith(
      "claude_code",
      "claude-sonnet-4-6",
    );

    const codexSelect = screen.getByTestId(
      "runtime-env-default-model-select-codex",
    ) as HTMLSelectElement;
    expect(codexSelect.value).toBe(CODEX_CHAT_DEFAULT_MODEL);
    const alternateCodexModel = Array.from(codexSelect.options)
      .map((option) => option.value)
      .find((value) => value && value !== CODEX_CHAT_DEFAULT_MODEL);
    expect(alternateCodexModel).toBeTruthy();
    fireEvent.change(codexSelect, { target: { value: alternateCodexModel } });
    expect(onDefaultAgentModelChange).toHaveBeenCalledWith(
      "codex",
      alternateCodexModel,
    );
  });

  it("shows a friendly model label without exposing its upstream slug", () => {
    renderDefaults({
      defaultAgentModels: { claude_code: CLAUDE_CHAT_DEFAULT_MODEL },
    });

    const select = screen.getByTestId(
      "runtime-env-default-model-select-claude_code",
    ) as HTMLSelectElement;
    expect(select.value).toBe("anthropic/claude-opus-5.5");
    expect(
      screen.getByRole("option", { name: "Opus 5.5" }),
    ).toBeInTheDocument();
    expect(select.options[select.selectedIndex].text).not.toContain(
      "anthropic/",
    );
  });

  it("submits a custom model value without client-side route validation", () => {
    const onDefaultAgentModelChange = vi.fn();
    const promptSpy = vi
      .spyOn(window, "prompt")
      .mockReturnValue("claude-custom-unlisted"); // unknown bare id
    renderConfig({
      org: scopedConfig("org", {
        ANTHROPIC_BASE_URL: "https://openrouter.ai/api",
      }),
      defaultAgentModels: { claude_code: "anthropic/claude-opus-4.8" },
      onDefaultAgentModelChange,
    });

    fireEvent.change(
      screen.getByTestId("runtime-env-default-model-select-claude_code"),
      { target: { value: "__other__" } },
    );
    expect(onDefaultAgentModelChange).toHaveBeenCalledWith(
      "claude_code",
      "claude-custom-unlisted",
    );
    promptSpy.mockRestore();
  });
});

// ---------------------------------------------------------------------------
// Resolution line (D17/D19; contract D11)
// ---------------------------------------------------------------------------

describe("RuntimeEnvConfig — no resolution line, no provenance badges", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("renders no resolver-fed line under either agent row and never calls the resolver (settings-page D19, chat 9275474a)", () => {
    gates.providerAccounts = true;
    installAccounts([CLAUDE_ACCOUNT, CODEX_ACCOUNT]);
    renderDefaults({
      org: scopedConfig("org", {}, ["ANTHROPIC_AUTH_TOKEN"], {
        ANTHROPIC_AUTH_TOKEN: "sk-ant-api03",
      }),
    });

    for (const harness of ["claude_code", "codex"] as const) {
      const row = screen.getByTestId(`runtime-env-default-model-${harness}`);
      for (const suffix of [
        "resolution",
        "resolution-loading",
        "resolution-error",
        "resolution-unavailable",
        "resolution-hint",
        "resolution-hint-source",
        "assignment-source",
        "assignment-override",
        "assignment-drawer",
      ]) {
        expect(
          within(row).queryByTestId(`provider-account-${suffix}-${harness}`),
        ).toBeNull();
      }
    }
    expect(screen.queryByText(/resolves to/)).toBeNull();
    expect(screen.queryByText(/Through the G5 proxy/)).toBeNull();
    // The legacy hint never renders on the Defaults card (only the Accounts
    // rows and custom secret rows show hints).
    expect(screen.queryByText(/sk-ant-api03/)).toBeNull();
  });

  it("renders no source badge under the Default agent helper text (chat b2028fca)", () => {
    renderDefaults({
      user: scopedConfig("user", { default_agent_provider: "codex" }),
    });
    const defaultAgent = screen.getByTestId("runtime-env-default-agent");
    expect(
      within(defaultAgent).getByText(
        /Agent provider new chats and tasks start on/,
      ),
    ).toBeInTheDocument();
    expect(within(defaultAgent).queryByText("default")).toBeNull();
    expect(within(defaultAgent).queryByText("You")).toBeNull();
    expect(within(defaultAgent).queryByText("Org")).toBeNull();
    expect(within(defaultAgent).queryByText("Workspace")).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// Account selects (D17 hosted in D19; contract D14 kill-switch)
// ---------------------------------------------------------------------------

describe("RuntimeEnvConfig — account selects", () => {
  it("edits only org defaults and assignments in Admin, ignoring personal and workspace overrides", async () => {
    gates.providerAccounts = true;
    installAccounts([
      CLAUDE_ACCOUNT,
      account({ id: "shared", scope: "org", userId: null, name: "Shared" }),
    ]);
    const { updateOrg, updateUser, updateWorkspace } = renderConfig({
      context: "organization",
      org: scopedConfig("org", { default_agent_provider: "claude_code" }),
      workspace: scopedConfig("workspace", {
        default_agent_provider: "codex",
        claude_account_id: "acc-1",
      }),
      user: scopedConfig("user", {
        default_agent_provider: "codex",
        claude_account_id: "acc-1",
      }),
    });
    expect(screen.getByTestId("runtime-env-default-agent-select")).toHaveValue(
      "claude_code",
    );
    expect(
      screen.queryByTestId("runtime-env-default-model-select-claude_code"),
    ).toBeNull();
    const options = await openThemedSelect(
      "provider-account-assignment-select-claude_code",
    );
    expect(within(options).queryByText(/Personal Max/)).toBeNull();
    await userEvent.click(
      screen.getByTestId(
        "provider-account-assignment-select-claude_code-option-shared",
      ),
    );
    await waitFor(() =>
      expect(updateOrg).toHaveBeenCalledWith({
        settings: { claude_account_id: "shared" },
      }),
    );
    fireEvent.change(screen.getByTestId("runtime-env-default-agent-select"), {
      target: { value: "codex" },
    });
    await waitFor(() =>
      expect(updateOrg).toHaveBeenCalledWith({
        settings: { default_agent_provider: "codex" },
      }),
    );
    expect(updateUser).not.toHaveBeenCalled();
    expect(updateWorkspace).not.toHaveBeenCalled();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("renders one account select per visible agent while the provider-accounts gate is on", async () => {
    gates.providerAccounts = true;
    installAccounts([CLAUDE_ACCOUNT, CODEX_ACCOUNT]);
    renderDefaults();

    const claudeRow = screen.getByTestId(
      "runtime-env-default-model-claude_code",
    );
    const claudeControl = within(claudeRow).getByTestId(
      "provider-account-assignment-claude_code",
    );
    expect(
      within(claudeControl).getByText("Default account"),
    ).toBeInTheDocument();
    expect(within(claudeControl).queryByText("Account")).toBeNull();
    expect(
      within(claudeControl).queryByTestId(
        "provider-account-assignment-source-claude_code",
      ),
    ).toBeNull();
    expect(
      within(claudeControl).queryByTestId(
        "provider-account-assignment-override-claude_code",
      ),
    ).toBeNull();
    const codexRow = screen.getByTestId("runtime-env-default-model-codex");
    expect(
      within(codexRow).getByTestId("provider-account-assignment-select-codex"),
    ).toBeInTheDocument();

    const listbox = await openThemedSelect(
      "provider-account-assignment-select-claude_code",
    );
    const values = within(listbox)
      .getAllByRole("option")
      .map((el) => el.getAttribute("data-testid") ?? "")
      .map((id) => id.replace(/^.*-option-/, ""));
    expect(values).toEqual(["empty", "acc-1"]);
    expect(
      within(listbox).getByTestId(
        "provider-account-assignment-select-claude_code-option-empty",
      ),
    ).toHaveTextContent("No account");
    await userEvent.keyboard("{Escape}");
  });

  it("offers Runtime variables only while a legacy key exists for that family", async () => {
    gates.providerAccounts = true;
    installAccounts([CLAUDE_ACCOUNT, CODEX_ACCOUNT]);
    renderDefaults({
      // A Codex-only legacy key: Codex offers Runtime variables, Claude not.
      workspace: scopedConfig("workspace", {
        OPENAI_BASE_URL: "https://api.openai.com/v1",
      }),
    });

    let listbox = await openThemedSelect(
      "provider-account-assignment-select-codex",
    );
    expect(
      within(listbox).getByTestId(
        "provider-account-assignment-select-codex-option-empty",
      ),
    ).toHaveTextContent("Runtime variables");
    await userEvent.keyboard("{Escape}");

    listbox = await openThemedSelect(
      "provider-account-assignment-select-claude_code",
    );
    expect(
      within(listbox).getByTestId(
        "provider-account-assignment-select-claude_code-option-empty",
      ),
    ).toHaveTextContent("No account");
    await userEvent.keyboard("{Escape}");
  });

  it("persists a selection to the user scope immediately as a minimal delta with an optimistic overlay", async () => {
    gates.providerAccounts = true;
    installAccounts([CLAUDE_ACCOUNT]);
    // Hold the write so the optimistic overlay is observable (the props-fed
    // configs never refetch in this suite; the hook-wired flow is covered in
    // ProviderAccountAssignment.test).
    let settle: () => void = () => undefined;
    const updateUser = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          settle = resolve;
        }),
    );
    renderDefaults({ updateUser });

    await openThemedSelect("provider-account-assignment-select-claude_code");
    await userEvent.click(
      await screen.findByTestId(
        "provider-account-assignment-select-claude_code-option-acc-1",
      ),
    );
    await waitFor(() =>
      expect(updateUser).toHaveBeenCalledWith({
        settings: { claude_account_id: "acc-1" },
      }),
    );
    expect(updateUser).toHaveBeenCalledTimes(1);
    expect(
      screen.getByTestId("provider-account-assignment-select-claude_code"),
    ).toHaveTextContent("Personal Max · You");
    expect(
      screen.queryByTestId("provider-account-assignment-source-claude_code"),
    ).toBeNull();
    // The account write never goes through the custom-variable Save bar.
    expect(screen.getByTestId("runtime-env-save")).toBeDisabled();
    settle();
  });

  it("renders no account select while the provider-accounts gate is off", () => {
    gates.providerAccounts = false;
    renderDefaults({
      user: scopedConfig("user", { claude_account_id: "acc-1" }),
    });
    expect(
      screen.queryByTestId("provider-account-assignment-claude_code"),
    ).toBeNull();
    expect(
      screen.queryByTestId("runtime-env-row-claude_account_id"),
    ).toBeNull();
    // Hidden, not deleted: no write is issued by rendering.
    expect(screen.getByTestId("runtime-env-save")).toBeDisabled();
  });
});

// ---------------------------------------------------------------------------
// Custom-variable editor (D5, D9)
// ---------------------------------------------------------------------------

describe("RuntimeEnvConfig — custom variables", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("shows the effective value with the winning scope badge (workspace overrides org)", () => {
    renderConfig({
      org: scopedConfig("org", { MY_REGION: "eu" }),
      workspace: scopedConfig("workspace", { MY_REGION: "us" }),
      user: scopedConfig("user"),
    });

    const row = screen.getByTestId("runtime-env-row-MY_REGION");
    expect(within(row).getByText("us")).toBeInTheDocument();
    expect(within(row).getByText("Workspace")).toBeInTheDocument();
    expect(within(row).queryByText("Org: eu")).toBeNull();
    expect(within(row).getByText("Workspace: us")).toBeInTheDocument();
    expect(within(row).getByText("You: —")).toBeInTheDocument();
  });

  it("renders the beginning-of-key hint for saved custom secrets per scope and for the winning scope (settings-page D9)", () => {
    renderConfig({
      org: scopedConfig("org", {}, ["GH_TOKEN"], { GH_TOKEN: "ghp_org01234" }),
      workspace: scopedConfig("workspace"),
      user: scopedConfig("user", {}, ["GH_TOKEN"], {
        GH_TOKEN: "ghp_user0123",
      }),
    });

    const row = screen.getByTestId("runtime-env-row-GH_TOKEN");
    expect(
      within(row).getByTestId("runtime-env-hint-GH_TOKEN"),
    ).toHaveTextContent("ghp_user0123…");
    expect(within(row).getByText("configured")).toBeInTheDocument();
    expect(within(row).queryByText("•••• configured")).not.toBeInTheDocument();
    expect(within(row).queryByText("Org: ghp_org01234…")).toBeNull();
    expect(within(row).getByText("Workspace: —")).toBeInTheDocument();
    expect(within(row).getByText("You: ghp_user0123…")).toBeInTheDocument();
  });

  it("keeps the masked display when the server provides no hint (ineligible value)", () => {
    renderConfig({ org: scopedConfig("org", {}, ["GH_TOKEN"]) });

    const row = screen.getByTestId("runtime-env-row-GH_TOKEN");
    expect(within(row).getByText("•••• configured")).toBeInTheDocument();
    expect(within(row).queryByTestId("runtime-env-hint-GH_TOKEN")).toBeNull();
    expect(within(row).queryByText("Org: set")).toBeNull();
  });

  it("never derives a hint from typed secret input before save (settings-page D9)", async () => {
    renderConfig({ org: scopedConfig("org", {}, ["GH_TOKEN"]) });

    fireEvent.click(screen.getByTestId("runtime-env-override-GH_TOKEN"));
    fireEvent.change(
      await screen.findByTestId("runtime-env-secret-input-GH_TOKEN"),
      { target: { value: "ghp_freshly-typed-secret-value" } },
    );
    fireEvent.click(screen.getByTestId("runtime-env-apply-GH_TOKEN"));

    const row = screen.getByTestId("runtime-env-row-GH_TOKEN");
    await waitFor(() =>
      expect(within(row).getByText("You: set")).toBeInTheDocument(),
    );
    expect(within(row).getByText("•••• configured")).toBeInTheDocument();
    expect(within(row).queryByTestId("runtime-env-hint-GH_TOKEN")).toBeNull();
    expect(screen.queryByText(/ghp_freshly-typed/)).not.toBeInTheDocument();
  });

  it("rejects every built-in provider key as a custom variable, pointing at the model and account", () => {
    const alertSpy = vi
      .spyOn(window, "alert")
      .mockImplementation(() => undefined);
    const confirmSpy = vi
      .spyOn(window, "confirm")
      .mockImplementation(() => true);
    renderConfig();

    for (const key of [...HIDDEN_PROVIDER_KEYS, "codex_subscription_enabled"]) {
      const promptSpy = vi.spyOn(window, "prompt").mockReturnValue(key);
      fireEvent.click(screen.getByTestId("runtime-env-add-var"));
      promptSpy.mockRestore();
    }
    expect(alertSpy).toHaveBeenCalledTimes(HIDDEN_PROVIDER_KEYS.length + 1);
    expect(alertSpy).toHaveBeenCalledWith(
      expect.stringContaining(
        "CLAUDE_CODE_OAUTH_TOKEN is a built-in provider runtime variable",
      ),
    );
    expect(alertSpy).toHaveBeenCalledWith(
      expect.stringContaining("derived from the default model and account"),
    );
    expect(confirmSpy).not.toHaveBeenCalled();
    for (const key of HIDDEN_PROVIDER_KEYS) {
      expect(screen.queryByTestId(`runtime-env-row-${key}`)).toBeNull();
    }
  });

  it("keeps a separate duplicate message for existing custom variables", () => {
    const promptSpy = vi
      .spyOn(window, "prompt")
      .mockReturnValue("CUSTOM_TOKEN");
    const alertSpy = vi
      .spyOn(window, "alert")
      .mockImplementation(() => undefined);
    renderConfig({ org: scopedConfig("org", { CUSTOM_TOKEN: "configured" }) });

    fireEvent.click(screen.getByTestId("runtime-env-add-var"));

    expect(promptSpy).toHaveBeenCalled();
    expect(alertSpy).toHaveBeenCalledWith(
      "That custom variable already exists.",
    );
  });

  it("rejects adding retired CODEX_AUTH_JSON as a custom Runtime Config key", () => {
    const prompt = vi
      .spyOn(window, "prompt")
      .mockReturnValue("CODEX_AUTH_JSON");
    const alert = vi.spyOn(window, "alert").mockImplementation(() => undefined);
    renderConfig();

    fireEvent.click(screen.getByText("+ Add custom variable"));

    expect(alert).toHaveBeenCalledWith(
      "CODEX_AUTH_JSON is retired; sign in to Codex in chat using device authorization.",
    );
    expect(screen.queryByTestId("runtime-env-row-CODEX_AUTH_JSON")).toBeNull();
    prompt.mockRestore();
    alert.mockRestore();
  });

  it("does not resurface hidden provider secrets or retired keys as custom rows", () => {
    renderConfig({
      org: scopedConfig("org", {}, ["OPENAI_API_KEY", "CODEX_AUTH_JSON"]),
    });
    expect(screen.queryByTestId("runtime-env-row-OPENAI_API_KEY")).toBeNull();
    expect(screen.queryByTestId("runtime-env-row-CODEX_AUTH_JSON")).toBeNull();
  });

  it("adds a custom variable and builds per-scope Save payloads: minimal settings patch at workspace + setSecrets at user", async () => {
    const updateOrg = vi.fn().mockResolvedValue(undefined);
    const updateUser = vi.fn().mockResolvedValue(undefined);
    const updateWorkspace = vi.fn().mockResolvedValue(undefined);
    renderConfig({
      // Workspace already has an existing setting. The client must NOT re-send it:
      // the server merges (D6), so echoing an untouched key back with its
      // fetched value would clobber a concurrent edit to that sibling.
      workspace: scopedConfig("workspace", {
        EXISTING_KEY: "keep-me",
        MY_REGION: "eu",
      }),
      user: scopedConfig("user", {}, ["GH_TOKEN"]),
      updateOrg,
      updateUser,
      updateWorkspace,
    });

    // --- Stage a non-secret change at WORKSPACE: MY_REGION = us.
    fireEvent.click(screen.getByTestId("runtime-env-override-MY_REGION"));
    fireEvent.change(screen.getByTestId("runtime-env-scope-MY_REGION"), {
      target: { value: "workspace" },
    });
    fireEvent.change(screen.getByTestId("runtime-env-text-input-MY_REGION"), {
      target: { value: "us" },
    });
    fireEvent.click(screen.getByTestId("runtime-env-apply-MY_REGION"));

    // --- Stage a SECRET at USER: GH_TOKEN.
    fireEvent.click(screen.getByTestId("runtime-env-override-GH_TOKEN"));
    fireEvent.change(screen.getByTestId("runtime-env-secret-input-GH_TOKEN"), {
      target: { value: "ghp-secret-123" },
    });
    fireEvent.click(screen.getByTestId("runtime-env-apply-GH_TOKEN"));

    fireEvent.click(screen.getByTestId("runtime-env-save"));

    await waitFor(() => expect(updateWorkspace).toHaveBeenCalledTimes(1));
    expect(updateWorkspace.mock.calls[0][0].settings).toEqual({
      MY_REGION: "us",
    });
    expect(updateWorkspace.mock.calls[0][0].setSecrets).toBeUndefined();

    expect(updateUser).toHaveBeenCalledTimes(1);
    expect(updateUser).toHaveBeenCalledWith(
      expect.objectContaining({ setSecrets: { GH_TOKEN: "ghp-secret-123" } }),
    );
    expect(updateUser.mock.calls[0][0].settings).toBeUndefined();
    expect(updateOrg).not.toHaveBeenCalled();
  });

  it("adds a new custom secret through the prompt/confirm flow and sends it on Save", async () => {
    vi.spyOn(window, "prompt").mockReturnValue("NEW_TOKEN");
    vi.spyOn(window, "confirm").mockReturnValue(true);
    const updateUser = vi.fn().mockResolvedValue(undefined);
    renderConfig({ updateUser });

    fireEvent.click(screen.getByTestId("runtime-env-add-var"));
    const row = await screen.findByTestId("runtime-env-row-NEW_TOKEN");
    expect(within(row).getByText("secret")).toBeInTheDocument();
    fireEvent.change(screen.getByTestId("runtime-env-secret-input-NEW_TOKEN"), {
      target: { value: "tok-0123456789" },
    });
    fireEvent.click(screen.getByTestId("runtime-env-apply-NEW_TOKEN"));
    fireEvent.click(screen.getByTestId("runtime-env-save"));

    await waitFor(() =>
      expect(updateUser).toHaveBeenCalledWith({
        setSecrets: { NEW_TOKEN: "tok-0123456789" },
      }),
    );
  });

  it.each([false, true])(
    "keeps staged workspace changes unsaved after ownership is lost (secret: %s)",
    async (secret) => {
      const updateWorkspace = vi.fn().mockResolvedValue(undefined);
      const updateUser = vi.fn().mockResolvedValue(undefined);
      const props = {
        orgId: "org-1",
        workspaceId: "ws-1",
        org: ok(scopedConfig("org")),
        workspace: ok(
          scopedConfig(
            "workspace",
            secret ? {} : { MY_VALUE: "before" },
            secret ? ["MY_VALUE"] : [],
          ),
        ),
        user: ok(scopedConfig("user", { MY_REGION: "eu" })),
        updateWorkspace,
        updateUser,
        canEditOrg: false,
      };
      function OwnershipHarness() {
        const [canEditWorkspace, setCanEditWorkspace] = useState(true);
        return (
          <>
            <button onClick={() => setCanEditWorkspace((value) => !value)}>
              Toggle workspace ownership
            </button>
            <RuntimeEnvConfig {...props} canEditWorkspace={canEditWorkspace} />
          </>
        );
      }
      renderApp(<OwnershipHarness />);
      fireEvent.click(screen.getByTestId("runtime-env-override-MY_VALUE"));
      fireEvent.change(screen.getByTestId("runtime-env-scope-MY_VALUE"), {
        target: { value: "workspace" },
      });
      fireEvent.change(
        screen.getByTestId(
          `runtime-env-${secret ? "secret" : "text"}-input-MY_VALUE`,
        ),
        { target: { value: "staged-value" } },
      );
      fireEvent.click(screen.getByTestId("runtime-env-apply-MY_VALUE"));
      fireEvent.click(screen.getByTestId("runtime-env-override-MY_REGION"));
      fireEvent.change(screen.getByTestId("runtime-env-text-input-MY_REGION"), {
        target: { value: "us" },
      });
      fireEvent.click(screen.getByTestId("runtime-env-apply-MY_REGION"));

      fireEvent.click(
        screen.getByRole("button", { name: "Toggle workspace ownership" }),
      );
      fireEvent.click(screen.getByTestId("runtime-env-save"));
      expect(
        await screen.findByText(/no longer have permission to save workspace/),
      ).toBeInTheDocument();
      expect(updateWorkspace).not.toHaveBeenCalled();
      expect(updateUser).not.toHaveBeenCalled();
      expect(screen.getByTestId("runtime-env-status")).toHaveTextContent(
        "Unsaved changes",
      );

      // Restoring authority can save the retained value, including a rotated
      // secret whose sentinel did not change while the plaintext was staged.
      fireEvent.click(
        screen.getByRole("button", { name: "Toggle workspace ownership" }),
      );
      fireEvent.click(screen.getByTestId("runtime-env-save"));
      await waitFor(() =>
        expect(updateWorkspace).toHaveBeenCalledWith({
          [secret ? "setSecrets" : "settings"]: { MY_VALUE: "staged-value" },
        }),
      );
      expect(updateUser).toHaveBeenCalledWith({
        settings: { MY_REGION: "us" },
      });
      expect(screen.getByTestId("runtime-env-status")).toHaveTextContent(
        "All changes saved",
      );
    },
  );

  it("requests an explicit delete when a scoped custom var is cleared", async () => {
    const updateOrg = vi.fn().mockResolvedValue(undefined);
    const updateUser = vi.fn().mockResolvedValue(undefined);
    renderConfig({
      context: "organization",
      org: scopedConfig("org", { MY_REGION: "eu", EXISTING_KEY: "keep-me" }),
      updateOrg,
      updateUser,
    });

    fireEvent.click(screen.getByTestId("runtime-env-override-MY_REGION"));
    fireEvent.change(screen.getByTestId("runtime-env-scope-MY_REGION"), {
      target: { value: "org" },
    });
    fireEvent.click(screen.getByTestId("runtime-env-clear-MY_REGION"));
    fireEvent.click(screen.getByTestId("runtime-env-save"));

    await waitFor(() => expect(updateOrg).toHaveBeenCalledTimes(1));
    const payload = updateOrg.mock.calls[0][0];
    expect(payload.removeSettingKeys).toEqual(["MY_REGION"]);
    expect(payload.settings).toBeUndefined();
    expect(updateUser).not.toHaveBeenCalled();
  });

  it("omits untouched sibling keys from the patch so a concurrent edit is not clobbered", async () => {
    const updateOrg = vi.fn().mockResolvedValue(undefined);
    renderConfig({
      context: "organization",
      org: scopedConfig("org", { MY_REGION: "eu", MY_TIER: "gold" }),
      updateOrg,
    });

    fireEvent.click(screen.getByTestId("runtime-env-override-MY_TIER"));
    fireEvent.change(screen.getByTestId("runtime-env-scope-MY_TIER"), {
      target: { value: "org" },
    });
    fireEvent.change(screen.getByTestId("runtime-env-text-input-MY_TIER"), {
      target: { value: "silver" },
    });
    fireEvent.click(screen.getByTestId("runtime-env-apply-MY_TIER"));
    fireEvent.click(screen.getByTestId("runtime-env-save"));

    await waitFor(() => expect(updateOrg).toHaveBeenCalledTimes(1));
    expect(updateOrg.mock.calls[0][0].settings).toEqual({ MY_TIER: "silver" });
    expect(updateOrg.mock.calls[0][0].settings).not.toHaveProperty("MY_REGION");
  });

  it("discard reverts staged changes and disables Save again", () => {
    renderConfig({ org: scopedConfig("org", { MY_REGION: "eu" }) });

    const saveBtn = screen.getByTestId("runtime-env-save");
    expect(saveBtn).toBeDisabled();

    fireEvent.click(screen.getByTestId("runtime-env-override-MY_REGION"));
    fireEvent.change(screen.getByTestId("runtime-env-text-input-MY_REGION"), {
      target: { value: "us" },
    });
    fireEvent.click(screen.getByTestId("runtime-env-apply-MY_REGION"));

    expect(saveBtn).not.toBeDisabled();
    expect(screen.getByTestId("runtime-env-status")).toHaveTextContent(
      "Unsaved changes",
    );

    fireEvent.click(screen.getByTestId("runtime-env-discard"));

    expect(saveBtn).toBeDisabled();
    expect(screen.getByTestId("runtime-env-status")).toHaveTextContent(
      "All changes saved",
    );
  });

  it("does not delete an existing secret when Apply is clicked with no new value", () => {
    renderConfig({
      context: "organization",
      org: scopedConfig("org", {}, ["GH_TOKEN"]),
    });

    const saveBtn = screen.getByTestId("runtime-env-save");
    expect(saveBtn).toBeDisabled();

    fireEvent.click(screen.getByTestId("runtime-env-override-GH_TOKEN"));
    fireEvent.change(screen.getByTestId("runtime-env-scope-GH_TOKEN"), {
      target: { value: "org" },
    });
    fireEvent.click(screen.getByTestId("runtime-env-apply-GH_TOKEN"));

    expect(saveBtn).toBeDisabled();
    expect(screen.getByTestId("runtime-env-status")).toHaveTextContent(
      "All changes saved",
    );
    const row = screen.getByTestId("runtime-env-row-GH_TOKEN");
    expect(within(row).getByText("Org: set")).toBeInTheDocument();
  });

  it("sends a rotated secret even when a value is already configured", async () => {
    const updateOrg = vi.fn().mockResolvedValue(undefined);
    renderConfig({
      context: "organization",
      org: scopedConfig("org", {}, ["GH_TOKEN"]),
      updateOrg,
    });

    fireEvent.click(screen.getByTestId("runtime-env-override-GH_TOKEN"));
    fireEvent.change(screen.getByTestId("runtime-env-scope-GH_TOKEN"), {
      target: { value: "org" },
    });
    fireEvent.change(screen.getByTestId("runtime-env-secret-input-GH_TOKEN"), {
      target: { value: "ghp-rotated-value" },
    });
    fireEvent.click(screen.getByTestId("runtime-env-apply-GH_TOKEN"));
    fireEvent.click(screen.getByTestId("runtime-env-save"));

    await waitFor(() => expect(updateOrg).toHaveBeenCalledTimes(1));
    expect(updateOrg).toHaveBeenCalledWith(
      expect.objectContaining({
        setSecrets: { GH_TOKEN: "ghp-rotated-value" },
      }),
    );
  });
});

// ---------------------------------------------------------------------------
// Default agent select (D6, D8)
// ---------------------------------------------------------------------------

describe("RuntimeEnvConfig — default agent", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("renders the default-agent selector reflecting the persisted value", () => {
    renderConfig({
      user: scopedConfig("user", { default_agent_provider: "codex" }),
    });
    const select = screen.getByTestId(
      "runtime-env-default-agent-select",
    ) as HTMLSelectElement;
    expect(select.value).toBe("codex");
    expect(Array.from(select.options).map((o) => o.text)).toEqual([
      "Auto (first available)",
      "Claude Code",
      "Codex",
    ]);
    expect(
      screen.queryByTestId("runtime-env-row-default_agent_provider"),
    ).toBeNull();
  });

  it("persists a default-agent change to the user scope", async () => {
    const updateUser = vi.fn().mockResolvedValue(undefined);
    renderConfig({ updateUser });
    fireEvent.change(screen.getByTestId("runtime-env-default-agent-select"), {
      target: { value: "codex" },
    });
    await waitFor(() =>
      expect(updateUser).toHaveBeenCalledWith({
        settings: { default_agent_provider: "codex" },
      }),
    );
  });

  it("deletes default_agent_provider explicitly when reset to Auto", async () => {
    const updateUser = vi.fn().mockResolvedValue(undefined);
    renderConfig({
      user: scopedConfig("user", { default_agent_provider: "claude_code" }),
      updateUser,
    });

    const select = screen.getByTestId(
      "runtime-env-default-agent-select",
    ) as HTMLSelectElement;
    expect(select.value).toBe("claude_code");
    fireEvent.change(select, { target: { value: "" } });

    await waitFor(() => expect(updateUser).toHaveBeenCalledTimes(1));
    expect(updateUser).toHaveBeenCalledWith({
      removeSettingKeys: ["default_agent_provider"],
    });
  });

  it("keeps a just-saved custom setting while a later default-agent write is a minimal delta", async () => {
    const updateUser = vi.fn().mockResolvedValue(undefined);
    renderConfig({
      user: scopedConfig("user", { MY_REGION: "eu" }),
      updateUser,
    });

    fireEvent.click(screen.getByTestId("runtime-env-override-MY_REGION"));
    fireEvent.change(screen.getByTestId("runtime-env-text-input-MY_REGION"), {
      target: { value: "us" },
    });
    fireEvent.click(screen.getByTestId("runtime-env-apply-MY_REGION"));
    fireEvent.click(screen.getByTestId("runtime-env-save"));

    await waitFor(() => expect(updateUser).toHaveBeenCalledTimes(1));
    expect(updateUser.mock.calls[0][0]).toEqual(
      expect.objectContaining({ settings: { MY_REGION: "us" } }),
    );

    fireEvent.change(screen.getByTestId("runtime-env-default-agent-select"), {
      target: { value: "claude_code" },
    });
    await waitFor(() => expect(updateUser).toHaveBeenCalledTimes(2));
    // The default-agent write is a MINIMAL delta: MY_REGION survives because
    // the server merges (config_service D6), not because it is re-sent.
    expect(updateUser.mock.calls[1][0]).toEqual({
      settings: { default_agent_provider: "claude_code" },
    });
  });

  it("offers Codex defaults without a Codex deployment marker", async () => {
    const { queryClient } = renderDefaults({
      user: scopedConfig("user", { codex_account_id: "acc-3" }),
    });
    await waitFor(() =>
      expect(queryClient.getQueryData(["instance-info"])).toEqual(
        expect.objectContaining({
          features: expect.not.arrayContaining(["codex_ui"]),
        }),
      ),
    );

    expect(
      screen.getByTestId("runtime-env-default-model-claude_code"),
    ).toBeInTheDocument();
    expect(
      screen.queryByTestId("runtime-env-default-model-codex"),
    ).toBeInTheDocument();
    expect(
      screen.queryByTestId("provider-account-resolution-loading-codex"),
    ).toBeNull();
    const select = screen.getByTestId(
      "runtime-env-default-agent-select",
    ) as HTMLSelectElement;
    const values = Array.from(select.options).map((o) => o.value);
    expect(values).toContain("codex");
    expect(values).toContain("claude_code");
  });

  it("preserves a visible persisted Codex default without rewriting it", async () => {
    const updateUser = vi.fn().mockResolvedValue(undefined);
    const { queryClient } = renderConfig({
      user: scopedConfig("user", { default_agent_provider: "codex" }),
      updateUser,
    });
    await waitFor(() =>
      expect(queryClient.getQueryData(["instance-info"])).toEqual(
        expect.objectContaining({
          features: expect.not.arrayContaining(["codex_ui"]),
        }),
      ),
    );
    const select = screen.getByTestId(
      "runtime-env-default-agent-select",
    ) as HTMLSelectElement;
    const values = Array.from(select.options).map((o) => o.value);
    expect(values).toContain("codex");
    expect(values).toContain("claude_code");
    expect(select.value).toBe("codex");
    // The stored setting is preserved untouched — merely rendering the
    // visible default must not trigger a settings write.
    expect(updateUser).not.toHaveBeenCalled();
  });
});

// ---------------------------------------------------------------------------
// jsdom does not implement window.prompt/confirm/alert by default; the tests
// that exercise them stub explicitly. Provide no-op defaults so unrelated
// interaction code never throws.
// ---------------------------------------------------------------------------
beforeEach(() => {
  // Codex availability does not depend on the advertised capability marker.
  server.use(
    http.get("/api/v1/instance/info", () =>
      HttpResponse.json({
        data: {
          ...MOCK_INSTANCE_INFO,
          features: MOCK_INSTANCE_INFO.features.filter((f) => f !== "codex_ui"),
        },
      }),
    ),
  );
  gates.providerAccounts = false;
  if (!window.prompt) {
    window.prompt = () => null;
  }
  if (!window.confirm) {
    window.confirm = () => false;
  }
  if (!window.alert) {
    window.alert = () => undefined;
  }
});
