// =============================================================================
// G5 Client — Shared Claude Code Model Lineup
// =============================================================================
// Hand-written helpers over generated catalog data. The option/label/effort
// DATA derives from harnessCatalog.gen.ts (Rust catalog SoT:
// g5-common/src/claude_models.rs, core D7/D8); the helpers, the legacy chat
// reasoning default, and the launcher inherit sentinel stay hand-written here —
// the "" sentinel never enters the catalog (empty-sentinel experience). Chat
// uses a concrete provider-aware default model pin; task launchers use the
// inherit-capable option list whose leading empty value means "configured
// default / inherited model".
// The chat default is stored as the canonical OpenRouter slug because the API
// and worker provider-aware resolver maps known Claude aliases to the active
// route's executable form. Every offered family runs its 1M window without
// Claude Code's `[1m]` suffix; saved `[1m]` selections stay resolver-known
// (unoffered catalog variants) and flow through set_model / ANTHROPIC_MODEL
// after route normalization. Options follow the catalog's release order,
// newest first (owner request 2026-11-09).
// =============================================================================

import { CLAUDE_HARNESS_CATALOG } from "./harnessCatalog.gen";

/** Canonical model id for ordinary chat's bundled default preference. */
export const CLAUDE_CHAT_DEFAULT_MODEL =
  CLAUDE_HARNESS_CATALOG.defaultModelOpenrouter;

/** Direct Claude Code executable alias for the bundled default. */
export const CLAUDE_DIRECT_CHAT_DEFAULT_MODEL =
  CLAUDE_HARNESS_CATALOG.defaultModelDirect;

const DEFAULT_FAMILY = CLAUDE_HARNESS_CATALOG.families.find(
  (family) =>
    family.openrouter === CLAUDE_HARNESS_CATALOG.defaultModelOpenrouter,
);

/** Label for ordinary chat's provider-normalized default. */
export const CLAUDE_CHAT_DEFAULT_MODEL_LABEL =
  DEFAULT_FAMILY?.openrouterLabel ?? CLAUDE_CHAT_DEFAULT_MODEL;

/** Label for inherit/configured-default entries that resolve to Claude Code's default. */
export const CLAUDE_CODE_DEFAULT_MODEL_LABEL = CLAUDE_CHAT_DEFAULT_MODEL_LABEL;

/**
 * Fallback effort for models the catalog does not know (#1370, core D7) —
 * out-of-catalog pins have no `defaultEffort` to derive. Per-agent by user
 * decision and codegen-emitted so this value cannot drift from g5-common.
 */
export const CLAUDE_UNKNOWN_MODEL_EFFORT =
  CLAUDE_HARNESS_CATALOG.unknownModelEffort;

export interface ClaudeModelOption {
  value: string;
  label: string;
  description: string;
}

export const CLAUDE_CONCRETE_MODEL_OPTIONS: ClaudeModelOption[] =
  CLAUDE_HARNESS_CATALOG.families.flatMap((family) =>
    family.variants
      .filter((variant) => variant.offered)
      .map((variant) => ({
        value: variant.id,
        label: variant.label,
        description: variant.id,
      })),
  );

export const CLAUDE_OPENROUTER_MODEL_OPTIONS: Array<[string, string]> =
  CLAUDE_HARNESS_CATALOG.families
    .filter((family) => family.openrouterOffered)
    .map((family): [string, string] => [
      family.openrouter,
      family.openrouterLabel
        ? `${family.openrouterLabel} — ${family.openrouter}`
        : family.openrouter,
    ]);

export const CLAUDE_DIRECT_MODEL_OPTIONS: Array<[string, string]> =
  CLAUDE_CONCRETE_MODEL_OPTIONS.map((option) => [
    option.value,
    `${option.label} — ${option.value}`,
  ]);

export const KNOWN_CLAUDE_MODEL_VALUES = new Set([
  ...CLAUDE_OPENROUTER_MODEL_OPTIONS.map(([value]) => value),
  ...CLAUDE_DIRECT_MODEL_OPTIONS.map(([value]) => value),
]);

/**
 * Composer options in the catalog's release order (newest first). The default
 * family is represented once, by the bundled default's canonical slug, in its
 * own release position; its direct variants are not listed a second time.
 */
export const CLAUDE_CHAT_MODEL_OPTIONS: ClaudeModelOption[] =
  CLAUDE_HARNESS_CATALOG.families.flatMap((family) =>
    family === DEFAULT_FAMILY
      ? [
          {
            value: CLAUDE_CHAT_DEFAULT_MODEL,
            label: CLAUDE_CHAT_DEFAULT_MODEL_LABEL,
            description: CLAUDE_CHAT_DEFAULT_MODEL,
          },
        ]
      : family.variants
          .filter((variant) => variant.offered)
          .map((variant) => ({
            value: variant.id,
            label: variant.label,
            description: variant.id,
          })),
  );

/** OpenRouter accepts the family's preferred alias, not its direct-only variants. */
export const CLAUDE_OPENROUTER_CHAT_MODEL_OPTIONS =
  CLAUDE_CHAT_MODEL_OPTIONS.filter((option) => {
    const family = claudeCatalogEntryFor(option.value);
    return (
      family?.openrouterOffered &&
      (option.value === family.openrouter ||
        option.value === family.directPreferred)
    );
  });

