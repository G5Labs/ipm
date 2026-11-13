// Collector for the full exported surface of lib/claudeModels.ts, used by the
// behavior-preservation baseline test (plan codex-multi-harness-model-effort
// Task 0.3). Probe inputs are FIXED literals — independent of the module's own
// option lists — so a pre/post comparison always exercises identical inputs.

import {
  CLAUDE_CHAT_DEFAULT_MODEL,
  CLAUDE_CHAT_DEFAULT_MODEL_LABEL,
  CLAUDE_CHAT_MODEL_OPTIONS,
  CLAUDE_CODE_DEFAULT_MODEL_LABEL,
  CLAUDE_CONCRETE_MODEL_OPTIONS,
  CLAUDE_DIRECT_CHAT_DEFAULT_MODEL,
  CLAUDE_DIRECT_MODEL_OPTIONS,
  CLAUDE_MODEL_OPTIONS,
  CLAUDE_OPENROUTER_MODEL_OPTIONS,
  CLAUDE_UNKNOWN_MODEL_EFFORT,
  KNOWN_CLAUDE_MODEL_VALUES,
  claudeLaunchEffortForModel,
  claudeLaunchModelFromChatDefault,
  claudeModelLabelFor,
  normalizeClaudeChatModel,
  resolvedClaudeEffortForModel,
} from "../lib/claudeModels";

const PROBE_MODELS = [
  "",
  "  ",
  "claude-fable-5-2",
  "claude-fable-5-1",
  "claude-fable-5-1[1m]",
  "claude-fable-5",
  "claude-fable-5[1m]",
  "claude-opus-5-5",
  "claude-opus-5-5[1m]",
  "claude-opus-5",
  "claude-opus-5[1m]",
  "claude-opus-4-8",
  "claude-opus-4-8[1m]",
  "claude-opus-4-7",
  "claude-opus-4-6",
  "claude-sonnet-5-5",
  "claude-sonnet-5",
  "claude-sonnet-4-6",
  "claude-haiku-4-5",
  "anthropic/claude-fable-5.2",
  "anthropic/claude-fable-5.1",
  "anthropic/claude-fable-5",
  "anthropic/claude-opus-5.5",
  "anthropic/claude-opus-5",
  "anthropic/claude-opus-4.8",
  "anthropic/claude-opus-4.7",
  "anthropic/claude-sonnet-5.5",
  "anthropic/claude-sonnet-5",
  "anthropic/claude-sonnet-4.6",
  "anthropic/claude-haiku-4.5",
  "custom/provider-model",
  "claude-3-7-sonnet-20250219",
] as const;

const PROBE_EFFORTS = ["", "low", "medium", "high", "xhigh", "max"] as const;

const PROBE_NORMALIZE_INPUTS = [
  undefined,
  null,
  "",
  "  ",
  "claude-sonnet-4-6",
  " anthropic/claude-opus-4.8 ",
] as const;

export function collectClaudeModelsSurface() {
  return {
    constants: {
      chatDefaultModel: CLAUDE_CHAT_DEFAULT_MODEL,
      directChatDefaultModel: CLAUDE_DIRECT_CHAT_DEFAULT_MODEL,
      chatDefaultModelLabel: CLAUDE_CHAT_DEFAULT_MODEL_LABEL,
      codeDefaultModelLabel: CLAUDE_CODE_DEFAULT_MODEL_LABEL,
      unknownModelEffort: CLAUDE_UNKNOWN_MODEL_EFFORT,
    },
    concreteModelOptions: CLAUDE_CONCRETE_MODEL_OPTIONS,
    openrouterModelOptions: CLAUDE_OPENROUTER_MODEL_OPTIONS,
    directModelOptions: CLAUDE_DIRECT_MODEL_OPTIONS,
    chatModelOptions: CLAUDE_CHAT_MODEL_OPTIONS,
    modelOptions: CLAUDE_MODEL_OPTIONS,
    knownModelValues: [...KNOWN_CLAUDE_MODEL_VALUES].sort(),
    labelProbes: PROBE_MODELS.map((model) => [
      model,
      claudeModelLabelFor(model),
    ]),
    resolvedEffortProbes: PROBE_MODELS.flatMap((model) =>
      PROBE_EFFORTS.map((effort) => [
        model,
        effort,
        resolvedClaudeEffortForModel(model, effort),
      ]),
    ),
    launchEffortProbes: PROBE_MODELS.flatMap((model) =>
      [undefined, ...PROBE_EFFORTS].map((effort) => [
        model,
        effort ?? null,
        claudeLaunchEffortForModel(model, effort) ?? null,
      ]),
    ),
    launchModelProbes: PROBE_MODELS.map((model) => [
      model,
      claudeLaunchModelFromChatDefault(model) ?? null,
    ]),
    normalizeProbes: PROBE_NORMALIZE_INPUTS.map((input) => [
      input ?? null,
      normalizeClaudeChatModel(input),
    ]),
  };
}
