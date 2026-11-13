// #1370 (shared-components D37): the launch control offers THREE choices —
// Agent, Model, Effort — and displays the harness-forced mode read-only
// (types/core D15/D19). Claude and Codex run through the same helpers, so
// every behavior here is asserted for both harnesses where it differs only
// by catalog data.
import { useState } from "react";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import {
  AgentLaunchConfigControls,
  AgentLaunchFieldControls,
  AgentLaunchResolutionStatus,
  agentLaunchPreferencesForConfig,
  reconcileAgentConfig,
} from "./AgentLaunchConfigControls";
import type {
  ResolvedAgentExecutionV1,
  SyncAgentConfig,
} from "../../lib/types";

const BASE_CLAUDE_CONFIG: SyncAgentConfig = {
  agentProvider: "claude_code",
  launchSource: "explicit",
  permissionMode: "bypassPermissions",
  permissionLabel: "Bypass",
  model: undefined,
  modelLabel: "Opus 5.5",
  effort: undefined,
};

const BASE_CODEX_CONFIG: SyncAgentConfig = {
  agentProvider: "codex",
  launchSource: "explicit",
  permissionMode: "never/danger-full-access",
  permissionLabel: "Full Access",
  model: undefined,
  modelLabel: "GPT 5.6 Sol",
  effort: undefined,
};

const CONFIGURED_RESOLUTION: ResolvedAgentExecutionV1 = {
  schemaVersion: 1,
  resolutionId: "resolution-configured",
  configRevision: "config-7",
  catalogVersion: "catalog-3",
  provider: {
    value: "codex",
    provenance: "configured",
    configKey: "workspace.default_agent_provider",
  },
  providerRoute: "openai_direct",
  model: {
    value: "gpt-5.6-sol",
    provenance: "configured",
    configKey: "provider.codex.model",
  },
  upstreamModel: "openai/gpt-5.6-sol",
  permissionMode: {
    value: "never/danger-full-access",
    provenance: "canonical_default",
  },
  effort: { value: "xhigh", provenance: "configured" },
  authMode: "chat_gpt_subscription",
  egressMode: "direct_provider",
  credentialReady: true,
  requiresInteractiveAuth: false,
};

const HAIKU_RESOLUTION: ResolvedAgentExecutionV1 = {
  schemaVersion: 1,
  resolutionId: "resolution-haiku",
  configRevision: "config-7",
  catalogVersion: "catalog-3",
  provider: { value: "claude_code", provenance: "explicit" },
  providerRoute: "anthropic_direct",
  model: { value: "claude-haiku-4-5", provenance: "explicit" },
  upstreamModel: "anthropic/claude-haiku-4-5",
  permissionMode: { value: "bypassPermissions", provenance: "explicit" },
  effort: { value: "none", provenance: "explicit" },
  authMode: "anthropic_api_key",
  egressMode: "direct_provider",
  credentialReady: true,
  requiresInteractiveAuth: false,
};

