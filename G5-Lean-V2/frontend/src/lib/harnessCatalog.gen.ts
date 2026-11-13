// AUTO-GENERATED — do not edit; source: g5-contracts/src/{claude,codex}_models.rs
// (emitter: g5-contracts/src/model_catalog_gen.rs). Regenerate with `just gen-model-catalog`.
// Catalog data only (g5-common/core D7/D8/D9): client helpers stay hand-written.

export interface HarnessCatalogVariant {
  readonly id: string;
  readonly label: string;
  readonly offered: boolean;
}

export interface HarnessCatalogFamily {
  readonly openrouter: string;
  readonly directPreferred: string;
  readonly openrouterOffered: boolean;
  readonly openrouterLabel: string | null;
  readonly efforts: readonly string[];
  readonly defaultEffort: string;
  /** Anthropic's release day, YYYY-MM-DD; families are listed newest first. */
  readonly released: string;
  readonly variants: readonly HarnessCatalogVariant[];
}

export interface HarnessCatalog {
  readonly defaultModelDirect: string;
  readonly defaultModelOpenrouter: string;
  readonly effortOrder: readonly string[];
  readonly unknownModelEffort: string;
  readonly modes: readonly string[];
  readonly defaultMode: string;
  readonly families: readonly HarnessCatalogFamily[];
}

