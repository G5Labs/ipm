// Surface-drift guard (plan Task 0.3, ontology core D7/D8; baseline
// regenerated for the intentional #1370 effort-default alignment): the
// exported claudeModels.ts surface must reproduce the frozen fixture
// exactly, keep the launcher inherit sentinel (F3), and resolve efforts from
// per-model catalog data.
import { describe, expect, it } from "vitest";

import { CLAUDE_MODELS_BASELINE } from "../test/claudeModelsBaseline";
import { collectClaudeModelsSurface } from "../test/claudeModelsSurface";
import {
  CLAUDE_CODE_DEFAULT_MODEL_LABEL,
  CLAUDE_MODEL_OPTIONS,
  resolvedClaudeEffortForModel,
} from "./claudeModels";
import { CLAUDE_HARNESS_CATALOG } from "./harnessCatalog.gen";

describe("claudeModels catalog re-point stays behavior-preserving", () => {
  it("reproduces the frozen pre-catalog exported surface exactly", () => {
    expect(collectClaudeModelsSurface()).toEqual(CLAUDE_MODELS_BASELINE);
  });

  it("keeps the launcher inherit sentinel as the leading launch option", () => {
    expect(CLAUDE_MODEL_OPTIONS[0]).toEqual({
      value: "",
      label: CLAUDE_CODE_DEFAULT_MODEL_LABEL,
      description: "Claude Code configured default.",
    });
    const sentinelCount = CLAUDE_MODEL_OPTIONS.filter(
      (option) => option.value === "",
    ).length;
    expect(sentinelCount).toBe(1);
  });

  it("resolves efforts from catalog data per model (#1370 alignment)", () => {
    expect(CLAUDE_HARNESS_CATALOG.families.length).toBeGreaterThan(0);
    for (const family of CLAUDE_HARNESS_CATALOG.families) {
      for (const variant of family.variants) {
        // Blank resolves to the model's defaultEffort; out-of-range requests
        // clamp to it; in-range requests pass through.
        expect(resolvedClaudeEffortForModel(variant.id, "")).toBe(
          family.defaultEffort,
        );
        expect(resolvedClaudeEffortForModel(variant.id, "max")).toBe(
          family.efforts.includes("max") ? "max" : family.defaultEffort,
        );
        expect(resolvedClaudeEffortForModel(variant.id, "low")).toBe(
          family.efforts.includes("low") ? "low" : family.defaultEffort,
        );
      }
    }
  });
});
