// =============================================================================
// Design-Token Contrast Guard (design_system D7)
// =============================================================================
// Computes WCAG 2.x contrast for the text-bearing token pairs of every theme
// palette in index.css and fails when any palette drops below the contract:
//
//   - text tiers --g5-text0/1/2 over surfaces --g5-bg0/1/2  →  ≥ 4.5:1 (AA
//     normal text; alpha-carrying tiers are composited over each surface
//     first, which is exactly what the browser paints)
//   - --g5-linenum over --g5-gutter, --g5-editor and --g5-bg0 → ≥ 4.5:1.
//     Line numbers render at 12–13px (CodeEditor.tsx, ArtifactRenderer.tsx),
//     which is WCAG normal-size text, so the 3:1 large-text exception does
//     not apply; ArtifactRenderer paints its gutter on --g5-bg0, so that
//     surface is in scope alongside the editor surfaces.
//
// Checklist-design audit 2026-08-11: --g5-text2 failed AA in 14/18 palettes
// and --g5-linenum sat at 1.98–2.77 in all 18. This guard makes those
// regressions impossible to reintroduce silently.
//
// Tokens are resolved per palette with a fallback to :root — custom
// properties inherit, so a token declared only in :root still paints inside
// every palette.
// =============================================================================

import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import {
  parsePaletteValues,
  parseRootTokenValues,
} from "./designTokenScanners";

const TEXT_TIER_MIN = 4.5;
const LINENUM_MIN = 4.5;

interface Rgba {
  r: number;
  g: number;
  b: number;
  a: number;
}

// Palette color tokens use the space-separated rgb() form, optionally with
// `/ alpha` (enforced by the assertions below — a new syntax must extend the
// parser, not silently skip the pair).
export function parseRgb(value: string): Rgba | null {
  const m = value.match(
    /^rgb\(\s*(\d+)\s+(\d+)\s+(\d+)\s*(?:\/\s*([\d.]+))?\)$/,
  );
  if (!m) return null;
  return { r: +m[1], g: +m[2], b: +m[3], a: m[4] === undefined ? 1 : +m[4] };
}

function linearize(channel: number): number {
  const c = channel / 255;
  return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
}

export function relativeLuminance({ r, g, b }: Rgba): number {
  return 0.2126 * linearize(r) + 0.7152 * linearize(g) + 0.0722 * linearize(b);
}

/** Composite a (possibly translucent) foreground over an opaque background. */
export function composite(fg: Rgba, bg: Rgba): Rgba {
  return {
    r: fg.a * fg.r + (1 - fg.a) * bg.r,
    g: fg.a * fg.g + (1 - fg.a) * bg.g,
    b: fg.a * fg.b + (1 - fg.a) * bg.b,
    a: 1,
  };
}

/** WCAG contrast ratio of fg text composited over an opaque bg surface. */
export function contrastRatio(fg: Rgba, bg: Rgba): number {
  const f = relativeLuminance(composite(fg, bg));
  const s = relativeLuminance(bg);
  return (Math.max(f, s) + 0.05) / (Math.min(f, s) + 0.05);
}

describe("contrast math", () => {
  it("matches WCAG reference values", () => {
    const white: Rgba = { r: 255, g: 255, b: 255, a: 1 };
    const black: Rgba = { r: 0, g: 0, b: 0, a: 1 };
    expect(contrastRatio(white, black)).toBeCloseTo(21, 5);
    expect(contrastRatio(black, white)).toBeCloseTo(21, 5);
    expect(contrastRatio(white, white)).toBeCloseTo(1, 5);
    // #767676 on white is the canonical just-passing AA gray (~4.54:1).
    expect(
      contrastRatio({ r: 118, g: 118, b: 118, a: 1 }, white),
    ).toBeGreaterThan(4.5);
    expect(contrastRatio({ r: 128, g: 128, b: 128, a: 1 }, white)).toBeLessThan(
      4.5,
    );
  });

  it("composites alpha before measuring", () => {
    const bg: Rgba = { r: 0, g: 0, b: 0, a: 1 };
    const halfWhite: Rgba = { r: 255, g: 255, b: 255, a: 0.5 };
    const opaque = contrastRatio({ r: 128, g: 128, b: 128, a: 1 }, bg);
    const translucent = contrastRatio(halfWhite, bg);
    // 50% white over black composites to mid-gray, not white.
    expect(Math.abs(translucent - opaque)).toBeLessThan(0.15);
  });
});

describe("--g5-* palette contrast contract (D7)", () => {
  const css = readFileSync(join(__dirname, "..", "index.css"), "utf8");
  const palettes = parsePaletteValues(css);
  const rootValues = parseRootTokenValues(css);

  it("finds all 24 theme palettes", () => {
    expect(palettes.size).toBe(24);
  });

  const color = (values: Map<string, string>, token: string): Rgba => {
    // Palette-local value wins; a :root declaration inherits into every
    // palette, so fall back there before treating the token as undefined.
    const raw = values.get(token) ?? rootValues.get(token);
    expect(
      raw,
      `${token} must be defined in the palette or :root`,
    ).toBeDefined();
    const parsed = parseRgb(raw as string);
    expect(
      parsed,
      `${token} value "${raw}" must be parseable rgb()`,
    ).not.toBeNull();
    return parsed as Rgba;
  };

  it(`text tiers text0/1/2 hold ≥ ${TEXT_TIER_MIN}:1 over bg0/1/2 in every palette`, () => {
    const offenders: string[] = [];
    for (const [selector, values] of palettes) {
      const surfaces = ["--g5-bg0", "--g5-bg1", "--g5-bg2"].map((t) => ({
        name: t,
        rgba: color(values, t),
      }));
      for (const tier of ["--g5-text0", "--g5-text1", "--g5-text2"]) {
        const fg = color(values, tier);
        for (const surface of surfaces) {
          const ratio = contrastRatio(fg, surface.rgba);
          if (ratio < TEXT_TIER_MIN) {
            offenders.push(
              `${selector}: ${tier} over ${surface.name} = ${ratio.toFixed(2)}:1`,
            );
          }
        }
      }
    }
    expect(
      offenders,
      `text tiers below AA ${TEXT_TIER_MIN}:1:\n${offenders.join("\n")}`,
    ).toEqual([]);
  });

  it(`--g5-linenum holds ≥ ${LINENUM_MIN}:1 over gutter, editor and bg0 in every palette`, () => {
    const offenders: string[] = [];
    for (const [selector, values] of palettes) {
      const fg = color(values, "--g5-linenum");
      for (const surfaceToken of ["--g5-gutter", "--g5-editor", "--g5-bg0"]) {
        const surface = color(values, surfaceToken);
        const ratio = contrastRatio(fg, surface);
        if (ratio < LINENUM_MIN) {
          offenders.push(
            `${selector}: --g5-linenum over ${surfaceToken} = ${ratio.toFixed(2)}:1`,
          );
        }
      }
    }
    expect(
      offenders,
      `line numbers below ${LINENUM_MIN}:1:\n${offenders.join("\n")}`,
    ).toEqual([]);
  });
});