export const CLAUDE_HARNESS_CATALOG: HarnessCatalog = {
  defaultModelDirect: "claude-opus-5-5",
  defaultModelOpenrouter: "anthropic/claude-opus-5.5",
  effortOrder: ["low", "medium", "high", "xhigh", "max"],
  unknownModelEffort: "xhigh",
  modes: ["default", "bypassPermissions"],
  defaultMode: "default",
  families: [
    {
      openrouter: "anthropic/claude-fable-5.2",
      directPreferred: "claude-fable-5-2",
      openrouterOffered: true,
      openrouterLabel: null,
      efforts: ["low", "medium", "high", "xhigh", "max"],
      defaultEffort: "high",
      released: "2026-11-09",
      variants: [
        {
          id: "claude-fable-5-2",
          label: "Fable 5.2",
          offered: true,
        },
      ],
    },
    {
      openrouter: "anthropic/claude-sonnet-5.5",
      directPreferred: "claude-sonnet-5-5",
      openrouterOffered: true,
      openrouterLabel: null,
      efforts: ["low", "medium", "high", "xhigh", "max"],
      defaultEffort: "high",
      released: "2026-09-28",
      variants: [
        {
          id: "claude-sonnet-5-5",
          label: "Sonnet 5.5",
          offered: true,
        },
      ],
    },
    {
      openrouter: "anthropic/claude-opus-5.5",
      directPreferred: "claude-opus-5-5",
      openrouterOffered: true,
      openrouterLabel: "Opus 5.5",
      efforts: ["low", "medium", "high", "xhigh", "max"],
      defaultEffort: "high",
      released: "2026-09-22",
      variants: [
        {
          id: "claude-opus-5-5",
          label: "Opus 5.5",
          offered: true,
        },
        {
          id: "claude-opus-5-5[1m]",
          label: "Opus 5.5 (1M)",
          offered: false,
        },
      ],
    },
    {
      openrouter: "anthropic/claude-fable-5.1",
      directPreferred: "claude-fable-5-1",
      openrouterOffered: true,
      openrouterLabel: null,
      efforts: ["low", "medium", "high", "xhigh", "max"],
      defaultEffort: "high",
      released: "2026-09-01",
      variants: [
        {
          id: "claude-fable-5-1",
          label: "Fable 5.1",
          offered: true,
        },
        {
          id: "claude-fable-5-1[1m]",
          label: "Fable 5.1 (1M context)",
          offered: false,
        },
      ],
    },
    {
      openrouter: "anthropic/claude-opus-5",
      directPreferred: "claude-opus-5",
      openrouterOffered: true,
      openrouterLabel: "Opus 5",
      efforts: ["low", "medium", "high", "xhigh", "max"],
      defaultEffort: "high",
      released: "2026-07-24",
      variants: [
        {
          id: "claude-opus-5",
          label: "Opus 5",
          offered: true,
        },
        {
          id: "claude-opus-5[1m]",
          label: "Opus 5 (1M)",
          offered: false,
        },
      ],
    },
    {
      openrouter: "anthropic/claude-sonnet-5",
      directPreferred: "claude-sonnet-5",
      openrouterOffered: true,
      openrouterLabel: null,
      efforts: ["low", "medium", "high", "xhigh", "max"],
      defaultEffort: "high",
      released: "2026-06-30",
      variants: [
        {
          id: "claude-sonnet-5",
          label: "Sonnet 5",
          offered: true,
        },
      ],
    },
    {
      openrouter: "anthropic/claude-fable-5",
      directPreferred: "claude-fable-5",
      openrouterOffered: true,
      openrouterLabel: null,
      efforts: ["low", "medium", "high", "xhigh", "max"],
      defaultEffort: "high",
      released: "2026-06-09",
      variants: [
        {
          id: "claude-fable-5",
          label: "Fable 5",
          offered: true,
        },
        {
          id: "claude-fable-5[1m]",
          label: "Fable 5 (1M context)",
          offered: false,
        },
      ],
    },
    {
      openrouter: "anthropic/claude-opus-4.8",
      directPreferred: "claude-opus-4-8",
      openrouterOffered: true,
      openrouterLabel: "Opus 4.8",
      efforts: ["low", "medium", "high", "xhigh", "max"],
      defaultEffort: "high",
      released: "2026-05-28",
      variants: [
        {
          id: "claude-opus-4-8",
          label: "Opus 4.8",
          offered: true,
        },
        {
          id: "claude-opus-4-8[1m]",
          label: "Opus 4.8 (1M)",
          offered: false,
        },
      ],
    },
    {
      openrouter: "anthropic/claude-opus-4.7",
      directPreferred: "claude-opus-4-7",
      openrouterOffered: false,
      openrouterLabel: null,
      efforts: ["low", "medium", "high", "xhigh", "max"],
      defaultEffort: "high",
      released: "2026-04-16",
      variants: [
        {
          id: "claude-opus-4-7",
          label: "Opus 4.7",
          offered: false,
        },
      ],
    },
    {
      openrouter: "anthropic/claude-sonnet-4.6",
      directPreferred: "claude-sonnet-4-6",
      openrouterOffered: true,
      openrouterLabel: null,
      efforts: ["low", "medium", "high"],
      defaultEffort: "high",
      released: "2026-02-17",
      variants: [
        {
          id: "claude-sonnet-4-6",
          label: "Sonnet 4.6",
          offered: true,
        },
      ],
    },
    {
      openrouter: "anthropic/claude-opus-4.6",
      directPreferred: "claude-opus-4-6",
      openrouterOffered: false,
      openrouterLabel: null,
      efforts: ["low", "medium", "high", "xhigh", "max"],
      defaultEffort: "high",
      released: "2026-02-05",
      variants: [
        {
          id: "claude-opus-4-6",
          label: "Opus 4.6",
          offered: false,
        },
      ],
    },
    {
      openrouter: "anthropic/claude-haiku-4.5",
      directPreferred: "claude-haiku-4-5",
      openrouterOffered: true,
      openrouterLabel: null,
      efforts: ["none"],
      defaultEffort: "none",
      released: "2025-10-15",
      variants: [
        {
          id: "claude-haiku-4-5",
          label: "Haiku 4.5",
          offered: true,
        },
      ],
    },
  ],
};

export interface CodexCatalogModel {
  readonly id: string;
  readonly openrouter: string | null;
  readonly label: string;
  readonly offered: boolean;
  readonly efforts: readonly string[];
  readonly defaultEffort: string;
}

