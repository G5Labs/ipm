// =============================================================================
// G5 Client — Harness Picker Cascade Derivation
// =============================================================================
// Pure derivation of the 4-choice cascade option sets from the generated
// harness catalogs (core D7/D8/D9; plan codex-multi-harness-model-effort
// D-5/D-6): Harness → Model + Mode; Model → Effort. Rendering/consumption
// lands with the picker (Task 2.2). Harness-list derivation stays with the
// flag-gated supported-provider catalog (WorkspacePage
// SUPPORTED_CLOUD_AGENT_PROVIDERS, right-panel D26).

import {
  CLAUDE_CHAT_DEFAULT_MODEL,
  CLAUDE_CHAT_MODEL_OPTIONS,
  CLAUDE_MODEL_OPTIONS,
  claudeLaunchModelFromChatDefault,
  claudeModelLabelFor,
} from "./claudeModels";
import {
  CODEX_CHAT_DEFAULT_MODEL,
  CODEX_CHAT_MODEL_OPTIONS,
  CODEX_MODEL_OPTIONS,
  codexLaunchModelFromChatDefault,
  codexModelLabelFor,
} from "./codexModels";
import {
  CLAUDE_HARNESS_CATALOG,
  CODEX_HARNESS_CATALOG,
} from "./harnessCatalog.gen";

import type { HarnessChoice, PickerHarness, SyncAgentConfig } from "./types";

// Re-exported so picker consumers keep a single import surface.
export type { HarnessChoice, PickerHarness } from "./types";

export interface PickerModelOption {
  value: string;
  label: string;
  description: string;
}

export interface HarnessModeSelection {
  modes: readonly string[];
  defaultMode: string;
}

export interface ModelEffortSelection {
  efforts: readonly string[];
  defaultEffort: string;
  /** False when the model id is not in the harness catalog (custom pin). */
  known: boolean;
}

export function pickerHarnessFor(
  provider: string | null | undefined,
): PickerHarness {
  return provider === "codex" ? "codex" : "claude_code";
}

/** Model group: the harness's chat options (concrete ids only; Claude's follow
 * the catalog's release order, newest first). */
export function chatModelOptionsForHarness(
  harness: string | null | undefined,
): PickerModelOption[] {
  return pickerHarnessFor(harness) === "codex"
    ? CODEX_CHAT_MODEL_OPTIONS
    : CLAUDE_CHAT_MODEL_OPTIONS;
}

/** Mode group: the harness's native mode values + catalog default, verbatim
 * (no cross-harness translation — D24). */
export function modeValuesForHarness(
  harness: string | null | undefined,
): HarnessModeSelection {
  const catalog =
    pickerHarnessFor(harness) === "codex"
      ? CODEX_HARNESS_CATALOG
      : CLAUDE_HARNESS_CATALOG;
  return { modes: catalog.modes, defaultMode: catalog.defaultMode };
}

function claudeEffortEntryFor(
  model: string,
): { efforts: readonly string[]; defaultEffort: string } | undefined {
  const trimmed = model.trim();
  const family = CLAUDE_HARNESS_CATALOG.families.find(
    (candidate) =>
      candidate.openrouter === trimmed ||
      candidate.variants.some((variant) => variant.id === trimmed),
  );
  return family
    ? { efforts: family.efforts, defaultEffort: family.defaultEffort }
    : undefined;
}

function codexEffortEntryFor(
  model: string,
): { efforts: readonly string[]; defaultEffort: string } | undefined {
  const trimmed = model.trim();
  const entry = CODEX_HARNESS_CATALOG.models.find(
    (candidate) => candidate.id === trimmed || candidate.openrouter === trimmed,
  );
  return entry
    ? { efforts: entry.efforts, defaultEffort: entry.defaultEffort }
    : undefined;
}

/** Effort group for the selected model (populated on model choice). Unknown
 * models offer the harness's full ladder with the legacy chat default. */
export function effortSelectionForModel(
  harness: string | null | undefined,
  model: string,
): ModelEffortSelection {
  const codex = pickerHarnessFor(harness) === "codex";
  const entry = codex
    ? codexEffortEntryFor(model)
    : claudeEffortEntryFor(model);
  if (entry) {
    return {
      efforts: entry.efforts,
      defaultEffort: entry.defaultEffort,
      known: true,
    };
  }
  const catalog = codex ? CODEX_HARNESS_CATALOG : CLAUDE_HARNESS_CATALOG;
  return {
    efforts: catalog.effortOrder,
    // Out-of-catalog pins have no defaultEffort to derive; the per-agent
    // unknown-model fallback applies (#1370, codegen-emitted).
    defaultEffort: catalog.unknownModelEffort,
    known: false,
  };
}

