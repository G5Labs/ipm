// =============================================================================
// Design-Token Scanners — shared parsing for the token contract guards
// =============================================================================
// Pure scanners over index.css and client source, used by two test guards:
//   - design-tokens.test.ts     (reference resolution — design_system D6)
//   - design-token-contrast.test.ts (WCAG contrast — design_system D7)
// Flat rule parsing is sufficient for index.css: token definitions live in
// top-level `:root` / `[data-theme…]` blocks. Nested at-rule bodies (e.g.
// @keyframes) parse as junk rules but define no --g5-* tokens.
// =============================================================================

import { readdirSync, statSync } from "node:fs";
import { join } from "node:path";

export function stripCssComments(css: string): string {
  return css.replace(/\/\*[\s\S]*?\*\//g, "");
}

// Strip comments before the reference scans so a commented-out or documented
// `var(--g5-…)` / `"--g5-…"` example never counts as a live reference. CSS has
// only block comments; TS/TSX also has `//` line comments, but we skip the
// `//` in a URL scheme (`://`) so string literals like `https://…` survive.
export function stripSourceComments(content: string, isCss: boolean): string {
  const noBlock = content.replace(/\/\*[\s\S]*?\*\//g, "");
  if (isCss) return noBlock;
  return noBlock.replace(/(^|[^:])\/\/[^\n]*/g, "$1");
}

export interface TokenDefinitions {
  /** Union of tokens defined under any selector. */
  all: Set<string>;
  /** Tokens defined under `:root`. */
  root: Set<string>;
  /** Tokens per theme palette block (selectors carrying `data-theme`). */
  palettes: Map<string, Set<string>>;
}

export function parseTokenDefinitions(css: string): TokenDefinitions {
  const all = new Set<string>();
  const root = new Set<string>();
  const palettes = new Map<string, Set<string>>();
  for (const match of stripCssComments(css).matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    const selector = match[1].trim();
    const tokens = [...match[2].matchAll(/(--g5-[a-z0-9-]+)\s*:/g)].map(
      (m) => m[1],
    );
    if (tokens.length === 0) continue;
    for (const token of tokens) all.add(token);
    if (selector === ":root") {
      for (const token of tokens) root.add(token);
    }
    if (selector.includes("data-theme")) {
      const set = palettes.get(selector) ?? new Set<string>();
      for (const token of tokens) set.add(token);
      palettes.set(selector, set);
    }
  }
  return { all, root, palettes };
}

// The trailing semicolon is optional so a final declaration without one
// still parses (mirrors parseTokenDefinitions, which ignores values).
const TOKEN_VALUE_DECL = /(--g5-[a-z0-9-]+)\s*:\s*([^;]+?)\s*(?:;|$)/g;

/**
 * Token → declared value text per theme palette block. Same selector
 * classification as parseTokenDefinitions, but keeps the values so the
 * contrast guard can resolve colors.
 */
export function parsePaletteValues(
  css: string,
): Map<string, Map<string, string>> {
  const palettes = new Map<string, Map<string, string>>();
  for (const match of stripCssComments(css).matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    const selector = match[1].trim();
    if (!selector.includes("data-theme")) continue;
    const values = palettes.get(selector) ?? new Map<string, string>();
    for (const decl of match[2].matchAll(TOKEN_VALUE_DECL)) {
      values.set(decl[1], decl[2].trim());
    }
    if (values.size > 0) palettes.set(selector, values);
  }
  return palettes;
}

/**
 * Token → declared value text under `:root`. Custom properties inherit, so a
 * token defined only in `:root` still paints inside every palette; value
 * consumers must fall back here before treating a token as undefined.
 */
export function parseRootTokenValues(css: string): Map<string, string> {
  const values = new Map<string, string>();
  for (const match of stripCssComments(css).matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    if (match[1].trim() !== ":root") continue;
    for (const decl of match[2].matchAll(TOKEN_VALUE_DECL)) {
      values.set(decl[1], decl[2].trim());
    }
  }
  return values;
}

export interface TokenSite {
  token: string;
  line: number;
}

function lineOf(content: string, index: number): number {
  return content.slice(0, index).split("\n").length;
}

/** Every var(--g5-…) reference, fallback-carrying or not. */
export function findVarReferences(content: string): TokenSite[] {
  return [...content.matchAll(/var\(\s*(--g5-[a-z0-9-]+)/g)].map((m) => ({
    token: m[1],
    line: lineOf(content, m.index ?? 0),
  }));
}

/** var(--g5-…) references that carry ANY fallback (comma after the token). */
export function findFallbackReferences(content: string): TokenSite[] {
  return [...content.matchAll(/var\(\s*(--g5-[a-z0-9-]+)\s*,/g)].map((m) => ({
    token: m[1],
    line: lineOf(content, m.index ?? 0),
  }));
}

/** Quoted "--g5-…" strings — forwarding whitelists, getPropertyValue args. */
export function findQuotedTokenNames(content: string): TokenSite[] {
  return [...content.matchAll(/["'`](--g5-[a-z0-9-]+)["'`]/g)].map((m) => ({
    token: m[1],
    line: lineOf(content, m.index ?? 0),
  }));
}

const SOURCE_EXTENSIONS = new Set([".ts", ".tsx", ".css"]);

/** Every .ts/.tsx/.css file under `dir`, recursively (repo-wide scans). */
export function collectSourceFiles(dir: string, acc: string[] = []): string[] {
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) {
      collectSourceFiles(full, acc);
    } else if (SOURCE_EXTENSIONS.has(full.slice(full.lastIndexOf(".")))) {
      acc.push(full);
    }
  }
  return acc;
}

/**
 * Split a CSS selector list on its top-level commas, ignoring commas inside
 * `:is()`, `:not()`, `:has()` and attribute brackets. Theme scoping tests
 * check every member, so `[data-theme="x"] a, button` cannot pass on the
 * strength of its first selector.
 */
export function splitSelectorList(selector: string): string[] {
  const parts: string[] = [];
  let depth = 0;
  let current = "";
  for (const ch of selector) {
    if (ch === "(" || ch === "[") depth++;
    else if (ch === ")" || ch === "]") depth--;
    if (ch === "," && depth === 0) {
      parts.push(current.trim());
      current = "";
    } else {
      current += ch;
    }
  }
  if (current.trim()) parts.push(current.trim());
  return parts;
}
