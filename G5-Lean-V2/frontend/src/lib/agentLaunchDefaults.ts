// =============================================================================
// Default agent launch configs
// =============================================================================
// The launch configs a new chat or agent task offers, seeded from the person's
// per-harness choices, and the persisted default provider from scoped config.
// Shared by the workspace page and dialogs that start agent work elsewhere
// (the fork dialog's "Start work").
// =============================================================================

import { launchAgentConfigFor } from "./harnessPicker";
import type {
  ChatAgentProvider,
  HarnessChoice,
  PickerHarness,
  SyncAgentConfig,
} from "./types";

// Launch configs derive entirely from harnessPicker.launchAgentConfigFor
// since #1370 — modes are harness-forced (types/core D15/D19) and model/
// effort seed from the per-harness defaultAgentProviderChoices entries; the
// former SYNC_* per-provider constants are gone with the old builders.

/**
 * The launch config a new chat starts with: the configured default agent
 * when available; otherwise Auto, which prefers the first available agent
 * that can authenticate (an assigned account or raw keys, chat
 * g5.chat.account-defaults) and falls back to the first available agent.
 */
export function getPreferredSyncAgentConfig(
  defaultAgentProvider?: ChatAgentProvider | null,
  defaultAgentProviderChoices?: Record<PickerHarness, HarnessChoice> | null,
  authenticatableAgentProviders?: ReadonlyArray<ChatAgentProvider> | null,
): SyncAgentConfig {
  const options = getAvailableSyncAgentConfigs(defaultAgentProviderChoices);
  const preferred = defaultAgentProvider
    ? options.find((option) => option.agentProvider === defaultAgentProvider)
    : null;
  const authenticatable = authenticatableAgentProviders?.length
    ? options.find((option) =>
        authenticatableAgentProviders.includes(option.agentProvider),
      )
    : null;
  return (
    preferred ??
    authenticatable ??
    options[0] ??
    launchAgentConfigFor(
      "claude_code",
      defaultAgentProviderChoices?.claude_code,
    )
  );
}

// Per-harness launch configs seed uniformly from the harness's
// defaultAgentProviderChoices entry (#1370 — replaces the interim
// claude_code-only default-model read, plan D-11); modes are harness-forced
// (types/core D15/D19).
export function getAvailableSyncAgentConfigs(
  defaultAgentProviderChoices?: Record<PickerHarness, HarnessChoice> | null,
): SyncAgentConfig[] {
  const configs = [
    launchAgentConfigFor(
      "claude_code",
      defaultAgentProviderChoices?.claude_code,
    ),
  ];
  configs.push(
    launchAgentConfigFor("codex", defaultAgentProviderChoices?.codex),
  );
  return configs;
}

/**
 * Read the persisted non-secret `default_agent_provider` (codex_support I11/D3)
 * out of a scoped config's effective settings. Returns a recognized
 * `ChatAgentProvider` or null when unset/unrecognized — callers resolve it
 * against advertised worker capabilities before honoring it.
 */
export function defaultAgentProviderFromConfig(
  effectiveSettings: Record<string, unknown> | null | undefined,
): ChatAgentProvider | null {
  const raw = effectiveSettings?.["default_agent_provider"];
  if (raw === "claude_code" || raw === "codex") {
    return raw;
  }
  return null;
}
