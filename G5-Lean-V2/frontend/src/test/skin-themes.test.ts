// =============================================================================
// Skin themes — registration and scoping contract
// =============================================================================
// A skin is an ordinary `[data-theme][data-mode]` palette pair (covered by
// the D7 contrast guard) plus a surface section that reaches the shell only
// through the `data-surface` hooks Aero Glass already uses. This test keeps
// each skin registered consistently and keeps its rules scoped to the theme,
// so a skin cannot leak into other themes or into component code.
// =============================================================================

import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { THEME_OPTIONS } from "../lib/defaults";
import { THEME_OPTIONS as MOCK_THEME_OPTIONS } from "../data/mock";
import { splitSelectorList, stripCssComments } from "./designTokenScanners";

const rawCss = readFileSync(join(__dirname, "..", "index.css"), "utf8");
const css = stripCssComments(rawCss);

/** Each skin: its theme id, section header and theme-only knob prefix. */
const SKINS = [
  {
    id: "classic-98",
    header: "Old school — Surface overrides",
    knob: "--c98-",
  },
  { id: "green-room", header: "Green room — Surface overrides", knob: "--jb-" },
] as const;

/** A skin's surface section: from its header to the next section header. */
function section(header: string): string {
  const headerAt = rawCss.indexOf(header);
  expect(headerAt, header).toBeGreaterThan(0);
  const start = rawCss.lastIndexOf("/*", headerAt);
  const end = rawCss.indexOf("— Surface overrides", headerAt + header.length);
  expect(end).toBeGreaterThan(start);
  return stripCssComments(rawCss.slice(start, rawCss.lastIndexOf("/*", end)));
}

const aeroSurfaces = new Set(
  [
    ...css.matchAll(
      /\[data-theme="aero-glass"\][^{]*?data-surface="([a-z-]+)"/g,
    ),
  ].map((m) => m[1]),
);

describe.each(SKINS)("skin theme $id", ({ id, header, knob }) => {
  it("is registered in the option list, the mock and both palette modes", () => {
    expect(THEME_OPTIONS.map((t) => t.name)).toContain(id);
    expect(MOCK_THEME_OPTIONS.map((t) => t.name)).toEqual(
      THEME_OPTIONS.map((t) => t.name),
    );
    expect(css).toContain(`[data-theme="${id}"][data-mode="dark"] {`);
    expect(css).toContain(`[data-theme="${id}"][data-mode="light"] {`);
  });

  it("uses only data-surface hooks that Aero Glass also styles", () => {
    // Skins add no component hooks; every surface they paint is one the
    // shell already renders for Aero Glass (aero-glass-theme.test.ts checks
    // that set against the components).
    const own = new Set(
      [...section(header).matchAll(/data-surface="([a-z-]+)"/g)].map(
        (m) => m[1],
      ),
    );
    expect(own.size).toBeGreaterThan(0);
    for (const s of own) expect(aeroSurfaces, `surface ${s}`).toContain(s);
  });

  it("keeps its surface rules scoped to the theme and free of !important", () => {
    const text = section(header);
    expect(text).not.toContain("!important");
    expect(text).not.toMatch(/url\(/); // no bundled images or fonts
    for (const match of text.matchAll(/^([^@{}\n][^{}]*)\{/gm)) {
      for (const selector of splitSelectorList(match[1])) {
        expect(selector, `unscoped selector: ${selector}`).toMatch(
          new RegExp(`\\[data-theme="${id}"\\]`),
        );
      }
    }
  });

  it("keeps theme-only knobs out of the --g5- token namespace", () => {
    // designTokenScanners counts every `--g5-*` token per palette; a skin's
    // private knobs use their own prefix so they do not widen the contract.
    const dark = css.slice(
      css.indexOf(`[data-theme="${id}"][data-mode="dark"] {`),
      css.indexOf(`[data-theme="${id}"][data-mode="light"] {`),
    );
    expect(dark).toContain(knob);
    expect(section(header)).not.toMatch(/--g5-[a-z0-9-]+:/);
  });
});

describe("skin scoping helper", () => {
  it("checks every member of a selector list, not just the first", () => {
    expect(
      splitSelectorList(
        '[data-theme="x"] :is(a, button), [data-theme="x"] [data-surface="y"], button',
      ),
    ).toEqual([
      '[data-theme="x"] :is(a, button)',
      '[data-theme="x"] [data-surface="y"]',
      "button",
    ]);
  });
});
