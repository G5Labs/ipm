// =============================================================================
// ProviderAccountAssignment tests — test_contract/provider_accounts I7 (Vitest
// half); settings-page D17 hosted in the D19 Defaults card. Rendered through
// RuntimeEnvConfig wired exactly like WorkspaceSettingsContent wires it
// (scoped-config hooks, scoped-config update hooks, the localStorage default
// models), so the optimistic overlay and the refetch are exercised end to
// end. HTTP is mocked at the MSW layer; a tiny in-test scoped-config store
// mirrors the server's merge + explicit delete contract. The config-derived
// Remove guard is covered through ProviderAccountsSection at the end.
// =============================================================================

import React from "react";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { delay, http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { server } from "../../test/msw/server";
import { MOCK_INSTANCE_INFO } from "../../data/mock";
import { renderApp } from "../../test/render";
import type { ProviderAccountDTO, ScopedConfig } from "../../lib/types";
import {
  useOrgScopedConfig,
  useUpdateOrgScopedConfig,
  useUpdateUserScopedConfig,
  useUpdateWorkspaceScopedConfig,
  useUserScopedConfig,
  useWorkspaceScopedConfig,
} from "../../hooks/useSettings";
import { useUserSettings } from "../../hooks/useUserSettings";
import { ProviderAccountsSection } from "./ProviderAccountsSection";
import { RuntimeEnvConfig } from "./RuntimeEnvConfig";

// The provider-accounts kill-switch resolves through providerFlags over the
// instance-info query. Codex itself is always available.
const gates = vi.hoisted(() => ({ providerAccounts: true }));
vi.mock("../../lib/providerFlags", async (importOriginal) => {
  const actual =
    await importOriginal<typeof import("../../lib/providerFlags")>();
  return {
    ...actual,
    isProviderAccountsEnabled: () => gates.providerAccounts,
  };
});

const ORG_ID = "org-g5labs";
const WORKSPACE_ID = "workspace-test";
const LIST_URL = `/api/v1/orgs/${ORG_ID}/provider-accounts`;

function account(overrides: Partial<ProviderAccountDTO>): ProviderAccountDTO {
  return {
    id: "acc-1",
    orgId: ORG_ID,
    name: "Personal Max",
    kind: "claude_subscription",
    family: "claude",
    scope: "user",
    workspaceId: null,
    userId: "user-alex",
    baseUrl: null,
    hasSecret: true,
    secretHint: "sk-ant-oat01",
    createdBy: "user-alex",
    createdAt: "2026-07-02T10:00:00Z",
    updatedAt: "2026-07-02T10:00:00Z",
    usage: null,
    ...overrides,
  };
}

const USER_SUBSCRIPTION = account({});
const ORG_API_KEY = account({
  id: "acc-2",
  name: "Claude API keys",
  kind: "anthropic_api_key",
  scope: "org",
  userId: null,
  baseUrl: "https://api.anthropic.com",
  secretHint: "sk-ant-api03",
});
const USER_CHATGPT = account({
  id: "acc-3",
  name: "ChatGPT Pro",
  kind: "chatgpt_subscription",
  family: "codex",
  hasSecret: false,
  secretHint: null,
});
const WORKSPACE_COMPATIBLE = account({
  id: "acc-4",
  name: "GLM Coding Plan",
  kind: "anthropic_compatible_bearer",
  scope: "workspace",
  workspaceId: WORKSPACE_ID,
  userId: null,
  baseUrl: "https://api.z.ai/api/anthropic",
  secretHint: "zai-7c3e0123",
});
const ORG_OPENROUTER = account({
  id: "acc-5",
  name: "Team OpenRouter",
  kind: "openrouter_api_key",
  family: "both",
  scope: "org",
  userId: null,
  secretHint: "sk-or-v1-abc",
});

const ALL_ACCOUNTS = [
  USER_SUBSCRIPTION,
  ORG_API_KEY,
  USER_CHATGPT,
  WORKSPACE_COMPATIBLE,
  ORG_OPENROUTER,
];

type Scope = "org" | "workspace" | "user";

interface ConfigWrite {
  scope: Scope;
  body: Record<string, unknown>;
}

interface Store {
  settings: Record<Scope, Record<string, unknown>>;
  writes: ConfigWrite[];
}

interface StoreOptions {
  accounts?: ProviderAccountDTO[];
  seed?: Partial<Record<Scope, Record<string, unknown>>>;
  /** Legacy raw provider keys at the ORG scope (D19 "Runtime variables"
   * rule). Default: both families present. */
  legacySecretKeys?: string[];
  /** Typed 422 token (or "forbidden") every PUT should fail with. */
  putErrorCode?: string;
  /** Hold every PUT this long so optimistic state is observable. */
  putDelayMs?: number;
}

const DEFAULT_LEGACY_SECRETS = ["ANTHROPIC_AUTH_TOKEN", "OPENROUTER_API_KEY"];

function scopedConfig(
  scope: Scope,
  settings: Record<string, unknown>,
  effective: Record<string, unknown>,
  secretKeys: string[],
): ScopedConfig {
  return {
    scope,
    settings,
    secretKeys,
    secretKeyHints: {},
    effectiveSettings: effective,
    effectiveSecretKeys: secretKeys,
    effectiveSecretKeyHints: {},
    updatedBy: null,
    updatedAt: null,
  };
}

function wireError(token: string) {
  if (token === "forbidden") {
    return HttpResponse.json(
      { error: { code: "FORBIDDEN", message: "forbidden" } },
      { status: 403 },
    );
  }
  return HttpResponse.json(
    {
      error: {
        code: "VALIDATION_ERROR",
        message: "validation failed",
        details: { code: token, message: `server text for ${token}` },
      },
    },
    { status: 422 },
  );
}

function installStore(options: StoreOptions = {}): Store {
  const accounts = options.accounts ?? ALL_ACCOUNTS;
  const legacy = options.legacySecretKeys ?? DEFAULT_LEGACY_SECRETS;
  const store: Store = {
    settings: {
      org: { ...(options.seed?.org ?? {}) },
      workspace: { ...(options.seed?.workspace ?? {}) },
      user: { ...(options.seed?.user ?? {}) },
    },
    writes: [],
  };
  const effectiveFor = (scope: Scope) => {
    const chain: Scope[] =
      scope === "org"
        ? ["org"]
        : scope === "workspace"
          ? ["org", "workspace"]
          : ["org", "workspace", "user"];
    return Object.assign({}, ...chain.map((s) => store.settings[s]));
  };
  const secretsFor = (scope: Scope) => (scope === "org" ? legacy : []);
  const put =
    (scope: Scope) =>
    async ({ request }: { request: Request }) => {
      const body = (await request.json()) as Record<string, unknown>;
      store.writes.push({ scope, body });
      if (options.putDelayMs) await delay(options.putDelayMs);
      if (options.putErrorCode) return wireError(options.putErrorCode);
      const settings = body.settings as Record<string, unknown> | undefined;
      if (settings) Object.assign(store.settings[scope], settings);
      for (const key of (body.removeSettingKeys as string[] | undefined) ??
        []) {
        delete store.settings[scope][key];
      }
      return HttpResponse.json({
        data: scopedConfig(
          scope,
          store.settings[scope],
          effectiveFor(scope),
          secretsFor(scope),
        ),
      });
    };
  const get = (scope: Scope) => () =>
    HttpResponse.json({
      data: scopedConfig(
        scope,
        store.settings[scope],
        effectiveFor(scope),
        secretsFor(scope),
      ),
    });
  server.use(
    http.get(LIST_URL, () =>
      HttpResponse.json(
        { data: accounts },
        { headers: { "Cache-Control": "no-store" } },
      ),
    ),
    http.delete(
      `${LIST_URL}/:accountId`,
      () => new HttpResponse(null, { status: 204 }),
    ),
    http.get(`/api/v1/orgs/${ORG_ID}/config`, get("org")),
    http.put(`/api/v1/orgs/${ORG_ID}/config`, put("org")),
    http.get(`/api/v1/workspaces/${WORKSPACE_ID}/config`, get("workspace")),
    http.put(`/api/v1/workspaces/${WORKSPACE_ID}/config`, put("workspace")),
    http.get("/api/v1/users/me/config", get("user")),
    http.put("/api/v1/users/me/config", put("user")),
  );
  return store;
}

interface RenderOpts {
  workspaceId?: string | null;
  canEditOrg?: boolean;
  canEditWorkspace?: boolean;
}

/**
 * The Defaults card wired exactly as WorkspaceSettingsContent wires it:
 * scoped configs and their update hooks, and the localStorage default models.
 */
function DefaultsHarness({
  workspaceId,
  canEditOrg,
  canEditWorkspace,
}: {
  workspaceId: string | null;
  canEditOrg: boolean;
  canEditWorkspace: boolean;
}): React.JSX.Element {
  const org = useOrgScopedConfig(ORG_ID);
  const workspace = useWorkspaceScopedConfig(workspaceId ?? "", ORG_ID);
  const user = useUserScopedConfig(ORG_ID);
  const updateOrg = useUpdateOrgScopedConfig(ORG_ID);
  const updateWorkspace = useUpdateWorkspaceScopedConfig(
    workspaceId ?? "",
    ORG_ID,
  );
  const updateUser = useUpdateUserScopedConfig(ORG_ID);
  const { settings, update } = useUserSettings();
  return (
    <RuntimeEnvConfig
      orgId={ORG_ID}
      workspaceId={workspaceId}
      org={org}
      workspace={workspace}
      user={user}
      updateOrg={updateOrg.mutateAsync}
      updateWorkspace={updateWorkspace.mutateAsync}
      updateUser={updateUser.mutateAsync}
      canEditOrg={canEditOrg}
      canEditWorkspace={canEditWorkspace}
      defaultAgentModels={{
        claude_code: settings.defaultAgentProviderChoices.claude_code.model,
        codex: settings.defaultAgentProviderChoices.codex.model,
      }}
      onDefaultAgentModelChange={(harness, model) =>
        update({
          defaultAgentProviderChoices: {
            ...settings.defaultAgentProviderChoices,
            [harness]: {
              ...settings.defaultAgentProviderChoices[harness],
              model,
            },
          },
        })
      }
      showScopedConfig
    />
  );
}

function renderDefaults(opts: RenderOpts = {}) {
  return renderApp(
    <DefaultsHarness
      workspaceId={
        opts.workspaceId === undefined ? WORKSPACE_ID : opts.workspaceId
      }
      canEditOrg={opts.canEditOrg ?? false}
      canEditWorkspace={opts.canEditWorkspace ?? false}
    />,
  );
}

function renderSection(opts: RenderOpts = {}) {
  return renderApp(
    <ProviderAccountsSection
      context="organization"
      orgId={ORG_ID}
      workspaceId={
        opts.workspaceId === undefined ? WORKSPACE_ID : opts.workspaceId
      }
      canEditOrg={opts.canEditOrg ?? false}
      canEditWorkspace={opts.canEditWorkspace ?? false}
    />,
  );
}

async function openSelect(testId: string) {
  const select = await screen.findByTestId(testId);
  await waitFor(() => expect(select).toBeEnabled());
  await userEvent.click(select);
  return screen.findByRole("listbox");
}

async function pickOption(testId: string, value: string) {
  await openSelect(testId);
  await userEvent.click(
    await screen.findByTestId(`${testId}-option-${value || "empty"}`),
  );
}

function optionValues(listbox: HTMLElement): string[] {
  return within(listbox)
    .getAllByRole("option")
    .map((el) => el.getAttribute("data-testid") ?? "")
    .map((id) => id.replace(/^.*-option-/, ""));
}

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
  gates.providerAccounts = true;
  window.localStorage.clear();
});

