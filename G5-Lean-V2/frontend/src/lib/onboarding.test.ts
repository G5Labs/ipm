import { describe, expect, it } from "vitest";
import {
  fillOnboardingCopy,
  hasCompletedOnboarding,
  homeDefaultsToAll,
  ONBOARDING_STEP_COUNT,
  ONBOARDING_STEP_IDS,
  onboardingFirstName,
  onboardingIssueMessage,
  onboardingIssueTitle,
  onboardingNeverShown,
  onboardingResumeStep,
  stepPage,
} from "./onboarding";

describe("onboarding facts (ui g5.ui.onboarding-tour, users g5.user.onboarding)", () => {
  it("has seven steps whose finished value is the count", () => {
    expect(ONBOARDING_STEP_IDS).toHaveLength(ONBOARDING_STEP_COUNT);
    expect(hasCompletedOnboarding({ onboardingStep: 7 })).toBe(true);
    expect(hasCompletedOnboarding({ onboardingStep: 6 })).toBe(false);
    expect(hasCompletedOnboarding({ onboardingStep: null })).toBe(false);
    expect(hasCompletedOnboarding(undefined)).toBe(false);
    expect(hasCompletedOnboarding({})).toBe(false);
  });

  it("defaults Home to All until the tour is finished", () => {
    expect(homeDefaultsToAll({ onboardingStep: null })).toBe(true);
    expect(homeDefaultsToAll({ onboardingStep: 3 })).toBe(true);
    expect(homeDefaultsToAll({ onboardingStep: 7 })).toBe(false);
    expect(homeDefaultsToAll(null)).toBe(true);
  });

  it("shows the welcome card only to a person it was never shown to", () => {
    expect(onboardingNeverShown({ onboardingStep: null })).toBe(true);
    expect(onboardingNeverShown({})).toBe(true);
    expect(onboardingNeverShown({ onboardingStep: 0 })).toBe(false);
  });

  it("resumes an unfinished, unskipped tour at its recorded step", () => {
    expect(
      onboardingResumeStep({ onboardingStep: 4, onboardingSkipped: false }),
    ).toBe(4);
    expect(
      onboardingResumeStep({ onboardingStep: 4, onboardingSkipped: true }),
    ).toBeNull();
    expect(
      onboardingResumeStep({ onboardingStep: 7, onboardingSkipped: false }),
    ).toBeNull();
    expect(
      onboardingResumeStep({ onboardingStep: null, onboardingSkipped: false }),
    ).toBeNull();
    expect(onboardingResumeStep({ onboardingStep: 0 })).toBe(0);
  });

  it("maps steps to the page they are taken on", () => {
    expect(ONBOARDING_STEP_IDS.map((_, step) => stepPage(step))).toEqual([
      "home",
      "settings",
      "settings",
      "home",
      "home",
      "home",
      "workspace",
    ]);
  });

  it("addresses the person by first name and names the test issue after them", () => {
    expect(
      onboardingFirstName({ displayName: "Ada Lovelace", email: "ada@x.dev" }),
    ).toBe("Ada");
    expect(
      onboardingFirstName({ displayName: "  ", email: "grace@x.dev" }),
    ).toBe("grace");
    expect(onboardingFirstName({ displayName: "", email: "@" })).toBe("there");
    expect(onboardingIssueTitle("Ada")).toBe("[Test issue] Ada onboarding");
    expect(onboardingIssueMessage("Ada")).toContain(
      '"[Test issue] Ada onboarding"',
    );
    expect(onboardingIssueMessage("Ada")).toContain("assigned to me");
  });

  it("fills the copy placeholders", () => {
    expect(
      fillOnboardingCopy("Hi {name}, {org} has {issue}.", {
        name: "Ada",
        org: "Acme",
        issue: "[Test issue] Ada onboarding",
      }),
    ).toBe("Hi Ada, Acme has [Test issue] Ada onboarding.");
  });
});
