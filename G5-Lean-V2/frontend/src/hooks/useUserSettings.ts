import { useCallback, useEffect, useRef, useState } from "react";

import { DEFAULT_SETTINGS } from "../lib/defaults";
import { changedPreferences } from "../lib/preferenceTelemetry";
import { recordSettingChange } from "../lib/telemetry";
import { isActionBarStyle, isPhaseBarStyle } from "../lib/workbenchDefaults";
import {
  catalogDefaultChoiceFor,
  clampEffortForModel,
  type HarnessChoice,
  type PickerHarness,
} from "../lib/harnessPicker";
import {
  isChatBubbleLimit,
  isChatBubbleStyle,
  isHomeRowsPerTile,
  type UserSettings,
} from "../lib/types";

export const USER_SETTINGS_STORAGE_KEY = "g5-user-settings";
const USER_SETTINGS_CHANGED_EVENT = "g5-user-settings-changed";

/**
 * Earlier bundled Claude chat defaults. A persisted choice equal to one of them
 * was never changed by the person (the composer stores the default concretely),
 * so it follows the current bundled default: Opus 4.8 and its `[1m]` direct id
 * (the default until 2026-11-09) and the pre-catalog Sonnet 4 id.
 */
const LEGACY_DEFAULT_MODEL_VALUES = new Set([
  "claude-sonnet-4-20250514",
  "anthropic/claude-opus-4.8",
  "claude-opus-4-8[1m]",
]);

function normalizeChoiceModel(harness: PickerHarness, value: unknown): string {
  const fallback = catalogDefaultChoiceFor(harness).model;
  if (typeof value !== "string") return fallback;
  const trimmed = value.trim();
  // Never persist an empty chat model (empty-sentinel experience).
  if (!trimmed) return fallback;
  if (harness === "claude_code" && LEGACY_DEFAULT_MODEL_VALUES.has(trimmed)) {
    return fallback;
  }
  return trimmed;
}

function normalizeStoredChoice(
  harness: PickerHarness,
  value: unknown,
): HarnessChoice {
  const fallback = catalogDefaultChoiceFor(harness);
  const record =
    typeof value === "object" && value !== null
      ? (value as Record<string, unknown>)
      : {};
  const model = normalizeChoiceModel(harness, record.model);
  const mode =
    typeof record.mode === "string" && record.mode.trim()
      ? record.mode.trim()
      : fallback.mode;
  // Clamp the stored effort against the model rather than trusting it: a value
  // persisted before the model's effort tiers changed (or before the model was
  // switched) would otherwise survive indefinitely and reach the server on the
  // first turn. clampEffortForModel handles both branches — a missing prior
  // yields the catalog defaultEffort, an unknown model passes the prior through
  // (D23), and a known model rejects out-of-range values (plan D-6).
  const storedEffort =
    typeof record.effort === "string" && record.effort.trim()
      ? record.effort.trim()
      : null;
  const effort = clampEffortForModel(harness, model, storedEffort);
  return { model, mode, effort };
}

/**
 * Shape-versioned migration (#1342 plan Story 3, Task 3.2): a stored
 * `defaultAgentProviderChoices` map wins; else a legacy `defaultModel` folds
 * into the `claude_code` entry — mode `bypassPermissions` (plan D-12), effort
 * = the migrated model's catalog `defaultEffort` (plan D-6) — and the legacy
 * field is dropped on the next persist. Idempotent on reload; the server-side
 * `default_agent_provider` is a different subsystem and is never touched.
 */
function normalizeStoredChoices(
  parsed: Record<string, unknown>,
): UserSettings["defaultAgentProviderChoices"] {
  const stored = parsed.defaultAgentProviderChoices;
  if (typeof stored === "object" && stored !== null) {
    const record = stored as Record<string, unknown>;
    return {
      claude_code: normalizeStoredChoice("claude_code", record.claude_code),
      codex: normalizeStoredChoice("codex", record.codex),
    };
  }
  if (typeof parsed.defaultModel === "string") {
    const model = normalizeChoiceModel("claude_code", parsed.defaultModel);
    return {
      claude_code: {
        model,
        mode: "bypassPermissions",
        effort: clampEffortForModel("claude_code", model, null),
      },
      codex: catalogDefaultChoiceFor("codex"),
    };
  }
  return DEFAULT_SETTINGS.defaultAgentProviderChoices;
}

const SIDEBAR_DISPLAY_MODES: ReadonlySet<string> = new Set([
  "expanded",
  "collapse-to-rail",
  "expand-on-hover",
  "rail",
]);

/** Unknown/legacy stored values fall back to the default mode (ui g5.ui.sidebar-default). */
function normalizeSidebarDisplayMode(
  value: unknown,
): UserSettings["sidebarDisplayMode"] {
  return typeof value === "string" && SIDEBAR_DISPLAY_MODES.has(value)
    ? (value as UserSettings["sidebarDisplayMode"])
    : DEFAULT_SETTINGS.sidebarDisplayMode;
}

