// =============================================================================
// Automated Accessibility Scan — axe-core over real rendered surfaces
// =============================================================================
// Runs the axe-core engine (via @axe-core/playwright) against core surfaces
// in a real browser, where layout and resolved colors exist — complementing
// the jsdom component scans (src/test/a11y.test.tsx) and the token-level
// contrast guard (src/test/design-token-contrast.test.ts).
//
// Ratchet contract (same shape as the eslint jsx-a11y adoption): violations
// of critical/serious impact fail the scan, except rule ids recorded in the
// per-page KNOWN_FAILING allowlist at adoption time (checklist-design audit
// 2026-08-11 debt). Fixing a rule means deleting it from the allowlist so it
// cannot regress. Never add a rule id here to make a new feature pass.
// =============================================================================

import { test, expect, type Page } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

const ENFORCED_IMPACTS = new Set(["critical", "serious"]);

async function enforcedViolations(page: Page, knownFailing: string[]) {
  const results = await new AxeBuilder({ page }).analyze();
  return results.violations
    .filter((v) => ENFORCED_IMPACTS.has(v.impact ?? ""))
    .filter((v) => !knownFailing.includes(v.id))
    .map(
      (v) =>
        `${v.id} (${v.impact}): ${v.help}\n` +
        v.nodes.map((n) => `    ${n.html}`).join("\n"),
    );
}

test.describe("axe accessibility scan", () => {
  test.describe("unauthenticated", () => {
    // Fixture-level fresh state (login.spec.ts pattern) keeps the fixture
    // page — and with it the configured baseURL and automatic context
    // cleanup — instead of a hand-built browser context.
    test.use({ storageState: { cookies: [], origins: [] } });

    test("login page has no new critical/serious violations", async ({
      page,
    }) => {
      await page.goto("/login");
      // Prove the surface actually rendered before scanning — network quiet
      // alone would let axe pass trivially on a blank or redirected page.
      // Same public-CTA alternatives as login.spec.ts (provider link, dev
      // login, or the explicit no-providers message).
      await expect(
        page
          .getByRole("link", { name: /continue with github/i })
          .or(page.getByRole("button", { name: /dev login/i }))
          .or(page.getByText(/no sign-in providers are configured/i))
          .first(),
      ).toBeVisible({ timeout: 30_000 });
      const knownFailing: string[] = [];
      expect(await enforcedViolations(page, knownFailing)).toEqual([]);
    });
  });

  test("projects page has no new critical/serious violations", async ({
    page,
  }) => {
    await page.goto("/projects", { waitUntil: "domcontentloaded" });
    // Page-specific readiness (workspace-create.spec.ts pattern): the
    // projects heading or the New Project control, not just any heading —
    // a generic assertion would pass on a redirect target and scan the
    // wrong surface.
    await expect(
      page
        .getByRole("heading", { name: /projects/i })
        .or(page.getByRole("button", { name: /new project/i }))
        .first(),
    ).toBeVisible({ timeout: 30_000 });
    const knownFailing: string[] = [];
    expect(await enforcedViolations(page, knownFailing)).toEqual([]);
  });
});