describe("AgentLaunchConfigControls", () => {
  it("uses the shared themed picker by default", () => {
    render(
      <AgentLaunchConfigControls
        value={BASE_CLAUDE_CONFIG}
        options={[BASE_CLAUDE_CONFIG, BASE_CODEX_CONFIG]}
        onChange={vi.fn()}
        testIdPrefix="shared-default"
      />,
    );

    expect(screen.getByTestId("shared-default-agent-select")).toHaveAttribute(
      "aria-haspopup",
      "listbox",
    );
    expect(screen.getByTestId("shared-default-model-select")).toHaveAttribute(
      "aria-haspopup",
      "listbox",
    );
    expect(screen.getByTestId("shared-default-effort-select")).toHaveAttribute(
      "aria-haspopup",
      "listbox",
    );
  });

  it("keeps the saved Haiku default visible and resolves it as the exact explicit launch", async () => {
    const resolvePreferences = vi
      .fn()
      .mockImplementation((preferences) =>
        Promise.resolve(
          preferences.provider.source === "explicit"
            ? HAIKU_RESOLUTION
            : CONFIGURED_RESOLUTION,
        ),
      );
    const onChange = vi.fn();

    render(
      <AgentLaunchConfigControls
        value={{
          ...BASE_CLAUDE_CONFIG,
          launchSource: "inherit_configured",
          model: "claude-haiku-4-5",
          modelLabel: "Haiku 4.5",
          effort: "high",
        }}
        options={[BASE_CLAUDE_CONFIG, BASE_CODEX_CONFIG]}
        onChange={onChange}
        resolvePreferences={resolvePreferences}
        testIdPrefix="agent"
        selectVariant="native"
      />,
    );

    await waitFor(() => expect(resolvePreferences).toHaveBeenCalledTimes(1));
    expect(screen.queryByTestId("agent-source-select")).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("agent-resolution-summary"),
    ).not.toBeInTheDocument();
    expect(screen.getByTestId("agent-agent-select")).toHaveValue("claude_code");
    expect(screen.getByTestId("agent-model-select")).toHaveValue(
      "claude-haiku-4-5",
    );
    expect(screen.getByTestId("agent-effort-select")).toHaveValue("none");
    expect(screen.getByTestId("agent-effort-select")).toBeDisabled();
    expect(screen.getByTestId("agent-mode-display")).toHaveValue("Bypass");
    expect(onChange).not.toHaveBeenCalled();
    expect(resolvePreferences).toHaveBeenCalledWith(
      expect.objectContaining({
        provider: { source: "explicit", value: "claude_code" },
        model: { source: "explicit", value: "claude-haiku-4-5" },
        permissionMode: { source: "explicit", value: "bypassPermissions" },
        effort: { source: "explicit", value: "none" },
      }),
    );

    expect(onChange).not.toHaveBeenCalled();
  });

  it("editing a supported configured effort makes the displayed launch explicit", () => {
    const onChange = vi.fn();
    render(
      <AgentLaunchConfigControls
        value={{
          ...BASE_CLAUDE_CONFIG,
          launchSource: "inherit_configured",
          model: "claude-sonnet-4-6",
          modelLabel: "Sonnet 4.6",
          effort: "high",
        }}
        onChange={onChange}
        testIdPrefix="agent"
        selectVariant="native"
      />,
    );
    fireEvent.change(screen.getByTestId("agent-effort-select"), {
      target: { value: "low" },
    });
    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({
        launchSource: "explicit",
        model: "claude-sonnet-4-6",
        effort: "low",
      }),
    );
    expect(
      agentLaunchPreferencesForConfig(onChange.mock.calls[0][0]).effort,
    ).toEqual({
      source: "explicit",
      value: "low",
    });
  });

  it("unlocks the direct fields after resolution fails so an edit can re-resolve", async () => {
    const resolvePreferences = vi
      .fn()
      .mockRejectedValueOnce(new Error("No route for this model"))
      .mockResolvedValueOnce({
        ...HAIKU_RESOLUTION,
        resolutionId: "resolution-sonnet",
        model: { value: "claude-sonnet-4-6", provenance: "explicit" },
        upstreamModel: "anthropic/claude-sonnet-4-6",
      } satisfies ResolvedAgentExecutionV1);

    function RecoveryHarness() {
      const [value, setValue] = useState<SyncAgentConfig>({
        ...BASE_CLAUDE_CONFIG,
        model: "claude-haiku-4-5",
        modelLabel: "Haiku 4.5",
        effort: "high",
      });
      return (
        <AgentLaunchConfigControls
          value={value}
          options={[BASE_CLAUDE_CONFIG, BASE_CODEX_CONFIG]}
          onChange={setValue}
          resolvePreferences={resolvePreferences}
          testIdPrefix="agent"
          selectVariant="native"
        />
      );
    }

    render(<RecoveryHarness />);

    expect(
      await screen.findByTestId("agent-resolution-error"),
    ).toHaveTextContent(/No route for this model/i);
    expect(screen.getByTestId("agent-agent-select")).toBeEnabled();
    expect(screen.getByTestId("agent-model-select")).toBeEnabled();
    expect(screen.getByTestId("agent-effort-select")).toBeDisabled();

    fireEvent.change(screen.getByTestId("agent-model-select"), {
      target: { value: "claude-sonnet-4-6" },
    });

    await waitFor(() => expect(resolvePreferences).toHaveBeenCalledTimes(2));
    await waitFor(() =>
      expect(
        screen.queryByTestId("agent-resolution-error"),
      ).not.toBeInTheDocument(),
    );
    expect(screen.getByTestId("agent-model-select")).toHaveValue(
      "claude-sonnet-4-6",
    );
  });

  it("keeps credential warnings visible while launch details are collapsed", async () => {
    const resolvePreferences = vi.fn().mockResolvedValue({
      ...CONFIGURED_RESOLUTION,
      credentialReady: true,
      requiresInteractiveAuth: true,
    });

    render(
      <AgentLaunchConfigControls
        value={{
          ...BASE_CLAUDE_CONFIG,
          launchSource: "inherit_configured",
        }}
        options={[BASE_CLAUDE_CONFIG, BASE_CODEX_CONFIG]}
        onChange={vi.fn()}
        resolvePreferences={resolvePreferences}
        testIdPrefix="agent"
        selectVariant="native"
      />,
    );

    await waitFor(() => expect(resolvePreferences).toHaveBeenCalledTimes(1));
    expect(
      screen.queryByTestId("agent-resolution-summary"),
    ).not.toBeInTheDocument();
    expect(
      screen.getByTestId("agent-resolution-auth-warning"),
    ).toHaveTextContent(/Sign-in will be requested/i);
  });

  it("clears confirmation while a change re-resolves and restores only the fresh result", async () => {
    let releaseSecond: (value: typeof CONFIGURED_RESOLUTION) => void = () => {};
    const onResolutionSelectionChange = vi.fn();
    const initialPreferences = agentLaunchPreferencesForConfig({
      ...BASE_CLAUDE_CONFIG,
      model: "claude-opus-4-8[1m]",
      modelLabel: "Opus 4.8 [1M]",
      effort: "high",
    });
    const nextPreferences = agentLaunchPreferencesForConfig({
      ...BASE_CLAUDE_CONFIG,
      model: "claude-opus-4-8[1m]",
      modelLabel: "Opus 4.8 [1M]",
      effort: "low",
    });
    const resolvePreferences = vi
      .fn()
      .mockResolvedValueOnce(CONFIGURED_RESOLUTION)
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            releaseSecond = resolve;
          }),
      );

    const { rerender } = render(
      <AgentLaunchResolutionStatus
        preferences={initialPreferences}
        resolvePreferences={resolvePreferences}
        onSelectionChange={onResolutionSelectionChange}
        testIdPrefix="agent"
      />,
    );
    await waitFor(() =>
      expect(onResolutionSelectionChange).toHaveBeenLastCalledWith(
        expect.objectContaining({
          confirmedResolutionId: "resolution-configured",
        }),
      ),
    );

    // A preference change invalidates the prior confirmation immediately.
    // Submission must remain blocked until the backend confirms the fresh
    // exact values displayed by the direct picker.
    rerender(
      <AgentLaunchResolutionStatus
        preferences={nextPreferences}
        resolvePreferences={resolvePreferences}
        onSelectionChange={onResolutionSelectionChange}
        testIdPrefix="agent"
      />,
    );
    await waitFor(() => expect(resolvePreferences).toHaveBeenCalledTimes(2));
    expect(onResolutionSelectionChange).toHaveBeenLastCalledWith(null);
    expect(screen.getByTestId("agent-resolution-loading")).toBeInTheDocument();
    expect(screen.queryByTestId("agent-resolution-summary")).toBeNull();

    // The fresh result, rather than the stale resolution, restores submission.
    releaseSecond({
      ...CONFIGURED_RESOLUTION,
      resolutionId: "resolution-low",
      effort: { value: "low", provenance: "explicit" },
    });
    await waitFor(() =>
      expect(onResolutionSelectionChange).toHaveBeenLastCalledWith(
        expect.objectContaining({
          confirmedResolutionId: "resolution-low",
          preferences: expect.objectContaining({
            effort: { source: "explicit", value: "low" },
          }),
        }),
      ),
    );
    expect(screen.queryByTestId("agent-resolution-loading")).toBeNull();
    expect(screen.queryByTestId("agent-resolution-summary")).toBeNull();
  });

  it("renders Agent, Model, and Effort selects plus a read-only mode display", () => {
    render(
      <AgentLaunchConfigControls
        value={BASE_CLAUDE_CONFIG}
        options={[BASE_CLAUDE_CONFIG, BASE_CODEX_CONFIG]}
        onChange={vi.fn()}
        testIdPrefix="agent"
        selectVariant="native"
      />,
    );

    expect(screen.getByTestId("agent-agent-select")).toBeInTheDocument();
    expect(screen.getByTestId("agent-model-select")).toBeInTheDocument();
    expect(screen.getByTestId("agent-effort-select")).toBeInTheDocument();
    const mode = screen.getByTestId("agent-mode-display") as HTMLInputElement;
    expect(mode).toHaveValue("Bypass");
    expect(mode).toHaveAttribute("readonly");
  });

  it("switches the mode display with the harness — mode is never a choice", () => {
    const onChange = vi.fn();
    render(
      <AgentLaunchConfigControls
        value={{ ...BASE_CODEX_CONFIG, launchSource: "explicit" }}
        options={[BASE_CLAUDE_CONFIG, BASE_CODEX_CONFIG]}
        onChange={onChange}
        testIdPrefix="agent"
        selectVariant="native"
      />,
    );

    expect(screen.getByTestId("agent-mode-display")).toHaveValue("Full Access");
    fireEvent.change(screen.getByTestId("agent-agent-select"), {
      target: { value: "claude_code" },
    });
    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({
        agentProvider: "claude_code",
        launchSource: "explicit",
        permissionMode: "bypassPermissions",
        permissionLabel: "Bypass",
      }),
    );
  });

  it("offers the full Codex catalog lineup without a default sentinel row", () => {
    render(
      <AgentLaunchConfigControls
        value={BASE_CODEX_CONFIG}
        onChange={vi.fn()}
        testIdPrefix="agent"
        selectVariant="native"
      />,
    );

    const model = screen.getByTestId("agent-model-select");
    const values = Array.from((model as HTMLSelectElement).options).map(
      (option) => option.value,
    );
    expect(values).not.toContain("");
    expect(values).toEqual([
      "gpt-6.1-sol",
      "gpt-6-astra",
      "gpt-6-sol",
      "gpt-6-luna",
      "gpt-5.6-sol",
      "gpt-5.6-terra",
      "gpt-5.6-luna",
      "gpt-5.5",
    ]);
    // No longer offered at the Codex 0.159.3 pin (g5.provider.catalog.pinned).
    expect(values).not.toContain("gpt-5.4");
    expect(values).not.toContain("gpt-5.4-mini");
    expect(values).not.toContain("gpt-5.3-codex-spark");
  });

  it("keeps Haiku selectable with an honest read-only absence of effort control", () => {
    const config = {
      ...BASE_CLAUDE_CONFIG,
      model: "claude-haiku-4-5",
      effort: "none",
    };
    render(
      <AgentLaunchConfigControls
        value={config}
        onChange={vi.fn()}
        testIdPrefix="haiku"
        selectVariant="native"
      />,
    );
    expect(screen.getByTestId("haiku-effort-select")).toBeDisabled();
    expect(screen.getByTestId("haiku-effort-select")).toHaveValue("none");
    expect(
      screen.getByRole("option", { name: "Not supported" }),
    ).toBeInTheDocument();
    expect(agentLaunchPreferencesForConfig(config).effort).toEqual({
      source: "explicit",
      value: "none",
    });
  });

  it("recomputes Claude effort when the launch model changes", () => {
    const onChange = vi.fn();

    render(
      <AgentLaunchConfigControls
        value={{
          ...BASE_CLAUDE_CONFIG,
          model: "claude-opus-4-8[1m]",
          effort: "xhigh",
        }}
        onChange={onChange}
        testIdPrefix="agent"
        selectVariant="native"
      />,
    );

    fireEvent.change(screen.getByTestId("agent-model-select"), {
      target: { value: "claude-sonnet-4-6" },
    });

    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({
        agentProvider: "claude_code",
        model: "claude-sonnet-4-6",
        modelLabel: "Sonnet 4.6",
        effort: "high",
      }),
    );
  });

  it("clamps Codex effort to the new model's catalog default on model change", () => {
    const onChange = vi.fn();

    render(
      <AgentLaunchConfigControls
        value={{
          ...BASE_CODEX_CONFIG,
          model: "gpt-5.6-sol",
          modelLabel: "GPT 5.6 Sol",
          effort: "ultra",
        }}
        onChange={onChange}
        testIdPrefix="agent"
        selectVariant="native"
      />,
    );

    fireEvent.change(screen.getByTestId("agent-model-select"), {
      target: { value: "gpt-5.5" },
    });

    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({
        agentProvider: "codex",
        model: "gpt-5.5",
        modelLabel: "GPT 5.5",
        effort: "xhigh",
      }),
    );
  });

  it("clears Claude effort when the launch model inherits the configured default", () => {
    const onChange = vi.fn();

    render(
      <AgentLaunchConfigControls
        value={{
          ...BASE_CLAUDE_CONFIG,
          model: "claude-sonnet-4-6",
          modelLabel: "Sonnet 4.6",
          effort: "high",
        }}
        onChange={onChange}
        testIdPrefix="agent"
        selectVariant="native"
      />,
    );

    fireEvent.change(screen.getByTestId("agent-model-select"), {
      target: { value: "" },
    });

    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({
        agentProvider: "claude_code",
        model: undefined,
        modelLabel: "Opus 5.5",
        effort: undefined,
      }),
    );
  });

  it("keeps an in-range prior effort when the model changes back to Opus (D-6 clamp)", () => {
    const onChange = vi.fn();

    render(
      <AgentLaunchConfigControls
        value={{
          ...BASE_CLAUDE_CONFIG,
          model: "claude-sonnet-4-6",
          modelLabel: "Sonnet 4.6",
          effort: "high",
        }}
        onChange={onChange}
        testIdPrefix="agent"
        selectVariant="native"
      />,
    );

    fireEvent.change(screen.getByTestId("agent-model-select"), {
      target: { value: "claude-opus-4-8" },
    });

    // Uniform D-6 clamp (chat parity): `high` is inside Opus's catalog set,
    // so it persists — only out-of-range priors reset to the model default.
    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({
        agentProvider: "claude_code",
        model: "claude-opus-4-8",
        modelLabel: "Opus 4.8",
        effort: "high",
      }),
    );
  });

  it("offers the selected model's effort ladder without an inherit sentinel", () => {
    const { rerender } = render(
      <AgentLaunchConfigControls
        value={{
          ...BASE_CODEX_CONFIG,
          model: "gpt-5.6-sol",
          modelLabel: "GPT 5.6 Sol",
          effort: "ultra",
        }}
        onChange={vi.fn()}
        testIdPrefix="agent"
        selectVariant="native"
      />,
    );

    const effortValues = () =>
      Array.from(
        (screen.getByTestId("agent-effort-select") as HTMLSelectElement)
          .options,
      ).map((option) => option.value);
    expect(effortValues()).toEqual([
      "low",
      "medium",
      "high",
      "xhigh",
      "max",
      "ultra",
    ]);

    rerender(
      <AgentLaunchConfigControls
        value={BASE_CODEX_CONFIG}
        onChange={vi.fn()}
        testIdPrefix="agent"
        selectVariant="native"
      />,
    );
    expect(effortValues()).toEqual([
      "low",
      "medium",
      "high",
      "xhigh",
      "max",
      "ultra",
    ]);
    expect(screen.getByTestId("agent-effort-select")).toBeEnabled();
  });

  it("sends the selected effort through the completed config", () => {
    const onChange = vi.fn();

    render(
      <AgentLaunchConfigControls
        value={{
          ...BASE_CODEX_CONFIG,
          model: "gpt-5.6-sol",
          modelLabel: "GPT 5.6 Sol",
          effort: "ultra",
        }}
        onChange={onChange}
        testIdPrefix="agent"
        selectVariant="native"
      />,
    );

    fireEvent.change(screen.getByTestId("agent-effort-select"), {
      target: { value: "medium" },
    });

    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({
        agentProvider: "codex",
        model: "gpt-5.6-sol",
        effort: "medium",
        permissionMode: "never/danger-full-access",
      }),
    );
  });

  it("keeps provider-specific Claude model ids selectable", () => {
    const onChange = vi.fn();

    render(
      <AgentLaunchConfigControls
        value={{
          ...BASE_CLAUDE_CONFIG,
          model: "anthropic/claude-sonnet-4.6",
          modelLabel: "anthropic/claude-sonnet-4.6",
          effort: "high",
        }}
        onChange={onChange}
        testIdPrefix="agent"
        selectVariant="native"
      />,
    );

    expect(screen.getByTestId("agent-model-select")).toHaveValue(
      "anthropic/claude-sonnet-4.6",
    );
  });
});

