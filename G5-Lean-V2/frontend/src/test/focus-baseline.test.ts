import { describe, expect, it } from "vitest";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { basename, join, relative } from "node:path";

// Guard for the global keyboard-focus baseline (design-system D16,
// test_contract/keyboard_focus_baseline I1; checklist-design audit
// 2026-08-11 finding 3): the app-wide focus indicator is a base-layer
// `:focus-visible` rule in index.css, ring offsets default to a themed
// surface token instead of Tailwind's white, and component focus-indicator
// styles are keyboard-gated. Four source invariants:
//   1. index.css defines the global :focus-visible outline on --g5-info
//      with a non-zero width and offset, and the --tw-ring-offset-color
//      default bound to a --g5-* token — both INSIDE an @layer base block
//      (unlayered author CSS would beat Tailwind's utilities layer, so
//      focus-visible:outline-none could never override the global rule);
//   2. no `focus:`-gated ring variant (rings must not fire on mouse focus);
//   3. no `focus:outline-none` and no bare `outline-none` — outline
//      suppression only in the keyboard-gated `focus-visible:outline-none`
//      form;
//   4. every `focus-visible:outline-none` carries a same-state replacement
//      marker (focus-visible:ring / focus:border- / focus:bg-) in its
//      immediate class expression, so suppression never leaves keyboard
//      focus invisible.

const SRC_ROOT = join(__dirname, "..");
const SOURCE_EXTENSIONS = new Set([".ts", ".tsx", ".css"]);
const GUARD_FILE = basename(__filename);

// Comment text is blanked (not removed) so violation line numbers computed
// from the stripped content still match the real file.
function blankComment(comment: string): string {
  return comment.replace(/[^\r\n]/g, " ");
}

function stripSourceComments(content: string, isCss: boolean): string {
  const noBlock = content.replace(/\/\*[\s\S]*?\*\//g, blankComment);
  if (isCss) return noBlock;
  return noBlock.replace(
    /(^|[^:])\/\/[^\n]*/g,
    (m, pre: string) => `${pre}${blankComment(m.slice(pre.length))}`,
  );
}

// Extract the concatenated contents of every top-level `@layer base { ... }`
// block via brace counting (the blocks nest rules, so a flat regex is not
// enough).
export function extractLayerBaseBlocks(css: string): string {
  const out: string[] = [];
  const open = /@layer\s+base\s*\{/g;
  for (const match of css.matchAll(open)) {
    let depth = 1;
    let i = (match.index ?? 0) + match[0].length;
    const start = i;
    while (i < css.length && depth > 0) {
      if (css[i] === "{") depth += 1;
      else if (css[i] === "}") depth -= 1;
      i += 1;
    }
    out.push(css.slice(start, i - 1));
  }
  return out.join("\n");
}

function listSourceFiles(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) {
      out.push(...listSourceFiles(full));
      continue;
    }
    const dot = entry.lastIndexOf(".");
    if (dot === -1) continue;
    if (!SOURCE_EXTENSIONS.has(entry.slice(dot))) continue;
    if (entry === GUARD_FILE) continue;
    out.push(full);
  }
  return out;
}

interface Violation {
  file: string;
  line: number;
  match: string;
}

type LineCheck = (lines: string[], lineIndex: number) => boolean;

function scanViolations(pattern: RegExp, extraCheck?: LineCheck): Violation[] {
  const violations: Violation[] = [];
  for (const file of listSourceFiles(SRC_ROOT)) {
    const raw = readFileSync(file, "utf8");
    const content = stripSourceComments(raw, file.endsWith(".css"));
    const lines = content.split("\n");
    for (const match of content.matchAll(pattern)) {
      // RegExpMatchArray.index is optional in lib types; matchAll always
      // sets it, but narrow explicitly for strict mode.
      const line = content.slice(0, match.index ?? 0).split("\n").length;
      if (extraCheck && extraCheck(lines, line - 1)) continue;
      violations.push({
        file: relative(SRC_ROOT, file),
        line,
        match: match[0].trim(),
      });
    }
  }
  return violations;
}

