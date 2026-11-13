// =============================================================================
// Aero Glass theme — hook and registration contract
// =============================================================================
// The Aero Glass palette is an ordinary `[data-theme][data-mode]` pair (covered
// by the D7 contrast guard), but its glass treatment reaches the shell only
// through `data-surface` hooks rendered by components. This test keeps the
// stylesheet and the components in step: every surface the stylesheet styles
// is rendered somewhere, every rendered surface is styled, and the theme is
// registered consistently in the type union, the option list and the mock.
// =============================================================================

import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { THEME_OPTIONS } from "../lib/defaults";
import { THEME_OPTIONS as MOCK_THEME_OPTIONS } from "../data/mock";
import {
  collectSourceFiles,
  splitSelectorList,
  stripCssComments,
} from "./designTokenScanners";

const SRC_ROOT = join(__dirname, "..");
const rawCss = readFileSync(join(SRC_ROOT, "index.css"), "utf8");
const css = stripCssComments(rawCss);

function surfacesIn(text: string): Set<string> {
  return new Set(
    [...text.matchAll(/data-surface="([a-z-]+)"/g)].map((m) => m[1]),
  );
}

describe("aero-glass theme", () => {
  it("is registered in the option list, the mock and both palette modes", () => {
    expect(THEME_OPTIONS.map((t) => t.name)).toContain("aero-glass");
    expect(MOCK_THEME_OPTIONS.map((t) => t.name)).toEqual(
      THEME_OPTIONS.map((t) => t.name),
    );
    expect(css).toContain('[data-theme="aero-glass"][data-mode="dark"] {');
    expect(css).toContain('[data-theme="aero-glass"][data-mode="light"] {');
  });

  it("styles exactly the data-surface hooks the components render", () => {
    const styled = surfacesIn(css);
    const rendered = new Set<string>();
    for (const file of collectSourceFiles(SRC_ROOT)) {
      if (file.endsWith(".css") || /\.test\.tsx?$/.test(file)) continue;
      for (const s of surfacesIn(readFileSync(file, "utf8"))) rendered.add(s);
    }
    expect([...styled].sort()).toEqual([...rendered].sort());
    expect(styled.size).toBeGreaterThan(0);
  });

  it("paints the chrome's active tab on the tab's group, not the tab button", () => {
    // WorkspaceTabStrip renders each tab as a `role="tab"` button beside a
    // sibling close button inside a group. A pill painted on the button alone
    // stops before the close control, and a margin on that full-height button
    // overflows the tablist into a scrollbar.
    const rules = [
      ...css.matchAll(
        /\[data-theme="aero-glass"\]\s*\[data-surface="chrome"\]\s*([^{}]*\[role="tab"\]\[aria-selected="true"\][^{}]*)\{([^}]*)\}/g,
      ),
    ];
    expect(rules.length).toBeGreaterThan(0);
    for (const [, selector, body] of rules) {
      expect(body, selector).not.toMatch(/margin/);
      if (/background|border/.test(body)) {
        expect(selector).toMatch(
          /:has\(> \[role="tab"\]\[aria-selected="true"\]\)/,
        );
      }
    }
  });

  it("keeps its surface rules scoped to the theme and free of !important", () => {
    // Section boundaries come from the comment headers, so slice the raw
    // file and strip comments afterwards (a comment may mention !important).
    const start = rawCss.lastIndexOf(
      "/*",
      rawCss.indexOf("Aero Glass — Surface overrides"),
    );
    const end = rawCss.indexOf("Syntax Highlighting Classes", start);
    expect(start).toBeGreaterThan(0);
    const section = stripCssComments(rawCss.slice(start, end));
    expect(section).not.toContain("!important");
    for (const match of section.matchAll(/^([^@{}\n][^{}]*)\{/gm)) {
      for (const selector of splitSelectorList(match[1])) {
        expect(selector, `unscoped selector: ${selector}`).toMatch(
          /\[data-theme="aero-glass"\]/,
        );
      }
    }
  });
});
