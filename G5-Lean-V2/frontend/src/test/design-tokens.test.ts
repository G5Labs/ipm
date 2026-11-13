import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { join, relative } from "node:path";
// Scanners live in designTokenScanners.ts (shared with the D7 contrast
// guard); the table-driven tests below still exercise them directly.
import {
  collectSourceFiles,
  stripCssComments,
  stripSourceComments,
  parseTokenDefinitions,
  findVarReferences,
  findFallbackReferences,
  findQuotedTokenNames,
} from "./designTokenScanners";

// Guard for the design-system token contract (design_system D1/D6, issue
// #775): an undefined --g5-* custom property is invalid at computed-value
// time, so background-color degrades to transparent and color to inheritance
// with no runtime error — the breakage only shows up as invisible/off-theme
// UI in one mode. Four invariants, per D6:
//   1. every var(--g5-…) reference names a defined token;
//   2. no --g5-* reference carries any fallback (the bare no-fallback form
//      is the only valid one — the contract has no sanctioned fallback use);
//   3. every programmatic token name (quoted "--g5-…" string in TS/TSX)
//      names a defined token;
//   4. every referenced token is defined in :root or in ALL theme×mode
//      palette blocks, so the 18-palette guarantee holds per palette.

const SRC_ROOT = join(__dirname, "..");

/** Read a source file with its comments stripped, ready for the token scans. */
function readForScan(file: string): string {
  return stripSourceComments(readFileSync(file, "utf8"), file.endsWith(".css"));
}

// This file's own table fixtures build token strings by concatenation so the
// repo-wide scan below never matches them as literal references.
const T = (name: string) => "--g5-" + name;
const VAR = (name: string, fallback?: string) =>
  "var(" + T(name) + (fallback === undefined ? ")" : `, ${fallback})`);

// ---------------------------------------------------------------------------
// Scanner unit tests (table-driven)
// ---------------------------------------------------------------------------

describe("design-token scanners", () => {
  it("parses definitions per selector and ignores comments", () => {
    const css = [
      `:root { ${T("a")}: red; }`,
      `/* ${T("commented")}: blue; */`,
      `[data-theme="x"][data-mode="light"] { ${T("a")}: pink; ${T("b")}: teal; }`,
      `[data-theme="x"][data-mode="dark"] { ${T("a")}: navy; }`,
      `@keyframes k { from { opacity: 0 } to { opacity: 1 } }`,
    ].join("\n");
    const defs = parseTokenDefinitions(css);
    expect([...defs.all].sort()).toEqual([T("a"), T("b")]);
    expect([...defs.root]).toEqual([T("a")]);
    expect(defs.palettes.size).toBe(2);
    expect(defs.palettes.get('[data-theme="x"][data-mode="dark"]')).toEqual(
      new Set([T("a")]),
    );
  });

  const refCases: Array<[string, string, string[]]> = [
    ["plain reference", `color: ${VAR("a")};`, [T("a")]],
    ["reference with spaces", `color: var( ${T("a")})`, [T("a")]],
    ["tailwind arbitrary value", `class="bg-[${VAR("a")}]/10"`, [T("a")]],
    ["no references", "color: red;", []],
  ];
  it.each(refCases)("findVarReferences: %s", (_name, content, tokens) => {
    expect(findVarReferences(content).map((s) => s.token)).toEqual(tokens);
  });

  const fallbackCases: Array<[string, string, string[]]> = [
    ["hex fallback", VAR("a", "#fff"), [T("a")]],
    ["keyword fallback", VAR("a", "transparent"), [T("a")]],
    ["keyword color fallback", VAR("a", "orange"), [T("a")]],
    ["length fallback", VAR("a", "6px"), [T("a")]],
    ["nested var fallback", VAR("a", VAR("b")), [T("a")]],
    ["rgb fallback", VAR("a", "rgb(0 0 0)"), [T("a")]],
    ["no fallback", VAR("a"), []],
  ];
  it.each(fallbackCases)(
    "findFallbackReferences: %s",
    (_name, content, tokens) => {
      expect(findFallbackReferences(content).map((s) => s.token)).toEqual(
        tokens,
      );
    },
  );

  it("findQuotedTokenNames catches whitelist-style strings in all quote styles", () => {
    const content = `const NAMES = ["${T("a")}", '${T("b")}', \`${T("c")}\`];`;
    expect(findQuotedTokenNames(content).map((s) => s.token)).toEqual([
      T("a"),
      T("b"),
      T("c"),
    ]);
  });

  it("stripSourceComments drops commented token examples but keeps live code and URLs", () => {
    const tsx = [
      `const live = ${VAR("a")};`,
      `// a commented example ${VAR("b")} must not count`,
      `/* block ${VAR("c")} also ignored */`,
      `const url = "https://example.com/x"; // ${VAR("d")}`,
    ].join("\n");
    const scanned = stripSourceComments(tsx, false);
    expect(findVarReferences(scanned).map((s) => s.token)).toEqual([T("a")]);
    // The `://` in the URL is preserved, so the string literal survives.
    expect(scanned).toContain("https://example.com/x");

    const css = `:root { color: ${VAR("a")}; } /* ${VAR("b")} */`;
    expect(
      findVarReferences(stripSourceComments(css, true)).map((s) => s.token),
    ).toEqual([T("a")]);
  });
});

