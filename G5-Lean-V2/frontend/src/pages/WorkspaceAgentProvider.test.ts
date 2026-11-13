import { describe, expect, it } from "vitest";

import { catalogDefaultChoiceFor } from "../lib/harnessPicker";
import type { HarnessChoice, PickerHarness } from "../lib/types";
import {
  defaultAgentProviderFromConfig,
  getAvailableSyncAgentConfigs,
  getPreferredSyncAgentConfig,
} from "./WorkspacePage";

function choices(
  overrides: Partial<Record<PickerHarness, Partial<HarnessChoice>>> = {},
): Record<PickerHarness, HarnessChoice> {
  return {
    claude_code: {
      ...catalogDefaultChoiceFor("claude_code"),
      ...overrides.claude_code,
    },
    codex: { ...catalogDefaultChoiceFor("codex"), ...overrides.codex },
  };
}

describe("workspace agent launch configuration", () => {
  it("defaults to the shipped Claude configuration", () => {
    expect(getPreferredSyncAgentConfig()).toEqual({
      agentProvider: "claude_code",
      model: undefined,
      modelLabel: "Opus 5.5",
      permissionMode: "bypassPermissions",
      permissionLabel: "Bypass",
      effort: undefined,
    });
  });

  it("uses the saved Claude model and clamps unsupported effort", () => {
    expect(
      getPreferredSyncAgentConfig(
        "claude_code",
        choices({
          claude_code: { model: "claude-sonnet-4-6", effort: "max" },
        }),
      ),
    ).toEqual({
      agentProvider: "claude_code",
      model: "claude-sonnet-4-6",
      modelLabel: "Sonnet 4.6",
      permissionMode: "bypassPermissions",
      permissionLabel: "Bypass",
      effort: "high",
    });
  });

  it("honors the saved Codex default without worker advertisements", () => {
    expect(
      getPreferredSyncAgentConfig(
        "codex",
        choices({ codex: { model: "gpt-5.5", effort: "ultra" } }),
      ),
    ).toEqual({
      agentProvider: "codex",
      model: "gpt-5.5",
      modelLabel: "GPT 5.5",
      permissionMode: "never/danger-full-access",
      permissionLabel: "Full Access",
      effort: "xhigh",
    });
  });

  it("always offers both shipped providers", () => {
    expect(
      getAvailableSyncAgentConfigs().map((config) => config.agentProvider),
    ).toEqual(["claude_code", "codex"]);
  });
});

describe("defaultAgentProviderFromConfig", () => {
  it("accepts only supported provider identifiers", () => {
    expect(
      defaultAgentProviderFromConfig({ default_agent_provider: "codex" }),
    ).toBe("codex");
    expect(
      defaultAgentProviderFromConfig({ default_agent_provider: "claude_code" }),
    ).toBe("claude_code");
    expect(
      defaultAgentProviderFromConfig({ default_agent_provider: "gemini" }),
    ).toBeNull();
    expect(defaultAgentProviderFromConfig(null)).toBeNull();
  });
});
