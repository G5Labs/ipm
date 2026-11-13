// =============================================================================
// axe-core Test Helper — Automated Accessibility Assertions in jsdom
// =============================================================================
// Runs the axe-core engine against a rendered DOM subtree and returns
// formatted violation strings so a test can assert `toEqual([])` and get a
// readable failure listing every offending node.
//
// jsdom has no layout or paint engine, so rules that depend on computed
// geometry or resolved colors cannot produce trustworthy results there and
// are disabled. Color contrast is enforced separately at the token level by
// design-token-contrast.test.ts (design_system D7), and in real browsers by
// the Playwright axe scan (e2e/flows/a11y-scan.spec.ts).
// =============================================================================

import axe from "axe-core";

const JSDOM_UNSUPPORTED_RULES = [
  // Needs real color resolution + pixel geometry; jsdom reports none.
  "color-contrast",
  // Needs layout to detect scrollable overflow regions.
  "scrollable-region-focusable",
];

export interface AxeOptions {
  /** Additional rule ids to disable for one call (document why at the call site). */
  disableRules?: string[];
}

/**
 * Run axe-core on `node` and return one formatted string per violation.
 * Assert with: `expect(await axeViolations(container)).toEqual([])`.
 */
export async function axeViolations(
  node: Element,
  options: AxeOptions = {},
): Promise<string[]> {
  const disabled = [
    ...JSDOM_UNSUPPORTED_RULES,
    ...(options.disableRules ?? []),
  ];
  const results = await axe.run(node, {
    rules: Object.fromEntries(disabled.map((id) => [id, { enabled: false }])),
  });
  return results.violations.map((violation) => {
    const nodes = violation.nodes
      .map(
        (n) =>
          `    ${n.html}${n.failureSummary ? `\n      ${n.failureSummary.replace(/\n/g, "\n      ")}` : ""}`,
      )
      .join("\n");
    return `${violation.id} (${violation.impact ?? "unknown"}): ${violation.help}\n${nodes}`;
  });
}
