import {
  fireEvent,
  render as rtlRender,
  screen,
  within,
} from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactNode } from "react";

import type { AsyncState, ScopedConfig } from "./types";
import { ChatPanel, type ChatPanelProps } from "../components/chat/ChatPanel";
import { RuntimeEnvConfig } from "../components/settings/RuntimeEnvConfig";
import {
  getAvailableSyncAgentConfigs,
  supportedCloudAgentProviders,
} from "../pages/WorkspacePage";
import { renderApp } from "../test/render";

function render(ui: ReactNode) {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });
  return rtlRender(ui, {
    wrapper: ({ children }) => (
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    ),
  });
}

// ---------------------------------------------------------------------------
// Fixtures (minimal copies of the per-surface suites' helpers)
// ---------------------------------------------------------------------------

function chatPanelProps(): ChatPanelProps {
  return {
    session: null,
    messages: [],
    isStreaming: false,
    timelineEntries: [],
    scopeType: "workspace",
    scopeRef: "workspace-1",
    scopeLabel: "Workspace",
    sessions: [],
    selectedAgentProvider: "codex",
    availableAgentProviders: ["claude_code", "codex"],

    onSelectAgentProvider: vi.fn(),
    setupError: null,
    composerDisabledReason: null,
    onSendMessage: vi.fn(),
    onStopStreaming: vi.fn(),
    onForkSession: vi.fn(),
    onSummarize: vi.fn(),
    onCreateSession: vi.fn(),
    onSelectSession: vi.fn(),
  };
}

function scopedConfig(scope: ScopedConfig["scope"]): ScopedConfig {
  return {
    scope,
    settings: {},
    secretKeys: [],
    effectiveSettings: {},
    effectiveSecretKeys: [],
    updatedBy: null,
    updatedAt: null,
  };
}

function ok(data: ScopedConfig): AsyncState<ScopedConfig> {
  return { status: "ok", data };
}

// ---------------------------------------------------------------------------
// Surface 1: provider catalog + sync/create-explainer/create-doc/merge launch
// options (WorkspacePage; workspace-page-shell D-launch derivations)
// ---------------------------------------------------------------------------

describe("provider catalog and launch options by default", () => {
  it("offers Codex in the supported providers and launch configs by default", () => {
    expect(supportedCloudAgentProviders()).toContain("codex");
    const configs = getAvailableSyncAgentConfigs(undefined);
    const codex = configs.find((config) => config.agentProvider === "codex");
    expect(codex).toBeDefined();
    // With no choices map the codex config carries the configured-default
    // model sentinel and the harness-forced launch mode (#1370).
    expect(codex?.model).toBeUndefined();
    expect(codex?.modelLabel).toBe("GPT 5.6 Sol");
    expect(codex?.permissionMode).toBe("never/danger-full-access");
  });

  it("offers Codex with no deployment input", () => {
    expect(supportedCloudAgentProviders()).toEqual(["claude_code", "codex"]);
    const configs = getAvailableSyncAgentConfigs(undefined);
    expect(configs.some((config) => config.agentProvider === "codex")).toBe(
      true,
    );
  });
});

// ---------------------------------------------------------------------------
// Surface 2: settings default-agent dropdown + the Codex Defaults row
// (RuntimeEnvConfig; settings-page D8/D19 — no provider variable rows render
// in either gate state)
// ---------------------------------------------------------------------------

describe("settings surfaces by default", () => {
  function renderConfig() {
    renderApp(
      <RuntimeEnvConfig
        orgId="org-1"
        workspaceId="ws-1"
        org={ok(scopedConfig("org"))}
        workspace={ok(scopedConfig("workspace"))}
        user={ok(scopedConfig("user"))}
        updateOrg={vi.fn().mockResolvedValue(undefined)}
        updateWorkspace={vi.fn().mockResolvedValue(undefined)}
        updateUser={vi.fn().mockResolvedValue(undefined)}
        canEditOrg
        canEditWorkspace
        defaultAgentModels={{
          claude_code: "anthropic/claude-opus-4.8",
          codex: "gpt-5.6-sol",
        }}
        onDefaultAgentModelChange={vi.fn()}
      />,
    );
  }

  it("offers the Codex default-agent option and the Codex Defaults row by default", () => {
    renderConfig();

    const select = screen.getByTestId(
      "runtime-env-default-agent-select",
    ) as HTMLSelectElement;
    expect(Array.from(select.options).map((o) => o.value)).toContain("codex");
    expect(
      screen.getByTestId("runtime-env-default-model-claude_code"),
    ).toBeInTheDocument();
    expect(
      screen.getByTestId("runtime-env-default-model-codex"),
    ).toBeInTheDocument();
    // Provider variable rows never render (D19); subscription auth is
    // task-bound and Settings never renders auth.json.
    expect(
      screen.queryByTestId("runtime-env-row-OPENAI_API_KEY"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("runtime-env-row-OPENROUTER_API_KEY"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("runtime-env-row-CODEX_AUTH_JSON"),
    ).not.toBeInTheDocument();
    expect(screen.queryByTestId("runtime-env-codex-subscription")).toBeNull();
  });

  it("offers Codex defaults without fetching deployment configuration", () => {
    renderConfig();

    const select = screen.getByTestId(
      "runtime-env-default-agent-select",
    ) as HTMLSelectElement;
    expect(Array.from(select.options).map((o) => o.value)).toContain("codex");
    expect(
      screen.getByTestId("runtime-env-default-model-claude_code"),
    ).toBeInTheDocument();
    expect(
      screen.queryByTestId("runtime-env-default-model-codex"),
    ).toBeInTheDocument();
    expect(
      screen.queryByTestId("runtime-env-row-OPENAI_API_KEY"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("runtime-env-row-OPENROUTER_API_KEY"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("runtime-env-row-CODEX_AUTH_JSON"),
    ).not.toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// Surface 3: Codex chat model lineup (ChatPanel; right-panel D24, plan D-8 —
// the full lineup is only offered where the gate is on)
// ---------------------------------------------------------------------------

describe("codex chat model lineup by default", () => {
  it("offers the full catalog lineup by default", () => {
    render(<ChatPanel {...chatPanelProps()} />);

    fireEvent.click(screen.getByTestId("codex-settings-control"));
    fireEvent.click(screen.getByTestId("chat-model-select"));
    const models = within(screen.getByRole("listbox")).getAllByRole("option");
    // D9 defines exactly eight non-hidden offered Codex models; a >= assertion
    // would pass if a hidden model (e.g. codex-auto-review) leaked into the picker.
    expect(models.length).toBe(8);
    expect(
      screen.getByTestId("chat-model-select-option-gpt-6-astra"),
    ).toBeInTheDocument();
    expect(
      screen.getByTestId("chat-model-select-option-gpt-5.6-sol"),
    ).toBeInTheDocument();
    expect(
      screen.getByTestId("chat-model-select-option-gpt-5.5"),
    ).toBeInTheDocument();
  });

  it("offers the full model catalog without a deployment prop", () => {
    render(<ChatPanel {...chatPanelProps()} />);

    fireEvent.click(screen.getByTestId("codex-settings-control"));
    fireEvent.click(screen.getByTestId("chat-model-select"));
    const models = within(screen.getByRole("listbox")).getAllByRole("option");
    expect(models).toHaveLength(8);
    expect(
      screen.getByTestId("chat-model-select-option-gpt-5.6-sol"),
    ).toBeInTheDocument();
    expect(
      screen.queryByTestId("chat-model-select-option-gpt-5.5"),
    ).toBeInTheDocument();
  });
});