// Launch modals reconcile their selection through this helper when the
// preferred config or option set changes mid-open.
describe("reconcileAgentConfig", () => {
  it("adopts the recomputed preferred config while the user has not picked", () => {
    const result = reconcileAgentConfig(
      BASE_CLAUDE_CONFIG,
      false,
      BASE_CODEX_CONFIG,
      [BASE_CLAUDE_CONFIG, BASE_CODEX_CONFIG],
    );
    expect(result.agentProvider).toBe("codex");
  });

  it("returns the same reference when the untouched seed is content-identical", () => {
    const previous = reconcileAgentConfig(
      BASE_CLAUDE_CONFIG,
      false,
      BASE_CLAUDE_CONFIG,
      [BASE_CLAUDE_CONFIG],
    );
    const next = reconcileAgentConfig(previous, false, BASE_CLAUDE_CONFIG, [
      BASE_CLAUDE_CONFIG,
    ]);
    expect(next).toBe(previous);
  });

  it("keeps an explicit user pick while its provider remains offered", () => {
    const result = reconcileAgentConfig(
      BASE_CLAUDE_CONFIG,
      true,
      BASE_CODEX_CONFIG,
      [BASE_CLAUDE_CONFIG, BASE_CODEX_CONFIG],
    );
    expect(result).toBe(BASE_CLAUDE_CONFIG);
  });

  it("re-seeds an explicit pick whose provider is no longer offered", () => {
    const result = reconcileAgentConfig(
      BASE_CODEX_CONFIG,
      true,
      BASE_CLAUDE_CONFIG,
      [BASE_CLAUDE_CONFIG],
    );
    expect(result.agentProvider).toBe("claude_code");
  });
});

