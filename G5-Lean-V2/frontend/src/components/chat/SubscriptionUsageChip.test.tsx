import { render, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import {
  SubscriptionUsageChip,
  shouldRenderSubscriptionUsageChip,
} from "./SubscriptionUsageChip";
import type { AgentAuthModeV1, SubscriptionUsage } from "../../lib/types";

// Fixed clock so capture-age and reset-relative rendering are deterministic.
const NOW = Date.parse("2026-08-19T12:00:00Z");

function usage(overrides: Partial<SubscriptionUsage> = {}): SubscriptionUsage {
  return {
    fiveHourUtilization: 0.5,
    fiveHourResetAt: "2026-08-19T15:00:00Z",
    sevenDayUtilization: 0.38,
    sevenDayResetAt: "2026-08-24T12:00:00Z",
    representativeClaim: "five_hour",
    unifiedStatus: "allowed",
    capturedAt: "2026-08-19T11:59:00Z", // 1 min ago — fresh
    ...overrides,
  };
}

function session(
  authMode: AgentAuthModeV1 | null | undefined,
  snapshot: SubscriptionUsage | null | undefined,
) {
  return { authMode, subscriptionUsage: snapshot };
}

describe("shouldRenderSubscriptionUsageChip (agent_interaction_ux D8)", () => {
  it("renders Codex subscription usage while excluding API-key billing", () => {
    expect(
      shouldRenderSubscriptionUsageChip(
        session("chat_gpt_subscription", usage()),
      ),
    ).toBe(true);
    expect(
      shouldRenderSubscriptionUsageChip(session("open_ai_api_key", usage())),
    ).toBe(false);
  });
  it("renders for an Anthropic OAuth session with a snapshot", () => {
    expect(
      shouldRenderSubscriptionUsageChip(session("anthropic_oauth", usage())),
    ).toBe(true);
  });

  it("is hidden for a non-OAuth (api key) session even with a snapshot", () => {
    expect(
      shouldRenderSubscriptionUsageChip(session("anthropic_api_key", usage())),
    ).toBe(false);
  });

  it("is hidden for an OAuth session when the snapshot is missing", () => {
    expect(
      shouldRenderSubscriptionUsageChip(session("anthropic_oauth", null)),
    ).toBe(false);
    expect(
      shouldRenderSubscriptionUsageChip(session("anthropic_oauth", undefined)),
    ).toBe(false);
  });

  it("is hidden when auth mode is absent", () => {
    expect(shouldRenderSubscriptionUsageChip(session(null, usage()))).toBe(
      false,
    );
  });
});

describe("SubscriptionUsageChip (agent_interaction_ux D8)", () => {
  it("hides the meter when all readings are expired or malformed", () => {
    render(
      <SubscriptionUsageChip
        usage={usage({
          sevenDayResetAt: "2026-08-19T11:59:00Z",
          fiveHourUtilization: -1,
        })}
        nowMs={NOW}
      />,
    );
    expect(
      screen.queryByTestId("subscription-usage-chip"),
    ).not.toBeInTheDocument();
  });

  it.each(["5h", "7d"] as const)(
    "shows only the available 7-day window when %s is preferred",
    (preferred) => {
      render(
        <SubscriptionUsageChip
          usage={usage({ fiveHourUtilization: null })}
          window={preferred}
          nowMs={NOW}
        />,
      );
      expect(
        screen.queryByTestId("usage-tooltip-window-5h"),
      ).not.toBeInTheDocument();
      expect(screen.getByTestId("usage-tooltip-window-7d")).toHaveTextContent(
        "38% used",
      );
      expect(screen.getByTestId("usage-tooltip-window-7d")).toHaveAttribute(
        "data-selected",
        "true",
      );
      expect(screen.getByRole("button")).toHaveAccessibleName(
        "Subscription usage: 62% left in the 7-day window",
      );
      expect(screen.getByTestId("subscription-usage-chip")).toHaveAttribute(
        "data-window",
        "7d",
      );
    },
  );

  it("uses an available zero-usage 5-hour window when the preferred 7-day window is missing", () => {
    render(
      <SubscriptionUsageChip
        usage={usage({ fiveHourUtilization: 0, sevenDayUtilization: null })}
        nowMs={NOW}
      />,
    );
    expect(
      screen.queryByTestId("usage-tooltip-window-7d"),
    ).not.toBeInTheDocument();
    expect(screen.getByTestId("usage-tooltip-window-5h")).toHaveTextContent(
      "0% used",
    );
    expect(screen.getByTestId("usage-tooltip-window-5h")).toHaveAttribute(
      "data-selected",
      "true",
    );
    expect(screen.getByRole("button")).toHaveAccessibleName(
      "Subscription usage: 100% left in the 5-hour window",
    );
    expect(screen.getByTestId("subscription-usage-chip")).toHaveAttribute(
      "data-window",
      "5h",
    );
  });
  it("renders remaining budget for the default 7-day window (0.38 -> 62% left)", () => {
    render(<SubscriptionUsageChip usage={usage()} nowMs={NOW} />);
    expect(screen.getByTestId("subscription-usage-headline")).toHaveTextContent(
      "62% left",
    );
    // Default window is 7d.
    expect(screen.getByTestId("subscription-usage-chip")).toHaveAttribute(
      "data-window",
      "7d",
    );
  });

  it("computes remaining from the selected 5-hour window when requested", () => {
    render(<SubscriptionUsageChip usage={usage()} window="5h" nowMs={NOW} />);
    // 1 - 0.5 = 0.5 -> 50% left
    expect(screen.getByTestId("subscription-usage-headline")).toHaveTextContent(
      "50% left",
    );
  });

  it("uses ok tone at healthy remaining budget", () => {
    render(<SubscriptionUsageChip usage={usage()} nowMs={NOW} />);
    expect(screen.getByTestId("subscription-usage-chip")).toHaveAttribute(
      "data-tone",
      "ok",
    );
  });

  it("uses amber tone below 20% left", () => {
    // sevenDayUtilization 0.85 -> 15% left -> amber
    render(
      <SubscriptionUsageChip
        usage={usage({ sevenDayUtilization: 0.85 })}
        nowMs={NOW}
      />,
    );
    expect(screen.getByTestId("subscription-usage-chip")).toHaveAttribute(
      "data-tone",
      "amber",
    );
  });

  it("uses red tone below 10% left", () => {
    // sevenDayUtilization 0.95 -> 5% left -> red
    render(
      <SubscriptionUsageChip
        usage={usage({ sevenDayUtilization: 0.95 })}
        nowMs={NOW}
      />,
    );
    expect(screen.getByTestId("subscription-usage-chip")).toHaveAttribute(
      "data-tone",
      "red",
    );
  });

  it("tooltip shows both windows with utilization and marks the selected one", () => {
    render(<SubscriptionUsageChip usage={usage()} nowMs={NOW} />);
    const tooltip = screen.getByTestId("subscription-usage-tooltip");
    const fiveHour = within(tooltip).getByTestId("usage-tooltip-window-5h");
    const sevenDay = within(tooltip).getByTestId("usage-tooltip-window-7d");
    expect(fiveHour).toHaveTextContent("50% used");
    expect(sevenDay).toHaveTextContent("38% used");
    // 7d is the default selected window.
    expect(sevenDay).toHaveAttribute("data-selected", "true");
    expect(fiveHour).toHaveAttribute("data-selected", "false");
  });

  it("shows a stale dot and an 'as of' capture age when the snapshot is old", () => {
    // captured 20 min ago (> 15 min) -> stale
    render(
      <SubscriptionUsageChip
        usage={usage({ capturedAt: "2026-08-19T11:40:00Z" })}
        nowMs={NOW}
      />,
    );
    expect(screen.getByTestId("subscription-usage-chip")).toHaveAttribute(
      "data-stale",
      "true",
    );
    expect(screen.getByTestId("subscription-usage-stale-dot")).toBeVisible();
    expect(
      screen.getByTestId("subscription-usage-capture-age"),
    ).toHaveTextContent(/as of/i);
  });

  it("does not show the stale marker for a fresh snapshot", () => {
    render(<SubscriptionUsageChip usage={usage()} nowMs={NOW} />);
    expect(screen.getByTestId("subscription-usage-chip")).toHaveAttribute(
      "data-stale",
      "false",
    );
    expect(
      screen.queryByTestId("subscription-usage-stale-dot"),
    ).not.toBeInTheDocument();
    expect(
      screen.getByTestId("subscription-usage-capture-age"),
    ).toHaveTextContent(/captured/i);
  });

  it("hides a window after reset and uses the remaining available window", () => {
    // 5-hour reset already passed relative to NOW.
    render(
      <SubscriptionUsageChip
        usage={usage({ fiveHourResetAt: "2026-08-19T09:00:00Z" })}
        window="5h"
        nowMs={NOW}
      />,
    );
    expect(
      screen.queryByTestId("usage-tooltip-window-5h"),
    ).not.toBeInTheDocument();
    expect(screen.getByTestId("subscription-usage-headline")).toHaveTextContent(
      "62% left",
    );
    expect(screen.getByTestId("usage-tooltip-window-7d")).toHaveAttribute(
      "data-selected",
      "true",
    );
  });
});
