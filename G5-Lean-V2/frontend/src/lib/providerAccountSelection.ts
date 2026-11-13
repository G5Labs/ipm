// =============================================================================
// G5 Client — Account field values
// =============================================================================
// The chat composer and the fork dialog's "Start work" controls share one
// Account field (g5.chat.configuration, g5.chat.accounts): a provider-account
// id, or the "Runtime variables" sentinel that leaves the launch on the raw
// credential keys of Runtime Config. These helpers map the field's string
// value to the versioned launch contract and back, and keep the family
// filter in one place.
// =============================================================================

import type {
  AgentAccountSelectionV1,
  ChatAgentProvider,
  PickerHarness,
  ProviderAccountDTO,
  ScopedConfig,
} from "./types";

/** Field value meaning "no named account: use Runtime Config's keys". */
export const RUNTIME_CONFIG_ACCOUNT_VALUE = "runtime_config";
export const RUNTIME_CONFIG_ACCOUNT_LABEL = "Runtime variables";
/**
 * Label of the unassigned choice when no raw credential key exists for the
 * agent's family (chat g5.chat.account-defaults): there is nothing for the
 * launch to run on, so the choice names the absence rather than a route.
 */
export const NO_ACCOUNT_LABEL = "No account";

// ---------------------------------------------------------------------------
// Raw credential keys (chat g5.chat.account-defaults): the "Runtime
// variables" choice is offered only while the effective config still carries
// a raw credential or route key the resolver would use for that family with
// no account chosen.
// ---------------------------------------------------------------------------

const LEGACY_PROVIDER_KEYS: Record<
  PickerHarness,
  { secrets: readonly string[]; settings: readonly string[] }
> = {
  claude_code: {
    secrets: [
      "ANTHROPIC_AUTH_TOKEN",
      "CLAUDE_CODE_OAUTH_TOKEN",
      "ANTHROPIC_API_KEY",
    ],
    settings: ["ANTHROPIC_BASE_URL"],
  },
  codex: {
    secrets: ["OPENAI_API_KEY", "OPENROUTER_API_KEY"],
    settings: ["OPENAI_BASE_URL"],
  },
};

/**
 * Whether any scope in context carries a raw credential/route key for the
 * harness's family. Computed as the union of each scope's OWN values: a key
 * set at any scope is part of the effective merge (config_service D6), so
 * the union equals effective presence. Credentials count only from the
 * secret channel (a credential in the readable settings map is never honored
 * by the resolver); base URLs count from the settings map.
 */
export function legacyProviderKeysPresent(
  harness: PickerHarness,
  configs: ReadonlyArray<ScopedConfig | null | undefined>,
): boolean {
  const { secrets, settings } = LEGACY_PROVIDER_KEYS[harness];
  return configs.some((cfg) => {
    if (!cfg) return false;
    const secretKeys = Array.isArray(cfg.secretKeys) ? cfg.secretKeys : [];
    if (secrets.some((key) => secretKeys.includes(key))) return true;
    return settings.some((key) => {
      const value = cfg.settings?.[key];
      return typeof value === "string" && value.trim() !== "";
    });
  });
}

/** Raw-key presence per harness for the scopes in context. */
export function runtimeVariablesAvailableFrom(
  configs: ReadonlyArray<ScopedConfig | null | undefined>,
): Record<PickerHarness, boolean> {
  return {
    claude_code: legacyProviderKeysPresent("claude_code", configs),
    codex: legacyProviderKeysPresent("codex", configs),
  };
}

/**
 * The unassigned option of an Account field (chat g5.chat.account-defaults):
 * "Runtime variables" while raw keys exist for the harness; otherwise "No
 * account", offered only while it is the current value, since with no raw
 * key there is nothing to fall back to once an account is chosen.
 */
export function unassignedAccountOption(
  runtimeVariablesAvailable: boolean,
  currentValue: string,
): { value: string; label: string } | null {
  if (runtimeVariablesAvailable) {
    return {
      value: RUNTIME_CONFIG_ACCOUNT_VALUE,
      label: RUNTIME_CONFIG_ACCOUNT_LABEL,
    };
  }
  if (currentValue === RUNTIME_CONFIG_ACCOUNT_VALUE || currentValue === "") {
    return { value: RUNTIME_CONFIG_ACCOUNT_VALUE, label: NO_ACCOUNT_LABEL };
  }
  return null;
}

/**
 * Agents that can authenticate as configured: an effective account
 * assignment or raw keys for the family (chat g5.chat.account-defaults, the
 * Auto default-agent rule).
 */
export function authenticatableAgentProviders(
  assignments: Record<PickerHarness, string> | null | undefined,
  runtimeVariablesAvailable: Record<PickerHarness, boolean> | null | undefined,
): ChatAgentProvider[] {
  const harnesses: PickerHarness[] = ["claude_code", "codex"];
  return harnesses.filter(
    (harness) =>
      Boolean(assignments?.[harness]) ||
      Boolean(runtimeVariablesAvailable?.[harness]),
  );
}

/** The field value for a launch selection; `undefined` for none. */
export function accountSelectionValue(
  selection: AgentAccountSelectionV1 | null | undefined,
): string | undefined {
  if (!selection) return undefined;
  return selection.kind === "account"
    ? selection.accountId
    : RUNTIME_CONFIG_ACCOUNT_VALUE;
}

/** The launch selection a field value stands for. */
export function accountSelectionFromValue(
  value: string,
): AgentAccountSelectionV1 {
  return value === RUNTIME_CONFIG_ACCOUNT_VALUE
    ? { kind: "runtime_config" }
    : { kind: "account", accountId: value };
}

/** Family filter for the Account field (contract D14): `both` fits either harness. */
export function accountMatchesHarness(
  account: ProviderAccountDTO,
  provider: ChatAgentProvider,
): boolean {
  if (account.family === "both") return true;
  return provider === "codex"
    ? account.family === "codex"
    : account.family === "claude";
}
