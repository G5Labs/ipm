/**
 * Markdown to rich-editor document and back (`g5.documents.preview.rich-editing`).
 *
 * Loading parses with the shared remark pipeline and maps what the editor
 * models (paragraphs, headings, lists, task items, quotes, code, tables,
 * links and inline formatting) to editor nodes. Everything else becomes an
 * opaque node that keeps its source. Saving serializes the edited document:
 * a top-level block whose content is unchanged since load is written as its
 * original bytes, together with the whitespace that separated it from its
 * original neighbour; changed and new blocks are serialized in the file's
 * own marker style, and opaque blocks are always written verbatim.
 */
import { unified } from "unified";
import remarkParse from "remark-parse";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import type * as Md from "mdast";
import { toMarkdown, type Options } from "mdast-util-to-markdown";
import { gfmToMarkdown } from "mdast-util-gfm";
import { mathToMarkdown } from "mdast-util-math";
import type { JSONContent } from "@tiptap/core";
import type { Mark, Node as PMNode } from "@tiptap/pm/model";
import { documentMarkup } from "./documentMarkup";
import { ID_BLOCK_LABEL_CANDIDATE_RE } from "../../lib/utils";
import { CURRENT_MARKER } from "../../lib/currentOntology";

const parser = unified().use(remarkParse).use(remarkGfm).use(remarkMath);

/** The shared renderer's front matter (`MarkdownPreview`). */
const FRONT_MATTER =
  /^(?:\uFEFF)?---\r?\n[\s\S]*?\r?\n(?:---|\.\.\.)[^\S\r\n]*(?:\r?\n|$)/;
/** The shared renderer's `:::note` admonitions (`remarkG5Markdown`). */
const ADMONITION =
  /^:::[ \t]*(note|tip|warning|danger)[ \t]*\r?\n[\s\S]*?^:::[ \t]*(?:\r?\n|$)/gm;
/** GitHub alert markers (`remarkGithubAlerts`). */
const ALERT_MARKER =
  /^\[!(note|tip|important|warning|caution)\][ \t]*(?:\r?\n|$)/i;

export type OpaqueKind =
  | "frontmatter"
  | "structure"
  | "label"
  | "html"
  | "math"
  | "definition"
  | "footnote"
  | "alert"
  | "question"
  | "list"
  | "image"
  | "reference"
  | "link"
  | "other";

/** How the file writes Markdown, so serialized blocks match it. */
export interface MarkdownStyle {
  bullet: "-" | "*" | "+";
  bulletOrdered: "." | ")";
  emphasis: "*" | "_";
  strong: "*" | "_";
  setext: boolean;
  fence: "`" | "~";
  rule: "-" | "*" | "_";
  ruleRepetition: number;
  ruleSpaces: boolean;
  incrementListMarker: boolean;
  tablePipeAlign: boolean;
  tableCellPadding: boolean;
}

interface Entry {
  /** The node's exact source as loaded, from its first character. */
  src: string;
  /**
   * The spaces before the node on its first line. A top-level block may be
   * indented up to three spaces without changing its meaning, and what
   * follows it can depend on that indentation, so a serialized block keeps it.
   */
  indent: string;
  /** The source between the node's line and the end of the one before it at load. */
  gap: string;
  /** The node that preceded it at load. */
  prev: string | null;
  /** Continues its predecessor's paragraph: a record label's first body line. */
  glued: boolean;
  /** Where the node began in the loaded source. */
  start: number;
  /** The editor's node as loaded; unchanged while structurally equal. */
  node: PMNode | null;
}

/** What serialization needs to know about the loaded file. */
export class RichBaseline {
  constructor(
    readonly lead: string,
    readonly tail: string,
    readonly eol: "\n" | "\r\n",
    readonly style: MarkdownStyle,
    readonly entries: ReadonlyMap<string, Entry>,
  ) {}

  /** Record the editor's own nodes as loaded, after it built its document. */
  seal(doc: PMNode): void {
    doc.forEach((child) => {
      const entry = this.entryOf(child);
      if (entry && !entry.node) entry.node = child;
    });
  }

  entryOf(node: PMNode): Entry | undefined {
    const uid = node.attrs.uid;
    return typeof uid === "string" ? this.entries.get(uid) : undefined;
  }
}

export interface RichDocument {
  json: JSONContent;
  baseline: RichBaseline;
}

interface TopNode {
  json: JSONContent;
  start: number;
  end: number;
  glued?: boolean;
}

type Point = { position?: Md.Node["position"] };

function stripPositions<T>(node: T): T {
  return JSON.parse(
    JSON.stringify(node, (key, value) =>
      key === "position" ? undefined : value,
    ),
  ) as T;
}

const normalizeEol = (text: string) => text.replace(/\r\n?/g, "\n");

function dominantEol(source: string): "\n" | "\r\n" {
  const crlf = source.match(/\r\n/g)?.length ?? 0;
  const lf = (source.match(/\n/g)?.length ?? 0) - crlf;
  return crlf > lf ? "\r\n" : "\n";
}

function majority<T extends string>(counts: Map<T, number>, fallback: T): T {
  let best = fallback;
  let max = 0;
  for (const [value, count] of counts)
    if (count > max) {
      best = value;
      max = count;
    }
  return best;
}

function count<T extends string>(counts: Map<T, number>, value: T) {
  counts.set(value, (counts.get(value) ?? 0) + 1);
}

