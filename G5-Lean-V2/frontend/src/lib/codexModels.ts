// =============================================================================
// G5 Client — Shared Codex Model Lineup
// =============================================================================
// Hand-written helpers over generated catalog data (harnessCatalog.gen.ts;
// Rust SoT: g5-common/src/codex_models.rs, core D9/D8), mirroring
// claudeModels.ts. Chat options carry concrete model ids — the "" inherit
// sentinel exists only in the launcher list (empty-sentinel experience) and
// never enters the catalog.
// =============================================================================

import { CODEX_HARNESS_CATALOG } from "./harnessCatalog.gen";

/** Concrete direct-route id for ordinary Codex chat's bundled default. */
export const CODEX_CHAT_DEFAULT_MODEL =
  CODEX_HARNESS_CATALOG.defaultModelDirect;

/** OpenRouter slug form of the bundled default. */
export const CODEX_OPENROUTER_CHAT_DEFAULT_MODEL =
  CODEX_HARNESS_CATALOG.defaultModelOpenrouter;

const DEFAULT_MODEL_ENTRY = CODEX_HARNESS_CATALOG.models.find(
  (model) => model.id === CODEX_HARNESS_CATALOG.defaultModelDirect,
);

/** Label for ordinary Codex chat's bundled default. */
export const CODEX_CHAT_DEFAULT_MODEL_LABEL =
  DEFAULT_MODEL_ENTRY?.label ?? CODEX_CHAT_DEFAULT_MODEL;

/** Label for inherit/configured-default entries that resolve to Codex's default. */
export const CODEX_DEFAULT_MODEL_LABEL = CODEX_CHAT_DEFAULT_MODEL_LABEL;

/**
 * Fallback effort for models the catalog does not know (#1370, core D9) —
 * out-of-catalog pins have no `defaultEffort` to derive. Per-agent by user
 * decision and codegen-emitted so this value cannot drift from g5-common.
 */
export const CODEX_UNKNOWN_MODEL_EFFORT =
  CODEX_HARNESS_CATALOG.unknownModelEffort;

export interface CodexModelOption {
  value: string;
  label: string;
  description: string;
}

const OFFERED_MODELS = CODEX_HARNESS_CATALOG.models.filter(
  (model) => model.offered,
);

export const CODEX_CONCRETE_MODEL_OPTIONS: CodexModelOption[] =
  OFFERED_MODELS.map((model) => ({
    value: model.id,
    label: model.label,
    description: model.id,
  }));

export const CODEX_OPENROUTER_MODEL_OPTIONS: Array<[string, string]> =
  OFFERED_MODELS.filter((model) => model.openrouter !== null).map(
    (model): [string, string] => [
      model.openrouter as string,
      `${model.label} — ${model.openrouter}`,
    ],
  );

export const KNOWN_CODEX_MODEL_VALUES = new Set([
  ...CODEX_CONCRETE_MODEL_OPTIONS.map((option) => option.value),
  ...CODEX_OPENROUTER_MODEL_OPTIONS.map(([value]) => value),
]);

/** Chat picker options: bundled default first, concrete ids only (never ""). */
export const CODEX_CHAT_MODEL_OPTIONS: CodexModelOption[] = [
  {
    value: CODEX_CHAT_DEFAULT_MODEL,
    label: CODEX_CHAT_DEFAULT_MODEL_LABEL,
    description: CODEX_CHAT_DEFAULT_MODEL,
  },
  ...CODEX_CONCRETE_MODEL_OPTIONS.filter(
    (option) => option.value !== CODEX_CHAT_DEFAULT_MODEL,
  ),
];

/** Launcher options: leading "" means "configured default / inherited model". */
export const CODEX_MODEL_OPTIONS: CodexModelOption[] = [
  {
    value: "",
    label: CODEX_DEFAULT_MODEL_LABEL,
    description: "Codex configured default.",
  },
  ...CODEX_CONCRETE_MODEL_OPTIONS,
];

export function normalizeCodexChatModel(
  value: string | null | undefined,
): string {
  const trimmed = value?.trim();
  return trimmed || CODEX_CHAT_DEFAULT_MODEL;
}

/**
 * Launcher-boundary model resolution (#1370) — the Codex mirror of
 * `claudeLaunchModelFromChatDefault`: the chat default (either route form)
 * and unknown pins map to `undefined` (the launcher's `""` configured-default
 * sentinel); other known catalog values pin concretely.
 */
export function codexLaunchModelFromChatDefault(
  value: string | null | undefined,
): string | undefined {
  const model = normalizeCodexChatModel(value);
  if (
    model === CODEX_CHAT_DEFAULT_MODEL ||
    model === CODEX_OPENROUTER_CHAT_DEFAULT_MODEL
  ) {
    return undefined;
  }
  if (KNOWN_CODEX_MODEL_VALUES.has(model)) {
    return model;
  }
  return undefined;
}

/** Display label for a model id, falling back to the raw id for unknown pins. */
export function codexModelLabelFor(value: string): string {
  const option = [...CODEX_CHAT_MODEL_OPTIONS, ...CODEX_MODEL_OPTIONS].find(
    (candidate) => candidate.value === value,
  );
  const openRouterOption = CODEX_OPENROUTER_MODEL_OPTIONS.find(
    ([model]) => model === value,
  );
  return (
    (option?.label ?? openRouterOption?.[1] ?? value) ||
    CODEX_DEFAULT_MODEL_LABEL
  );
}

/** Friendly catalog label without exposing a route-specific upstream slug. */
export function codexFriendlyModelLabelFor(value: string): string {
  const entry = CODEX_HARNESS_CATALOG.models.find(
    (candidate) => candidate.id === value || candidate.openrouter === value,
  );
  return entry?.label ?? value;
}

function catalogEntryFor(model: string) {
  const trimmed = model.trim();
  return CODEX_HARNESS_CATALOG.models.find(
    (candidate) => candidate.id === trimmed || candidate.openrouter === trimmed,
  );
}

/** The effort set a model supports; the full ladder for unknown pins. */
export function codexEffortsForModel(model: string): readonly string[] {
  return catalogEntryFor(model)?.efforts ?? CODEX_HARNESS_CATALOG.effortOrder;
}

/**
 * Resolve a requested effort against the model's supported set: an
 * out-of-range effort clamps to the model's `defaultEffort` (`xhigh` for
 * every model since chat intent 534d50ad — core D9). Unknown models pass the request
 * through unchanged.
 */
export function resolvedCodexEffortForModel(
  model: string,
  effort: string,
): string {
  const entry = catalogEntryFor(model);
  const requested =
    effort || (entry?.defaultEffort ?? CODEX_UNKNOWN_MODEL_EFFORT);
  if (!entry) return requested;
  return entry.efforts.includes(requested) ? requested : entry.defaultEffort;
}