// Optional Account field (shared-components D37; right-panel
// session_provider_controls D30; test_contract/provider_accounts I8).
describe("AgentLaunchFieldControls Account field", () => {
  const fieldProps = {
    provider: "claude_code",
    providerOptions: [{ value: "claude_code", label: "Claude Code" }],
    model: "claude-opus-4-8[1m]",
    modelOptions: [{ value: "claude-opus-4-8[1m]", label: "Opus 4.8 [1M]" }],
    effort: "high",
    effortOptions: [{ value: "high", label: "High" }],
    modeLabel: "Bypass",
    onProviderChange: vi.fn(),
    onModelChange: vi.fn(),
    onEffortChange: vi.fn(),
    testIdPrefix: "task",
    testId: "task-fields",
  };

  it("renders no Account field when the host passes no accountOptions (task modals in v1)", () => {
    render(<AgentLaunchFieldControls {...fieldProps} />);
    expect(screen.queryByTestId("task-account-select")).not.toBeInTheDocument();
    expect(screen.queryByText("Account")).not.toBeInTheDocument();
    expect(screen.getByTestId("task-fields")).toHaveClass("sm:grid-cols-4");
  });

  it("renders the Account select between Agent and Model with the host's options and reports changes", () => {
    const onAccountChange = vi.fn();
    render(
      <AgentLaunchFieldControls
        {...fieldProps}
        accountOptions={[
          { value: "runtime_config", label: "Runtime variables" },
          {
            value: "acct-1",
            label: "Team subscription",
            assistiveDescription: "Claude subscription · Org",
          },
        ]}
        account="acct-1"
        onAccountChange={onAccountChange}
      />,
    );
    const labels = Array.from(
      document.querySelectorAll("label > span:first-child"),
    ).map((node) => node.textContent);
    expect(labels).toEqual(["Agent", "Account", "Model", "Effort", "Mode"]);
    // All five fields share one row in the wide layout.
    expect(screen.getByTestId("task-fields")).toHaveClass("sm:grid-cols-5");
    expect(screen.getByTestId("task-account-select")).toHaveTextContent(
      "Team subscription",
    );
    fireEvent.click(screen.getByTestId("task-account-select"));
    fireEvent.click(screen.getByRole("option", { name: /Runtime variables/ }));
    expect(onAccountChange).toHaveBeenCalledWith("runtime_config");
  });

  it("disables only the Account field through accountDisabled", () => {
    render(
      <AgentLaunchFieldControls
        {...fieldProps}
        accountOptions={[
          { value: "runtime_config", label: "Runtime variables" },
        ]}
        account="runtime_config"
        accountDisabled
        accountTestId="custom-account"
      />,
    );
    expect(screen.getByTestId("custom-account")).toBeDisabled();
    expect(screen.getByTestId("task-model-select")).toBeEnabled();
  });
});

