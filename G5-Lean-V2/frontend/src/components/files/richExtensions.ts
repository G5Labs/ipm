/**
 * The schema of Preview's rich Markdown editor
 * (`g5.documents.preview.rich-editing`): the Markdown constructs it models,
 * and opaque nodes that carry everything else verbatim. The editor and the
 * Markdown conversion share it, so they agree on what each node means.
 */
import {
  Extension,
  Node,
  isMacOS,
  isiOS,
  mergeAttributes,
  type AnyExtension,
  type NodeViewRenderer,
} from "@tiptap/core";
import {
  NodeSelection,
  Plugin,
  PluginKey,
  Selection,
  TextSelection,
  type EditorState,
  type Transaction,
} from "@tiptap/pm/state";
import { GapCursor } from "@tiptap/pm/gapcursor";
import { Decoration, DecorationSet } from "@tiptap/pm/view";
import type { Node as PMNode } from "@tiptap/pm/model";
import StarterKit from "@tiptap/starter-kit";
import { Code } from "@tiptap/extension-code";
import { TaskItem, TaskList } from "@tiptap/extension-list";
import {
  Table,
  TableCell,
  TableHeader,
  TableRow,
} from "@tiptap/extension-table";
import { CURRENT_MARKER } from "../../lib/currentOntology";
import { randomUuid } from "../../lib/randomUuid";

/** Marks a transaction that loads content rather than editing it. */
export const RICH_LOAD_META = "g5RichLoad";

/** Node types that may stand at the top level of a document. */
export const TOP_LEVEL_TYPES = [
  "paragraph",
  "heading",
  "bulletList",
  "orderedList",
  "taskList",
  "blockquote",
  "codeBlock",
  "horizontalRule",
  "table",
  "opaqueBlock",
];

/** A string or flag attribute that survives copy and paste through the DOM. */
function dataAttribute(name: string, fallback: string | boolean) {
  return {
    default: fallback,
    parseHTML: (element: HTMLElement) => {
      const value = element.getAttribute(`data-${name}`);
      if (typeof fallback === "boolean") return value === "true";
      return value ?? fallback;
    },
    renderHTML: (attributes: Record<string, unknown>) =>
      attributes[name] === fallback
        ? {}
        : { [`data-${name}`]: String(attributes[name]) },
  };
}

/**
 * Markdown the editor does not model, kept as source. `src` is the exact
 * top-level source as loaded (empty when the node was nested); `mdast` is the
 * parsed construct, used wherever the node is serialized inside a container.
 * `protected` record lines may not be removed or duplicated in the editor.
 * In ontology files a `hidden` record line stays in the document but is not
 * displayed; `statement` marks a hidden statement marker with the heading
 * level that ends its statement.
 */
export const OpaqueBlock = Node.create({
  name: "opaqueBlock",
  group: "block",
  atom: true,
  selectable: true,
  draggable: false,
  isolating: true,
  addAttributes() {
    return {
      kind: dataAttribute("kind", "other"),
      src: dataAttribute("src", ""),
      mdast: dataAttribute("mdast", ""),
      protected: dataAttribute("protected", false),
      hidden: dataAttribute("hidden", false),
      statement: {
        default: 0,
        parseHTML: (element: HTMLElement) =>
          Number(element.getAttribute("data-statement")) || 0,
        renderHTML: (attributes: Record<string, unknown>) =>
          attributes.statement
            ? { "data-statement": String(attributes.statement) }
            : {},
      },
    };
  },
  parseHTML() {
    return [{ tag: "div[data-g5-opaque-block]" }];
  },
  renderHTML({ HTMLAttributes }) {
    return [
      "div",
      mergeAttributes(HTMLAttributes, { "data-g5-opaque-block": "" }),
    ];
  },
});

/**
 * A soft line break inside a paragraph: it reads as a space, as rendered
 * Markdown does, and saves as the line ending the source had, so a
 * hard-wrapped paragraph keeps its wrapping where the person did not type.
 */
export const SoftBreak = Node.create({
  name: "softBreak",
  group: "inline",
  inline: true,
  atom: true,
  selectable: false,
  parseHTML() {
    return [{ tag: "span[data-g5-soft-break]" }];
  },
  renderHTML() {
    return ["span", { "data-g5-soft-break": "", class: "g5-soft-break" }, " "];
  },
  renderText() {
    return " ";
  },
});

