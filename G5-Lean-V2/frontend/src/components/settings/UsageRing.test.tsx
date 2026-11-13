// =============================================================================
// UsageRing tests — test_contract/provider_accounts I11 (Vitest half, ring
// thresholds); settings-page D18.
// =============================================================================

import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { UsageRing, usageLevel, usagePercent } from "./UsageRing";

describe("UsageRing — thresholds (settings-page D18)", () => {
  it.each([
    [0.0, 0, "ok"],
    [0.49, 49, "ok"],
    [0.5, 50, "warn"],
    [0.79, 79, "warn"],
    [0.8, 80, "danger"],
    [1.0, 100, "danger"],
  ])(
    "utilization %s renders %s%% at level %s",
    (utilization, percent, level) => {
      render(<UsageRing utilization={utilization} label="5h" testId="ring" />);
      const ring = screen.getByTestId("ring");
      expect(ring).toHaveAttribute("role", "img");
      expect(ring).toHaveAttribute("data-level", level);
      expect(ring).toHaveAttribute("data-percent", String(percent));
      expect(ring).toHaveAttribute("aria-label", `5h ${percent}% used`);
    },
  );

  it("renders the empty track with 'no data' for a null utilization", () => {
    render(<UsageRing utilization={null} label="7d" testId="ring" />);
    const ring = screen.getByTestId("ring");
    expect(ring).toHaveAttribute("data-level", "none");
    expect(ring).toHaveAttribute("data-percent", "");
    expect(ring).toHaveAttribute("aria-label", "7d no data");
    // Track only: no fill arc.
    expect(ring.querySelectorAll("circle")).toHaveLength(1);
  });

  it("carries the note as the hover title", () => {
    render(
      <UsageRing
        utilization={0.62}
        label="5h"
        note="5-hour: 62% used · resets in 2h · as of 5m ago"
        testId="ring"
      />,
    );
    expect(screen.getByTestId("ring").querySelector("title")).toHaveTextContent(
      "5-hour: 62% used · resets in 2h · as of 5m ago",
    );
  });

  it("draws the fill arc as the utilization fraction of the circumference, starting at twelve o'clock", () => {
    render(<UsageRing utilization={0.25} label="7d" testId="ring" />);
    const circles = screen.getByTestId("ring").querySelectorAll("circle");
    expect(circles).toHaveLength(2);
    const fill = circles[1];
    // 2π · 6.5 = 40.84; a quarter is 10.21.
    expect(fill.getAttribute("stroke-dasharray")).toBe("10.21 40.84");
    expect(fill.getAttribute("transform")).toBe("rotate(-90 9 9)");
    expect(fill.getAttribute("stroke")).toBe("var(--g5-ok)");
  });

  it("uses the warn and danger tokens for the arc color", () => {
    const { rerender } = render(
      <UsageRing utilization={0.6} label="7d" testId="ring" />,
    );
    expect(
      screen
        .getByTestId("ring")
        .querySelectorAll("circle")[1]
        .getAttribute("stroke"),
    ).toBe("var(--g5-warn)");
    rerender(<UsageRing utilization={0.95} label="7d" testId="ring" />);
    expect(
      screen
        .getByTestId("ring")
        .querySelectorAll("circle")[1]
        .getAttribute("stroke"),
    ).toBe("var(--g5-blocked)");
  });
});

describe("usagePercent / usageLevel", () => {
  it("clamps and rounds the fraction, and treats non-finite input as no data", () => {
    expect(usagePercent(1.4)).toBe(100);
    expect(usagePercent(-0.2)).toBe(0);
    expect(usagePercent(0.456)).toBe(46);
    expect(usagePercent(Number.NaN)).toBeNull();
    expect(usagePercent(null)).toBeNull();
    expect(usagePercent(undefined)).toBeNull();
  });

  it("maps whole percents onto ok / warn / danger / none", () => {
    expect(usageLevel(null)).toBe("none");
    expect(usageLevel(49)).toBe("ok");
    expect(usageLevel(50)).toBe("warn");
    expect(usageLevel(79)).toBe("warn");
    expect(usageLevel(80)).toBe("danger");
  });
});
