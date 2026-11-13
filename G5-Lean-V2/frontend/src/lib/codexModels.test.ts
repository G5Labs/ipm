// Task 1.2 unit tests (plan codex-multi-harness-model-effort, core D8/D9):
// option lists match the generated catalog, chat options carry concrete ids
// (never the "" sentinel — empty-sentinel experience), label lookup falls
// back to the raw id, and effort resolution clamps to the model's
// defaultEffort.
import { describe, expect, it } from "vitest";

import {
  CODEX_CHAT_DEFAULT_MODEL,
  CODEX_CHAT_MODEL_OPTIONS,
  CODEX_CONCRETE_MODEL_OPTIONS,
  CODEX_MODEL_OPTIONS,
  CODEX_OPENROUTER_MODEL_OPTIONS,
  codexEffortsForModel,
  codexModelLabelFor,
  normalizeCodexChatModel,
  resolvedCodexEffortForModel,
} from "./codexModels";
import { CODEX_HARNESS_CATALOG } from "./harnessCatalog.gen";

describe("codexModels catalog-derived options", () => {
  it("derives the concrete option list from the offered catalog models", () => {
    const offered = CODEX_HARNESS_CATALOG.models.filter(
      (model) => model.offered,
    );
    expect(CODEX_CONCRETE_MODEL_OPTIONS).toEqual(
      offered.map((model) => ({
        value: model.id,
        label: model.label,
        description: model.id,
      })),
    );
    // 2026-09-30: the Codex 0.159.3 pin re-check (`codex app-server
    // model/list`, g5.provider.catalog.pinned) adds GPT 6.1 Sol, GPT 6 Sol
    // and GPT 6 Luna and stops offering gpt-5.4, gpt-5.4-mini and
    // gpt-5.3-codex-spark.
    expect(CODEX_CONCRETE_MODEL_OPTIONS.map((option) => option.value)).toEqual([
      "gpt-6.1-sol",
      "gpt-6-astra",
      "gpt-6-sol",
      "gpt-6-luna",
      "gpt-5.6-sol",
      "gpt-5.6-terra",
      "gpt-5.6-luna",
      "gpt-5.5",
    ]);
  });

  it("omits direct-only models from the OpenRouter options", () => {
    // core D9 (chat intents 8c04277e/a429913a): astra's OpenRouter slug
    // landed 2026-09-04; the direct-only spark is no longer offered at the
    // 0.159.3 pin and the astra-pro serving variant is deliberately excluded
    // from the catalog.
    const slugs = CODEX_OPENROUTER_MODEL_OPTIONS.map(([value]) => value);
    expect(slugs).toHaveLength(8);
    expect(slugs).toContain("openai/gpt-6-astra");
    expect(slugs).toContain("openai/gpt-6.1-sol");
    expect(slugs.every((slug) => slug.startsWith("openai/"))).toBe(true);
    expect(slugs.some((slug) => slug.includes("spark"))).toBe(false);
    expect(slugs.some((slug) => slug.includes("pro"))).toBe(false);
  });

  it("keeps chat options concrete — the empty sentinel only leads the launcher list", () => {
    expect(CODEX_CHAT_MODEL_OPTIONS[0]).toEqual({
      value: CODEX_CHAT_DEFAULT_MODEL,
      label: "GPT 5.6 Sol",
      description: CODEX_CHAT_DEFAULT_MODEL,
    });
    expect(
      CODEX_CHAT_MODEL_OPTIONS.every((option) => option.value.trim() !== ""),
    ).toBe(true);
    expect(CODEX_MODEL_OPTIONS[0]).toEqual({
      value: "",
      label: "GPT 5.6 Sol",
      description: "Codex configured default.",
    });
    expect(
      CODEX_MODEL_OPTIONS.filter((option) => option.value === ""),
    ).toHaveLength(1);
  });

  it("normalizes blank chat models to the concrete bundled default", () => {
    expect(normalizeCodexChatModel(undefined)).toBe(CODEX_CHAT_DEFAULT_MODEL);
    expect(normalizeCodexChatModel("  ")).toBe(CODEX_CHAT_DEFAULT_MODEL);
    expect(normalizeCodexChatModel(" gpt-5.6-sol ")).toBe("gpt-5.6-sol");
  });

  it("labels known ids and falls back to the raw id for unknown pins", () => {
    expect(codexModelLabelFor("gpt-5.6-sol")).toBe("GPT 5.6 Sol");
    expect(codexModelLabelFor("openai/gpt-5.6-terra")).toBe(
      "GPT 5.6 Terra — openai/gpt-5.6-terra",
    );
    expect(codexModelLabelFor("custom/unknown-model")).toBe(
      "custom/unknown-model",
    );
    expect(codexModelLabelFor("")).toBe("GPT 5.6 Sol");
  });

  it("resolves efforts per model and clamps out-of-range values to the model default", () => {
    expect(codexEffortsForModel("gpt-5.6-sol")).toContain("ultra");
    expect(codexEffortsForModel("gpt-5.5")).not.toContain("max");
    expect(resolvedCodexEffortForModel("gpt-5.6-sol", "ultra")).toBe("ultra");
    expect(resolvedCodexEffortForModel("gpt-5.5", "ultra")).toBe("xhigh");
    expect(resolvedCodexEffortForModel("gpt-5.6-luna", "ultra")).toBe("xhigh");
    expect(resolvedCodexEffortForModel("openai/gpt-5.5", "max")).toBe("xhigh");
    expect(resolvedCodexEffortForModel("gpt-5.5", "")).toBe("xhigh");
    expect(resolvedCodexEffortForModel("unknown-model", "max")).toBe("max");
  });
});
