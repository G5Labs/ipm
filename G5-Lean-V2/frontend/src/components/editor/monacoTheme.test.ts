// =============================================================================
// monacoTheme — token colour conversion
// =============================================================================
// The g5 Monaco themes are built from the page's `--g5-*` tokens. Monaco's
// token colour map accepts only `#rrggbb` or `#rrggbbaa`; the production CSS
// minifier rewrites some authored `rgb()` values as short hex (`#fff`), which
// crashed every Monaco mount on Aero Glass light (issue #733).
// =============================================================================

import { afterEach, describe, expect, it, vi } from "vitest";
import { defineG5Themes, rgbToHexHash } from "./monacoTheme";

/** Monaco's `ColorMap.getId` acceptance test (tokenization.ts). */
const MONACO_TOKEN_COLOR = /^#?([0-9A-Fa-f]{6})([0-9A-Fa-f]{2})?$/;

describe("rgbToHexHash", () => {
  it("converts authored rgb() tokens, preserving alpha", () => {
    expect(rgbToHexHash("rgb(240 246 255)")).toBe("#f0f6ff");
    expect(rgbToHexHash("rgb(12, 19, 33)")).toBe("#0c1321");
    expect(rgbToHexHash("rgb(240 246 255 / 0.45)")).toBe("#f0f6ff73");
  });

  it("expands the minifier's short hex, which Monaco rejects", () => {
    expect(MONACO_TOKEN_COLOR.test("#fff")).toBe(false);
    expect(rgbToHexHash("#fff")).toBe("#ffffff");
    expect(rgbToHexHash("#FfF")).toBe("#FFffFF");
    expect(rgbToHexHash("#fff8")).toBe("#ffffff88");
  });

  it("passes 6- and 8-digit hex through", () => {
    expect(rgbToHexHash("#0c1321")).toBe("#0c1321");
    expect(rgbToHexHash("#f0f6ff80")).toBe("#f0f6ff80");
  });

  it("falls back to black for empty or unparseable values", () => {
    expect(rgbToHexHash("")).toBe("#000000");
    expect(rgbToHexHash("white")).toBe("#000000");
    expect(rgbToHexHash("#ggg")).toBe("#000000");
    expect(rgbToHexHash("#12345")).toBe("#000000");
  });

  it("always yields a colour Monaco accepts", () => {
    for (const value of [
      "rgb(255 255 255)",
      "rgb(255 255 255 / 0.14)",
      "#fff",
      "#fff8",
      "#ffffff",
      "#ffffff80",
      "",
      "transparent",
    ]) {
      expect(rgbToHexHash(value), value).toMatch(MONACO_TOKEN_COLOR);
    }
  });
});

afterEach(() => document.documentElement.removeAttribute("style"));

it.each([
  ["rgb(255 255 255 / 0.08)", "#ffffff14"],
  ["rgba(0, 0, 0, 0.06)", "#0000000f"],
  ["rgb(255 255 255 / 8%)", "#ffffff14"],
  ["rgb(100% 100% 100% / 8%)", "#ffffff14"],
])(
  "keeps %s translucent in both registered Monaco themes",
  (token, expected) => {
    document.documentElement.style.setProperty("--g5-overlay-hover", token);
    document.documentElement.style.setProperty("--g5-editor", "#fff");
    const defineTheme = vi.fn();
    defineG5Themes({ editor: { defineTheme } });
    expect(defineTheme.mock.calls.map(([name]) => name)).toEqual([
      "g5-light",
      "g5-dark",
    ]);
    for (const [, theme] of defineTheme.mock.calls) {
      expect(theme.colors["editor.lineHighlightBackground"]).toBe(expected);
      expect(theme.colors["editor.background"]).toBe("#ffffff");
    }
  },
);
