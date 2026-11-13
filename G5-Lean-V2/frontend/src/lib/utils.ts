import {
  CURRENT_ANCHOR,
  CURRENT_MARKER,
  currentRecordSpans,
  isCurrentOntologyPath,
} from "./currentOntology";
// =============================================================================
// G5 Client — Shared Utility Functions
// =============================================================================
// highlightRust, parseMarkdownToBlocks, parseFrontmatter, resolveSpecRef,
// filterTree, formatLargeNumber, formatCost,
// renderInlineMarkdown, classNames
//
// All utility functions handle edge cases (empty strings, null inputs,
// malformed data) without throwing.
// =============================================================================

import React from "react";
import hljs from "highlight.js/lib/core";
import langBash from "highlight.js/lib/languages/bash";
import langC from "highlight.js/lib/languages/c";
import langCpp from "highlight.js/lib/languages/cpp";
import langCsharp from "highlight.js/lib/languages/csharp";
import langCss from "highlight.js/lib/languages/css";
import langDockerfile from "highlight.js/lib/languages/dockerfile";
import langElixir from "highlight.js/lib/languages/elixir";
import langGo from "highlight.js/lib/languages/go";
import langGraphql from "highlight.js/lib/languages/graphql";
import langHaskell from "highlight.js/lib/languages/haskell";
import langIni from "highlight.js/lib/languages/ini";
import langJava from "highlight.js/lib/languages/java";
import langJavascript from "highlight.js/lib/languages/javascript";
import langJson from "highlight.js/lib/languages/json";
import langKotlin from "highlight.js/lib/languages/kotlin";
import langLua from "highlight.js/lib/languages/lua";
import langMakefile from "highlight.js/lib/languages/makefile";
import langMarkdown from "highlight.js/lib/languages/markdown";
import langPerl from "highlight.js/lib/languages/perl";
import langPhp from "highlight.js/lib/languages/php";
import langProtobuf from "highlight.js/lib/languages/protobuf";
import langPython from "highlight.js/lib/languages/python";
import langR from "highlight.js/lib/languages/r";
import langRuby from "highlight.js/lib/languages/ruby";
import langRust from "highlight.js/lib/languages/rust";
import langSql from "highlight.js/lib/languages/sql";
import langSwift from "highlight.js/lib/languages/swift";
import langTypescript from "highlight.js/lib/languages/typescript";
import langXml from "highlight.js/lib/languages/xml";
import langYaml from "highlight.js/lib/languages/yaml";

// ---------------------------------------------------------------------------
// displayNodeName — Strips .semlift.md / .md suffixes for cleaner display
// ---------------------------------------------------------------------------

/**
 * Returns a human-friendly display name for an ontology node.
 * - Strips `.semlift.md` suffix
 * - Strips `.md` suffix (for design docs)
 * - Replaces underscores with spaces
 */
export function displayNodeName(name: string): string {
  let display = name;
  if (display.endsWith(".semlift.md")) {
    display = display.slice(0, -".semlift.md".length);
  } else if (display.endsWith(".md")) {
    display = display.slice(0, -".md".length);
  }
  // Replace underscores/hyphens with spaces for readability (keep as-is if mixed)
  return display;
}

/**
 * Returns the file name (last path segment, excluding directories) from a
 * path. Trailing slashes are ignored. Falls back to the input when there is
 * no separator. e.g. "designs/foo/bar.md" → "bar.md".
 */
export function fileNameFromPath(path: string): string {
  const trimmed = path.replace(/\/+$/, "");
  const segment = trimmed.split("/").pop();
  return segment && segment.length > 0 ? segment : trimmed;
}

/**
 * Display-only: replace a leading `docs/<workspace_id>` segment of a doc path
 * with the owning workspace's display name (e.g.
 * `docs/019ec…/a.md` → `docs/compliance-service/a.md`). Unknown ids are left as
 * the raw UUID. No-op on non-`docs/<known-id>` paths (`views/…`, `src/…`).
 *
 * This never changes the stored/submitted `filePath` — it only relabels what a
 * human sees, so the `docs/<workspace_id>/` convention is preserved (see
 * `workspace_docs_folders.pm_view.md` D2/D8).
 */