/**
 * Keep the chosen family when a named OpenRouter account cannot take a
 * direct-only variant: a saved `[1m]` selection becomes the family's plain id,
 * which OpenRouter's slug mapping accepts.
 */
export function claudeChatModelForOpenRouter(model: string): string {
  const family = claudeCatalogEntryFor(model);
  if (!family?.openrouterOffered) return model;
  if (CLAUDE_CHAT_MODEL_OPTIONS.some((option) => option.value === model)) {
    return model;
  }
  return (
    CLAUDE_CHAT_MODEL_OPTIONS.find(
      (option) => claudeCatalogEntryFor(option.value) === family,
    )?.value ?? family.directPreferred
  );
}

export const CLAUDE_MODEL_OPTIONS: ClaudeModelOption[] = [
  {
    value: "",
    label: CLAUDE_CODE_DEFAULT_MODEL_LABEL,
    description: "Claude Code configured default.",
  },
  ...CLAUDE_CONCRETE_MODEL_OPTIONS,
];

const CLAUDE_LAUNCH_MODEL_VALUES = new Set(
  CLAUDE_MODEL_OPTIONS.map((option) => option.value),
);

export function normalizeClaudeChatModel(
  value: string | null | undefined,
): string {
  const trimmed = value?.trim();
  if (!trimmed) return CLAUDE_CHAT_DEFAULT_MODEL;
  // The default family's direct forms (its plain id or a saved `[1m]` pick)
  // name the bundled default's model, which the composer represents once.
  if (DEFAULT_FAMILY?.variants.some((variant) => variant.id === trimmed)) {
    return CLAUDE_CHAT_DEFAULT_MODEL;
  }
  return trimmed;
}

export function claudeLaunchModelFromChatDefault(
  value: string | null | undefined,
): string | undefined {
  const model = normalizeClaudeChatModel(value);
  if (
    model === CLAUDE_CHAT_DEFAULT_MODEL ||
    model === CLAUDE_DIRECT_CHAT_DEFAULT_MODEL
  ) {
    return undefined;
  }
  if (CLAUDE_LAUNCH_MODEL_VALUES.has(model)) {
    return model;
  }
  if (KNOWN_CLAUDE_MODEL_VALUES.has(model)) {
    return model;
  }
  return undefined;
}

/** Catalog label for an unoffered but resolver-known variant (a saved `[1m]` pick). */
function claudeUnofferedVariantLabelFor(value: string): string | undefined {
  for (const family of CLAUDE_HARNESS_CATALOG.families) {
    const variant = family.variants.find((candidate) => candidate.id === value);
    if (variant) return variant.label;
  }
  return undefined;
}

/** Display label for a model id, falling back to the raw id for unknown pins. */
export function claudeModelLabelFor(value: string): string {
  const option = [...CLAUDE_CHAT_MODEL_OPTIONS, ...CLAUDE_MODEL_OPTIONS].find(
    (o) => o.value === value,
  );
  const openRouterOption = CLAUDE_OPENROUTER_MODEL_OPTIONS.find(
    ([model]) => model === value,
  );
  return (
    (option?.label ??
      openRouterOption?.[1] ??
      claudeUnofferedVariantLabelFor(value) ??
      value) ||
    CLAUDE_CODE_DEFAULT_MODEL_LABEL
  );
}

/** Friendly catalog label without exposing a route-specific upstream slug. */
export function claudeFriendlyModelLabelFor(value: string): string {
  const direct = CLAUDE_CONCRETE_MODEL_OPTIONS.find(
    (candidate) => candidate.value === value,
  );
  if (direct) return direct.label;
  const family = CLAUDE_HARNESS_CATALOG.families.find(
    (candidate) => candidate.openrouter === value,
  );
  return (
    family?.openrouterLabel ??
    family?.variants.find((variant) => variant.offered)?.label ??
    claudeUnofferedVariantLabelFor(value) ??
    value
  );
}

function claudeCatalogEntryFor(model: string) {
  const trimmed = model.trim();
  const family = CLAUDE_HARNESS_CATALOG.families.find(
    (candidate) =>
      candidate.openrouter === trimmed ||
      candidate.variants.some((variant) => variant.id === trimmed),
  );
  return family ?? null;
}

/**
 * Resolve a requested effort against the model's catalog set (#1370, core
 * D7, plan D-6): a blank request resolves to the model's `defaultEffort`
 * (`high` for every family since chat intent 36046a83), an out-of-range
 * request clamps to it, and
 * unknown models pass the request through (blank → the per-agent
 * unknown-model fallback). One rule shared with the workers.
 */
export function resolvedClaudeEffortForModel(
  model: string,
  effort: string,
): string {
  const entry = claudeCatalogEntryFor(model);
  const requested =
    effort || (entry?.defaultEffort ?? CLAUDE_UNKNOWN_MODEL_EFFORT);
  if (!entry) return requested;
  return entry.efforts.includes(requested) ? requested : entry.defaultEffort;
}

export function claudeLaunchEffortForModel(
  model: string | null | undefined,
  effort: string | null | undefined = "",
): string | undefined {
  const concreteModel = model?.trim();
  if (!concreteModel) return undefined;
  return resolvedClaudeEffortForModel(concreteModel, effort || "");
}