/**
 * D-6 clamp on model change: a prior effort outside the newly selected
 * model's set clamps to that model's `defaultEffort` (the catalog default:
 * `high` for Claude, `xhigh` for Codex — chat intents 36046a83/534d50ad).
 * Unknown models pass the prior through unchanged; a blank prior
 * resolves to the selection default.
 */
export function clampEffortForModel(
  harness: string | null | undefined,
  model: string,
  priorEffort: string | null | undefined,
): string {
  const selection = effortSelectionForModel(harness, model);
  const prior = priorEffort?.trim();
  if (!prior) return selection.defaultEffort;
  if (!selection.known) return prior;
  return selection.efforts.includes(prior) ? prior : selection.defaultEffort;
}

/**
 * Catalog-default choice for a harness (plan D-6/D-12): the harness's chat
 * default model, its catalog default mode, and that model's `defaultEffort`.
 */
export function catalogDefaultChoiceFor(
  harness: string | null | undefined,
): HarnessChoice {
  // The bundled default by identity, not by list position: composer options
  // follow release order, so the default is not necessarily listed first.
  const model =
    pickerHarnessFor(harness) === "codex"
      ? CODEX_CHAT_DEFAULT_MODEL
      : CLAUDE_CHAT_DEFAULT_MODEL;
  return {
    model,
    mode: modeValuesForHarness(harness).defaultMode,
    effort: clampEffortForModel(harness, model, null),
  };
}

// ---------------------------------------------------------------------------
// Launch-surface derivations (#1370; shared-components D37; types/core
// D15/D19). Launch modals offer THREE choices — Harness, Model, Effort — and
// display the harness-forced mode read-only (mode is never a launch choice).
// Both harnesses run through these same helpers; per-harness differences are
// data lookups, not call-site branches.
// ---------------------------------------------------------------------------

export interface LaunchModeDisplay {
  mode: string;
  label: string;
}

/** Harness-forced launch mode, mirroring the backend task policy the API and
 * worker enforce (types/core D15/D19) — displayed, never chosen. */
export function launchModeForHarness(
  harness: string | null | undefined,
): LaunchModeDisplay {
  return pickerHarnessFor(harness) === "codex"
    ? { mode: "never/danger-full-access", label: "Full Access" }
    : { mode: "bypassPermissions", label: "Bypass" };
}

/** Launcher model options: the leading `""` configured-default/inherit
 * sentinel plus the harness's catalog lineup. */
export function launcherModelOptionsForHarness(
  harness: string | null | undefined,
): Array<{ value: string; label: string }> {
  const options =
    pickerHarnessFor(harness) === "codex"
      ? CODEX_MODEL_OPTIONS
      : CLAUDE_MODEL_OPTIONS;
  return options.map((option) => ({
    value: option.value,
    label: option.label,
  }));
}

/** Launcher-boundary model resolution: the harness chat default and unknown
 * values map to `undefined` (the `""` configured-default sentinel); other
 * known catalog values pin concretely. */
export function launchModelFromChoice(
  harness: string | null | undefined,
  model: string | null | undefined,
): string | undefined {
  return pickerHarnessFor(harness) === "codex"
    ? codexLaunchModelFromChatDefault(model)
    : claudeLaunchModelFromChatDefault(model);
}

/** Display label for a launcher model value (`""` shows the configured
 * default's label; unknown pins fall back to the raw id). */
export function launchModelLabelFor(
  harness: string | null | undefined,
  model: string | null | undefined,
): string {
  const value = model ?? "";
  return pickerHarnessFor(harness) === "codex"
    ? codexModelLabelFor(value)
    : claudeModelLabelFor(value);
}

/** Launch effort: a concrete model clamps the prior effort to its catalog set
 * (plan D-6); the `""` configured-default/inherit model inherits effort too
 * (`undefined` — the runtime default for whatever model resolves). */
export function launchEffortForModel(
  harness: string | null | undefined,
  model: string | null | undefined,
  priorEffort: string | null | undefined,
): string | undefined {
  const concrete = model?.trim();
  if (!concrete) return undefined;
  return clampEffortForModel(harness, concrete, priorEffort);
}

/** Complete launch config for a harness seeded from its
 * `defaultAgentProviderChoices` entry (#1370 — replaces the interim
 * `claude_code`-only default-model read, plan D-11). */
export function launchAgentConfigFor(
  harness: PickerHarness,
  choice: HarnessChoice | null | undefined,
): SyncAgentConfig {
  const model = launchModelFromChoice(harness, choice?.model);
  const { mode, label } = launchModeForHarness(harness);
  return {
    agentProvider: harness,
    model,
    modelLabel: launchModelLabelFor(harness, model ?? ""),
    permissionMode: mode,
    permissionLabel: label,
    effort: launchEffortForModel(harness, model, choice?.effort),
  };
}