export function labelWorkspaceFolderInPath(
  path: string,
  workspaceNamesById: Record<string, string> | undefined,
): string {
  if (!workspaceNamesById) return path;
  const segs = path.split("/");
  if (segs.length >= 2 && segs[0] === "docs") {
    const name = workspaceNamesById[segs[1]];
    if (name) {
      // Render the name as a SINGLE display segment — guard a name containing
      // "/" so it can't masquerade as extra folder nesting.
      segs[1] = name.replace(/\//g, "∕"); // U+2215 division slash
      return segs.join("/");
    }
  }
  return path;
}

// ---------------------------------------------------------------------------
// classifyChangedPath — Shared changed-path artifact taxonomy
// ---------------------------------------------------------------------------

/**
 * Artifact categories for a changed workspace path. Consumed by both the
 * Source Control dirty section and the CommitModal file selection list so the
 * two panels present identical grouping.
 * See `draft-management.semlift.md` D16 (Changed-Path Artifact Taxonomy).
 */
export type ArtifactCategory =
  | "code_docs"
  | "ontology"
  | "intent_grounding"
  | "other";

/**
 * Classify a RAW, repo-relative changed path (the stored path, not the
 * workspace-relabeled display string) into an `ArtifactCategory`.
 *
 * Matching is on path SEGMENTS, not anchored prefixes, so both a prefixed form
 * (`generation/.semlift/specs/…`) and a stripped form (`.semlift/specs/…`)
 * resolve identically. First match wins, in this order (D16):
 *   1. `ontology` — a `.semlift/specs/` segment sequence, or a `.semlift.md` file.
 *   2. `intent_grounding` — an `intent_record/` or `intent_grounding/` segment.
 *   3. `other` — generated human-view projections or internal `.g5/` state.
 *   4. `code_docs` — default catch-all (source, docs, views, config, tests).
 *
 * Pure and total: never throws, and always returns a category.
 */
export function classifyChangedPath(path: string): ArtifactCategory {
  const raw = (path ?? "").trim();
  const segments = raw.split("/").filter((segment) => segment.length > 0);

  // 1. Ontology: `.semlift/specs/` segment sequence OR a `.semlift.md` file.
  const hasSemliftSpecs = segments.some(
    (segment, index) =>
      segment === ".semlift" && segments[index + 1] === "specs",
  );
  if (
    isCurrentOntologyPath(raw) ||
    hasSemliftSpecs ||
    raw.endsWith(".semlift.md")
  ) {
    return "ontology";
  }

  // 2. Intent and grounding: an intent_record/ or intent_grounding/ segment.
  if (
    segments.includes("intent_record") ||
    segments.includes("intent_grounding")
  ) {
    return "intent_grounding";
  }

  // 3. Other: non-user, non-methodology artifacts kept out of Code / docs.
  const isGeneratedProjection =
    raw.endsWith(".human-view.json") || raw.endsWith(".human-view.html");
  const isInternalG5State = segments.includes(".g5");
  if (isGeneratedProjection || isInternalG5State) {
    return "other";
  }

  // 4. Code / docs: default catch-all for real user artifacts.
  return "code_docs";
}

export interface ArtifactCategoryMeta {
  /** Category identity. */
  category: ArtifactCategory;
  /** Human-facing section label. */
  label: string;
  /** Whether the section is expanded by default (only Code / docs). */
  defaultOpen: boolean;
}

/**
 * Display metadata for each artifact category, in stable presentation order
 * (Code / docs first, then Ontology, Intent and grounding, Other). The order
 * is independent of the `classifyChangedPath` first-match predicate order.
 * See `draft-management.semlift.md` D16.
 */
export const ARTIFACT_CATEGORY_META: readonly ArtifactCategoryMeta[] = [
  { category: "code_docs", label: "Code / docs", defaultOpen: true },
  { category: "ontology", label: "Ontology", defaultOpen: false },
  {
    category: "intent_grounding",
    label: "Intent and grounding",
    defaultOpen: false,
  },
  { category: "other", label: "Other", defaultOpen: false },
];

/** Stable display order of artifact categories. */
export const ARTIFACT_CATEGORY_ORDER: readonly ArtifactCategory[] =
  ARTIFACT_CATEGORY_META.map((meta) => meta.category);

// ---------------------------------------------------------------------------
// classNames — Falsy-filtering class concatenation utility
// ---------------------------------------------------------------------------

/**
 * Utility for joining CSS class names, filtering out falsy values.
 * Same pattern as the `clsx` library.
 */
export function classNames(
  ...args: (string | false | null | undefined)[]
): string {
  return args.filter(Boolean).join(" ");
}

const UUID_RE =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

export function isUuid(value?: string | null): value is string {
  return Boolean(value && UUID_RE.test(value));
}

// ---------------------------------------------------------------------------
// highlightCode — Regex-based multi-language syntax highlighting
// ---------------------------------------------------------------------------

/**
 * Maps token type → inline style using CSS custom properties.
 * Using inline styles instead of CSS classes ensures colors always
 * apply regardless of Tailwind CSS purging / layer specificity.
 */
const TOKEN_STYLES: Record<string, string> = {
  keyword: "color:var(--g5-syn-keyword)",
  string: "color:var(--g5-syn-string)",
  number: "color:var(--g5-syn-number)",
  type: "color:var(--g5-syn-type)",
  comment: "color:var(--g5-syn-comment);font-style:italic",
  function: "color:var(--g5-syn-function)",
  macro: "color:var(--g5-syn-function);font-weight:500",
  attribute: "color:var(--g5-syn-attribute)",
  lifetime: "color:var(--g5-syn-constant);font-style:italic",
  constant: "color:var(--g5-syn-constant)",
  operator: "color:var(--g5-syn-operator)",
  punctuation: "color:var(--g5-syn-punctuation)",
  variable: "color:var(--g5-syn-variable)",
  tag: "color:var(--g5-syn-tag)",
};

// ---------------------------------------------------------------------------
// canonicalLang — Normalize language aliases
// ---------------------------------------------------------------------------

function canonicalLang(lang: string): string {
  const l = lang.toLowerCase().trim();
  const ALIASES: Record<string, string> = {
    rs: "rust",
    ts: "typescript",
    tsx: "typescript",
    jsx: "javascript",
    js: "javascript",
    py: "python",
    rb: "ruby",
    sh: "bash",
    zsh: "bash",
    shell: "bash",
    yml: "yaml",
    "c++": "cpp",
    "c#": "csharp",
    cs: "csharp",
    kt: "kotlin",
    kts: "kotlin",
    ex: "elixir",
    exs: "elixir",
    hs: "haskell",
    pl: "perl",
    r: "r",
    tf: "hcl",
    terraform: "hcl",
    dockerfile: "docker",
    make: "makefile",
    mk: "makefile",
    md: "markdown",
    htm: "html",
    xml: "html",
    svg: "html",
    jsonc: "json",
    json5: "json",
    graphql: "graphql",
    gql: "graphql",
    proto: "protobuf",
    protobuf: "protobuf",
    scss: "css",
    sass: "css",
    less: "css",
    styl: "css",
    psql: "sql",
    mysql: "sql",
    pgsql: "sql",
    sqlite: "sql",
    plsql: "sql",
  };
  return ALIASES[l] ?? l;
}

// ---------------------------------------------------------------------------
// highlight.js language registry
// ---------------------------------------------------------------------------
// Languages are registered under the canonical names emitted by
// `canonicalLang`, so lookups need no second alias table. The registered set
// covers the languages G5 renders in spec/code blocks; anything else falls
// back to escaped plain text.

hljs.registerLanguage("rust", langRust);
hljs.registerLanguage("typescript", langTypescript);
hljs.registerLanguage("javascript", langJavascript);
hljs.registerLanguage("python", langPython);
hljs.registerLanguage("go", langGo);
hljs.registerLanguage("java", langJava);
hljs.registerLanguage("c", langC);
hljs.registerLanguage("cpp", langCpp);
hljs.registerLanguage("csharp", langCsharp);
hljs.registerLanguage("kotlin", langKotlin);
hljs.registerLanguage("ruby", langRuby);
hljs.registerLanguage("bash", langBash);
hljs.registerLanguage("sql", langSql);
hljs.registerLanguage("html", langXml);
hljs.registerLanguage("css", langCss);
hljs.registerLanguage("json", langJson);
hljs.registerLanguage("yaml", langYaml);
hljs.registerLanguage("toml", langIni);
hljs.registerLanguage("markdown", langMarkdown);
hljs.registerLanguage("elixir", langElixir);
hljs.registerLanguage("docker", langDockerfile);
hljs.registerLanguage("makefile", langMakefile);
hljs.registerLanguage("protobuf", langProtobuf);
hljs.registerLanguage("graphql", langGraphql);
hljs.registerLanguage("haskell", langHaskell);
hljs.registerLanguage("perl", langPerl);
hljs.registerLanguage("r", langR);
hljs.registerLanguage("swift", langSwift);
hljs.registerLanguage("php", langPhp);
hljs.registerLanguage("lua", langLua);

// ---------------------------------------------------------------------------
// highlight.js scope → G5 token style mapping
// ---------------------------------------------------------------------------

/**
 * Maps highlight.js scope class names onto the G5 token palette
 * (`TOKEN_STYLES`). Compound scopes like `title.function_` surface as the
 * class list `"hljs-title function_"`, so the sub-scope markers
 * (`function_`, `class_`) are listed here too and win over the base scope.
 */
const HLJS_TOKEN_FOR: Record<string, string> = {
  // Sub-scope markers (checked against every class token)
  function_: "function",
  class_: "type",
  // Base scopes
  "hljs-keyword": "keyword",
  "hljs-literal": "constant",
  "hljs-symbol": "constant",
  "hljs-variable": "variable",
  "hljs-template-variable": "variable",
  "hljs-property": "variable",
  "hljs-params": "variable",
  "hljs-string": "string",
  "hljs-char": "string",
  "hljs-regexp": "string",
  "hljs-number": "number",
  "hljs-type": "type",
  "hljs-built_in": "type",
  "hljs-title": "function",
  "hljs-comment": "comment",
  "hljs-doctag": "comment",
  "hljs-quote": "comment",
  "hljs-meta": "attribute",
  "hljs-attr": "attribute",
  "hljs-attribute": "attribute",
  "hljs-operator": "operator",
  "hljs-punctuation": "punctuation",
  "hljs-tag": "tag",
  "hljs-name": "tag",
  "hljs-selector-tag": "tag",
  "hljs-selector-id": "constant",
  "hljs-selector-class": "type",
  "hljs-selector-attr": "attribute",
  "hljs-selector-pseudo": "attribute",
  "hljs-section": "keyword",
  "hljs-bullet": "punctuation",
  "hljs-link": "string",
  "hljs-code": "string",
  "hljs-formula": "constant",
  "hljs-addition": "string",
  "hljs-deletion": "keyword",
  "hljs-subst": "variable",
};

/** Resolve an hljs class list to an inline style, or empty when unmapped. */
function styleForHljsClasses(classList: string): string {
  const classes = classList.split(/\s+/);
  // Sub-scope markers win over base scopes: "hljs-title class_" is a type
  // name, not a function name.
  for (const cls of classes) {
    if (cls === "function_" || cls === "class_") {
      return TOKEN_STYLES[HLJS_TOKEN_FOR[cls]] ?? "";
    }
  }
  for (const cls of classes) {
    const token = HLJS_TOKEN_FOR[cls];
    if (token) {
      return TOKEN_STYLES[token] ?? "";
    }
  }
  return "";
}

// ---------------------------------------------------------------------------
// highlightCode — highlight.js-backed multi-language syntax highlighter
// ---------------------------------------------------------------------------

/** Escape HTML entities for the plain-text fallback path. */
function escapeCodeHtml(code: string): string {
  return code
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

/**
 * Highlight `code` as `language`, returning an HTML string.
 *
 * Tokenization is highlight.js (registered language set above); its scope
 * classes are post-mapped onto the G5 inline token styles (`TOKEN_STYLES`,
 * `--g5-syn-*` theme variables) so colors follow the active theme and are
 * immune to Tailwind CSS purging. Unknown languages return escaped plain
 * text. highlight.js performs its own HTML escaping, so raw code is passed
 * in untouched (the previous hand-rolled tokenizer escaped first and then
 * mis-tokenized code containing `&`, `<`, `>`).
 */
export function highlightCode(code: string, language = ""): string {
  if (!code) return "";

  const lang = canonicalLang(language);
  if (!hljs.getLanguage(lang)) {
    return escapeCodeHtml(code);
  }

  let value: string;
  try {
    value = hljs.highlight(code, { language: lang }).value;
  } catch {
    return escapeCodeHtml(code);
  }

  // Map scope classes to inline styles; unmapped scopes become plain spans.
  return value.replace(
    /<span class="([^"]*)">/g,
    (_match, classList: string) => {
      const style = styleForHljsClasses(classList);
      return style ? `<span style="${style}">` : "<span>";
    },
  );
}

/** @deprecated Use highlightCode(code, 'rust') instead */
export function highlightRust(code: string): string {
  return highlightCode(code, "rust");
}

// ---------------------------------------------------------------------------
// Block types for parseMarkdownToBlocks
// ---------------------------------------------------------------------------

export type FrontmatterValue = string | string[] | null;
export type TableAlignment = "left" | "center" | "right" | null;
export type BulletListMarker = "-" | "*" | "+";

/** Severity vocabulary a `Q` record's label may carry (center-panel D69). */
export type RecordTagSeverity = "low" | "medium" | "high" | "critical";

/**
 * Canonical semantic-record metadata projected from the I/D/Q label suffix.
 *
 * `human`/`uncertainty` are AUTHORED by a person on any record. `answered` and
 * `severity` are SYSTEM-MAINTAINED from the record's `g5-question` payload and
 * are valid on `Q` records ONLY — question state is stored metadata, not a
 * derived display projection, so `is_human`, the tag filters, registration,
 * and every agent reading the label see it without parsing the fence.
 */