export interface CodexHarnessCatalog {
  readonly defaultModelDirect: string;
  readonly defaultModelOpenrouter: string;
  readonly effortOrder: readonly string[];
  readonly unknownModelEffort: string;
  readonly modes: readonly string[];
  readonly defaultMode: string;
  readonly models: readonly CodexCatalogModel[];
}

export const CODEX_HARNESS_CATALOG: CodexHarnessCatalog = {
  defaultModelDirect: "gpt-5.6-sol",
  defaultModelOpenrouter: "openai/gpt-5.6-sol",
  effortOrder: ["low", "medium", "high", "xhigh", "max", "ultra"],
  unknownModelEffort: "xhigh",
  modes: [
    "never/read-only",
    "never/workspace-write",
    "never/danger-full-access",
  ],
  defaultMode: "never/workspace-write",
  models: [
    {
      id: "gpt-6.1-sol",
      openrouter: "openai/gpt-6.1-sol",
      label: "GPT 6.1 Sol",
      offered: true,
      efforts: ["low", "medium", "high", "xhigh", "max", "ultra"],
      defaultEffort: "xhigh",
    },
    {
      id: "gpt-6-astra",
      openrouter: "openai/gpt-6-astra",
      label: "GPT 6 Astra",
      offered: true,
      efforts: ["low", "medium", "high", "xhigh", "max", "ultra"],
      defaultEffort: "xhigh",
    },
    {
      id: "gpt-6-sol",
      openrouter: "openai/gpt-6-sol",
      label: "GPT 6 Sol",
      offered: true,
      efforts: ["low", "medium", "high", "xhigh", "max", "ultra"],
      defaultEffort: "xhigh",
    },
    {
      id: "gpt-6-luna",
      openrouter: "openai/gpt-6-luna",
      label: "GPT 6 Luna",
      offered: true,
      efforts: ["low", "medium", "high", "xhigh", "max"],
      defaultEffort: "xhigh",
    },
    {
      id: "gpt-5.6-sol",
      openrouter: "openai/gpt-5.6-sol",
      label: "GPT 5.6 Sol",
      offered: true,
      efforts: ["low", "medium", "high", "xhigh", "max", "ultra"],
      defaultEffort: "xhigh",
    },
    {
      id: "gpt-5.6-terra",
      openrouter: "openai/gpt-5.6-terra",
      label: "GPT 5.6 Terra",
      offered: true,
      efforts: ["low", "medium", "high", "xhigh", "max", "ultra"],
      defaultEffort: "xhigh",
    },
    {
      id: "gpt-5.6-luna",
      openrouter: "openai/gpt-5.6-luna",
      label: "GPT 5.6 Luna",
      offered: true,
      efforts: ["low", "medium", "high", "xhigh", "max"],
      defaultEffort: "xhigh",
    },
    {
      id: "gpt-5.5",
      openrouter: "openai/gpt-5.5",
      label: "GPT 5.5",
      offered: true,
      efforts: ["low", "medium", "high", "xhigh"],
      defaultEffort: "xhigh",
    },
    {
      id: "gpt-5.4",
      openrouter: "openai/gpt-5.4",
      label: "GPT 5.4",
      offered: false,
      efforts: ["low", "medium", "high", "xhigh"],
      defaultEffort: "xhigh",
    },
    {
      id: "gpt-5.4-mini",
      openrouter: "openai/gpt-5.4-mini",
      label: "GPT 5.4 Mini",
      offered: false,
      efforts: ["low", "medium", "high", "xhigh"],
      defaultEffort: "xhigh",
    },
    {
      id: "gpt-5.3-codex-spark",
      openrouter: null,
      label: "GPT 5.3 Codex Spark",
      offered: false,
      efforts: ["low", "medium", "high", "xhigh"],
      defaultEffort: "xhigh",
    },
  ],
};