function inferStyle(
  tree: Md.Root,
  source: string,
  base: number,
): MarkdownStyle {
  const bullets = new Map<"-" | "*" | "+", number>();
  const delimiters = new Map<"." | ")", number>();
  const emphasis = new Map<"*" | "_", number>();
  const strong = new Map<"*" | "_", number>();
  const fences = new Map<"`" | "~", number>();
  const rules = new Map<"-" | "*" | "_", number>();
  let setext = 0;
  let atx = 0;
  let repeatedNumbers = 0;
  let incrementedNumbers = 0;
  let ruleRepetition = 3;
  let ruleSpaces = false;
  let tables = 0;
  let alignedTables = 0;
  let paddedTables = 0;
  const at = (node: Point) =>
    node.position?.start.offset === undefined
      ? -1
      : node.position.start.offset + base;
  const text = (node: Point) =>
    node.position?.start.offset === undefined ||
    node.position?.end.offset === undefined
      ? ""
      : source.slice(
          node.position.start.offset + base,
          node.position.end.offset + base,
        );
  const visit = (node: Md.Nodes) => {
    switch (node.type) {
      case "list":
        if (node.ordered) {
          const numbers: number[] = [];
          for (const item of node.children) {
            const match = /^\s*(\d+)([.)])/.exec(text(item));
            if (!match) continue;
            numbers.push(Number(match[1]));
            count(delimiters, match[2] as "." | ")");
          }
          if (numbers.length > 1) {
            if (numbers.every((n) => n === numbers[0])) repeatedNumbers++;
            else incrementedNumbers++;
          }
        } else
          for (const item of node.children) {
            const marker = source.charAt(at(item));
            if (marker === "-" || marker === "*" || marker === "+")
              count(bullets, marker);
          }
        break;
      case "emphasis": {
        const marker = source.charAt(at(node));
        if (marker === "*" || marker === "_") count(emphasis, marker);
        break;
      }
      case "strong": {
        const marker = source.charAt(at(node));
        if (marker === "*" || marker === "_") count(strong, marker);
        break;
      }
      case "heading":
        if (node.depth <= 2) {
          if (source.charAt(at(node)) === "#") atx++;
          else setext++;
        }
        break;
      case "code": {
        const first = text(node).trimStart().charAt(0);
        if (first === "`" || first === "~") count(fences, first);
        break;
      }
      case "thematicBreak": {
        const line = text(node).trim();
        const marker = line.charAt(0);
        if (marker === "-" || marker === "*" || marker === "_") {
          count(rules, marker);
          ruleSpaces = /\s/.test(line);
          ruleRepetition = Math.max(3, line.replace(/\s/g, "").length);
        }
        break;
      }
      case "table": {
        tables++;
        const lines = text(node).split(/\r?\n/);
        if (lines.every((line) => line.length === lines[0].length))
          alignedTables++;
        if (/\|\s/.test(lines[0] ?? "")) paddedTables++;
        break;
      }
    }
    if ("children" in node)
      for (const child of node.children as Md.Nodes[]) visit(child);
  };
  visit(tree);
  return {
    bullet: majority(bullets, "-"),
    bulletOrdered: majority(delimiters, "."),
    emphasis: majority(emphasis, "*"),
    strong: majority(strong, "*"),
    setext: setext > atx,
    fence: majority(fences, "`"),
    rule: majority(rules, "-"),
    ruleRepetition,
    ruleSpaces,
    incrementListMarker: repeatedNumbers <= incrementedNumbers,
    tablePipeAlign: tables === 0 || alignedTables * 2 >= tables,
    tableCellPadding: tables === 0 || paddedTables * 2 >= tables,
  };
}

const ATTENTION = new Set(["bold", "italic", "strike"]);

/**
 * The record lines Rich text hides in an ontology file: exactly those Preview
 * hides, so one rule decides both views. That is `documentMarkup`'s hidden set
 * (paired metadata and the Metadata heading from
 * `disclosedCurrentMetadataIndices`, each statement's anchor and identity
 * heading) and each statement's marker, which Preview's record frame replaces;
 * a statement with a diagnostic keeps its own lines visible for repair.
 */
export function hiddenRecordLines(
  markup: ReturnType<typeof documentMarkup>,
): Set<number> {
  const hidden = new Set(markup.hidden);
  for (const record of markup.records)
    if (record.current) hidden.add(record.spanStart);
  for (const record of markup.records) {
    const current = record.current;
    if (current?.diagnostic)
      for (const index of [
        current.anchorIndex,
        current.headingIndex,
        record.spanStart,
      ])
        hidden.delete(index);
  }
  hidden.delete(-1);
  return hidden;
}

/** How a protected record line shows in Rich text. */
interface Presentation {
  /** Kept in the document, but not displayed. */
  hidden: boolean;
  /**
   * On a hidden statement marker: the heading level at or above which a
   * heading ends the statement (its identity heading's level, else 6), the
   * boundary `currentRecordSpans` uses. Zero elsewhere.
   */
  statement: number;
}

class Loader {
  readonly structureStarts = new Set<number>();
  readonly labelStarts = new Set<number>();
  /** Source offsets of the blocks `documentMarkup` parsed, in order. */
  readonly blockStarts: number[] = [];
  readonly hiddenStarts = new Set<number>();
  readonly statementLevels = new Map<number, number>();

