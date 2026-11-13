// Severity presentation shared by every surface that paints urgency.
// Governing intent: center-panel D69 "Card urgency language".

import { describe, expect, it } from "vitest";

import {
  SEVERITY_STATE,
  SEVERITY_TOKEN,
  SEVERITY_TOKEN_NEUTRAL,
  questionUrgencyStyle,
  type Severity,
} from "./severity";

const ALL: Severity[] = ["low", "medium", "high", "critical"];

describe("questionUrgencyStyle", () => {
  it("maps each severity to its colour role: blue low, amber medium, red high/critical", () => {
    expect(questionUrgencyStyle("low", false).borderLeftColor).toBe(
      "var(--g5-info)",
    );
    expect(questionUrgencyStyle("medium", false).borderLeftColor).toBe(
      "var(--g5-warn)",
    );
    expect(questionUrgencyStyle("high", false).borderLeftColor).toBe(
      "var(--g5-blocked)",
    );
    expect(questionUrgencyStyle("critical", false).borderLeftColor).toBe(
      "var(--g5-blocked)",
    );
  });

  it("separates the blocking tier from high by more than colour", () => {
    const critical = questionUrgencyStyle("critical", false);
    const high = questionUrgencyStyle("high", false);
    // Same red — so the distinction cannot rest on the colour.
    expect(critical.borderLeftColor).toBe(high.borderLeftColor);
    expect(critical.borderLeftWidth).not.toBe(high.borderLeftWidth);
    expect(critical.backgroundColor).toBeTruthy();
    expect(high.backgroundColor).toBeUndefined();
  });

  it("renders answered questions neutral REGARDLESS of severity", () => {
    for (const severity of ALL) {
      const style = questionUrgencyStyle(severity, true);
      expect(style.borderLeftColor).toBe(SEVERITY_TOKEN_NEUTRAL);
      // A decided question carries no urgency: no leftover tint either.
      expect(style.backgroundColor).toBeUndefined();
    }
  });

  it("never invents an urgency for a question with no severity", () => {
    expect(questionUrgencyStyle(null, false).borderLeftColor).toBe(
      SEVERITY_TOKEN_NEUTRAL,
    );
    expect(questionUrgencyStyle(undefined, false).borderLeftColor).toBe(
      SEVERITY_TOKEN_NEUTRAL,
    );
  });

  it("keeps the spine and the chip on the same colour role", () => {
    // The chip renders from SEVERITY_STATE and the spine from SEVERITY_TOKEN;
    // if these two tables drift, one card shows two different urgencies.
    const roleToToken: Record<string, string> = {
      info: "var(--g5-info)",
      warn: "var(--g5-warn)",
      blocked: "var(--g5-blocked)",
    };
    for (const severity of ALL) {
      expect(SEVERITY_TOKEN[severity]).toBe(
        roleToToken[SEVERITY_STATE[severity]],
      );
    }
  });
});
