import { describe, expect, it } from "vitest";

import { hasInstanceFeature, isGuidedJourneyEnabled } from "./instanceFeatures";

describe("isGuidedJourneyEnabled", () => {
  it("is enabled only when instance info advertises the guided_journey feature", () => {
    expect(isGuidedJourneyEnabled({ features: ["guided_journey"] })).toBe(true);
    expect(isGuidedJourneyEnabled({ features: ["chat"] })).toBe(false);
  });

  it("fails closed on missing, unloaded, or malformed instance info", () => {
    expect(isGuidedJourneyEnabled(null)).toBe(false);
    expect(isGuidedJourneyEnabled(undefined)).toBe(false);
    expect(isGuidedJourneyEnabled({})).toBe(false);
    expect(isGuidedJourneyEnabled({ features: null })).toBe(false);
    expect(
      isGuidedJourneyEnabled({
        features: "guided_journey" as unknown as string[],
      }),
    ).toBe(false);
    expect(hasInstanceFeature({ features: [] }, "guided_journey")).toBe(false);
  });
});