export interface BlockTag {
  kind: "human" | "uncertainty" | "answered" | "severity";
  /** Present only when `kind` is `"severity"`. */
  severity?: RecordTagSeverity;
  /** Metadata is persisted only as an inline semantic-record label suffix. */
  syntax: "inline";
}

/** Tags a person may author; the rest are written from the question payload. */
export const AUTHORED_TAG_KINDS: readonly BlockTag["kind"][] = [
  "human",
  "uncertainty",
];

/** True when the tag is system-maintained and must never be hand-edited. */
export function isSystemMaintainedTag(tag: BlockTag): boolean {
  return !AUTHORED_TAG_KINDS.includes(tag.kind);
}

/** Common properties shared across all Block variants */
interface BlockCommon {
  /** Character offset range in the source markdown, for live workspace file offset mapping */
  sourceRange?: { start: number; length: number };
}

export type Block =
  | (BlockCommon & {
      type: "frontmatter";
      content: string;
      fields: Record<string, FrontmatterValue>;
    })
  | (BlockCommon & {
      type: "heading";
      level: 1 | 2 | 3 | 4 | 5 | 6;
      content: string;
    })
  | (BlockCommon & {
      type: "code";
      language: string;
      content: string;
      /** Exact opening fence marker (` ``` ` or `~~~`, length included). */
      fence?: string;
      /** Exact bytes after the opening fence, retained until language changes. */
      openingSuffix?: string;
      /** Exact closing fence marker when it differs from the opener. */
      closingFence?: string;
      /** Exact trailing bytes after the closing fence. */
      closingSuffix?: string;
      /** Leading whitespace retained for fenced-block round-trip. */
      indent?: string;
      closingIndent?: string;
      /** False for an unterminated block, which stays source-fallback only. */
      closed?: boolean;
    })
  | (BlockCommon & {
      type: "bullet-list";
      items: string[];
      /** Marker authored for each item; absent means canonical `-`. */
      markers?: BulletListMarker[];
      /** False when indentation implies nested/list-extension syntax that the
       * flat Rich controls must leave to bounded Markdown editing. */
      nativeEditable?: boolean;
      /** Per-boundary blank-line flags: `looseSeparators[k]` = a blank line sits
       *  between `items[k]` and `items[k+1]`. Length `items.length - 1`; absent
       *  means all-tight. Preserves mixed tight/loose spacing on round-trip. [D58] */
      looseSeparators?: boolean[];
    })
  | (BlockCommon & {
      type: "numbered-list";
      items: string[];
      start?: number;
      looseSeparators?: boolean[];
    })
  | (BlockCommon & { type: "blockquote"; content: string })
  | (BlockCommon & {
      type: "image";
      alt: string;
      src: string;
      title?: string;
    })
  | (BlockCommon & {
      type: "table";
      headers: string[];
      rows: string[][];
      /** GFM alignment owned by each header column. */
      alignments?: TableAlignment[];
      /** Preserve whether the authored table used outer border pipes. */
      leadingPipe?: boolean;
      trailingPipe?: boolean;
    })
  | (BlockCommon & {
      type: "callout";
      kind: "note" | "warning" | "tip" | "danger";
      content: string;
      /** False for an unterminated directive, which stays source-fallback only. */
      closed?: boolean;
    })
  | (BlockCommon & { type: "hr"; marker?: "---" | "***" | "___" })
  | (BlockCommon & { type: "paragraph"; content: string });

export type RecordKind = "I" | "D" | "Q";

/**
 * Parser-owned semantic record overlay. `spanStart` is inclusive and
 * `spanEnd` is exclusive, so `blocks.slice(spanStart, spanEnd)` is the exact
 * flat block sequence owned by the record.
 */
export interface RecordSpan {
  current?: import("./currentOntology").CurrentRecord;
  id: string;
  kind: RecordKind;
  title: string;
  spanStart: number;
  spanEnd: number;
  /** Display-compatible metadata parsed from a `(Tags: …)` label suffix. */
  metadataTags?: readonly BlockTag[];
  /** Exact title-line suffix bytes, retained even when malformed. */
  metadataSuffix?: string;
  /** False when a present suffix cannot be interpreted losslessly. */
  metadataValid?: boolean;
}

/**
 * Flat heading-delimited unit used only when a document has no semantic
 * records. An empty title identifies leading preface or no-heading content.
 */
export interface SectionSpan {
  title: string;
  spanStart: number;
  spanEnd: number;
}

/**
 * Backward-compatible parsed block array with additive semantic metadata.
 * Metadata properties are non-enumerable so existing array equality,
 * serialization, iteration, and spread behavior stay unchanged.
 */
export type ParsedMarkdownBlocks = Block[] & {
  readonly recordSpans: readonly RecordSpan[];
  readonly sectionSpans: readonly SectionSpan[];
};