function formatViolations(violations: Violation[]): string {
  return violations.map((v) => `${v.file}:${v.line} — ${v.match}`).join("\n");
}

const indexCss = stripSourceComments(
  readFileSync(join(SRC_ROOT, "index.css"), "utf8"),
  true,
);
const layerBase = extractLayerBaseBlocks(indexCss);

describe("keyboard focus baseline (design-system D16)", () => {
  it("index.css defines the global :focus-visible outline on --g5-info inside @layer base", () => {
    const rule = layerBase.match(/:focus-visible\s*\{([^}]*)\}/);
    expect(
      rule,
      "no :focus-visible rule inside an @layer base block of index.css",
    ).not.toBeNull();
    // Exact D16 values: 2px solid --g5-info at 2px offset — a thinner
    // outline or different token is a contract change, not a variation.
    const body = rule![1];
    expect(body).toMatch(/outline:\s*2px\s+solid\s+var\(--g5-info\)/);
    expect(body).toMatch(/outline-offset:\s*2px/);
  });

  it("index.css defaults --tw-ring-offset-color to --g5-bg1 inside @layer base", () => {
    expect(layerBase).toMatch(/--tw-ring-offset-color:\s*var\(--g5-bg1\)/);
  });

  it("no focus:-gated ring variants (rings must not fire on mouse focus)", () => {
    // "focus:ring" matches only the mouse-firing variant; the keyboard-gated
    // form reads "focus-visible:ring" and never contains this substring.
    const violations = scanViolations(/(?<![\w-])focus:ring[\w[\]()/.-]*/g);
    expect(
      violations,
      `focus:ring-* fires on mouse focus; use focus-visible:ring-* (D16):\n${formatViolations(violations)}`,
    ).toEqual([]);
  });

  it("no focus:outline-none (suppression must be keyboard-gated)", () => {
    const violations = scanViolations(/(?<![\w-])focus:outline-none/g);
    expect(
      violations,
      `focus:outline-none suppresses the indicator on any focus; use focus-visible:outline-none with a replacement (D16):\n${formatViolations(violations)}`,
    ).toEqual([]);
  });

  it("only the focus-visible: variant may carry outline-none", () => {
    // Flags bare outline-none AND any other variant gating (hover:, md:,
    // focus-within:, …) — focus-visible:outline-none (checked by the pairing
    // test below) is the single allowed form.
    const violations = scanViolations(
      /(?<![\w-])(?<!focus-visible:)outline-none\b/g,
    );
    expect(
      violations,
      `bare outline-none removes the focus indicator in every state; delete it or gate it as focus-visible:outline-none with a replacement (D16):\n${formatViolations(violations)}`,
    ).toEqual([]);
  });

  it("every focus-visible:outline-none is paired with a same-state replacement", () => {
    // The replacement must live in the same class expression. Class lists in
    // this codebase are single template/string literals or classNames(...)
    // arrays whose entries sit on adjacent lines, so a ±3-line window covers
    // the expression without reaching a neighboring element's classes.
    const REPLACEMENT = /focus-visible:ring|focus:border-|focus:bg-/;
    const hasReplacementNearby: LineCheck = (lines, lineIndex) => {
      const window = lines
        .slice(Math.max(0, lineIndex - 3), lineIndex + 4)
        .join("\n");
      return REPLACEMENT.test(window);
    };
    const violations = scanViolations(
      /focus-visible:outline-none/g,
      hasReplacementNearby,
    );
    expect(
      violations,
      `focus-visible:outline-none without a same-state replacement (focus-visible:ring / focus:border- / focus:bg-) leaves keyboard focus invisible (D16):\n${formatViolations(violations)}`,
    ).toEqual([]);
  });
});
