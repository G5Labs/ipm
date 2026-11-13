import { describe, expect, it } from "vitest";

import {
  VERIFICATION_ACTION_ENABLED_SETTING,
  isOrgVerificationActionEnabled,
} from "./verificationActionFlags";

describe("isOrgVerificationActionEnabled (ui g5.workbench.review-prompts)", () => {
  it("is off unless the organization's setting is exactly true", () => {
    expect(isOrgVerificationActionEnabled(undefined)).toBe(false);
    expect(isOrgVerificationActionEnabled(null)).toBe(false);
    expect(isOrgVerificationActionEnabled({ settings: null })).toBe(false);
    expect(isOrgVerificationActionEnabled({ settings: {} })).toBe(false);
    for (const value of [false, "true", 1, null]) {
      expect(
        isOrgVerificationActionEnabled({
          settings: { [VERIFICATION_ACTION_ENABLED_SETTING]: value },
        }),
      ).toBe(false);
    }
    expect(
      isOrgVerificationActionEnabled({
        settings: { [VERIFICATION_ACTION_ENABLED_SETTING]: true },
      }),
    ).toBe(true);
  });
});