/**
 * Non-boolean stored values fall back to the canonical default (status band:
 * shell D53; center tab strip: workspaces g5.workspace.ui.center-tabs).
 */
function normalizeBooleanSetting(value: unknown, fallback: boolean): boolean {
  return typeof value === "boolean" ? value : fallback;
}

/** Home tile page size: only the offered sizes persist (g5.workbench.home-workspaces). */
function normalizeHomeRowsPerTile(
  value: unknown,
): UserSettings["homeRowsPerTile"] {
  return isHomeRowsPerTile(value) ? value : DEFAULT_SETTINGS.homeRowsPerTile;
}

/**
 * Workbench header styles (workbench g5.workbench.header-styles): unknown or
 * legacy stored values fall back to the defaults.
 */
function normalizePhaseBarStyle(value: unknown): UserSettings["phaseBarStyle"] {
  return isPhaseBarStyle(value) ? value : DEFAULT_SETTINGS.phaseBarStyle;
}

function normalizeActionBarStyle(
  value: unknown,
): UserSettings["actionBarStyle"] {
  return isActionBarStyle(value) ? value : DEFAULT_SETTINGS.actionBarStyle;
}

/**
 * Chat bubble stack settings (ui g5.workspace.ui.chat-bubble-stack): unknown
 * stored values fall back to the defaults.
 */
function normalizeChatBubbleStyle(
  value: unknown,
): UserSettings["chatBubbleStyle"] {
  return isChatBubbleStyle(value) ? value : DEFAULT_SETTINGS.chatBubbleStyle;
}

function normalizeChatBubbleLimit(
  value: unknown,
): UserSettings["chatBubbleLimit"] {
  return isChatBubbleLimit(value) ? value : DEFAULT_SETTINGS.chatBubbleLimit;
}

const SUBSCRIPTION_USAGE_WINDOWS: ReadonlySet<string> = new Set([
  "7d",
  "5h",
  "off",
]);

/**
 * Unknown/legacy stored values fall back to the default window
 * (agent_interaction_ux D8): a value persisted before this setting existed, or
 * a corrupt entry, must never reach the chip as an unknown window.
 */
function normalizeSubscriptionUsageWindow(
  value: unknown,
): UserSettings["subscriptionUsageWindow"] {
  return typeof value === "string" && SUBSCRIPTION_USAGE_WINDOWS.has(value)
    ? (value as UserSettings["subscriptionUsageWindow"])
    : DEFAULT_SETTINGS.subscriptionUsageWindow;
}

/**
 * The persisted settings, normalized; the hook's initial state and the
 * telemetry bridge's preference snapshot source (`App` TelemetryBridge).
 */
export function readUserSettings(): UserSettings {
  try {
    const stored = localStorage.getItem(USER_SETTINGS_STORAGE_KEY);
    if (stored) {
      const parsed = JSON.parse(stored);
      // Drop legacy fields so the next persist completes the migration:
      // `defaultModel` (replaced by per-harness choices) and the notification
      // delivery preferences whose dev-mode settings UI was removed (Lean
      // P8.2); nothing reads them.
      const {
        defaultModel: _legacyDefaultModel,
        notifyTaskComplete: _legacyNotifyTaskComplete,
        notifyAgentBlocked: _legacyNotifyAgentBlocked,
        notifyChannels: _legacyNotifyChannels,
        ...parsedRest
      } = parsed;
      const merged = {
        ...DEFAULT_SETTINGS,
        ...parsedRest,
        editor: { ...DEFAULT_SETTINGS.editor, ...(parsed.editor ?? {}) },
      };
      return {
        ...merged,
        defaultAgentProviderChoices: normalizeStoredChoices(parsed),
        sidebarDisplayMode: normalizeSidebarDisplayMode(
          parsed.sidebarDisplayMode,
        ),
        subscriptionUsageWindow: normalizeSubscriptionUsageWindow(
          parsed.subscriptionUsageWindow,
        ),
        phaseBarStyle: normalizePhaseBarStyle(parsed.phaseBarStyle),
        actionBarStyle: normalizeActionBarStyle(parsed.actionBarStyle),
        showTabBar: normalizeBooleanSetting(
          parsed.showTabBar,
          DEFAULT_SETTINGS.showTabBar,
        ),
        showGuidedJourney: normalizeBooleanSetting(
          parsed.showGuidedJourney,
          DEFAULT_SETTINGS.showGuidedJourney,
        ),
        homeRowsPerTile: normalizeHomeRowsPerTile(parsed.homeRowsPerTile),
        chatBubbleStyle: normalizeChatBubbleStyle(parsed.chatBubbleStyle),
        chatBubbleLimit: normalizeChatBubbleLimit(parsed.chatBubbleLimit),
      };
    }
  } catch (err) {
    console.warn("[UserSettings] Failed to parse stored settings:", err);
  }
  return DEFAULT_SETTINGS;
}