// ---------------------------------------------------------------------------
// Repo-wide contract checks
// ---------------------------------------------------------------------------

describe("--g5-* design token contract", () => {
  const defs = parseTokenDefinitions(
    readFileSync(join(SRC_ROOT, "index.css"), "utf8"),
  );
  const files = collectSourceFiles(SRC_ROOT);

  it("finds the token definitions and the 24 theme palettes in index.css", () => {
    expect(defs.all.size).toBeGreaterThan(0);
    expect(defs.palettes.size).toBe(24);
  });

  it("every var(--g5-…) reference names a token defined in index.css", () => {
    const offenders: string[] = [];
    for (const file of files) {
      const content = readForScan(file);
      for (const site of findVarReferences(content)) {
        if (!defs.all.has(site.token)) {
          offenders.push(
            `${relative(SRC_ROOT, file)}:${site.line} ${site.token}`,
          );
        }
      }
    }
    expect(
      offenders,
      `undefined --g5-* token references:\n${offenders.join("\n")}`,
    ).toEqual([]);
  });

  it("no --g5-* reference carries a fallback", () => {
    const offenders: string[] = [];
    for (const file of files) {
      const content = readForScan(file);
      for (const site of findFallbackReferences(content)) {
        offenders.push(
          `${relative(SRC_ROOT, file)}:${site.line} ${site.token}`,
        );
      }
    }
    expect(
      offenders,
      `fallbacks bypassing theming:\n${offenders.join("\n")}`,
    ).toEqual([]);
  });

  it("every programmatic (quoted) token name is defined in index.css", () => {
    const offenders: string[] = [];
    for (const file of files) {
      if (file.endsWith(".css")) continue;
      const content = readFileSync(file, "utf8");
      for (const site of findQuotedTokenNames(content)) {
        if (!defs.all.has(site.token)) {
          offenders.push(
            `${relative(SRC_ROOT, file)}:${site.line} ${site.token}`,
          );
        }
      }
    }
    expect(
      offenders,
      `undefined programmatic token names:\n${offenders.join("\n")}`,
    ).toEqual([]);
  });

  it("every referenced token is defined in :root or in all theme palettes", () => {
    const referenced = new Set<string>();
    for (const file of files) {
      const content = readForScan(file);
      for (const site of findVarReferences(content)) referenced.add(site.token);
      if (!file.endsWith(".css")) {
        for (const site of findQuotedTokenNames(content))
          referenced.add(site.token);
      }
    }
    const offenders: string[] = [];
    for (const token of [...referenced].sort()) {
      if (!defs.all.has(token) || defs.root.has(token)) continue;
      const missingFrom = [...defs.palettes.entries()]
        .filter(([, tokens]) => !tokens.has(token))
        .map(([selector]) => selector);
      if (missingFrom.length > 0) {
        offenders.push(`${token} missing from: ${missingFrom.join("; ")}`);
      }
    }
    expect(
      offenders,
      `tokens not covered by :root or all palettes:\n${offenders.join("\n")}`,
    ).toEqual([]);
  });
});

// The app shell's boot-screen colors (issue #763): content portaled onto
// <body> (Modal, popovers, drawers) inherits body's color, so nothing in
// index.html may outrank the themed body rule in index.css. An inline body
// style did, and dialogs rendered the boot screen's pale text on light themes.
describe("index.html leaves <body> colors to the theme", () => {
  // HTML comments go first: they may mention "<body>" themselves.
  const html = readFileSync(join(SRC_ROOT, "..", "index.html"), "utf8").replace(
    /<!--[\s\S]*?-->/g,
    "",
  );
  const styleBlocks = [...html.matchAll(/<style\b[^>]*>([\s\S]*?)<\/style>/g)]
    .map((m) => stripCssComments(m[1]))
    .join("\n");

  it("puts no inline style on <body>", () => {
    expect(html.match(/<body\b[^>]*>/)?.[0]).not.toMatch(/\bstyle\s*=/);
  });

  it("keeps boot-screen body rules at zero specificity", () => {
    const bodySelectors = [...styleBlocks.matchAll(/([^{}]+)\{/g)]
      .flatMap((m) => m[1].split(","))
      .map((s) => s.trim())
      .filter((s) => /\bbody\b/.test(s));
    expect(bodySelectors.length).toBeGreaterThan(0);
    expect(bodySelectors.filter((s) => s !== ":where(body)")).toEqual([]);
  });

  it("carries no --g5-* fallback", () => {
    expect(findFallbackReferences(html)).toEqual([]);
  });
});