  constructor(
    readonly source: string,
    readonly base: number,
    ontology: boolean,
  ) {
    const markup = documentMarkup(source, ontology);
    const start = (index: number) => markup.blocks[index]?.sourceRange?.start;
    if (ontology) {
      for (const block of markup.blocks)
        if (block.sourceRange) this.blockStarts.push(block.sourceRange.start);
      for (const index of hiddenRecordLines(markup)) {
        const offset = start(index);
        if (offset !== undefined) this.hiddenStarts.add(offset);
      }
      for (const record of markup.records) {
        const current = record.current;
        const offset = start(record.spanStart);
        if (!current || offset === undefined || !this.hiddenStarts.has(offset))
          continue;
        const heading = markup.blocks[current.headingIndex];
        this.statementLevels.set(
          offset,
          heading?.type === "heading" ? heading.level : 6,
        );
      }
    }
    for (const index of markup.hidden) {
      const offset = start(index);
      if (offset !== undefined) this.structureStarts.add(offset);
    }
    for (const record of markup.records) {
      const offset = start(record.spanStart);
      if (offset === undefined) continue;
      this.structureStarts.add(offset);
      if (!record.current) this.labelStarts.add(offset);
    }
    // The metadata footer's heading is structure even where some entries
    // stay visible: renaming it would undo every statement's disclosure.
    if (ontology) {
      const footer = markup.blocks.findIndex(
        (block) =>
          block.type === "heading" &&
          block.level === 2 &&
          block.content === "Metadata",
      );
      const offset = footer >= 0 ? start(footer) : undefined;
      if (offset !== undefined) this.structureStarts.add(offset);
    }
  }

  /** A protected node's presentation, from the blocks that begin within it. */
  presentation(start: number, end: number): Presentation {
    let low = 0;
    let high = this.blockStarts.length;
    while (low < high) {
      const middle = (low + high) >> 1;
      if (this.blockStarts[middle] < start) low = middle + 1;
      else high = middle;
    }
    const inside: number[] = [];
    for (let i = low; this.blockStarts[i] < end; i++)
      inside.push(this.blockStarts[i]);
    return {
      hidden:
        inside.length > 0 &&
        inside.every((offset) => this.hiddenStarts.has(offset)),
      statement: Math.max(
        0,
        ...inside.map((offset) => this.statementLevels.get(offset) ?? 0),
      ),
    };
  }

  startOf(node: Point): number {
    return (node.position?.start.offset ?? 0) + this.base;
  }

  endOf(node: Point): number {
    return (node.position?.end.offset ?? 0) + this.base;
  }

  slice(node: Point): string {
    return this.source.slice(this.startOf(node), this.endOf(node));
  }

  opaque(
    kind: OpaqueKind,
    node: Md.Nodes,
    topLevel: boolean,
    extra: {
      protected?: boolean;
      mdast?: Md.Nodes;
      presentation?: Presentation;
    } = {},
  ): JSONContent {
    return {
      type: "opaqueBlock",
      attrs: {
        kind,
        src: topLevel ? this.slice(node) : "",
        mdast: JSON.stringify(stripPositions(extra.mdast ?? node)),
        protected: extra.protected ?? false,
        ...extra.presentation,
      },
    };
  }

  block(node: Md.RootContent, topLevel: boolean): JSONContent {
    switch (node.type) {
      case "paragraph":
        return { type: "paragraph", content: this.inline(node.children, []) };
      case "heading":
        return {
          type: "heading",
          attrs: { level: node.depth },
          content: this.inline(node.children, []),
        };
      case "thematicBreak":
        return { type: "horizontalRule" };
      case "blockquote": {
        const alert = this.alert(node);
        if (alert)
          return this.opaque("alert", node, topLevel, { mdast: alert });
        return {
          type: "blockquote",
          content: node.children.length
            ? node.children.map((child) => this.block(child, false))
            : [{ type: "paragraph" }],
        };
      }
      case "code":
        if (/^g5-question/.test(node.lang ?? ""))
          return this.opaque("question", node, topLevel);
        return {
          type: "codeBlock",
          attrs: { language: node.lang ?? null, meta: node.meta ?? null },
          content: node.value
            ? [{ type: "text", text: normalizeEol(node.value) }]
            : undefined,
        };
      case "list":
        return this.list(node, topLevel);
      case "table":
        return this.table(node);
      case "html":
        return this.opaque("html", node, topLevel);
      case "math":
        return this.opaque("math", node, topLevel);
      case "definition":
        return this.opaque("definition", node, topLevel);
      case "footnoteDefinition":
        return this.opaque("footnote", node, topLevel);
      default:
        return this.opaque("other", node, topLevel);
    }
  }

  /** A GitHub alert, with its marker kept as source so it never gains an escape. */
  alert(node: Md.Blockquote): Md.Blockquote | null {
    const first = node.children[0];
    if (first?.type !== "paragraph") return null;
    const text = first.children[0];
    if (text?.type !== "text") return null;
    const match = ALERT_MARKER.exec(text.value);
    if (!match) return null;
    const copy = stripPositions(node);
    const paragraph = copy.children[0] as Md.Paragraph;
    const rest = (paragraph.children[0] as Md.Text).value.slice(
      match[0].length,
    );
    const marker = match[0].replace(/[ \t]*\r?\n$/, "");
    paragraph.children.splice(
      0,
      1,
      { type: "g5Raw", value: marker + (rest ? "\n" : "") } as never,
      ...(rest ? [{ type: "text", value: rest } as Md.Text] : []),
    );
    return copy;
  }