const ID_BLOCK_LABEL_LINE_RE = /^\[(?:I|D|Q)\d+[a-zA-Z]?\./i;
// Greedy title capture: a legal title may itself contain bracketed
// references (`… the HTML questionnaire path ([D59]) covers …`), so the
// closing bracket is the LAST `]` on the line — a lazy match would cut the
// title at the first nested `]`, leave garbage in the suffix, and silently
// drop the record's frame (live center-panel D69 regression).
export const ID_BLOCK_LABEL_CANDIDATE_RE =
  /^\[([IDQ])(\d+[a-zA-Z]?)\.\s*(.*)\](.*)$/i;

function isIdBlockLabelLine(line: string): boolean {
  return ID_BLOCK_LABEL_LINE_RE.test(line.trim());
}

function parseEscapedDelimited(
  text: string,
  startIndex: number,
  closeChar: string,
): { value: string; nextIndex: number } | null {
  let value = "";
  let escaped = false;
  for (let i = startIndex; i < text.length; i++) {
    const ch = text[i];
    if (escaped) {
      value += ch;
      escaped = false;
      continue;
    }
    if (ch === "\\") {
      escaped = true;
      continue;
    }
    if (ch === closeChar) {
      return { value, nextIndex: i + 1 };
    }
    value += ch;
  }
  return null;
}

function parseEscapedImageDestination(
  text: string,
  startIndex: number,
): { value: string; nextIndex: number; closed: boolean } | null {
  let value = "";
  let escaped = false;
  for (let i = startIndex; i < text.length; i++) {
    const ch = text[i];
    if (escaped) {
      value += ch;
      escaped = false;
      continue;
    }
    if (ch === "\\") {
      escaped = true;
      continue;
    }
    if (ch === ")") {
      return { value, nextIndex: i + 1, closed: true };
    }
    if (/\s/.test(ch)) {
      return { value, nextIndex: i, closed: false };
    }
    value += ch;
  }
  return null;
}

function parseMarkdownImageLine(
  text: string,
): { alt: string; src: string; title?: string } | null {
  if (!text.startsWith("![")) return null;

  const alt = parseEscapedDelimited(text, 2, "]");
  if (!alt || text[alt.nextIndex] !== "(") return null;

  let index = alt.nextIndex + 1;
  while (index < text.length && /\s/.test(text[index])) index++;

  const src = parseEscapedImageDestination(text, index);
  if (!src || !src.value) return null;
  index = src.nextIndex;

  if (src.closed) {
    return index === text.length ? { alt: alt.value, src: src.value } : null;
  }

  while (index < text.length && /\s/.test(text[index])) index++;
  if (index >= text.length) return null;

  let title: string | undefined;
  const titleOpen = text[index];
  if (titleOpen === '"' || titleOpen === "'") {
    const parsedTitle = parseEscapedDelimited(text, index + 1, titleOpen);
    if (!parsedTitle) return null;
    title = parsedTitle.value;
    index = parsedTitle.nextIndex;
  } else if (titleOpen === "(") {
    const parsedTitle = parseEscapedDelimited(text, index + 1, ")");
    if (!parsedTitle) return null;
    title = parsedTitle.value;
    index = parsedTitle.nextIndex;
  } else {
    return null;
  }

  while (index < text.length && /\s/.test(text[index])) index++;
  if (text[index] !== ")") return null;
  index++;

  return index === text.length
    ? { alt: alt.value, src: src.value, title }
    : null;
}

function escapeMarkdownImageAlt(value: string): string {
  return value.replace(/([\\\[\]])/g, "\\$1");
}

function escapeMarkdownImageDestination(value: string): string {
  return value.replace(/([\\()\s])/g, "\\$1");
}

function escapeMarkdownImageTitle(value: string): string {
  return value.replace(/([\\"])/g, "\\$1");
}

// ---------------------------------------------------------------------------
// parseFrontmatter — Extract YAML frontmatter from markdown
// ---------------------------------------------------------------------------

/**
 * Extracts YAML frontmatter from a markdown string into typed fields.
 * Returns an object where values are strings, string arrays, or null.
 */
export function parseFrontmatter(
  content: string,
): Record<string, FrontmatterValue> {
  if (!content) return {};

  const match = content.match(/^---\r?\n([\s\S]*?)\r?\n---/);
  if (!match) return {};

  const yaml = match[1];
  const fields: Record<string, FrontmatterValue> = {};

  const lines = yaml.split("\n");
  let currentKey: string | null = null;
  let currentArray: string[] | null = null;

  const hasArrayContinuation = (fromIndex: number): boolean => {
    for (let i = fromIndex + 1; i < lines.length; i++) {
      const next = lines[i];
      if (next.trim() === "" || next.trimStart().startsWith("#")) {
        continue;
      }
      return /^\s+-\s+/.test(next.trimEnd());
    }
    return false;
  };

  const parseScalar = (value: string): string =>
    value.trim().replace(/^['"]|['"]$/g, "");

  for (let index = 0; index < lines.length; index++) {
    const line = lines[index];
    const trimmed = line.trimEnd();

    // Array continuation: "  - value"
    if (currentKey && currentArray && /^\s+-\s+/.test(trimmed)) {
      const val = parseScalar(trimmed.replace(/^\s+-\s+/, ""));
      currentArray.push(val);
      fields[currentKey] = currentArray;
      continue;
    }

    // Key-value pair
    const kvMatch = trimmed.match(/^(\w[\w.-]*)\s*:\s*(.*)/);
    if (kvMatch) {
      // Save previous array
      currentKey = kvMatch[1];
      const rawValue = kvMatch[2].trim();

      if (rawValue === "") {
        if (hasArrayContinuation(index)) {
          currentArray = [];
          fields[currentKey] = currentArray;
        } else {
          fields[currentKey] = null;
          currentArray = null;
        }
      } else if (rawValue === "~" || rawValue === "null") {
        fields[currentKey] = null;
        currentArray = null;
      } else if (rawValue.startsWith("[") && rawValue.endsWith("]")) {
        // Inline array: [a, b, c]
        const inner = rawValue.slice(1, -1);
        fields[currentKey] = inner.split(",").map(parseScalar).filter(Boolean);
        currentArray = null;
      } else {
        // Plain string value (strip quotes)
        fields[currentKey] = parseScalar(rawValue);
        currentArray = [];
      }
    }
  }

  return fields;
}

// ---------------------------------------------------------------------------
// parseMarkdownToBlocks — Parse markdown into typed block array
// ---------------------------------------------------------------------------

/**
 * Parses a markdown string into a typed block array.
 * Handles frontmatter, headings, code blocks, lists, tables, callouts,
 * horizontal rules, and paragraphs.
 *
 * Malformed markdown degrades gracefully to paragraph blocks.
 */
const BULLET_MARKER_RE = /^\s*([-*+])\s+/;
const NUMBERED_MARKER_RE = /^\s*\d+\.\s+/;
const TABLE_DELIMITER_RE = /^\|?\s*[-:]+[-|\s:]*$/;
const CODE_FENCE_RE = /^(\s{0,3})(`{3,}|~{3,})([^\n]*)$/;
/**
 * True when the raw line at `lines[i]` begins a new block-level construct the
 * parser recognizes. Used to stop lazy list-item continuation so a heading, list
 * marker, quote, callout, hr, `[I…]`/`[D…]`/`[Q…]` label, image, or GFM table is
 * never swallowed into the previous item's text. Index-aware because a GFM table needs its delimiter row on the
 * next line, matching the outer parser's two-line table lookahead. [D58]
 */
function startsBlockConstruct(lines: string[], i: number): boolean {
  const line = lines[i];
  const trimmed = line.trim();
  if (trimmed === "") return true;
  if (BULLET_MARKER_RE.test(line) || NUMBERED_MARKER_RE.test(line)) return true;
  if (/^#{1,6}\s+/.test(trimmed)) return true;
  if (CODE_FENCE_RE.test(line)) return true;
  if (/^:::(note|warning|tip|danger)\s*$/.test(trimmed)) return true;
  if (trimmed.startsWith("> ") || trimmed === ">") return true;
  if (/^(---|\*\*\*|___)$/.test(trimmed)) return true;
  if (
    isIdBlockLabelLine(trimmed) ||
    CURRENT_MARKER.test(trimmed) ||
    CURRENT_ANCHOR.test(trimmed)
  )
    return true;
  if (parseMarkdownImageLine(trimmed)) return true;
  if (
    trimmed.includes("|") &&
    i + 1 < lines.length &&
    TABLE_DELIMITER_RE.test(lines[i + 1].trim())
  ) {
    return true;
  }
  return false;
}

interface ParsedRecordLabel {
  id: string;
  kind: RecordKind;
  title: string;
  metadataTags?: readonly BlockTag[];
  metadataSuffix?: string;
  metadataValid?: boolean;
  /** Heading level that structurally contains this record, when one exists. */
  enclosingHeadingLevel?: 1 | 2 | 3 | 4 | 5 | 6;
}

export interface ParsedRecordMetadata {
  tags: BlockTag[];
  valid: boolean;
  /** Every accepted suffix is canonical; retained for existing callers. */
  canonical: boolean;
}

/**
 * Canonical token order. A suffix must list its tags in this order to parse;
 * `(Tags: uncertain, human)` stays fail-closed exactly as before, because one
 * fact must have one spelling or diffs and live workspace file splices fight over reorderings.
 */
const RECORD_TAG_TOKEN_ORDER = [
  "human",
  "uncertain",
  "answered",
  "severity",
] as const;

const RECORD_TAG_SEVERITIES: readonly RecordTagSeverity[] = [
  "low",
  "medium",
  "high",
  "critical",
];

/** Exactly one space after the colon and after each comma — no looser form. */
const RECORD_METADATA_SUFFIX_RE = /^\(Tags: (.+)\)$/;

function tokenToTag(token: string): BlockTag | null {
  if (token === "human") return { kind: "human", syntax: "inline" };
  if (token === "uncertain") return { kind: "uncertainty", syntax: "inline" };
  if (token === "answered") return { kind: "answered", syntax: "inline" };
  if ((RECORD_TAG_SEVERITIES as readonly string[]).includes(token)) {
    return {
      kind: "severity",
      severity: token as RecordTagSeverity,
      syntax: "inline",
    };
  }
  return null;
}

function tagOrderIndex(tag: BlockTag): number {
  return RECORD_TAG_TOKEN_ORDER.indexOf(
    tag.kind === "uncertainty" ? "uncertain" : tag.kind,
  );
}

/**
 * Parse a canonical semantic-record tag suffix.
 *
 * Fail-closed on anything not exactly canonical: unknown tokens, duplicates,
 * out-of-order tokens, loose spacing, and `answered`/severity on a record that
 * is not a `Q`. An invalid suffix is not projected as a semantic record at all,
 * so a document keeps every source byte instead of gaining a card whose edits
 * cannot be serialized back.
 */
export function parseRecordMetadataSuffix(
  rawSuffix: string | undefined,
  kind?: RecordKind,
): ParsedRecordMetadata {
  if (!rawSuffix) return { tags: [], valid: true, canonical: true };
  const invalid: ParsedRecordMetadata = {
    tags: [],
    valid: false,
    canonical: false,
  };
  const match = RECORD_METADATA_SUFFIX_RE.exec(rawSuffix);
  if (!match) return invalid;

  const tags: BlockTag[] = [];
  const seen = new Set<BlockTag["kind"]>();
  let lastOrder = -1;
  for (const token of match[1].split(", ")) {
    const tag = tokenToTag(token);
    if (!tag) return invalid;
    // Question state belongs to a question. On an I/D record it would claim
    // metadata the record does not carry, so it is not merely ignored.
    if (isSystemMaintainedTag(tag) && kind !== "Q") return invalid;
    if (seen.has(tag.kind)) return invalid;
    const order = tagOrderIndex(tag);
    if (order <= lastOrder) return invalid;
    lastOrder = order;
    seen.add(tag.kind);
    tags.push(tag);
  }
  return { tags, valid: true, canonical: true };
}

/**
 * Serialize tags into the one canonical spelling `parse` accepts.
 *
 * `kind` is required and mirrors the parser's Q-only gate. Without it this
 * function would happily emit `(Tags: answered)` onto a D record — a suffix
 * `parseRecordMetadataSuffix` then rejects, which stops that record being
 * projected as a semantic record at all. Serialization must not be able to
 * write something reading cannot accept.
 */
export function serializeRecordMetadataTags(
  tags: readonly BlockTag[],
  kind: RecordKind,
): string | null {
  if (kind !== "Q" && tags.some(isSystemMaintainedTag)) return null;
  const entries: string[] = [];
  const seen = new Set<BlockTag["kind"]>();
  for (const token of RECORD_TAG_TOKEN_ORDER) {
    const kind = token === "uncertain" ? "uncertainty" : token;
    const matching = tags.filter((tag) => tag.kind === kind);
    if (matching.length === 0) continue;
    // One fact, one tag: a duplicate cannot be written canonically.
    if (matching.length > 1) return null;
    seen.add(matching[0].kind);
    if (matching[0].kind === "severity") {
      const severity = matching[0].severity;
      if (!severity || !RECORD_TAG_SEVERITIES.includes(severity)) return null;
      entries.push(severity);
      continue;
    }
    entries.push(token);
  }
  // An unknown kind must not silently vanish from the written label.
  if (tags.some((tag) => !seen.has(tag.kind))) return null;
  return entries.length > 0 ? `(Tags: ${entries.join(", ")})` : "";
}

interface ParsedRecordLabelCandidate {
  id: string;
  label: ParsedRecordLabel | null;
}

/**
 * Parse a backend-visible label boundary separately from deciding whether it
 * is safe to project as a semantic record. Invalid suffixes and duplicate ids
 * stay in the flat renderer, but they must still terminate a preceding valid
 * record so its actions can never absorb their bytes.
 */
function recordLabelCandidateForBlock(
  block: Block,
): ParsedRecordLabelCandidate | null {
  // The registration parser treats `### [D1. ...]` as a heading, not a label.
  if (block.type !== "paragraph") return null;
  const firstLine = block.content.split("\n", 1)[0]?.trim() ?? "";
  const match = firstLine.match(ID_BLOCK_LABEL_CANDIDATE_RE);
  if (!match) return null;
  const kind = match[1].toUpperCase() as RecordKind;
  const id = `${kind}${match[2]}`;
  let title = match[3].trim();
  let rawSuffix = match[4].trim();
  // Tolerant read for a misplaced metadata suffix INSIDE the brackets —
  // `[Q1. Title (Tags: critical)]` — written by agents despite the canonical
  // `[Q1. Title] (Tags: critical)` form: fold it into the suffix instead of
  // displaying it as title text. Only when no outside suffix exists, and
  // ONLY for pure severity tokens: the Rust registrar treats an in-bracket
  // group as literal title text, so folding `human`/`uncertain`/`answered`
  // here would project authority the store does not carry (review P1 —
  // a client-side human tag on a record registered `is_human: false`).
  if (!rawSuffix) {
    const inBracket = title.match(/^(.*?)\s*(\(Tags:\s*[^)]*\))$/i);
    if (inBracket) {
      const folded = parseRecordMetadataSuffix(inBracket[2], kind);
      if (
        folded.valid &&
        folded.tags.length > 0 &&
        folded.tags.every((tag) => tag.kind === "severity")
      ) {
        title = inBracket[1].trim();
        rawSuffix = inBracket[2];
      }
    }
  }
  // The kind gates the Q-only tokens, so it must reach the parser.
  const metadata = parseRecordMetadataSuffix(rawSuffix || undefined, kind);
  // A label-shaped line with a non-canonical metadata suffix is not safe to
  // project as a semantic record. Keep it in the ordinary block renderer so
  // every source byte remains preserved and editable instead of creating a card
  // whose structural edits cannot be serialized.
  if (rawSuffix && !metadata.valid) return { id, label: null };
  return {
    id,
    label: {
      id,
      kind,
      title,
      ...(rawSuffix
        ? { metadataSuffix: rawSuffix, metadataValid: metadata.valid }
        : {}),
      ...(metadata.tags.length > 0 ? { metadataTags: metadata.tags } : {}),
    },
  };
}

function computeRecordSpans(blocks: Block[]): RecordSpan[] {
  const current = currentRecordSpans(blocks);
  if (current) return current;
  const boundaries = new Set<number>();
  const candidates = new Map<
    number,
    ParsedRecordLabelCandidate & {
      enclosingHeadingLevel?: 1 | 2 | 3 | 4 | 5 | 6;
    }
  >();
  const identifierCounts = new Map<string, number>();
  const starts = new Map<number, ParsedRecordLabel>();
  let inReferencesIndex = false;
  let enclosingHeadingLevel: 1 | 2 | 3 | 4 | 5 | 6 | undefined;

  blocks.forEach((block, index) => {
    // H1 is the document title in SemLift files, not the record-owning section.
    if (block.type === "heading" && block.level >= 2) {
      enclosingHeadingLevel = block.level;
    }
    if (block.type === "heading" && block.level <= 2) {
      inReferencesIndex =
        block.level === 2 &&
        block.content.trim().toLowerCase() === "references";
    }
    if (inReferencesIndex) return;
    const candidate = recordLabelCandidateForBlock(block);
    if (candidate) {
      boundaries.add(index);
      candidates.set(index, { ...candidate, enclosingHeadingLevel });
      const identityKey = candidate.id.toUpperCase();
      identifierCounts.set(
        identityKey,
        (identifierCounts.get(identityKey) ?? 0) + 1,
      );
    }
  });

  for (const [index, candidate] of candidates) {
    if (
      candidate.label &&
      identifierCounts.get(candidate.id.toUpperCase()) === 1
    ) {
      starts.set(index, {
        ...candidate.label,
        enclosingHeadingLevel: candidate.enclosingHeadingLevel,
      });
    }
  }

  const spans: RecordSpan[] = [];
  for (const [spanStart, label] of starts) {
    let spanEnd = spanStart + 1;
    while (spanEnd < blocks.length) {
      const candidate = blocks[spanEnd];
      if (
        boundaries.has(spanEnd) ||
        (candidate.type === "heading" &&
          (label.enclosingHeadingLevel == null ||
            candidate.level <= label.enclosingHeadingLevel))
      ) {
        break;
      }
      spanEnd++;
    }
    const { enclosingHeadingLevel: _enclosingHeadingLevel, ...recordLabel } =
      label;
    spans.push({
      ...recordLabel,
      spanStart,
      spanEnd,
    });
  }
  return spans;
}

function computeSectionSpans(
  blocks: Block[],
  recordSpans: readonly RecordSpan[],
): SectionSpan[] {
  if (blocks.length === 0 || recordSpans.length > 0) return [];

  const headingIndices: number[] = [];
  blocks.forEach((block, index) => {
    if (block.type === "heading") headingIndices.push(index);
  });

  if (headingIndices.length === 0) {
    return [{ title: "", spanStart: 0, spanEnd: blocks.length }];
  }

  const spans: SectionSpan[] = [];
  if (headingIndices[0] > 0) {
    spans.push({ title: "", spanStart: 0, spanEnd: headingIndices[0] });
  }
  headingIndices.forEach((spanStart, headingIndex) => {
    const heading = blocks[spanStart];
    spans.push({
      title: heading.type === "heading" ? heading.content : "",
      spanStart,
      spanEnd: headingIndices[headingIndex + 1] ?? blocks.length,
    });
  });
  return spans;
}

function attachRecordSpans(
  blocks: Block[],
  currentOntology = false,
): ParsedMarkdownBlocks {
  const recordSpans = currentOntology
    ? (currentRecordSpans(blocks) ?? [])
    : computeRecordSpans(blocks);
  Object.defineProperty(blocks, "recordSpans", {
    value: recordSpans,
    enumerable: false,
    configurable: false,
    writable: false,
  });
  Object.defineProperty(blocks, "sectionSpans", {
    value: computeSectionSpans(blocks, recordSpans),
    enumerable: false,
    configurable: false,
    writable: false,
  });
  return blocks as ParsedMarkdownBlocks;
}

export function parseMarkdownToBlocks(
  markdown: string,
  currentOntology = false,
): ParsedMarkdownBlocks {
  if (!markdown) return attachRecordSpans([], currentOntology);

  const blocks: Block[] = [];
  const lines = markdown.split("\n");
  let i = 0;

  // Precompute character offset of each line start for sourceRange tracking.
  // This lets us record exact byte ranges in the source, avoiding lossy
  // reconstruction via blockToRawMarkdown for live workspace file offset mapping.
  const lineStarts: number[] = new Array(lines.length);
  {
    let pos = 0;
    for (let li = 0; li < lines.length; li++) {
      lineStarts[li] = pos;
      pos += lines[li].length + 1; // +1 for \n
    }
  }

  /** Helper: compute sourceRange from inclusive start/end line indices */
  const rangeOf = (startLine: number, endLine: number) => ({
    start: lineStarts[startLine],
    length: lineStarts[endLine] + lines[endLine].length - lineStarts[startLine],
  });

  // Check for frontmatter at start
  if (lines[0]?.trim() === "---") {
    let end = -1;
    for (let j = 1; j < lines.length; j++) {
      if (lines[j].trim() === "---") {
        end = j;
        break;
      }
    }
    if (end > 0) {
      const fmContent = lines.slice(1, end).join("\n");
      const fullContent = lines.slice(0, end + 1).join("\n");
      blocks.push({
        type: "frontmatter",
        content: fmContent,
        fields: parseFrontmatter(fullContent),
        sourceRange: rangeOf(0, end),
      });
      i = end + 1;
    }
  }

  while (i < lines.length) {
    const line = lines[i];
    const trimmed = line.trim();

    // Skip empty lines
    if (trimmed === "") {
      i++;
      continue;
    }

    // Identity lines stay separate even when anchor/marker/body have no blank lines.
    if (CURRENT_ANCHOR.test(trimmed) || CURRENT_MARKER.test(trimmed)) {
      blocks.push({
        type: "paragraph",
        content: trimmed,
        sourceRange: rangeOf(i, i),
      });
      i++;
      continue;
    }

    // Horizontal rule
    if (/^(---|\*\*\*|___)$/.test(trimmed) && i > 0) {
      blocks.push({
        type: "hr",
        marker: trimmed as "---" | "***" | "___",
        sourceRange: rangeOf(i, i),
      });
      i++;
      continue;
    }

    // ATX Heading
    const headingMatch = trimmed.match(/^(#{1,6})\s+(.*)/);
    if (headingMatch) {
      const level = headingMatch[1].length as 1 | 2 | 3 | 4 | 5 | 6;
      blocks.push({
        type: "heading",
        level,
        content: headingMatch[2].trim(),
        sourceRange: rangeOf(i, i),
      });
      i++;
      continue;
    }

    // Fenced code block. Preserve the exact marker so a Rich edit does not
    // normalize tildes, longer fences, or indentation elsewhere in the block.
    const codeFenceMatch = line.match(CODE_FENCE_RE);
    if (codeFenceMatch) {
      const codeStartLine = i;
      const indent = codeFenceMatch[1];
      const fence = codeFenceMatch[2];
      const openingSuffix = codeFenceMatch[3];
      const language = openingSuffix.trim();
      const codeLines: string[] = [];
      i++;
      let closingFence: string | undefined;
      let closingIndent: string | undefined;
      let closingSuffix: string | undefined;
      while (i < lines.length) {
        const closingMatch = lines[i].match(/^(\s{0,3})(`{3,}|~{3,})(\s*)$/);
        if (
          closingMatch &&
          closingMatch[2][0] === fence[0] &&
          closingMatch[2].length >= fence.length
        ) {
          closingIndent = closingMatch[1];
          closingFence = closingMatch[2];
          closingSuffix = closingMatch[3];
          break;
        }
        codeLines.push(lines[i]);
        i++;
      }
      const closed = closingFence != null;
      const codeEndLine = closed ? i : i - 1;
      if (closed) i++;
      blocks.push({
        type: "code",
        language,
        content: codeLines.join("\n"),
        fence,
        openingSuffix,
        ...(closingFence ? { closingFence } : {}),
        ...(closingSuffix ? { closingSuffix } : {}),
        ...(indent ? { indent } : {}),
        ...(closingIndent ? { closingIndent } : {}),
        closed,
        sourceRange: rangeOf(codeStartLine, codeEndLine),
      });
      continue;
    }

    // Callout directives (:::note, :::warning, :::tip, :::danger)
    const calloutMatch = trimmed.match(/^:::(note|warning|tip|danger)\s*$/);
    if (calloutMatch) {
      const calloutStartLine = i;
      const kind = calloutMatch[1] as "note" | "warning" | "tip" | "danger";
      const contentLines: string[] = [];
      i++;
      while (i < lines.length && lines[i].trim() !== ":::") {
        contentLines.push(lines[i]);
        i++;
      }
      const closed = i < lines.length;
      const calloutEndLine = closed ? i : i - 1;
      if (closed) i++; // skip closing :::
      blocks.push({
        type: "callout",
        kind,
        content: contentLines.join("\n"),
        closed,
        sourceRange: rangeOf(calloutStartLine, calloutEndLine),
      });
      continue;
    }

    // GFM pipe table
    if (trimmed.includes("|") && i + 1 < lines.length) {
      const nextLine = lines[i + 1]?.trim() ?? "";
      // Check if next line is a separator (e.g., |---|---|). Shares the named
      // TABLE_DELIMITER_RE with startsBlockConstruct so the two table checks
      // cannot drift out of sync. [D58]
      if (TABLE_DELIMITER_RE.test(nextLine)) {
        const tableStartLine = i;
        // GFM cell tokenizer: split on UNESCAPED pipes only, and treat a
        // backslash-escaped `\|` as a literal pipe kept within one cell (so
        // union types like `'a' \| 'b'` and escaped pipes inside code spans
        // stay in a single column). A raw, unescaped `|` — even inside a code
        // span — remains a delimiter, matching GFM/GitHub. The optional
        // leading/trailing border pipes are dropped. `blockToRawMarkdown`
        // re-escapes literal pipes so the round-trip is lossless.
        const parseTableRow = (row: string): string[] => {
          const trimmed = row.trim();
          const cells: string[] = [];
          let current = "";
          for (let c = 0; c < trimmed.length; c++) {
            const ch = trimmed[c];
            if (ch === "|") {
              if (isEscapedMarkdownPipe(trimmed, c)) {
                // One backslash escapes the pipe; any preceding pairs remain
                // literal backslashes in the semantic cell value.
                current = current.slice(0, -1) + "|";
              } else {
                cells.push(current);
                current = "";
              }
              continue;
            }
            current += ch;
          }
          cells.push(current);
          // Drop optional GFM border-delimiter empties (leading/trailing `|`).
          if (cells.length > 1 && trimmed.startsWith("|")) cells.shift();
          if (
            cells.length > 1 &&
            trimmed.endsWith("|") &&
            !isEscapedMarkdownPipe(trimmed, trimmed.length - 1)
          ) {
            cells.pop();
          }
          return cells.map((c) => c.trim());
        };

        const headers = parseTableRow(trimmed);
        const delimiterCells = parseTableRow(nextLine);
        const alignments: TableAlignment[] = headers.map((_, columnIndex) => {
          const delimiter = delimiterCells[columnIndex]?.trim() ?? "";
          const left = delimiter.startsWith(":");
          const right = delimiter.endsWith(":");
          if (left && right) return "center";
          if (left) return "left";
          if (right) return "right";
          return null;
        });
        const leadingPipe = trimmed.startsWith("|");
        const trailingPipe =
          trimmed.endsWith("|") &&
          !isEscapedMarkdownPipe(trimmed, trimmed.length - 1);
        i += 2; // skip header + separator
        const rows: string[][] = [];
        while (i < lines.length && lines[i].trim().includes("|")) {
          rows.push(parseTableRow(lines[i].trim()));
          i++;
        }
        blocks.push({
          type: "table",
          headers,
          rows,
          alignments,
          leadingPipe,
          trailingPipe,
          sourceRange: rangeOf(tableStartLine, i - 1),
        });
        continue;
      }
    }

    // Standalone image block
    const imageMatch = parseMarkdownImageLine(trimmed);
    if (imageMatch) {
      blocks.push({
        type: "image",
        alt: imageMatch.alt,
        src: imageMatch.src,
        title: imageMatch.title,
        sourceRange: rangeOf(i, i),
      });
      i++;
      continue;
    }

    // Bullet list — group lazy continuation lines into the current item and keep
    // single-blank-separated same-marker items in one list, recording each
    // boundary's spacing so mixed tight/loose lists round-trip. [D58]
    if (BULLET_MARKER_RE.test(line)) {
      const listStartLine = i;
      const items: string[] = [];
      const markers: BulletListMarker[] = [];
      const looseSeparators: boolean[] = [];
      let nativeEditable = true;
      let pendingLoose = false;
      let lastContentLine = i;
      while (i < lines.length) {
        if (!BULLET_MARKER_RE.test(lines[i])) {
          // Exactly one blank line before another bullet marker keeps one loose
          // boundary; 2+ blank lines (or a blank + non-list line) end the list so
          // authored spacing round-trips. [D58]
          if (
            lines[i].trim() === "" &&
            i + 1 < lines.length &&
            BULLET_MARKER_RE.test(lines[i + 1])
          ) {
            pendingLoose = true;
            i += 1;
            continue;
          }
          break;
        }
        if (items.length > 0) looseSeparators.push(pendingLoose);
        pendingLoose = false;
        const markerMatch = lines[i].match(BULLET_MARKER_RE);
        if (!/^[-*+]\s+/.test(lines[i])) nativeEditable = false;
        markers.push((markerMatch?.[1] ?? "-") as BulletListMarker);
        const itemLines = [lines[i].replace(BULLET_MARKER_RE, "").trim()];
        lastContentLine = i;
        i++;
        while (i < lines.length && !startsBlockConstruct(lines, i)) {
          itemLines.push(lines[i].trim());
          lastContentLine = i;
          i++;
        }
        items.push(itemLines.join("\n"));
      }
      blocks.push({
        type: "bullet-list",
        items,
        markers,
        ...(nativeEditable ? {} : { nativeEditable: false }),
        ...(looseSeparators.some(Boolean) ? { looseSeparators } : {}),
        sourceRange: rangeOf(listStartLine, lastContentLine),
      });
      continue;
    }

    // Numbered list — same lazy-continuation + loose grouping as bullet lists. [D58]
    const numberedListMatch = trimmed.match(/^(\d+)\.\s+/);
    if (numberedListMatch) {
      const listStartLine = i;
      const listStart = Number.parseInt(numberedListMatch[1], 10);
      const items: string[] = [];
      const looseSeparators: boolean[] = [];
      let pendingLoose = false;
      let lastContentLine = i;
      while (i < lines.length) {
        if (!NUMBERED_MARKER_RE.test(lines[i])) {
          // Exactly one blank line before another numbered marker keeps one loose
          // boundary; 2+ blank lines end the list so authored spacing round-trips. [D58]
          if (
            lines[i].trim() === "" &&
            i + 1 < lines.length &&
            NUMBERED_MARKER_RE.test(lines[i + 1])
          ) {
            pendingLoose = true;
            i += 1;
            continue;
          }
          break;
        }
        if (items.length > 0) looseSeparators.push(pendingLoose);
        pendingLoose = false;
        const itemLines = [lines[i].replace(NUMBERED_MARKER_RE, "").trim()];
        lastContentLine = i;
        i++;
        while (i < lines.length && !startsBlockConstruct(lines, i)) {
          itemLines.push(lines[i].trim());
          lastContentLine = i;
          i++;
        }
        items.push(itemLines.join("\n"));
      }
      blocks.push({
        type: "numbered-list",
        items,
        start: Number.isFinite(listStart) ? listStart : 1,
        ...(looseSeparators.some(Boolean) ? { looseSeparators } : {}),
        sourceRange: rangeOf(listStartLine, lastContentLine),
      });
      continue;
    }

    // Blockquote
    if (trimmed.startsWith("> ") || trimmed === ">") {
      const bqStartLine = i;
      const quoteLines: string[] = [];
      while (
        i < lines.length &&
        (lines[i].trim().startsWith("> ") || lines[i].trim() === ">")
      ) {
        quoteLines.push(lines[i].trim().replace(/^>\s?/, ""));
        i++;
      }
      blocks.push({
        type: "blockquote",
        content: quoteLines.join("\n").trim(),
        sourceRange: rangeOf(bqStartLine, i - 1),
      });
      continue;
    }

    // Paragraph (fallback) — join with \n to preserve original line structure
    const paraStartLine = i;
    const paraLines: string[] = [];
    while (i < lines.length) {
      const currentLine = lines[i].trim();
      if (
        currentLine === "" ||
        (paraLines.length > 0 && startsBlockConstruct(lines, i))
      ) {
        break;
      }

      paraLines.push(currentLine);
      i++;
    }
    if (paraLines.length > 0) {
      blocks.push({
        type: "paragraph",
        content: paraLines.join("\n"),
        sourceRange: rangeOf(paraStartLine, i - 1),
      });
    } else {
      i++;
    }
  }

  return attachRecordSpans(blocks, currentOntology);
}

// ---------------------------------------------------------------------------
// formatLargeNumber — Format numbers for display
// ---------------------------------------------------------------------------

/**
 * Formats numbers for display: 1234567 → "1.2M", 3456 → "3.5K", 123 → "123".
 */
export function formatLargeNumber(n: number): string {
  if (n == null || isNaN(n)) return "0";

  if (n >= 1_000_000) {
    return `${(n / 1_000_000).toFixed(1)}M`;
  }
  if (n >= 1_000) {
    return `${(n / 1_000).toFixed(1)}K`;
  }
  return String(n);
}

// ---------------------------------------------------------------------------
// formatCost — Format USD amounts
// ---------------------------------------------------------------------------

/**
 * Formats a USD amount: 12.5 -> "$12.50", 0.003 -> "<$0.01".
 */
export function formatCost(usd: number): string {
  if (usd == null || isNaN(usd)) return "$0.00";
  if (usd > 0 && usd < 0.01) return "<$0.01";
  return `$${usd.toFixed(2)}`;
}

// ---------------------------------------------------------------------------
// renderInlineMarkdown — Parse inline markdown to React elements
// ---------------------------------------------------------------------------

/**
 * Allows web, app-route, anchor/query, and relative links while rejecting local
 * filesystem, network-share, and unsupported-scheme hrefs.
 */
function isSafeInlineMarkdownHref(href: string): boolean {
  const trimmed = href.trim();
  if (!trimmed) return false;
  if (/\s/.test(trimmed)) return false;
  if (trimmed.startsWith("//") || trimmed.startsWith("\\\\")) return false;
  if (trimmed.startsWith("\\")) return false;
  if (trimmed.includes("\\")) return false;
  if (/^[a-zA-Z]:[\\/]/.test(trimmed)) return false;
  if (/(^|[\\/])worker-storage([\\/]|$)/i.test(trimmed)) return false;

  const scheme = trimmed.match(/^([a-zA-Z][a-zA-Z0-9+.-]*):/);
  if (scheme) {
    return ["http", "https", "mailto"].includes(scheme[1].toLowerCase());
  }

  if (trimmed.startsWith("/")) {
    if (
      /^\/(?:Users|workspace|work|tmp|var|home|etc|opt|root|private)(?:\/|$)/i.test(
        trimmed,
      )
    ) {
      return false;
    }
    return trimmed === "/workspaces" || trimmed.startsWith("/workspaces/");
  }

  return true;
}

/** Absolute http(s)/mailto href — the only links restricted Markdown keeps. */
export function isExternalWebHref(href: string): boolean {
  const trimmed = href.trim();
  if (!trimmed || /\s/.test(trimmed) || trimmed.includes("\\")) return false;
  return /^(?:https?:\/\/[^/]|mailto:)/i.test(trimmed);
}

/**
 * Visual identity of inline `g5://` reference labels.
 */
const DEEP_LINK_ANCHOR_CLASS =
  "cursor-pointer font-mono text-[var(--g5-info)] underline decoration-dotted underline-offset-2 hover:decoration-solid hover:bg-[var(--g5-info)]/10 rounded px-0.5";

/**
 * Shared builder for `g5://file/` deep-link anchors in inline markdown.
 * The hosting surface intercepts their marker; never use target=_blank (the g5 scheme
 * must not reach the browser). The authoring convention labels these links
 * with inline code (`[`component/id`](g5://…)`); render label formatting
 * recursively, just as for ordinary Markdown links.
 */
function createDeepLinkAnchor(
  key: number,
  href: string,
  label: string,
  dataAttribute: "data-g5-file-link",
): React.ReactElement {
  return React.createElement(
    "a",
    {
      key,
      href: href.trim(),
      [dataAttribute]: "true",
      className: DEEP_LINK_ANCHOR_CLASS,
    },
    renderInlineMarkdown(label),
  );
}

function renderUnsafeMarkdownLinkText(
  label: string,
  href: string,
  key: number,
): React.ReactElement {
  return React.createElement(
    React.Fragment,
    { key },
    label,
    " (",
    React.createElement("code", null, href),
    ")",
  );
}

/**
 * Parses inline markdown formatting into React elements:
 * - **bold** → <strong>
 * - *italic* → <em>
 * - `code` → <code>
 * - [text](url) → <a>
 * - Emoji shortcodes (:smile: etc.) → emoji characters
 *
 * `trusted: false` is the restricted mode for externally authored content
 * (e.g. GitHub issue bodies): `g5://` deep links render as plain text and only
 * absolute http(s)/mailto links become anchors — relative and app-route hrefs
 * render as plain text, so external content can never drive in-app navigation.
 */
export function replaceEmojiShortcodes(text: string): string {
  // Simple emoji map for common shortcodes
  const emojiMap: Record<string, string> = {
    ":smile:": "😊",
    ":grin:": "😁",
    ":laugh:": "😂",
    ":wink:": "😉",
    ":thumbsup:": "👍",
    ":thumbsdown:": "👎",
    ":heart:": "❤️",
    ":fire:": "🔥",
    ":star:": "⭐",
    ":check:": "✅",
    ":x:": "❌",
    ":warning:": "⚠️",
    ":info:": "ℹ️",
    ":rocket:": "🚀",
    ":bug:": "🐛",
    ":sparkles:": "✨",
    ":tada:": "🎉",
    ":thinking:": "🤔",
    ":eyes:": "👀",
    ":wave:": "👋",
    ":100:": "💯",
  };

  // Replace text leaves only; code and URLs retain their source.
  let processed = text;
  for (const [code, emoji] of Object.entries(emojiMap)) {
    processed = processed.replaceAll(code, emoji);
  }

  return processed;
}

export function renderInlineMarkdown(
  text: string,
  options: { trusted?: boolean } = {},
): React.ReactNode {
  if (!text) return null;
  const trusted = options.trusted !== false;

  const processed = replaceEmojiShortcodes(text);

  // Tokenize inline markdown
  const parts: React.ReactNode[] = [];
  // Combined regex for inline patterns
  const inlineRegex =
    /(\*\*\*([\s\S]+?)\*\*\*|\*\*([\s\S]+?)\*\*|\*([\s\S]+?)\*|`(.+?)`|\[(.+?)\]\((.+?)\))/g;

  let lastIndex = 0;
  let key = 0;
  let inlineMatch: RegExpExecArray | null;

  while ((inlineMatch = inlineRegex.exec(processed)) !== null) {
    // Add text before match
    if (inlineMatch.index > lastIndex) {
      parts.push(processed.slice(lastIndex, inlineMatch.index));
    }

    if (inlineMatch[2]) {
      // Combined emphasis must be recognized before the shorter bold token;
      // otherwise `***text***` leaks delimiter characters in read mode.
      parts.push(
        React.createElement(
          "strong",
          { key: key++ },
          React.createElement(
            "em",
            null,
            renderInlineMarkdown(inlineMatch[2], options),
          ),
        ),
      );
    } else if (inlineMatch[3]) {
      // Bold: **text**
      parts.push(
        React.createElement(
          "strong",
          { key: key++ },
          renderInlineMarkdown(inlineMatch[3], options),
        ),
      );
    } else if (inlineMatch[4]) {
      // Italic: *text*
      parts.push(
        React.createElement(
          "em",
          { key: key++ },
          renderInlineMarkdown(inlineMatch[4], options),
        ),
      );
    } else if (inlineMatch[5]) {
      // Inline code: `text`
      parts.push(React.createElement("code", { key: key++ }, inlineMatch[5]));
    } else if (inlineMatch[6] && inlineMatch[7]) {
      // Link: [text](url)
      const label = inlineMatch[6];
      const href = inlineMatch[7];
      if (!trusted) {
        parts.push(
          isExternalWebHref(href)
            ? React.createElement(
                "a",
                {
                  key: key++,
                  href: href.trim(),
                  target: "_blank",
                  rel: "noopener noreferrer nofollow",
                },
                renderInlineMarkdown(label, options),
              )
            : renderUnsafeMarkdownLinkText(label, href, key++),
        );
      } else if (/^g5:\/\/file\//.test(href.trim())) {
        // Workspace source-file deep link: the host workspace page intercepts the
        // `data-g5-file-link` marker and opens the file through the
        // citation file-open pathway.
        parts.push(
          createDeepLinkAnchor(key++, href, label, "data-g5-file-link"),
        );
      } else if (isSafeInlineMarkdownHref(href)) {
        parts.push(
          React.createElement(
            "a",
            {
              key: key++,
              href,
              target: "_blank",
              rel: "noopener noreferrer",
            },
            renderInlineMarkdown(label, options),
          ),
        );
      } else {
        parts.push(renderUnsafeMarkdownLinkText(label, href, key++));
      }
    }

    lastIndex = inlineMatch.index + inlineMatch[0].length;
  }

  // Add remaining text
  if (lastIndex < processed.length) {
    parts.push(processed.slice(lastIndex));
  }

  return parts.length === 1
    ? parts[0]
    : React.createElement(React.Fragment, null, ...parts);
}

/**
 * Serialize one list item (possibly multi-line, from a wrapped/lazy-continuation
 * source) under its marker, indenting continuation lines so they re-parse back
 * into the same item. [D58]
 */
function listItemToRawLines(item: string, marker: string): string {
  const indent = " ".repeat(marker.length);
  return item
    .split("\n")
    .map((line, index) =>
      index === 0 ? `${marker}${line}` : `${indent}${line}`,
    )
    .join("\n");
}

/** Join rendered list items, inserting a blank line at each loose boundary
 *  (`looseSeparators[k-1]`) so mixed tight/loose spacing round-trips. [D58] */
function joinListItems(
  rendered: string[],
  looseSeparators?: boolean[],
): string {
  if (rendered.length === 0) return "";
  let out = rendered[0];
  for (let k = 1; k < rendered.length; k++) {
    out += (looseSeparators?.[k - 1] ? "\n\n" : "\n") + rendered[k];
  }
  return out;
}

/** Serialize a bullet list, preserving each boundary's loose/tight spacing. [D58] */
export function bulletListToRaw(
  items: string[],
  looseSeparators?: boolean[],
  markers?: readonly BulletListMarker[],
): string {
  return joinListItems(
    items.map((item, index) =>
      listItemToRawLines(item, `${markers?.[index] ?? "-"} `),
    ),
    looseSeparators,
  );
}

function tableAlignmentDelimiter(alignment: TableAlignment): string {
  if (alignment === "left") return ":---";
  if (alignment === "right") return "---:";
  if (alignment === "center") return ":---:";
  return "---";
}

function escapeTableCell(cell: string): string {
  return cell.replace(/\s*\r?\n\s*/g, " ").replace(/\|/g, "\\|");
}

/** A pipe is escaped only by an odd run of immediately preceding backslashes. */
function isEscapedMarkdownPipe(line: string, pipeIndex: number): boolean {
  if (pipeIndex < 0 || line[pipeIndex] !== "|") return false;
  let backslashes = 0;
  for (
    let cursor = pipeIndex - 1;
    cursor >= 0 && line[cursor] === "\\";
    cursor--
  ) {
    backslashes++;
  }
  return backslashes % 2 === 1;
}

/** Serialize a rectangular GFM table while preserving column alignment and
 * outer-pipe style. Cell text is the parsed semantic value, so literal pipes
 * are escaped exactly once on output. */
export function tableToRawMarkdown(
  headers: string[],
  rows: string[][],
  alignments?: readonly TableAlignment[],
  leadingPipe = true,
  trailingPipe = true,
): string {
  const width = Math.max(1, headers.length, ...rows.map((row) => row.length));
  const normalizedHeaders = Array.from(
    { length: width },
    (_, index) => headers[index] ?? "",
  );
  const normalizedRows = rows.map((row) =>
    Array.from({ length: width }, (_, index) => row[index] ?? ""),
  );
  const renderRow = (cells: string[]) => {
    const body = cells.map(escapeTableCell).join(" | ");
    return `${leadingPipe ? "| " : ""}${body}${trailingPipe ? " |" : ""}`;
  };

  return [
    renderRow(normalizedHeaders),
    renderRow(
      normalizedHeaders.map((_, index) =>
        tableAlignmentDelimiter(alignments?.[index] ?? null),
      ),
    ),
    ...normalizedRows.map(renderRow),
  ].join("\n");
}

/** Serialize a numbered list from its start marker, preserving each boundary's
 *  loose/tight spacing. [D58] */
export function numberedListToRaw(
  items: string[],
  start = 1,
  looseSeparators?: boolean[],
): string {
  return joinListItems(
    items.map((item, index) => listItemToRawLines(item, `${start + index}. `)),
    looseSeparators,
  );
}

/**
 * Reconstruct the raw markdown for a single Block.
 */
export function blockToRawMarkdown(block: Block): string {
  let raw: string;
  switch (block.type) {
    case "frontmatter":
      return `---\n${block.content}\n---`;
    case "heading":
      raw = "#".repeat(block.level) + " " + block.content;
      break;
    case "paragraph":
      raw = block.content;
      break;
    case "code": {
      const fence = block.fence ?? "```";
      const indent = block.indent ?? "";
      const opening = `${indent}${fence}${block.openingSuffix ?? block.language}`;
      raw = `${opening}\n${block.content}`;
      if (block.closed !== false) {
        raw += `\n${block.closingIndent ?? indent}${block.closingFence ?? fence}${block.closingSuffix ?? ""}`;
      }
      break;
    }
    case "bullet-list":
      raw = bulletListToRaw(block.items, block.looseSeparators, block.markers);
      break;
    case "numbered-list":
      raw = numberedListToRaw(
        block.items,
        block.start ?? 1,
        block.looseSeparators,
      );
      break;
    case "blockquote":
      raw = block.content
        .split("\n")
        .map((l) => `> ${l}`)
        .join("\n");
      break;
    case "image":
      raw =
        `![${escapeMarkdownImageAlt(block.alt)}](` +
        `${escapeMarkdownImageDestination(block.src)}` +
        `${block.title ? ` "${escapeMarkdownImageTitle(block.title)}"` : ""})`;
      break;
    case "table": {
      raw = tableToRawMarkdown(
        block.headers,
        block.rows,
        block.alignments,
        block.leadingPipe ?? true,
        block.trailingPipe ?? true,
      );
      break;
    }
    case "callout":
      raw = `:::${block.kind}\n${block.content}`;
      if (block.closed !== false) raw += "\n:::";
      break;
    case "hr":
      return block.marker ?? "---";
    default:
      return "";
  }

  return raw;
}

// ---------------------------------------------------------------------------
// DOM helpers
// ---------------------------------------------------------------------------

/** Trigger a browser download of a Blob, cleaning up the temporary object URL. */
export function downloadBlob(blob: Blob, filename: string): void {
  if (typeof window === "undefined" || typeof document === "undefined") return;
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  anchor.style.display = "none";
  document.body.appendChild(anchor);
  anchor.click();
  anchor.remove();
  URL.revokeObjectURL(url);
}

/**
 * Defense-in-depth sanitizer for server-rendered inline SVG before it is
 * injected via dangerouslySetInnerHTML: drops <script>/<foreignObject>, any
 * on* event-handler attributes, and javascript: URLs. The SVG is produced by
 * the g5 backend renderer (with escaped node text), but this prevents an
 * unexpected payload from executing in the app origin. When no DOM parser is
 * available (SSR) the string is returned unchanged — it is never injected there.
 */
export function sanitizeInlineSvg(svg: string): string {
  if (
    typeof window === "undefined" ||
    typeof window.DOMParser === "undefined"
  ) {
    return svg;
  }
  const doc = new window.DOMParser().parseFromString(svg, "image/svg+xml");
  if (doc.getElementsByTagName("parsererror").length > 0) return "";
  doc.querySelectorAll("script, foreignObject").forEach((el) => el.remove());
  doc.querySelectorAll("*").forEach((el) => {
    for (const attr of Array.from(el.attributes)) {
      const name = attr.name.toLowerCase();
      const isEventHandler = name.startsWith("on");
      const isJsUrl =
        (name === "href" || name === "xlink:href") &&
        /^\s*javascript:/i.test(attr.value);
      if (isEventHandler || isJsUrl) el.removeAttribute(attr.name);
    }
  });
  return doc.documentElement ? doc.documentElement.outerHTML : "";
}