// A host that starts a chat (the fork dialog, #987) hands the Account field
// to the config control; the shown value resolves and confirms explicitly.
describe("AgentLaunchConfigControls Account field", () => {
  const accountOptions = [
    { value: "runtime_config", label: "Runtime variables" },
    { value: "acct-1", label: "Team subscription" },
  ];

  it("renders the host's Account field and resolves the shown account explicitly", async () => {
    const resolvePreferences = vi.fn().mockResolvedValue(HAIKU_RESOLUTION);
    const onAccountChange = vi.fn();
    const onSelection = vi.fn();
    render(
      <AgentLaunchConfigControls
        value={BASE_CLAUDE_CONFIG}
        options={[BASE_CLAUDE_CONFIG, BASE_CODEX_CONFIG]}
        onChange={vi.fn()}
        resolvePreferences={resolvePreferences}
        onResolutionSelectionChange={onSelection}
        accountOptions={accountOptions}
        account="acct-1"
        onAccountChange={onAccountChange}
        testIdPrefix="agent"
        selectVariant="native"
      />,
    );
    await waitFor(() => expect(resolvePreferences).toHaveBeenCalledTimes(1));
    expect(screen.getByTestId("agent-account-select")).toHaveValue("acct-1");
    expect(resolvePreferences).toHaveBeenCalledWith(
      expect.objectContaining({
        provider: { source: "explicit", value: "claude_code" },
        account: {
          source: "explicit",
          value: { kind: "account", accountId: "acct-1" },
        },
      }),
    );
    await waitFor(() =>
      expect(onSelection).toHaveBeenLastCalledWith(
        expect.objectContaining({
          confirmedResolutionId: HAIKU_RESOLUTION.resolutionId,
          preferences: expect.objectContaining({
            account: {
              source: "explicit",
              value: { kind: "account", accountId: "acct-1" },
            },
          }),
        }),
      ),
    );

    fireEvent.change(screen.getByTestId("agent-account-select"), {
      target: { value: "runtime_config" },
    });
    expect(onAccountChange).toHaveBeenCalledWith("runtime_config");
  });

  it("resolves without an account preference when the host shows no field", async () => {
    const resolvePreferences = vi.fn().mockResolvedValue(HAIKU_RESOLUTION);
    render(
      <AgentLaunchConfigControls
        value={BASE_CLAUDE_CONFIG}
        options={[BASE_CLAUDE_CONFIG]}
        onChange={vi.fn()}
        resolvePreferences={resolvePreferences}
        testIdPrefix="agent"
        selectVariant="native"
      />,
    );
    await waitFor(() => expect(resolvePreferences).toHaveBeenCalledTimes(1));
    expect(
      screen.queryByTestId("agent-account-select"),
    ).not.toBeInTheDocument();
    expect(resolvePreferences.mock.calls[0][0]).not.toHaveProperty("account");
  });
});

describe("agentLaunchPreferencesForConfig account extras", () => {
  it("omits the account preference unless the host displayed the field", () => {
    expect(
      agentLaunchPreferencesForConfig(BASE_CLAUDE_CONFIG).account,
    ).toBeUndefined();
    expect(
      agentLaunchPreferencesForConfig({
        ...BASE_CLAUDE_CONFIG,
        launchSource: "inherit_configured",
      }).account,
    ).toBeUndefined();
  });

  it("carries the displayed account as an explicit preference for every launch source", () => {
    expect(
      agentLaunchPreferencesForConfig(BASE_CLAUDE_CONFIG, {
        account: { kind: "account", accountId: "acct-1" },
      }).account,
    ).toEqual({
      source: "explicit",
      value: { kind: "account", accountId: "acct-1" },
    });
    expect(
      agentLaunchPreferencesForConfig(
        { ...BASE_CODEX_CONFIG, launchSource: "inherit_configured" },
        { account: { kind: "runtime_config" } },
      ),
    ).toEqual(
      expect.objectContaining({
        provider: { source: "inherit_configured" },
        account: { source: "explicit", value: { kind: "runtime_config" } },
      }),
    );
  });
});