  list(node: Md.List, topLevel: boolean): JSONContent {
    const checks = node.children.map((item) => item.checked);
    const task = checks.every((checked) => typeof checked === "boolean");
    const plain = checks.every((checked) => checked == null);
    if (
      (!task && !plain) ||
      (task && node.ordered) ||
      node.children.some(
        (item) =>
          item.children.length > 0 && item.children[0].type !== "paragraph",
      )
    )
      return this.opaque("list", node, topLevel);
    const items = node.children.map((item) => ({
      type: task ? "taskItem" : "listItem",
      attrs: {
        spread: !!item.spread,
        ...(task ? { checked: !!item.checked } : {}),
      },
      content: item.children.length
        ? item.children.map((child) => this.block(child, false))
        : [{ type: "paragraph" }],
    }));
    const spread = !!node.spread;
    if (task) return { type: "taskList", attrs: { spread }, content: items };
    if (node.ordered)
      return {
        type: "orderedList",
        attrs: { spread, start: node.start ?? 1 },
        content: items,
      };
    return { type: "bulletList", attrs: { spread }, content: items };
  }

  table(node: Md.Table): JSONContent {
    const width = Math.max(
      1,
      ...node.children.map((row) => row.children.length),
    );
    return {
      type: "table",
      content: node.children.map((row, r) => ({
        type: "tableRow",
        content: Array.from({ length: width }, (_, c) => {
          const cell = row.children[c];
          const content = cell ? this.inline(cell.children, []) : [];
          return {
            type: r === 0 ? "tableHeader" : "tableCell",
            attrs: { align: node.align?.[c] ?? null },
            content: [
              {
                type: "paragraph",
                ...(content.length ? { content } : {}),
              },
            ],
          };
        }),
      })),
    };
  }

  inline(
    children: Md.PhrasingContent[],
    marks: JSONContent["marks"] & object,
  ): JSONContent[] {
    const out: JSONContent[] = [];
    const text = (value: string, extra: typeof marks) => {
      normalizeEol(value)
        .split("\n")
        .forEach((part, index) => {
          if (index > 0)
            out.push({
              type: "softBreak",
              ...(extra.length ? { marks: extra } : {}),
            });
          if (part)
            out.push({
              type: "text",
              text: part,
              ...(extra.length ? { marks: extra } : {}),
            });
        });
    };
    const opaque = (kind: OpaqueKind, node: Md.PhrasingContent) =>
      out.push({
        type: "opaqueInline",
        attrs: {
          kind,
          mdast: JSON.stringify(stripPositions(node)),
          // Its exact source, written back when its paragraph is edited.
          src: node.position ? this.slice(node) : "",
        },
        ...(marks.length ? { marks } : {}),
      });
    for (const child of children) {
      switch (child.type) {
        case "text":
          text(child.value, marks);
          break;
        case "emphasis":
          out.push(
            ...this.inline(child.children, [...marks, { type: "italic" }]),
          );
          break;
        case "strong":
          out.push(
            ...this.inline(child.children, [...marks, { type: "bold" }]),
          );
          break;
        case "delete":
          out.push(
            ...this.inline(child.children, [...marks, { type: "strike" }]),
          );
          break;
        case "inlineCode":
          // A line ending inside a code span reads as a space.
          text(normalizeEol(child.value).replace(/\n/g, " "), [
            ...marks,
            { type: "code" },
          ]);
          break;
        case "break":
          out.push({ type: "hardBreak" });
          break;
        case "link":
          if (simpleLink(child))
            out.push(
              ...this.inline(child.children, [
                ...marks,
                {
                  type: "link",
                  attrs: { href: child.url, title: child.title ?? null },
                },
              ]),
            );
          else opaque("link", child);
          break;
        case "image":
          opaque("image", child);
          break;
        case "html":
          opaque("html", child);
          break;
        case "inlineMath":
          opaque("math", child);
          break;
        case "footnoteReference":
        case "linkReference":
        case "imageReference":
          opaque("reference", child);
          break;
        default:
          opaque("other", child);
      }
    }
    return out;
  }
}

function simpleLink(node: Md.Link): boolean {
  const simple = (child: Md.PhrasingContent): boolean =>
    child.type === "text" ||
    child.type === "inlineCode" ||
    ((child.type === "emphasis" ||
      child.type === "strong" ||
      child.type === "delete") &&
      child.children.every(simple));
  return node.children.length > 0 && node.children.every(simple);
}

/** A paragraph's first line, when the rest of its text can stand on its own. */
function splitLabel(
  loader: Loader,
  node: Md.Paragraph,
  start: number,
  end: number,
): TopNode[] | null {
  const first = node.children[0];
  if (first?.type !== "text") return null;
  const newline = first.value.search(/\r?\n/);
  if (newline < 0) return null;
  const lineEnd = loader.source.slice(start, end).search(/\r?\n/);
  if (lineEnd < 0) return null;
  const restStart =
    start + lineEnd + (loader.source.charAt(start + lineEnd) === "\r" ? 2 : 1);
  const restText = first.value.slice(newline).replace(/^\r?\n/, "");
  const rest: Md.PhrasingContent[] = [
    ...(restText ? [{ ...first, value: restText }] : []),
    ...node.children.slice(1),
  ];
  if (!rest.length) return null;
  return [
    {
      json: {
        type: "opaqueBlock",
        attrs: {
          kind: "structure",
          src: loader.source.slice(start, start + lineEnd),
          mdast: "",
          protected: true,
        },
      },
      start,
      end: start + lineEnd,
    },
    {
      json: { type: "paragraph", content: loader.inline(rest, []) },
      start: restStart,
      end,
      glued: true,
    },
  ];
}