/**
 * Inline Markdown the editor does not model: raw HTML, math, images,
 * references. `src` is its exact source; `mdast` the parsed construct.
 */
export const OpaqueInline = Node.create({
  name: "opaqueInline",
  group: "inline",
  inline: true,
  atom: true,
  selectable: true,
  draggable: false,
  addAttributes() {
    return {
      kind: dataAttribute("kind", "other"),
      src: dataAttribute("src", ""),
      mdast: dataAttribute("mdast", ""),
    };
  },
  parseHTML() {
    return [{ tag: "span[data-g5-opaque-inline]" }];
  },
  renderHTML({ HTMLAttributes }) {
    return [
      "span",
      mergeAttributes(HTMLAttributes, { "data-g5-opaque-inline": "" }),
    ];
  },
});

/**
 * Bookkeeping that serialization needs and the DOM never shows: a top-level
 * node's identity at load, list looseness, code info strings and column
 * alignment. None of it carries over when a node is split.
 */
const Bookkeeping = Extension.create({
  name: "g5Bookkeeping",
  addGlobalAttributes() {
    return [
      {
        types: TOP_LEVEL_TYPES,
        attributes: {
          uid: { default: null, keepOnSplit: false, rendered: false },
        },
      },
      {
        types: [
          "bulletList",
          "orderedList",
          "taskList",
          "listItem",
          "taskItem",
        ],
        attributes: {
          spread: { default: false, keepOnSplit: false, rendered: false },
        },
      },
      {
        types: ["codeBlock"],
        attributes: {
          meta: { default: null, keepOnSplit: false, rendered: false },
        },
      },
      {
        // On the cells, so a column keeps its alignment when columns are
        // inserted or deleted around it.
        types: ["tableHeader", "tableCell"],
        attributes: { align: { default: null, rendered: false } },
      },
    ];
  },
});

const signatures = new WeakMap<PMNode, string>();

/** The protected record lines a document holds, as a comparable multiset. */
export function protectedSignature(doc: PMNode): string {
  const cached = signatures.get(doc);
  if (cached !== undefined) return cached;
  const lines: string[] = [];
  doc.descendants((node) => {
    if (node.type.name === "opaqueBlock" && node.attrs.protected)
      lines.push(String(node.attrs.src));
    return !node.isTextblock && !node.isAtom;
  });
  const signature = lines.sort().join("\u0000");
  signatures.set(doc, signature);
  return signature;
}

