// Task 2.1 unit tests (plan codex-multi-harness-model-effort, D-5/D-6; core
// D7/D8/D9): each cascade step derives from the generated catalogs, with
// unknown-harness and unknown-model fallbacks and the clamp-to-defaultEffort
// rule from the Story 2 acceptance scenarios.
import { describe, expect, it } from "vitest";
import nativeCatalogText from "../../../tests/fixtures/native-provider-catalog.json?raw";

import {
  CLAUDE_CHAT_MODEL_OPTIONS,
  KNOWN_CLAUDE_MODEL_VALUES,
} from "./claudeModels";
import {
  CODEX_CHAT_MODEL_OPTIONS,
  KNOWN_CODEX_MODEL_VALUES,
} from "./codexModels";
import {
  chatModelOptionsForHarness,
  clampEffortForModel,
  effortSelectionForModel,
  modeValuesForHarness,
  pickerHarnessFor,
} from "./harnessPicker";

describe("harnessPicker cascade derivation", () => {
  it("offers exactly the model/mode/effort sets exercised on both native pins", () => {
    const matrix = JSON.parse(nativeCatalogText) as {
      providers: Record<
        string,
        {
          modes: string[];
          models: { id: string; efforts: string[]; defaultEffort: string }[];
        }
      >;
    };
    for (const [provider, native] of Object.entries(matrix.providers)) {
      const known =
        provider === "codex"
          ? KNOWN_CODEX_MODEL_VALUES
          : KNOWN_CLAUDE_MODEL_VALUES;
      expect([...known].sort()).toEqual(
        native.models.map((model) => model.id).sort(),
      );
      expect(modeValuesForHarness(provider).modes).toEqual(native.modes);
      for (const model of native.models) {
        expect(effortSelectionForModel(provider, model.id)).toEqual({
          efforts: model.efforts,
          defaultEffort: model.defaultEffort,
          known: true,
        });
        for (const effort of model.efforts) {
          expect(clampEffortForModel(provider, model.id, effort)).toBe(effort);
        }
      }
      for (const option of chatModelOptionsForHarness(provider)) {
        expect(known.has(option.value)).toBe(true);
      }
    }
  });
  it("normalizes provider ids to a picker harness with claude_code fallback", () => {
    expect(pickerHarnessFor("codex")).toBe("codex");
    expect(pickerHarnessFor("claude_code")).toBe("claude_code");
    expect(pickerHarnessFor("unknown")).toBe("claude_code");
    expect(pickerHarnessFor(undefined)).toBe("claude_code");
  });

  it("derives the model group from the harness's chat options (concrete ids)", () => {
    expect(chatModelOptionsForHarness("claude_code")).toBe(
      CLAUDE_CHAT_MODEL_OPTIONS,
    );
    expect(chatModelOptionsForHarness("codex")).toBe(CODEX_CHAT_MODEL_OPTIONS);
    expect(chatModelOptionsForHarness("unknown")).toBe(
      CLAUDE_CHAT_MODEL_OPTIONS,
    );
    for (const option of [
      ...chatModelOptionsForHarness("claude_code"),
      ...chatModelOptionsForHarness("codex"),
    ]) {
      expect(option.value.trim()).not.toBe("");
    }
  });

  it("derives the mode group verbatim from the harness catalog with its default", () => {
    const claude = modeValuesForHarness("claude_code");
    expect(claude.modes).toEqual(["default", "bypassPermissions"]);
    expect(claude.defaultMode).toBe("default");

    const codex = modeValuesForHarness("codex");
    expect(codex.modes).toEqual([
      "never/read-only",
      "never/workspace-write",
      "never/danger-full-access",
    ]);
    expect(codex.defaultMode).toBe("never/workspace-write");
  });

  it("populates efforts per model, including OpenRouter slug lookups", () => {
    const fable = effortSelectionForModel("claude_code", "claude-fable-5[1m]");
    expect(fable.known).toBe(true);
    expect(fable.efforts).toContain("max");
    expect(fable.defaultEffort).toBe("high");

    const sonnet = effortSelectionForModel("claude_code", "claude-sonnet-4-6");
    expect(sonnet.efforts).toEqual(["low", "medium", "high"]);
    expect(sonnet.defaultEffort).toBe("high");

    const opusSlug = effortSelectionForModel(
      "claude_code",
      "anthropic/claude-opus-4.8",
    );
    expect(opusSlug.known).toBe(true);
    expect(opusSlug.defaultEffort).toBe("high");

    const sol = effortSelectionForModel("codex", "gpt-5.6-sol");
    expect(sol.efforts).toContain("ultra");
    expect(sol.defaultEffort).toBe("xhigh");

    const gpt55Slug = effortSelectionForModel("codex", "openai/gpt-5.5");
    expect(gpt55Slug.known).toBe(true);
    expect(gpt55Slug.defaultEffort).toBe("xhigh");
  });

  it("falls back to the harness ladder with the legacy default for unknown models", () => {
    const unknownClaude = effortSelectionForModel(
      "claude_code",
      "custom/provider-model",
    );
    expect(unknownClaude.known).toBe(false);
    expect(unknownClaude.efforts).toEqual([
      "low",
      "medium",
      "high",
      "xhigh",
      "max",
    ]);
    expect(unknownClaude.defaultEffort).toBe("xhigh");

    const unknownCodex = effortSelectionForModel("codex", "gpt-9.9");
    expect(unknownCodex.known).toBe(false);
    expect(unknownCodex.efforts).toContain("ultra");
    expect(unknownCodex.defaultEffort).toBe("xhigh");
  });

  it("clamps an out-of-range prior effort to the new model's defaultEffort (D-6)", () => {
    expect(clampEffortForModel("codex", "gpt-5.5", "ultra")).toBe("xhigh");
    expect(clampEffortForModel("codex", "gpt-5.6-luna", "ultra")).toBe("xhigh");
    expect(clampEffortForModel("claude_code", "claude-sonnet-4-6", "max")).toBe(
      "high",
    );
    expect(clampEffortForModel("claude_code", "claude-sonnet-4-6", "low")).toBe(
      "low",
    );
    expect(clampEffortForModel("claude_code", "claude-fable-5", "xhigh")).toBe(
      "xhigh",
    );
    expect(clampEffortForModel("codex", "unknown-model", "max")).toBe("max");
    expect(clampEffortForModel("codex", "gpt-5.6-sol", "")).toBe("xhigh");
    expect(clampEffortForModel("claude_code", "claude-haiku-4-5", null)).toBe(
      "none",
    );
  });
});