/** Build the editor document for `source`, with what saving needs to keep it. */
export function markdownToDoc(
  source: string,
  options: { ontology?: boolean } = {},
): RichDocument {
  const eol = dominantEol(source);
  const bom = source.startsWith("\uFEFF") ? 1 : 0;
  const tops: TopNode[] = [];
  let base = bom;
  const frontMatter = FRONT_MATTER.exec(source);
  if (frontMatter) {
    const contentEnd =
      frontMatter[0].length - (/\r?\n$/.exec(frontMatter[0])?.[0].length ?? 0);
    tops.push({
      json: {
        type: "opaqueBlock",
        attrs: {
          kind: "frontmatter",
          src: source.slice(bom, contentEnd),
          mdast: "",
          protected: true,
        },
      },
      start: bom,
      end: contentEnd,
    });
    base = frontMatter[0].length;
  }
  const tree = parser.parse(source.slice(base)) as Md.Root;
  const loader = new Loader(source, base, options.ontology ?? false);
  const admonitions = [...source.matchAll(ADMONITION)].map((match) => ({
    start: match.index!,
    end: match.index! + match[0].length,
  }));
  const children = tree.children;
  for (let i = 0; i < children.length; i++) {
    const node = children[i];
    const start = loader.startOf(node);
    const end = loader.endOf(node);
    const admonition = admonitions.find((range) => range.start === start);
    if (admonition) {
      let last = i;
      while (
        last + 1 < children.length &&
        loader.startOf(children[last + 1]) < admonition.end
      )
        last++;
      const lastEnd = loader.endOf(children[last]);
      tops.push({
        json: {
          type: "opaqueBlock",
          attrs: {
            kind: "alert",
            src: source.slice(start, lastEnd),
            mdast: "",
            protected: false,
          },
        },
        start,
        end: lastEnd,
      });
      i = last;
      continue;
    }
    if (loader.structureStarts.has(start)) {
      if (loader.labelStarts.has(start) && node.type === "paragraph") {
        const split = splitLabel(loader, node, start, end);
        if (split) {
          tops.push(...split);
          continue;
        }
      }
      tops.push({
        json: loader.opaque("structure", node, true, {
          protected: true,
          presentation: loader.presentation(start, end),
        }),
        start,
        end,
      });
      continue;
    }
    if (node.type === "paragraph") {
      const firstLine = source.slice(start, end).split(/\r?\n/, 1)[0].trim();
      if (
        ID_BLOCK_LABEL_CANDIDATE_RE.test(firstLine) ||
        CURRENT_MARKER.test(firstLine)
      ) {
        tops.push({ json: loader.opaque("label", node, true), start, end });
        continue;
      }
    }
    tops.push({ json: loader.block(node, true), start, end });
  }
  const entries = new Map<string, Entry>();
  let prevUid: string | null = null;
  let prevEnd = 0;
  const indentOf = (top: TopNode, floor: number) => {
    const before = source.slice(floor, top.start);
    const indent = /[ \t]*$/.exec(before)![0];
    // Only the line's own leading spaces; a glued body may sit deeper.
    const lineStart = top.start - indent.length;
    const atLineStart =
      lineStart === floor ||
      source.charAt(lineStart - 1) === "\n" ||
      lineStart === 0;
    if (!atLineStart) return "";
    return top.glued || /^ {0,3}$/.test(indent) ? indent : "";
  };
  tops.forEach((top, index) => {
    const uid = `b${index}`;
    top.json.attrs = { ...top.json.attrs, uid };
    const indent = indentOf(top, index === 0 ? 0 : prevEnd);
    entries.set(uid, {
      src: source.slice(top.start, top.end),
      indent,
      gap: index === 0 ? "" : source.slice(prevEnd, top.start - indent.length),
      prev: prevUid,
      glued: !!top.glued,
      start: top.start,
      node: null,
    });
    prevUid = uid;
    prevEnd = top.end;
  });
  const firstIndent = tops.length ? (entries.get("b0")?.indent ?? "") : "";
  const lead = tops.length
    ? source.slice(0, tops[0].start - firstIndent.length)
    : "";
  const tail = tops.length ? source.slice(prevEnd) : source;
  // A hidden statement marker with no body before the next record line, a
  // heading that ends it or the end gets an empty paragraph, which writes
  // nothing: an empty statement can still be typed into or removed
  // (`g5.documents.ontology.creation`).
  const content: JSONContent[] = [];
  tops.forEach((top, index) => {
    content.push(top.json);
    const next = tops[index + 1]?.json;
    if (
      top.json.attrs?.hidden &&
      Number(top.json.attrs.statement) > 0 &&
      (!next ||
        (next.type === "heading" &&
          Number(next.attrs?.level) <= Number(top.json.attrs.statement)) ||
        (next.type === "opaqueBlock" && next.attrs?.protected))
    )
      content.push({ type: "paragraph" });
  });
  return {
    json: { type: "doc", content },
    baseline: new RichBaseline(
      lead,
      tail,
      eol,
      inferStyle(tree, source, base),
      entries,
    ),
  };
}

type ListKind = "ordered" | "unordered";