export function useUserSettings(): {
  settings: UserSettings;
  update: (patch: Partial<UserSettings>) => void;
} {
  const [settings, setSettings] = useState<UserSettings>(readUserSettings);
  const settingsRef = useRef(settings);

  useEffect(() => {
    settingsRef.current = settings;
  }, [settings]);

  useEffect(() => {
    document.documentElement.dataset.theme = settings.themeName;
    document.documentElement.dataset.mode = settings.themeMode;
  }, [settings.themeName, settings.themeMode]);

  useEffect(() => {
    function handleStorage(event: StorageEvent) {
      if (event.key === USER_SETTINGS_STORAGE_KEY) {
        const next = readUserSettings();
        settingsRef.current = next;
        setSettings(next);
      }
    }

    function handleLocalChange(event: Event) {
      const next = (event as CustomEvent<UserSettings>).detail;
      if (next) {
        settingsRef.current = next;
        setSettings(next);
      }
    }

    window.addEventListener("storage", handleStorage);
    window.addEventListener(USER_SETTINGS_CHANGED_EVENT, handleLocalChange);
    return () => {
      window.removeEventListener("storage", handleStorage);
      window.removeEventListener(
        USER_SETTINGS_CHANGED_EVENT,
        handleLocalChange,
      );
    };
  }, []);

  const update = useCallback((patch: Partial<UserSettings>) => {
    const prev = settingsRef.current;
    const next: UserSettings = {
      ...prev,
      ...patch,
      editor: patch.editor ? { ...prev.editor, ...patch.editor } : prev.editor,
      defaultAgentProviderChoices:
        patch.defaultAgentProviderChoices !== undefined
          ? {
              claude_code: normalizeStoredChoice(
                "claude_code",
                patch.defaultAgentProviderChoices.claude_code,
              ),
              codex: normalizeStoredChoice(
                "codex",
                patch.defaultAgentProviderChoices.codex,
              ),
            }
          : prev.defaultAgentProviderChoices,
      sidebarDisplayMode:
        patch.sidebarDisplayMode !== undefined
          ? normalizeSidebarDisplayMode(patch.sidebarDisplayMode)
          : prev.sidebarDisplayMode,
      subscriptionUsageWindow:
        patch.subscriptionUsageWindow !== undefined
          ? normalizeSubscriptionUsageWindow(patch.subscriptionUsageWindow)
          : prev.subscriptionUsageWindow,
      phaseBarStyle:
        patch.phaseBarStyle !== undefined
          ? normalizePhaseBarStyle(patch.phaseBarStyle)
          : prev.phaseBarStyle,
      actionBarStyle:
        patch.actionBarStyle !== undefined
          ? normalizeActionBarStyle(patch.actionBarStyle)
          : prev.actionBarStyle,
      showTabBar:
        patch.showTabBar !== undefined
          ? normalizeBooleanSetting(
              patch.showTabBar,
              DEFAULT_SETTINGS.showTabBar,
            )
          : prev.showTabBar,
      showGuidedJourney:
        patch.showGuidedJourney !== undefined
          ? normalizeBooleanSetting(
              patch.showGuidedJourney,
              DEFAULT_SETTINGS.showGuidedJourney,
            )
          : prev.showGuidedJourney,
      homeRowsPerTile:
        patch.homeRowsPerTile !== undefined
          ? normalizeHomeRowsPerTile(patch.homeRowsPerTile)
          : prev.homeRowsPerTile,
      chatBubbleStyle:
        patch.chatBubbleStyle !== undefined
          ? normalizeChatBubbleStyle(patch.chatBubbleStyle)
          : prev.chatBubbleStyle,
      chatBubbleLimit:
        patch.chatBubbleLimit !== undefined
          ? normalizeChatBubbleLimit(patch.chatBubbleLimit)
          : prev.chatBubbleLimit,
    };

    settingsRef.current = next;
    setSettings(next);

    try {
      localStorage.setItem(USER_SETTINGS_STORAGE_KEY, JSON.stringify(next));
    } catch (err) {
      console.warn(
        "[UserSettings] Failed to persist settings to localStorage:",
        err,
      );
    }
    if (patch.themeName !== undefined) {
      document.documentElement.dataset.theme = patch.themeName;
    }
    if (patch.themeMode !== undefined) {
      document.documentElement.dataset.mode = patch.themeMode;
    }
    window.dispatchEvent(
      new CustomEvent(USER_SETTINGS_CHANGED_EVENT, { detail: next }),
    );
    // Value-carrying telemetry for the Settings-page preferences (operations
    // g5.platform.client-telemetry): one `setting` event per preference whose
    // normalized value changed, whichever control changed it. Encoded values
    // only, never the raw patch.
    for (const change of changedPreferences(prev, next)) {
      recordSettingChange(change.key, change.value, change.previous);
    }
  }, []);

  return { settings, update };
}