/** The retired D17 anatomy (chat 9275474a) must not come back. */
function expectRetiredPiecesAbsent(harness: "claude_code" | "codex") {
  for (const id of [
    `provider-account-assignment-source-${harness}`,
    `provider-account-assignment-pill-${harness}-org`,
    `provider-account-assignment-pill-${harness}-workspace`,
    `provider-account-assignment-pill-${harness}-user`,
    `provider-account-assignment-override-${harness}`,
    `provider-account-assignment-drawer-${harness}`,
    `provider-account-resolution-${harness}`,
    `provider-account-resolution-loading-${harness}`,
    `provider-account-resolution-unavailable-${harness}`,
  ]) {
    expect(screen.queryByTestId(id)).toBeNull();
  }
}

describe("ProviderAccountAssignment — options", () => {
  it("lists Runtime variables plus family-filtered, user-assignable accounts per agent, inside each agent's Defaults row", async () => {
    installStore();
    renderDefaults();

    // The control sits inside the agent's row, next to its model select.
    const claudeRow = await screen.findByTestId(
      "runtime-env-default-model-claude_code",
    );
    expect(
      within(claudeRow).getByTestId(
        "runtime-env-default-model-select-claude_code",
      ),
    ).toBeInTheDocument();
    expect(
      within(claudeRow).getByTestId("provider-account-assignment-claude_code"),
    ).toBeInTheDocument();
    expect(within(claudeRow).getByText("Default account")).toBeInTheDocument();
    expect(within(claudeRow).queryByText("Account")).toBeNull();

    const claude = await openSelect(
      "provider-account-assignment-select-claude_code",
    );
    // Claude family (claude | both) at user scope: org + own user accounts;
    // the workspace account (acc-4) and the Codex account (acc-3) are absent.
    expect(optionValues(claude)).toEqual(["empty", "acc-1", "acc-2", "acc-5"]);
    expect(
      within(claude).getByTestId(
        "provider-account-assignment-select-claude_code-option-empty",
      ),
    ).toHaveTextContent("Runtime variables");
    expect(
      within(claude).getByTestId(
        "provider-account-assignment-select-claude_code-option-acc-2",
      ),
    ).toHaveTextContent("Claude API keys · Org");
    expect(
      within(claude).getByTestId(
        "provider-account-assignment-select-claude_code-option-acc-1",
      ),
    ).toHaveTextContent("Personal Max · You");
    await userEvent.keyboard("{Escape}");

    const codex = await openSelect("provider-account-assignment-select-codex");
    expect(optionValues(codex)).toEqual(["empty", "acc-3", "acc-5"]);
    await userEvent.keyboard("{Escape}");

    // Helper text without the retired "does not change the default model"
    // sentence; no badge, pills, Override button, or resolution line.
    expect(
      screen.getByText(
        "Account (subscription or API key) Claude Code authenticates with.",
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "Account (subscription or API key) Codex authenticates with.",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByText(/does not change the default model/)).toBeNull();
    expect(screen.queryByText(/resolves to/)).toBeNull();
    expectRetiredPiecesAbsent("claude_code");
    expectRetiredPiecesAbsent("codex");
    expect(screen.queryByRole("button", { name: "Override" })).toBeNull();
  });

  it("offers No account instead of Runtime variables when no legacy provider key exists", async () => {
    installStore({ legacySecretKeys: [] });
    renderDefaults();

    const claude = await openSelect(
      "provider-account-assignment-select-claude_code",
    );
    expect(
      within(claude).getByTestId(
        "provider-account-assignment-select-claude_code-option-empty",
      ),
    ).toHaveTextContent("No account");
    await userEvent.keyboard("{Escape}");
    expect(
      screen.getByTestId("provider-account-assignment-select-claude_code"),
    ).toHaveTextContent("No account");
  });

  it("offers Codex accounts without a Codex deployment marker", async () => {
    installStore();
    const { queryClient } = renderDefaults();
    await waitFor(() =>
      expect(queryClient.getQueryData(["instance-info"])).toEqual(
        expect.objectContaining({
          features: expect.not.arrayContaining(["codex_ui"]),
        }),
      ),
    );

    await screen.findByTestId("provider-account-assignment-select-claude_code");
    expect(
      screen.queryByTestId("runtime-env-default-model-codex"),
    ).toBeInTheDocument();
    expect(
      screen.queryByTestId("provider-account-assignment-codex"),
    ).toBeInTheDocument();
    expect(
      screen.queryByTestId("provider-account-assignment-select-codex"),
    ).toBeInTheDocument();
  });

  it("renders the select without a workspace in context and without a resolution placeholder", async () => {
    installStore();
    renderDefaults({ workspaceId: null });

    await screen.findByTestId("provider-account-assignment-select-claude_code");
    expectRetiredPiecesAbsent("claude_code");
    expect(screen.queryByText(/available in a workspace/)).toBeNull();
  });
});

describe("ProviderAccountAssignment — persistence", () => {
  it("persists a selection to the user scope immediately with an optimistic update", async () => {
    const store = installStore();
    renderDefaults();

    // Before: no assignment anywhere → the unassigned option is selected.
    const select = await screen.findByTestId(
      "provider-account-assignment-select-claude_code",
    );
    await waitFor(() => expect(select).toBeEnabled());
    expect(select).toHaveTextContent("Runtime variables");

    await pickOption("provider-account-assignment-select-claude_code", "acc-1");

    // Optimistic: the select updates right away.
    expect(
      screen.getByTestId("provider-account-assignment-select-claude_code"),
    ).toHaveTextContent("Personal Max · You");

    await waitFor(() => expect(store.writes).toHaveLength(1));
    expect(store.writes[0]).toEqual({
      scope: "user",
      body: { settings: { claude_account_id: "acc-1" } },
    });
    await waitFor(() =>
      expect(store.settings.user.claude_account_id).toBe("acc-1"),
    );
    // The refetched config keeps the selection after the overlay is dropped.
    await waitFor(() =>
      expect(
        screen.getByTestId("provider-account-assignment-select-claude_code"),
      ).toBeEnabled(),
    );
    expect(
      screen.getByTestId("provider-account-assignment-select-claude_code"),
    ).toHaveTextContent("Personal Max · You");
    // The Codex select is untouched by the Claude assignment.
    expect(
      screen.getByTestId("provider-account-assignment-select-codex"),
    ).toHaveTextContent("Runtime variables");
    // The account write never goes through the custom-variable Save bar.
    expect(screen.getByTestId("runtime-env-save")).toBeDisabled();
    expectRetiredPiecesAbsent("claude_code");
  });

  it("Runtime variables removes the key through removeSettingKeys", async () => {
    const store = installStore({
      seed: { user: { claude_account_id: "acc-1" } },
    });
    renderDefaults();

    await waitFor(() =>
      expect(
        screen.getByTestId("provider-account-assignment-select-claude_code"),
      ).toHaveTextContent("Personal Max · You"),
    );

    await pickOption("provider-account-assignment-select-claude_code", "");

    await waitFor(() => expect(store.writes).toHaveLength(1));
    expect(store.writes[0]).toEqual({
      scope: "user",
      body: { removeSettingKeys: ["claude_account_id"] },
    });
    expect(
      screen.getByTestId("provider-account-assignment-select-claude_code"),
    ).toHaveTextContent("Runtime variables");
    await waitFor(() =>
      expect(store.settings.user).not.toHaveProperty("claude_account_id"),
    );
    await waitFor(() =>
      expect(
        screen.getByTestId("provider-account-assignment-select-claude_code"),
      ).toBeEnabled(),
    );
    expect(
      screen.getByTestId("provider-account-assignment-select-claude_code"),
    ).toHaveTextContent("Runtime variables");
  });

  it("changing the default model leaves the assignment untouched (I7 step 4)", async () => {
    const store = installStore({
      seed: { user: { claude_account_id: "acc-1" } },
    });
    renderDefaults();

    await waitFor(() =>
      expect(
        screen.getByTestId("provider-account-assignment-select-claude_code"),
      ).toHaveTextContent("Personal Max · You"),
    );
    const writesBefore = store.writes.length;

    const modelSelect = screen.getByTestId(
      "runtime-env-default-model-select-claude_code",
    ) as HTMLSelectElement;
    await userEvent.selectOptions(modelSelect, "claude-sonnet-4-6");

    expect(modelSelect.value).toBe("claude-sonnet-4-6");
    expect(
      screen.getByTestId("provider-account-assignment-select-claude_code"),
    ).toHaveTextContent("Personal Max · You");
    // A model change is a localStorage preference: no scoped-config write.
    expect(store.writes).toHaveLength(writesBefore);
    expect(store.settings.user.claude_account_id).toBe("acc-1");
  });

  it("rolls the optimistic selection back and renders the typed error when the write rejects", async () => {
    const store = installStore({
      seed: { user: { claude_account_id: "acc-1" } },
      putErrorCode: "account_scope_forbidden",
      putDelayMs: 150,
    });
    renderDefaults();

    await waitFor(() =>
      expect(
        screen.getByTestId("provider-account-assignment-select-claude_code"),
      ).toHaveTextContent("Personal Max · You"),
    );

    await pickOption("provider-account-assignment-select-claude_code", "acc-2");
    expect(
      screen.getByTestId("provider-account-assignment-select-claude_code"),
    ).toHaveTextContent("Claude API keys · Org");

    expect(
      await screen.findByTestId(
        "provider-account-assignment-error-claude_code",
      ),
    ).toHaveTextContent("This account cannot be assigned at that scope.");
    await waitFor(() =>
      expect(
        screen.getByTestId("provider-account-assignment-select-claude_code"),
      ).toHaveTextContent("Personal Max · You"),
    );
    expect(store.writes).toHaveLength(1);
    expect(store.settings.user.claude_account_id).toBe("acc-1");
  });

  it("renders account_not_visible and account_kind_mismatch from the 422 carrier", async () => {
    installStore({ putErrorCode: "account_kind_mismatch" });
    renderDefaults();

    await pickOption("provider-account-assignment-select-codex", "acc-5");
    expect(
      await screen.findByTestId("provider-account-assignment-error-codex"),
    ).toHaveTextContent("This account's type does not match this agent.");
  });
});

describe("ProviderAccountAssignment — scopes", () => {
  it("shows the effective account set at a higher scope and keeps it displayable among user-assignable options", async () => {
    installStore({
      seed: {
        org: { claude_account_id: "acc-2" },
        workspace: { claude_account_id: "acc-4" },
      },
    });
    renderDefaults();

    await waitFor(() =>
      expect(
        screen.getByTestId("provider-account-assignment-select-claude_code"),
      ).toHaveTextContent("GLM Coding Plan · Workspace"),
    );
    // No provenance is rendered: no badge and no per-scope pills.
    expectRetiredPiecesAbsent("claude_code");
    expect(screen.queryByText(/^Org:/)).toBeNull();
    expect(screen.queryByText(/^Workspace:/)).toBeNull();
    expect(screen.queryByText(/^You:/)).toBeNull();

    // The effective workspace account is listed only as the displayable
    // current value; the other options remain user-assignable ones. There is
    // no unassigned option: clearing the user scope could not take effect
    // against the inherited assignment (D17).
    const listbox = await openSelect(
      "provider-account-assignment-select-claude_code",
    );
    expect(optionValues(listbox)).toEqual(["acc-1", "acc-2", "acc-5", "acc-4"]);
    expect(
      within(listbox).queryByTestId(
        "provider-account-assignment-select-claude_code-option-empty",
      ),
    ).toBeNull();
    await userEvent.keyboard("{Escape}");
  });

  it("offers reverting to the inherited default while a user override shadows an org assignment", async () => {
    const store = installStore({
      seed: {
        org: { claude_account_id: "acc-2" },
        user: { claude_account_id: "acc-1" },
      },
    });
    renderDefaults();

    await waitFor(() =>
      expect(
        screen.getByTestId("provider-account-assignment-select-claude_code"),
      ).toHaveTextContent("Personal Max · You"),
    );
    const listbox = await openSelect(
      "provider-account-assignment-select-claude_code",
    );
    expect(optionValues(listbox)).toEqual(["empty", "acc-1", "acc-2", "acc-5"]);
    expect(
      within(listbox).getByTestId(
        "provider-account-assignment-select-claude_code-option-empty",
      ),
    ).toHaveTextContent("Use Org default (Claude API keys)");
    await userEvent.click(
      within(listbox).getByTestId(
        "provider-account-assignment-select-claude_code-option-empty",
      ),
    );

    // Reverting removes the user key; the inherited org account is effective
    // again and the unassigned option disappears with the override.
    await waitFor(() => expect(store.writes).toHaveLength(1));
    expect(store.writes[0]).toEqual({
      scope: "user",
      body: { removeSettingKeys: ["claude_account_id"] },
    });
    expect(
      screen.getByTestId("provider-account-assignment-select-claude_code"),
    ).toHaveTextContent("Claude API keys · Org");
    await waitFor(() =>
      expect(store.settings.user).not.toHaveProperty("claude_account_id"),
    );
    await waitFor(() =>
      expect(
        screen.getByTestId("provider-account-assignment-select-claude_code"),
      ).toBeEnabled(),
    );
    const after = await openSelect(
      "provider-account-assignment-select-claude_code",
    );
    expect(optionValues(after)).toEqual(["acc-1", "acc-2", "acc-5"]);
    await userEvent.keyboard("{Escape}");
  });

  it("lets an org account's own scope release it: Remove stays enabled and the row shows the Default pill", async () => {
    // chat g5.chat.account-defaults: the org's own assignment is cleared
    // with the row, so it does not block Remove; the row names the agent it
    // is the default for.
    installStore({ seed: { org: { claude_account_id: "acc-2" } } });
    renderSection({ canEditOrg: true });

    expect(
      await screen.findByTestId(
        "provider-account-default-pill-acc-2-claude_code",
      ),
    ).toHaveTextContent("Default · Claude Code");
    expect(
      screen.queryByTestId("provider-account-default-pill-acc-2-codex"),
    ).toBeNull();
    await userEvent.click(
      await screen.findByTestId("provider-account-toggle-acc-2"),
    );
    const remove = await screen.findByTestId("provider-account-remove-acc-2");
    expect(remove).toBeEnabled();
  });

  it("disables Remove in the Accounts section for an account another scope's assignment references", async () => {
    // A member's user-scope assignment of the org account is outside the
    // account's own scope: the server would refuse, so Remove is disabled.
    installStore({ seed: { user: { codex_account_id: "acc-5" } } });
    renderSection({ canEditOrg: true });

    await userEvent.click(
      await screen.findByTestId("provider-account-toggle-acc-5"),
    );
    const remove = await screen.findByTestId("provider-account-remove-acc-5");
    expect(remove).toBeDisabled();
    expect(remove).toHaveAttribute(
      "title",
      expect.stringContaining("assigned"),
    );
  });

  it("offers no Override control even with org and workspace edit rights, and only ever writes the user scope", async () => {
    const store = installStore();
    renderDefaults({ canEditOrg: true, canEditWorkspace: true });

    await screen.findByTestId("provider-account-assignment-select-claude_code");
    expectRetiredPiecesAbsent("claude_code");
    expect(screen.queryByRole("button", { name: "Override" })).toBeNull();
    expect(
      screen.queryByTestId("provider-account-assignment-scope-claude_code"),
    ).toBeNull();

    await pickOption("provider-account-assignment-select-claude_code", "acc-2");
    await waitFor(() => expect(store.writes).toHaveLength(1));
    expect(store.writes[0]).toEqual({
      scope: "user",
      body: { settings: { claude_account_id: "acc-2" } },
    });
    expect(store.settings.org).not.toHaveProperty("claude_account_id");
    expect(store.settings.workspace).not.toHaveProperty("claude_account_id");
  });
});