function listKind(node: PMNode): ListKind | null {
  switch (node.type.name) {
    case "orderedList":
      return "ordered";
    case "bulletList":
    case "taskList":
      return "unordered";
    default:
      return null;
  }
}

/** The marker a list's source uses: its bullet, or its ordered delimiter. */
function markerOf(src: string, kind: ListKind): string {
  if (kind === "ordered") return /^\s*\d+([.)])/.exec(src)?.[1] ?? ".";
  return src.trimStart().charAt(0) || "-";
}

const MARKERS: Record<ListKind, string[]> = {
  unordered: ["-", "*", "+"],
  ordered: [".", ")"],
};

function serializerOptions(style: MarkdownStyle, marker?: string): Options {
  const bullet = (
    marker && marker !== "." && marker !== ")" ? marker : style.bullet
  ) as "-" | "*" | "+";
  const bulletOrdered = (
    marker === "." || marker === ")" ? marker : style.bulletOrdered
  ) as "." | ")";
  return {
    extensions: [
      gfmToMarkdown({
        tablePipeAlign: style.tablePipeAlign,
        tableCellPadding: style.tableCellPadding,
      }),
      mathToMarkdown(),
    ],
    bullet,
    bulletOther: bullet === "-" ? "*" : "-",
    bulletOrdered,
    emphasis: style.emphasis,
    strong: style.strong,
    fence: style.fence,
    fences: true,
    rule: style.rule,
    ruleRepetition: style.ruleRepetition,
    ruleSpaces: style.ruleSpaces,
    setext: style.setext,
    listItemIndent: "one",
    incrementListMarker: style.incrementListMarker,
    closeAtx: false,
    quote: '"',
    resourceLink: false,
    handlers: {
      g5Raw: (node: { value: string }) => node.value,
    } as unknown as Options["handlers"],
  };
}

const PRECEDENCE = ["link", "strike", "bold", "italic"];

function markNode(
  mark: Mark,
): Md.PhrasingContent & { children: Md.PhrasingContent[] } {
  switch (mark.type.name) {
    case "link":
      return {
        type: "link",
        url: String(mark.attrs.href ?? ""),
        title: mark.attrs.title ? String(mark.attrs.title) : null,
        children: [],
      };
    case "strike":
      return { type: "delete", children: [] };
    case "bold":
      return { type: "strong", children: [] };
    default:
      return { type: "emphasis", children: [] };
  }
}

interface Run {
  marks: readonly Mark[];
  leaf: Md.PhrasingContent;
  /** Plain text whose surrounding whitespace may move out of emphasis. */
  text?: string;
}

function parseStored(json: unknown): Md.Nodes | null {
  if (typeof json !== "string" || !json) return null;
  try {
    return JSON.parse(json) as Md.Nodes;
  } catch {
    return null;
  }
}

function collectRuns(parent: PMNode, softBreak: string, raw: boolean): Run[] {
  const runs: Run[] = [];
  parent.forEach((child) => {
    if (child.type.name === "softBreak") {
      runs.push({
        marks: child.marks,
        leaf: { type: "text", value: softBreak },
        text: softBreak,
      });
      return;
    }
    if (child.isText) {
      const code = child.marks.some((mark) => mark.type.name === "code");
      runs.push(
        code
          ? {
              marks: child.marks,
              leaf: { type: "inlineCode", value: child.text ?? "" },
            }
          : {
              marks: child.marks,
              leaf: { type: "text", value: child.text ?? "" },
              text: child.text ?? "",
            },
      );
    } else if (child.type.name === "hardBreak")
      runs.push({ marks: child.marks, leaf: { type: "break" } });
    else if (child.type.name === "opaqueInline") {
      // The source as loaded, so an edit beside it never rewrites it; the
      // parsed form serves a node that lost its source (pasted from outside)
      // and a context the serializer must escape for.
      const src = raw ? String(child.attrs.src ?? "") : "";
      const stored = parseStored(child.attrs.mdast);
      runs.push({
        marks: child.marks,
        leaf: src
          ? ({ type: "g5Raw", value: src } as never)
          : ((stored as Md.PhrasingContent | null) ?? {
              type: "text",
              value: child.textContent,
            }),
      });
    } else
      runs.push({
        marks: child.marks,
        leaf: { type: "text", value: child.textContent },
      });
  });
  return runs;
}

/**
 * Emphasis cannot open after or close before whitespace, so whitespace at the
 * edge of a bold, italic or struck run moves outside the marks that do not
 * continue across it.
 */
function hoistWhitespace(runs: Run[]): Run[] {
  const out: Run[] = [];
  runs.forEach((run, index) => {
    if (
      run.text === undefined ||
      !run.marks.some((m) => ATTENTION.has(m.type.name))
    ) {
      out.push(run);
      return;
    }
    const before = runs[index - 1]?.marks ?? [];
    const after = runs[index + 1]?.marks ?? [];
    const keep = (neighbour: readonly Mark[]) =>
      run.marks.filter(
        (mark) =>
          !ATTENTION.has(mark.type.name) || neighbour.some((n) => n.eq(mark)),
      );
    const match = /^(\s*)([\s\S]*?)(\s*)$/.exec(run.text)!;
    const [, leading, body, trailing] = match;
    if (!body) {
      out.push({
        marks: run.marks.filter(
          (mark) =>
            !ATTENTION.has(mark.type.name) ||
            (before.some((n) => n.eq(mark)) && after.some((n) => n.eq(mark))),
        ),
        leaf: { type: "text", value: run.text },
        text: run.text,
      });
      return;
    }
    if (leading)
      out.push({
        marks: keep(before),
        leaf: { type: "text", value: leading },
        text: leading,
      });
    out.push({
      marks: run.marks,
      leaf: { type: "text", value: body },
      text: body,
    });
    if (trailing)
      out.push({
        marks: keep(after),
        leaf: { type: "text", value: trailing },
        text: trailing,
      });
  });
  return out;
}