/** The level of a heading written as source (`### x`), else 0. */
function sourceHeadingLevel(src: string): number {
  return /^ {0,3}(#{1,6})(?:[ \t]|$)/.exec(src)?.[1].length ?? 0;
}

const isHiddenLine = (node: PMNode) => isProtected(node) && !!node.attrs.hidden;

/**
 * An ontology statement's protected lines as top-level indices: its identity
 * run (hidden anchor, identity heading and marker, ending at the marker) and
 * its metadata entry (the hidden `### ID` line and the hidden lines after it
 * up to the next heading of level 3 or above), empty when it has none.
 */
export interface StatementUnit {
  id: string;
  identity: { from: number; to: number };
  metadata: { from: number; to: number } | null;
}

/** The hidden `## Metadata` heading's index and every statement's unit. */
export function statementUnits(doc: PMNode): {
  footer: number;
  units: StatementUnit[];
} {
  const nodes: PMNode[] = [];
  doc.forEach((node) => nodes.push(node));
  const footer = nodes.findIndex(
    (node) =>
      isProtected(node) &&
      sourceHeadingLevel(String(node.attrs.src)) === 2 &&
      String(node.attrs.src)
        .replace(/^ {0,3}##/, "")
        .trim() === "Metadata",
  );
  const entries = new Map<string, { from: number; to: number }>();
  for (let index = footer + 1; footer >= 0 && index < nodes.length; index++) {
    const src = String(nodes[index].attrs.src ?? "");
    if (!isHiddenLine(nodes[index]) || sourceHeadingLevel(src) !== 3) continue;
    let to = index;
    while (
      to + 1 < nodes.length &&
      isHiddenLine(nodes[to + 1]) &&
      !between(sourceHeadingLevel(String(nodes[to + 1].attrs.src)), 1, 3)
    )
      to++;
    const id = src.replace(/^ {0,3}###/, "").trim();
    if (!entries.has(id)) entries.set(id, { from: index, to });
  }
  const units: StatementUnit[] = [];
  nodes.forEach((node, index) => {
    if (footer >= 0 && index > footer) return;
    if (!isHiddenLine(node) || !(Number(node.attrs.statement) > 0)) return;
    const lines = String(node.attrs.src).split(/\r?\n/);
    const id = CURRENT_MARKER.exec(lines[lines.length - 1].trim())?.[2];
    if (!id) return;
    let from = index;
    while (
      from > 0 &&
      isHiddenLine(nodes[from - 1]) &&
      !(Number(nodes[from - 1].attrs.statement) > 0)
    )
      from--;
    units.push({
      id,
      identity: { from, to: index },
      metadata: entries.get(id) ?? null,
    });
  });
  return { footer, units };
}

const between = (value: number, low: number, high: number) =>
  value >= low && value <= high;

/**
 * Protected lines outside statements, and each statement's lines with
 * whether it is complete (has its metadata entry), by ID.
 */
function structureOf(doc: PMNode): {
  loose: string;
  units: Map<string, { lines: string; complete: boolean }>;
} {
  const { units } = statementUnits(doc);
  const owned = new Set<number>();
  const byId = new Map<string, { lines: string[]; complete: boolean }>();
  const src = (index: number) => String(doc.child(index).attrs.src);
  for (const unit of units) {
    const lines: string[] = [];
    const ranges = [unit.identity, ...(unit.metadata ? [unit.metadata] : [])];
    for (const range of ranges)
      for (let index = range.from; index <= range.to; index++) {
        owned.add(index);
        lines.push(src(index));
      }
    const entry = byId.get(unit.id) ?? { lines: [], complete: true };
    entry.lines.push(lines.join("\u0000"));
    entry.complete &&= !!unit.metadata;
    byId.set(unit.id, entry);
  }
  const loose: string[] = [];
  doc.forEach((node, _pos, index) => {
    node.descendants((child) => {
      if (isProtected(child)) loose.push(String(child.attrs.src));
      return !child.isTextblock && !child.isAtom;
    });
    if (isProtected(node) && !owned.has(index))
      loose.push(String(node.attrs.src));
  });
  return {
    loose: loose.sort().join("\u0000"),
    units: new Map(
      [...byId].map(([id, entry]) => [
        id,
        { lines: entry.lines.sort().join("\u0001"), complete: entry.complete },
      ]),
    ),
  };
}

/**
 * Whether a change keeps record lines whole: lines outside statements and
 * every statement that remains are unchanged, and a statement appears or
 * disappears only complete, with its metadata entry
 * (`g5.documents.ontology.creation`). Undo and redo of an added or deleted
 * statement pass for the same reason.
 */
export function keepsStructure(before: PMNode, after: PMNode): boolean {
  if (protectedSignature(before) === protectedSignature(after)) return true;
  const was = structureOf(before);
  const is = structureOf(after);
  if (was.loose !== is.loose) return false;
  for (const [id, unit] of was.units) {
    const next = is.units.get(id);
    if (next ? next.lines !== unit.lines : !unit.complete) return false;
  }
  for (const [id, unit] of is.units)
    if (!was.units.has(id) && !unit.complete) return false;
  return true;
}

/**
 * Record identity, anchor and metadata lines change in Source, except that
 * Rich text adds and deletes whole statements: any other edit that would
 * remove or duplicate one is refused (`g5.documents.ontology.creation`).
 */
const ProtectStructure = Extension.create<{ onRefused?: () => void }>({
  name: "g5ProtectStructure",
  addOptions() {
    return { onRefused: undefined };
  },
  addProseMirrorPlugins() {
    const onRefused = this.options.onRefused;
    return [
      new Plugin({
        key: new PluginKey("g5ProtectStructure"),
        filterTransaction(transaction, state) {
          if (!transaction.docChanged || transaction.getMeta(RICH_LOAD_META))
            return true;
          if (keepsStructure(state.doc, transaction.doc)) return true;
          onRefused?.();
          return false;
        },
      }),
    ];
  },
});

/** A statement's body: the top-level nodes after its hidden marker. */
export interface StatementBody {
  /** The position of the hidden marker node. */
  marker: number;
  /** Top-level indices of the first and last body nodes; `first > last` when empty. */
  first: number;
  last: number;
}

/** How an ontology document shows: hidden top-level nodes and statement bodies. */
export interface StatementLayout {
  /** Per top-level index: kept in the document but not displayed. */
  hidden: boolean[];
  /** Per top-level index: the node's position. */
  positions: number[];
  statements: StatementBody[];
}

const layouts = new WeakMap<PMNode, StatementLayout>();

const isProtected = (node: PMNode) =>
  node.type.name === "opaqueBlock" && !!node.attrs.protected;

/**
 * Derived from the document on every change, never stored: a statement runs
 * from its hidden marker to the next record line (hidden or shown for repair)
 * or a heading at or above its identity heading's level, the boundary
 * `currentRecordSpans` uses. A document ending in hidden lines (the Metadata
 * section) shows nothing after them, so the empty paragraph the editor keeps
 * at the end cannot take text into the last metadata entry.
 */
export function statementLayout(doc: PMNode): StatementLayout {
  const cached = layouts.get(doc);
  if (cached) return cached;
  const hidden: boolean[] = [];
  const positions: number[] = [];
  const statements: StatementBody[] = [];
  const owned: boolean[] = [];
  let open: (StatementBody & { level: number }) | null = null;
  const close = () => {
    if (open)
      statements.push({
        marker: open.marker,
        first: open.first,
        last: open.last,
      });
    open = null;
  };
  doc.forEach((node, pos, index) => {
    positions.push(pos);
    const line = isProtected(node);
    hidden.push(line && !!node.attrs.hidden);
    if (
      open &&
      (line || (node.type.name === "heading" && node.attrs.level <= open.level))
    )
      close();
    if (line && node.attrs.hidden && Number(node.attrs.statement) > 0)
      open = {
        marker: pos,
        first: index + 1,
        last: index,
        level: Number(node.attrs.statement),
      };
    else if (open) open.last = index;
    owned.push(!!open && !line);
  });
  close();
  const lastHidden = hidden.lastIndexOf(true);
  if (
    lastHidden >= 0 &&
    hidden
      .slice(lastHidden + 1)
      .every(
        (_, i) =>
          !owned[lastHidden + 1 + i] &&
          doc.child(lastHidden + 1 + i).type.name === "paragraph" &&
          doc.child(lastHidden + 1 + i).childCount === 0,
      )
  )
    hidden.fill(true, lastHidden + 1);
  const layout = { hidden, positions, statements };
  layouts.set(doc, layout);
  return layout;
}

/**
 * The first selection from `pos` in direction `dir` that is not on a hidden
 * node, or null when only hidden nodes lie that way.
 */
export function visibleSelectionFrom(
  doc: PMNode,
  pos: number,
  dir: 1 | -1,
): Selection | null {
  const { hidden, positions } = statementLayout(doc);
  for (;;) {
    const found = Selection.findFrom(doc.resolve(pos), dir);
    if (!found) return null;
    const index = found.$from.index(0);
    if (!hidden[index]) return found;
    pos =
      dir > 0 ? positions[index] + doc.child(index).nodeSize : positions[index];
  }
}

/** Shown when a statement may not be added or deleted in Rich text. */
export const STATEMENT_REPAIR_NOTICE =
  "Statements can be added or deleted here only when the file's record lines are in order: repair them in plain text.";

/** Shown in an empty statement while it is edited. */
export const EMPTY_STATEMENT_PLACEHOLDER =
  "Type the statement, or press Backspace to remove it";

/** A statement whose body is one empty paragraph. */
export function isEmptyStatement(doc: PMNode, statement: StatementBody) {
  if (statement.first !== statement.last) return false;
  const body = doc.child(statement.first);
  return body.type.name === "paragraph" && body.childCount === 0;
}

/**
 * The unit of the statement whose marker is at `marker`, when statements may
 * be added or deleted: the file has a Metadata section, shows no record line
 * or unpaired metadata entry for repair, and the statement has its metadata
 * entry. The Metadata heading itself shows whenever the section holds
 * ordinary prose too; that is not a repair.
 */
function editableUnit(doc: PMNode, marker: number): StatementUnit | null {
  const { footer, units } = statementUnits(doc);
  if (footer < 0) return null;
  let repair = false;
  doc.forEach((node, _pos, index) => {
    if (
      index !== footer &&
      isProtected(node) &&
      node.attrs.kind === "structure" &&
      !node.attrs.hidden
    )
      repair = true;
    // An unpaired entry's `### ID` is an ordinary heading after the footer.
    if (
      index > footer &&
      node.type.name === "heading" &&
      node.attrs.level === 3
    )
      repair = true;
  });
  if (repair) return null;
  const index = doc.resolve(marker).index(0);
  return (
    units.find((unit) => unit.identity.to === index && unit.metadata) ?? null
  );
}

/**
 * Adds an Intent after `statement`, with its anchor and a metadata entry
 * tagged human-provided after the statement's own, and the caret in its
 * empty body; null when statements may not be added here.
 */
export function addStatement(
  state: EditorState,
  statement: StatementBody,
  id: string,
  eol: string,
): Transaction | null {
  const { doc, schema } = state;
  const unit = editableUnit(doc, statement.marker);
  if (!unit?.metadata) return null;
  const { positions } = statementLayout(doc);
  const end = (index: number) => positions[index] + doc.child(index).nodeSize;
  const line = (src: string, level = 0) =>
    schema.nodes.opaqueBlock.create({
      kind: "structure",
      src,
      mdast: "",
      protected: true,
      hidden: true,
      statement: level,
    });
  const tr = state.tr;
  // The footer comes after every statement: inserting there first leaves
  // the statement's positions as they are.
  tr.insert(end(unit.metadata.to), [
    line(`### ${id}`),
    line("- human-provided"),
  ]);
  const at = end(statement.last);
  const marker = line(`<a id="${id}"></a>${eol}[I · ${id}]`, 6);
  tr.insert(at, [marker, schema.nodes.paragraph.create()]);
  tr.setSelection(TextSelection.create(tr.doc, at + marker.nodeSize + 1));
  return tr.scrollIntoView();
}

/**
 * Deletes the empty statement `id` whole: its identity lines, body and
 * metadata entry, with the caret at the end of what came before; null when
 * it is not one empty statement with its metadata entry, or may not be
 * deleted here.
 */
export function deleteStatement(
  state: EditorState,
  id: string,
): Transaction | null {
  const { doc } = state;
  const layout = statementLayout(doc);
  const matching = statementUnits(doc).units.filter((unit) => unit.id === id);
  if (matching.length !== 1) return null;
  const markerPos = layout.positions[matching[0].identity.to];
  const statement = layout.statements.find((s) => s.marker === markerPos);
  if (!statement || !isEmptyStatement(doc, statement)) return null;
  const unit = editableUnit(doc, markerPos);
  if (!unit?.metadata) return null;
  const end = (index: number) =>
    layout.positions[index] + doc.child(index).nodeSize;
  const tr = state.tr;
  tr.delete(layout.positions[unit.metadata.from], end(unit.metadata.to));
  const from = layout.positions[unit.identity.from];
  tr.delete(from, end(statement.last));
  const caret =
    visibleSelectionFrom(tr.doc, from, -1) ??
    visibleSelectionFrom(tr.doc, from, 1);
  if (caret) tr.setSelection(caret);
  return tr.scrollIntoView();
}

/** The ID of the statement whose marker is at `marker`. */
function statementId(doc: PMNode, marker: number): string | null {
  const lines = String(doc.nodeAt(marker)?.attrs.src ?? "").split(/\r?\n/);
  return CURRENT_MARKER.exec(lines[lines.length - 1].trim())?.[2] ?? null;
}

/** The top-level index a selection rests on when that node is hidden. */
function hiddenRest(
  selection: Selection,
  hidden: boolean[],
): { index: number } | null {
  const { $from } = selection;
  if (selection instanceof NodeSelection && $from.depth === 0)
    return hidden[$from.index(0)] ? { index: $from.index(0) } : null;
  if (selection instanceof GapCursor) {
    const after = $from.index(0);
    if (hidden[after]) return { index: after };
    if (after > 0 && hidden[after - 1]) return { index: after - 1 };
    return null;
  }
  const { $head } = selection;
  if ($head.depth > 0 && hidden[$head.index(0)])
    return { index: $head.index(0) };
  return null;
}

const statementsKey = new PluginKey<{ hovered: number | null }>(
  "g5OntologyStatements",
);

const decorationCache = new WeakMap<
  PMNode,
  { key: string; set: DecorationSet }
>();

function statementDecorations(state: EditorState): DecorationSet {
  const { doc, selection } = state;
  const layout = statementLayout(doc);
  if (!layout.hidden.includes(true)) return DecorationSet.empty;
  const at =
    selection instanceof NodeSelection
      ? selection.$from.index(0)
      : selection.$head.index(0);
  const hovered = statementsKey.getState(state)?.hovered ?? null;
  const key = `${at}:${hovered}`;
  const cached = decorationCache.get(doc);
  if (cached?.key === key) return cached.set;
  const decorations: Decoration[] = [];
  layout.hidden.forEach((hidden, index) => {
    if (!hidden) return;
    const from = layout.positions[index];
    decorations.push(
      Decoration.node(from, from + doc.child(index).nodeSize, {
        "data-g5-hidden": "true",
      }),
    );
  });
  for (const statement of layout.statements) {
    const current = at >= statement.first && at <= statement.last;
    for (let index = statement.first; index <= statement.last; index++) {
      const from = layout.positions[index];
      const classes = ["g5-rich-statement"];
      if (index === statement.first) classes.push("g5-rich-statement-first");
      if (index === statement.last) classes.push("g5-rich-statement-last");
      if (current) classes.push("is-current");
      if (hovered === statement.marker) classes.push("is-hovered");
      decorations.push(
        Decoration.node(from, from + doc.child(index).nodeSize, {
          class: classes.join(" "),
          "data-g5-statement": String(statement.marker),
        }),
      );
    }
    // The statement's bullet and outline take both pseudo-elements, so the
    // placeholder is a widget, laid over the empty line.
    if (isEmptyStatement(doc, statement))
      decorations.push(
        Decoration.widget(
          layout.positions[statement.first] + 1,
          () => {
            const hint = document.createElement("span");
            hint.className = "g5-rich-statement-placeholder";
            hint.dataset.testid = "rich-statement-placeholder";
            hint.contentEditable = "false";
            hint.setAttribute("aria-hidden", "true");
            hint.textContent = EMPTY_STATEMENT_PLACEHOLDER;
            return hint;
          },
          { key: "g5-statement-placeholder", side: -1, ignoreSelection: true },
        ),
      );
  }
  const set = DecorationSet.create(doc, decorations);
  decorationCache.set(doc, { key, set });
  return set;
}

/**
 * Ontology Rich text shows prose only (`g5.documents.preview.rich-editing`):
 * hidden record lines take no box, each statement's body is marked so CSS
 * draws Preview's bullet and hover/focus outline, editing keys stop at a
 * statement's boundary, and a selection never rests on a hidden line.
 * Documents without hidden lines are unaffected.
 */
export interface OntologyStatementOptions {
  /** The file's line ending, for a new statement's anchor and marker lines. */
  eol: () => string;
  /** A new statement's ID. */
  newId: () => string;
  /** Adding or deleting was refused; says why. */
  onRefused?: (message: string) => void;
}

const OntologyStatements = Extension.create<OntologyStatementOptions>({
  name: "g5OntologyStatements",
  // Before the list and base keymaps, which would join across a boundary.
  priority: 1000,
  addOptions() {
    return {
      eol: () => "\n",
      newId: () => `user.${randomUuid()}`,
      onRefused: undefined,
    };
  },
  addKeyboardShortcuts() {
    /** The statement holding a collapsed caret in one of its top-level textblocks. */
    const caretStatement = () => {
      const { selection, doc } = this.editor.state;
      if (!(selection instanceof TextSelection) || !selection.empty)
        return null;
      const { $from } = selection;
      if ($from.depth !== 1) return null;
      const index = $from.index(0);
      const statement = statementLayout(doc).statements.find(
        (s) => index >= s.first && index <= s.last,
      );
      return statement ? { statement, index, $from } : null;
    };
    // Backspace in an empty statement deletes it, with no check of what
    // links to it: the deletion is one draft change that Undo and Discard
    // reverse. At the start of a top-level block that follows a hidden line
    // it does nothing, since joining would reach into the line. A list or
    // quote lifts first.
    const backspace = () => {
      const { selection, doc } = this.editor.state;
      if (!(selection instanceof TextSelection) || !selection.empty)
        return false;
      const { $from } = selection;
      if ($from.depth !== 1 || $from.parentOffset !== 0) return false;
      const at = caretStatement();
      if (at && isEmptyStatement(doc, at.statement)) {
        const id = statementId(doc, at.statement.marker);
        const tr = id ? deleteStatement(this.editor.state, id) : null;
        if (tr) this.editor.view.dispatch(tr);
        else this.options.onRefused?.(STATEMENT_REPAIR_NOTICE);
        return true;
      }
      const index = $from.index(0);
      return index > 0 && statementLayout(doc).hidden[index - 1];
    };
    // Enter at the end of a nonempty statement whose last block is a
    // paragraph adds a statement after it; on an empty statement it does
    // nothing. Elsewhere, lists, quotes, code and tables included, Enter
    // keeps its meaning.
    const enter = () => {
      const at = caretStatement();
      if (!at) return false;
      const { state, view } = this.editor;
      if (isEmptyStatement(state.doc, at.statement)) return true;
      const { $from, index, statement } = at;
      if (
        index !== statement.last ||
        $from.parent.type.name !== "paragraph" ||
        $from.parentOffset !== $from.parent.content.size
      )
        return false;
      const tr = addStatement(
        state,
        statement,
        this.options.newId(),
        this.options.eol(),
      );
      if (tr) view.dispatch(tr);
      else this.options.onRefused?.(STATEMENT_REPAIR_NOTICE);
      return true;
    };
    // Shift+Enter in a statement's paragraph starts a new paragraph of the
    // same statement; elsewhere it keeps its line break.
    const shiftEnter = () => {
      const at = caretStatement();
      if (!at || at.$from.parent.type.name !== "paragraph") return false;
      return this.editor.commands.splitBlock();
    };
    // Delete at the end of a top-level block before a hidden line, or at the
    // end of a statement's last body block.
    const forwardDelete = () => {
      const { selection, doc } = this.editor.state;
      if (!(selection instanceof TextSelection) || !selection.empty)
        return false;
      const { $from } = selection;
      if ($from.depth < 1 || $from.parentOffset !== $from.parent.content.size)
        return false;
      for (let depth = $from.depth; depth > 1; depth--)
        if ($from.index(depth - 1) !== $from.node(depth - 1).childCount - 1)
          return false;
      const layout = statementLayout(doc);
      const index = $from.index(0);
      return (
        !!layout.hidden[index + 1] ||
        layout.statements.some((statement) => statement.last === index)
      );
    };
    const keys: Record<string, () => boolean> = {
      Backspace: backspace,
      "Mod-Backspace": backspace,
      "Shift-Backspace": backspace,
      Delete: forwardDelete,
      "Mod-Delete": forwardDelete,
      Enter: enter,
      "Shift-Enter": shiftEnter,
    };
    if (isiOS() || isMacOS())
      Object.assign(keys, {
        "Ctrl-h": backspace,
        "Alt-Backspace": backspace,
        "Ctrl-d": forwardDelete,
        "Ctrl-Alt-Backspace": forwardDelete,
        "Alt-Delete": forwardDelete,
        "Alt-d": forwardDelete,
      });
    return keys;
  },
  addProseMirrorPlugins() {
    return [
      new Plugin<{ hovered: number | null }>({
        key: statementsKey,
        state: {
          init: () => ({ hovered: null }),
          apply(tr, value) {
            const meta = tr.getMeta(statementsKey) as
              | { hovered: number | null }
              | undefined;
            if (meta) return meta;
            if (!tr.docChanged || value.hovered === null) return value;
            const mapped = tr.mapping.mapResult(value.hovered);
            return { hovered: mapped.deleted ? null : mapped.pos };
          },
        },
        // A selection that lands on a hidden line (an arrow key selecting
        // it, a gap cursor beside it) moves on in its direction of travel.
        appendTransaction(transactions, before, state) {
          const layout = statementLayout(state.doc);
          const rest = hiddenRest(state.selection, layout.hidden);
          if (!rest) return null;
          let start = rest.index;
          let end = rest.index;
          while (start > 0 && layout.hidden[start - 1]) start--;
          while (end + 1 < layout.hidden.length && layout.hidden[end + 1])
            end++;
          let previous = before.selection.head;
          for (const tr of transactions) previous = tr.mapping.map(previous);
          const dir = state.selection.head < previous ? -1 : 1;
          const runStart = layout.positions[start];
          const runEnd = layout.positions[end] + state.doc.child(end).nodeSize;
          const target =
            dir > 0
              ? (visibleSelectionFrom(state.doc, runEnd, 1) ??
                visibleSelectionFrom(state.doc, runStart, -1))
              : (visibleSelectionFrom(state.doc, runStart, -1) ??
                visibleSelectionFrom(state.doc, runEnd, 1));
          if (!target) return null;
          const tr = state.tr.setSelection(target);
          return transactions.some((t) => t.scrolledIntoView)
            ? tr.scrollIntoView()
            : tr;
        },
        props: {
          decorations: statementDecorations,
          handleDOMEvents: {
            mouseover(view, event) {
              if (view.composing) return false;
              const target = event.target as Element | null;
              const element = target?.closest?.("[data-g5-statement]");
              const hovered =
                element && view.dom.contains(element)
                  ? Number(element.getAttribute("data-g5-statement"))
                  : null;
              if (statementsKey.getState(view.state)?.hovered !== hovered)
                view.dispatch(
                  view.state.tr.setMeta(statementsKey, { hovered }),
                );
              return false;
            },
            mouseleave(view) {
              if (statementsKey.getState(view.state)?.hovered !== null)
                view.dispatch(
                  view.state.tr.setMeta(statementsKey, { hovered: null }),
                );
              return false;
            },
          },
        },
      }),
    ];
  },
});

const Shortcuts = Extension.create<{
  onSave?: () => void;
  onLink?: () => void;
}>({
  name: "g5RichShortcuts",
  addOptions() {
    return { onSave: undefined, onLink: undefined };
  },
  addKeyboardShortcuts() {
    return {
      "Mod-s": () => {
        this.options.onSave?.();
        return true;
      },
      "Mod-k": () => {
        this.options.onLink?.();
        return true;
      },
    };
  },
});

export interface RichExtensionOptions {
  opaqueBlockView?: NodeViewRenderer;
  opaqueInlineView?: NodeViewRenderer;
  onStructureRefused?: () => void;
  onSave?: () => void;
  onLink?: () => void;
  /** Adding and deleting ontology statements. */
  statements?: Partial<OntologyStatementOptions>;
}

/** The editor's extensions; without views, the bare schema for conversion and tests. */
export function richMarkdownExtensions(
  options: RichExtensionOptions = {},
): AnyExtension[] {
  const { opaqueBlockView, opaqueInlineView } = options;
  return [
    StarterKit.configure({
      // Markdown has no underline; code is configured below.
      underline: false,
      code: false,
      link: {
        openOnClick: false,
        enableClickSelection: true,
        autolink: true,
        linkOnPaste: true,
        defaultProtocol: "https",
      },
    }),
    // Markdown nests code inside links and emphasis (`[`name`](path)`).
    Code.extend({ excludes: "" }),
    TaskList,
    TaskItem.configure({ nested: true }),
    Table.configure({ resizable: false, allowTableNodeSelection: false }),
    TableRow,
    // A GFM cell holds one line of inline content.
    TableHeader.extend({ content: "paragraph" }),
    TableCell.extend({ content: "paragraph" }),
    SoftBreak,
    opaqueBlockView
      ? OpaqueBlock.extend({ addNodeView: () => opaqueBlockView })
      : OpaqueBlock,
    opaqueInlineView
      ? OpaqueInline.extend({ addNodeView: () => opaqueInlineView })
      : OpaqueInline,
    Bookkeeping,
    ProtectStructure.configure({ onRefused: options.onStructureRefused }),
    options.statements
      ? OntologyStatements.configure(options.statements)
      : OntologyStatements,
    Shortcuts.configure({ onSave: options.onSave, onLink: options.onLink }),
  ];
}
