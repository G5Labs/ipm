// Account field values and the account-defaults rules (chat
// g5.chat.account-defaults): when "Runtime variables" is a real choice, what
// the unassigned choice reads otherwise, and which agents Auto may prefer.
import { describe, expect, it } from "vitest";

import type { ScopedConfig } from "./types";
import {
  NO_ACCOUNT_LABEL,
  RUNTIME_CONFIG_ACCOUNT_LABEL,
  RUNTIME_CONFIG_ACCOUNT_VALUE,
  authenticatableAgentProviders,
  legacyProviderKeysPresent,
  runtimeVariablesAvailableFrom,
  unassignedAccountOption,
} from "./providerAccountSelection";

function config(
  secretKeys: string[] = [],
  settings: Record<string, unknown> = {},
): ScopedConfig {
  return {
    scope: "org",
    settings,
    secretKeys,
    secretKeyHints: {},
    effectiveSettings: settings,
    effectiveSecretKeys: secretKeys,
    effectiveSecretKeyHints: {},
    updatedBy: null,
    updatedAt: null,
  };
}

describe("legacyProviderKeysPresent", () => {
  it("counts a raw credential in any scope's secret channel for its family only", () => {
    const configs = [null, config(["ANTHROPIC_API_KEY"]), config()];
    expect(legacyProviderKeysPresent("claude_code", configs)).toBe(true);
    expect(legacyProviderKeysPresent("codex", configs)).toBe(false);
    expect(runtimeVariablesAvailableFrom(configs)).toEqual({
      claude_code: true,
      codex: false,
    });
  });

  it("counts a base URL from the settings map but never a credential placed there", () => {
    expect(
      legacyProviderKeysPresent("codex", [
        config([], { OPENAI_BASE_URL: "https://gateway.example" }),
      ]),
    ).toBe(true);
    expect(
      legacyProviderKeysPresent("codex", [
        config([], { OPENAI_API_KEY: "sk-leaked-into-settings" }),
      ]),
    ).toBe(false);
    expect(
      legacyProviderKeysPresent("claude_code", [
        config([], { ANTHROPIC_BASE_URL: "  " }),
      ]),
    ).toBe(false);
  });
});

describe("unassignedAccountOption", () => {
  it("offers Runtime variables while raw keys exist, whatever is selected", () => {
    expect(unassignedAccountOption(true, "acct-1")).toEqual({
      value: RUNTIME_CONFIG_ACCOUNT_VALUE,
      label: RUNTIME_CONFIG_ACCOUNT_LABEL,
    });
  });

  it("reads No account without raw keys, and only while nothing is chosen", () => {
    expect(
      unassignedAccountOption(false, RUNTIME_CONFIG_ACCOUNT_VALUE),
    ).toEqual({
      value: RUNTIME_CONFIG_ACCOUNT_VALUE,
      label: NO_ACCOUNT_LABEL,
    });
    expect(unassignedAccountOption(false, "")).toEqual({
      value: RUNTIME_CONFIG_ACCOUNT_VALUE,
      label: NO_ACCOUNT_LABEL,
    });
    expect(unassignedAccountOption(false, "acct-1")).toBeNull();
  });
});

describe("authenticatableAgentProviders", () => {
  it("names the agents with an assigned account or raw keys, in harness order", () => {
    expect(
      authenticatableAgentProviders(
        { claude_code: "", codex: "acct-codex" },
        { claude_code: false, codex: false },
      ),
    ).toEqual(["codex"]);
    expect(
      authenticatableAgentProviders(
        { claude_code: "", codex: "" },
        { claude_code: true, codex: false },
      ),
    ).toEqual(["claude_code"]);
    expect(
      authenticatableAgentProviders(
        { claude_code: "acct-a", codex: "acct-b" },
        null,
      ),
    ).toEqual(["claude_code", "codex"]);
    expect(authenticatableAgentProviders(undefined, undefined)).toEqual([]);
  });
});