function inlineToMdast(parent: PMNode, softBreak = "\n"): Md.PhrasingContent[] {
  const root: Md.PhrasingContent[] = [];
  const stack: { mark: Mark; children: Md.PhrasingContent[] }[] = [];
  // Raw inline source may stand only where any source may: a paragraph.
  // A heading or table cell holds one line the serializer escapes (a pipe
  // would split the cell), so it gets the parsed form.
  const raw = softBreak === "\n";
  for (const run of hoistWhitespace(collectRuns(parent, softBreak, raw))) {
    const marks = run.marks
      .filter((mark) => mark.type.name !== "code")
      .slice()
      .sort(
        (a, b) =>
          PRECEDENCE.indexOf(a.type.name) - PRECEDENCE.indexOf(b.type.name),
      );
    let keep = 0;
    while (
      keep < stack.length &&
      keep < marks.length &&
      stack[keep].mark.eq(marks[keep])
    )
      keep++;
    stack.length = keep;
    for (let i = keep; i < marks.length; i++) {
      const node = markNode(marks[i]);
      (stack.at(-1)?.children ?? root).push(node);
      stack.push({ mark: marks[i], children: node.children });
    }
    (stack.at(-1)?.children ?? root).push(run.leaf);
  }
  return root;
}

/** A table cell holds one line: breaks and further paragraphs become `<br>`. */
function cellToMdast(cell: PMNode): Md.PhrasingContent[] {
  const out: Md.PhrasingContent[] = [];
  const br = (nodes: Md.PhrasingContent[]): Md.PhrasingContent[] =>
    nodes.map((node) =>
      node.type === "break"
        ? { type: "html", value: "<br>" }
        : "children" in node
          ? ({
              ...node,
              children: br(node.children as Md.PhrasingContent[]),
            } as Md.PhrasingContent)
          : node,
    );
  cell.forEach((block, _offset, index) => {
    if (index > 0) out.push({ type: "html", value: "<br>" });
    out.push(
      ...br(
        block.isTextblock
          ? inlineToMdast(block, " ")
          : [{ type: "text", value: block.textContent }],
      ),
    );
  });
  return out;
}

function blocksToMdast(parent: PMNode): Md.RootContent[] {
  const out: Md.RootContent[] = [];
  parent.forEach((child) => {
    const node = toMdast(child, false);
    if (node) out.push(node);
  });
  return out;
}

function toMdast(node: PMNode, topLevel: boolean): Md.RootContent | null {
  switch (node.type.name) {
    case "paragraph":
      return node.childCount
        ? { type: "paragraph", children: inlineToMdast(node) }
        : null;
    case "heading":
      return {
        type: "heading",
        depth: Math.min(
          6,
          Math.max(1, Number(node.attrs.level) || 1),
        ) as Md.Heading["depth"],
        // An ATX heading is one line; a soft break in a setext title reads as a space.
        children: inlineToMdast(node, " "),
      };
    case "horizontalRule":
      return { type: "thematicBreak" };
    case "blockquote":
      return {
        type: "blockquote",
        children: blocksToMdast(node) as Md.BlockContent[],
      };
    case "codeBlock":
      return {
        type: "code",
        lang: node.attrs.language ? String(node.attrs.language) : null,
        meta: node.attrs.meta ? String(node.attrs.meta) : null,
        value: node.textContent,
      };
    case "bulletList":
    case "orderedList":
    case "taskList": {
      const ordered = node.type.name === "orderedList";
      const task = node.type.name === "taskList";
      const spread = !!node.attrs.spread;
      const children: Md.ListItem[] = [];
      node.forEach((item) =>
        children.push({
          type: "listItem",
          spread: !!item.attrs.spread,
          checked: task ? !!item.attrs.checked : null,
          children: blocksToMdast(item) as Md.BlockContent[],
        }),
      );
      return {
        type: "list",
        ordered,
        start: ordered ? Number(node.attrs.start ?? 1) : null,
        spread,
        children,
      };
    }
    case "table": {
      const rows: Md.TableRow[] = [];
      node.forEach((row) => {
        const cells: Md.TableCell[] = [];
        row.forEach((cell) =>
          cells.push({ type: "tableCell", children: cellToMdast(cell) }),
        );
        rows.push({ type: "tableRow", children: cells });
      });
      const width = Math.max(0, ...rows.map((row) => row.children.length));
      // The header row's cells carry the columns' alignment.
      const align: Md.AlignType[] = [];
      node.firstChild?.forEach((cell) =>
        align.push((cell.attrs.align ?? null) as Md.AlignType),
      );
      return {
        type: "table",
        align: Array.from({ length: width }, (_, i) => align[i] ?? null),
        children: rows,
      };
    }
    case "opaqueBlock": {
      const src = String(node.attrs.src ?? "");
      if (topLevel && src) return { type: "g5Raw", value: src } as never;
      const stored = parseStored(node.attrs.mdast);
      if (stored) return stored as Md.RootContent;
      return src ? ({ type: "g5Raw", value: src } as never) : null;
    }
    default:
      return node.textContent
        ? {
            type: "paragraph",
            children: [{ type: "text", value: node.textContent }],
          }
        : null;
  }
}

/** Markdown for one opaque construct, for showing it where it stands. */
export function opaqueMarkdown(mdast: unknown, inline = false): string {
  const stored = parseStored(mdast);
  if (!stored) return "";
  const tree = inline ? { type: "paragraph", children: [stored] } : stored;
  return toMarkdown(
    { type: "root", children: [tree] } as Md.Root,
    serializerOptions(inferStyle({ type: "root", children: [] }, "", 0)),
  ).replace(/\n$/, "");
}

function serializeBlock(
  node: PMNode,
  style: MarkdownStyle,
  eol: string,
  indent: string,
  marker?: string,
): string | null {
  const mdast = toMdast(node, true);
  if (!mdast) return null;
  const text = toMarkdown(
    { type: "root", children: [mdast] },
    serializerOptions(style, marker),
  )
    .replace(/\n$/, "")
    .split("\n")
    .map((line) => (line ? indent + line : line))
    .join(eol);
  return text;
}

interface Output {
  node: PMNode;
  index: number;
  /** The node's leading spaces, written before its first line. */
  indent: string;
  uid: string | null;
  entry: Entry | undefined;
  preserved: boolean;
  text: string | null;
  kind: ListKind | null;
  marker: string | null;
}

/**
 * The file text for an edited document. `starts` gives each top-level node's
 * offset in that text (-1 for a node that writes nothing, an empty paragraph).
 */
export function docToMarkdown(
  doc: PMNode,
  baseline: RichBaseline,
): { text: string; starts: number[] } {
  const { eol, style } = baseline;
  const outputs: Output[] = [];
  doc.forEach((node, _offset, index) => {
    if (node.type.name === "paragraph" && node.childCount === 0) return;
    const entry = baseline.entryOf(node);
    let text: string | null = null;
    if (node.type.name === "opaqueBlock" && node.attrs.src)
      text = String(node.attrs.src);
    else if (entry?.node && (node === entry.node || node.eq(entry.node)))
      text = entry.src;
    const kind = listKind(node);
    outputs.push({
      node,
      index,
      // A new block takes the indentation of the block before it.
      indent: entry?.indent ?? outputs.at(-1)?.indent ?? "",
      uid: typeof node.attrs.uid === "string" ? node.attrs.uid : null,
      entry,
      preserved: text !== null,
      text,
      kind,
      marker: kind && text !== null ? markerOf(text, kind) : null,
    });
  });
  const adjacentAtLoad = (output: Output, previous: Output | undefined) =>
    !!previous &&
    !!output.entry &&
    output.entry.prev !== null &&
    output.entry.prev === previous.uid;
  // Two lists of the same kind that become neighbours would merge into one;
  // a serialized list takes a marker its neighbours do not use.
  outputs.forEach((output, i) => {
    const previous = outputs[i - 1];
    const sameKindBefore =
      !!output.kind && previous?.kind === output.kind ? previous : undefined;
    const keptApart =
      sameKindBefore &&
      adjacentAtLoad(output, sameKindBefore) &&
      output.preserved &&
      sameKindBefore.preserved;
    if (output.text !== null) {
      if (
        !sameKindBefore ||
        keptApart ||
        sameKindBefore.marker !== output.marker
      )
        return;
    }
    let marker: string | undefined;
    if (output.kind) {
      const next = outputs[i + 1];
      const avoid = [
        sameKindBefore?.marker,
        next?.kind === output.kind && next.preserved ? next.marker : null,
      ];
      const preferred =
        output.kind === "ordered" ? style.bulletOrdered : style.bullet;
      marker =
        [preferred, ...MARKERS[output.kind]].find((m) => !avoid.includes(m)) ??
        MARKERS[output.kind].find((m) => m !== sameKindBefore?.marker) ??
        preferred;
    }
    output.text = serializeBlock(
      output.node,
      style,
      eol,
      output.indent,
      marker,
    );
    output.preserved = false;
    output.marker = marker ?? null;
  });
  const starts = new Array<number>(doc.childCount).fill(-1);
  let text = baseline.lead;
  let previous: Output | undefined;
  for (const output of outputs) {
    if (output.text === null) continue;
    if (previous) {
      let gap = eol + eol;
      const entry = output.entry;
      if (entry && adjacentAtLoad(output, previous)) {
        if (entry.glued && output.node.type.name === "paragraph")
          gap = entry.gap;
        else if (output.preserved && previous.preserved) gap = entry.gap;
        else if (/\n[ \t]*\r?\n/.test(entry.gap)) gap = entry.gap;
      }
      text += gap;
    }
    starts[output.index] = text.length;
    // A serialized block carries its indentation already.
    text += output.preserved ? output.indent + output.text : output.text;
    previous = output;
  }
  return { text: text + baseline.tail, starts };
}

/** The top-level node that began at, or most closely before, a source offset. */
export function topLevelPositionAt(
  doc: PMNode,
  baseline: RichBaseline,
  offset: number,
): number | null {
  let best = -1;
  let position: number | null = null;
  doc.forEach((node, pos) => {
    const entry = baseline.entryOf(node);
    if (entry && entry.start <= offset && entry.start > best) {
      best = entry.start;
      position = pos;
    }
  });
  return position;
}
