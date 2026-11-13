import type { Element, ElementContent, Root, RootContent } from "hast";
import type { FindingEntry, FindingsFile } from "../../lib/findings";

const HEADING = /^h[1-6]$/;
const EVIDENCE = [
  ["Found by:", "found-by"],
  ["Reproduced by:", "reproduced-by"],
  ["Comparison:", "comparison"],
] as const;
const EVIDENCE_LINE = /\n(?=(?:Found by|Reproduced by|Comparison):)/;

/**
 * Presentation of a findings file (`g5.documents.findings.files`): each entry's
 * blocks go into a `g5-finding-entry` element, so the filters can hide them,
 * and its status line becomes a `g5-finding-status` element, the entry's
 * control row. An entry without a status line gets the row after its heading.
 * Text written directly under the status line stays.
 *
 * Everything else is description, which a filtered document hides: it goes
 * into `g5-findings-context` elements, except the heading that introduces a
 * group of entries, which goes into a `g5-findings-heading` element naming its
 * section's entries. Source offsets only place elements; they never become
 * write offsets.
 *
 * An entry's evidence lines (`Found by:`, `Reproduced by:`, `Comparison:`)
 * become `g5-finding-evidence` elements the reader renders with the recorded
 * runs; an actionable bug without `Reproduced by:` gets an empty one. A
 * comparison moves to the end of its entry.
 */
export function findingsMarkup(
  entries: readonly FindingEntry[],
  file?: FindingsFile,
) {
  const status = (entryIndex: number): Element => ({
    type: "element",
    tagName: "g5-finding-status",
    properties: { entryIndex },
    children: [],
  });
  return function plugin() {
    return (tree: Root) => {
      // Blocks and entries are both in source order: one pass finds each
      // block's entry.
      let cursor = 0;
      const entryAt = (offset: number) => {
        while (cursor < entries.length && entries[cursor].offsets.end <= offset)
          cursor++;
        const entry = entries[cursor];
        return entry && offset >= entry.offsets.start ? cursor : -1;
      };
      const children: RootContent[] = [];
      let group: Element | null = null;
      let groupIndex = -1;
      // Inserted nodes without a source position (review cards) go with the
      // next block that has one.
      let pending: RootContent[] = [];
      for (const node of tree.children) {
        const start = node.position?.start.offset;
        if (start === undefined) {
          pending.push(node);
          continue;
        }
        const index = entryAt(start);
        if (index < 0) {
          group = null;
          children.push(...pending, node);
          pending = [];
          continue;
        }
        if (index !== groupIndex || !group) {
          group = {
            type: "element",
            tagName: "g5-finding-entry",
            properties: { entryIndex: index },
            children: [],
          };
          groupIndex = index;
          children.push(group);
        }
        group.children.push(...(pending as Element["children"]));
        pending = [];
        const entry = entries[index];
        const { status: at, statusEnd } = entry.offsets;
        if (at !== null && statusEnd !== null && start === at) {
          group.children.push(status(index));
          if (node.type === "element" && keepAfter(node, statusEnd))
            group.children.push(...evidenceOf(node, index));
          continue;
        }
        if (node.type === "element")
          group.children.push(...evidenceOf(node, index));
        else if (node.type !== "doctype") group.children.push(node);
        if (at === null && start === entry.offsets.heading)
          group.children.push(status(index));
      }
      children.push(...pending);
      for (const node of children)
        if (node.type === "element" && node.tagName === "g5-finding-entry") {
          if (file === "bugs_issues.md")
            withReproduction(node, entries[Number(node.properties.entryIndex)]);
          comparisonLast(node);
        }
      tree.children = describe(children, entries);
    };
  };
}

/** Wrap the description between entries; keep each run's last heading apart. */
function describe(
  children: RootContent[],
  entries: readonly FindingEntry[],
): RootContent[] {
  const out: RootContent[] = [];
  let run: RootContent[] = [];
  const sections: { heading: Element; entries: number[] }[] = [];
  const wrap = (nodes: RootContent[]) => {
    if (nodes.some((n) => n.type !== "text" || n.value.trim() !== ""))
      out.push({
        type: "element",
        tagName: "g5-findings-context",
        properties: {},
        children: nodes as Element["children"],
      });
    else out.push(...nodes);
  };
  const flush = (beforeEntry: boolean) => {
    let at = -1;
    if (beforeEntry)
      for (let i = run.length - 1; i >= 0 && at < 0; i--) {
        const node = run[i];
        if (node.type === "element" && HEADING.test(node.tagName)) at = i;
      }
    if (at < 0) wrap(run);
    else {
      wrap(run.slice(0, at));
      const heading: Element = {
        type: "element",
        tagName: "g5-findings-heading",
        properties: {},
        children: [run[at] as Element],
      };
      sections.push({ heading, entries: [] });
      out.push(heading);
      wrap(run.slice(at + 1));
    }
    run = [];
  };
  for (const node of children) {
    if (node.type !== "element" || node.tagName !== "g5-finding-entry") {
      run.push(node);
      continue;
    }
    flush(true);
    const index = Number(node.properties.entryIndex);
    if (entries[index]) sections.at(-1)?.entries.push(index);
    out.push(node);
  }
  flush(false);
  for (const section of sections)
    section.heading.properties.entries = section.entries.join(" ");
  return out;
}

/** A paragraph's evidence lines as evidence elements; anything else as it is. */
function evidenceOf(node: Element, entryIndex: number): ElementContent[] {
  if (node.tagName !== "p") return [node];
  return splitLines(node).map(
    (part) => evidenceElement(part, entryIndex) ?? part,
  );
}

/** Evidence lines written without a blank line between them, as paragraphs. */
function splitLines(node: Element): Element[] {
  const parts: Element[] = [];
  let current: ElementContent[] = [];
  const close = () => {
    if (current.length) parts.push({ ...node, children: current });
    current = [];
  };
  for (const child of node.children) {
    if (child.type !== "text" || !EVIDENCE_LINE.test(child.value)) {
      current.push(child);
      continue;
    }
    child.value.split(EVIDENCE_LINE).forEach((piece, i) => {
      if (i > 0) close();
      if (piece) current.push({ ...child, value: piece });
    });
  }
  close();
  return parts.length ? parts : [node];
}

const textOf = (nodes: readonly ElementContent[]) =>
  nodes.map((n) => (n.type === "text" ? n.value : "")).join("");

function evidenceElement(node: Element, entryIndex: number): Element | null {
  const first = node.children[0];
  if (!first || first.type !== "text") return null;
  const leading = first.value.replace(/^\s+/, "");
  const found = EVIDENCE.find(([label]) => leading.startsWith(label));
  if (!found) return null;
  const [label, kind] = found;
  const rest = [
    { ...first, value: leading.slice(label.length).replace(/^\s+/, "") },
    ...node.children.slice(1),
  ].filter((c) => c.type !== "text" || c.value !== "");
  const link = (n: ElementContent): n is Element =>
    n.type === "element" && n.tagName === "a";
  const describeLink = (a: Element) => ({
    href: String(a.properties.href ?? ""),
    label: textOf(a.children),
  });
  if (kind !== "comparison")
    return {
      type: "element",
      tagName: "g5-finding-evidence",
      properties: {
        kind,
        entryIndex,
        links: JSON.stringify(rest.filter(link).map(describeLink)),
      },
      children: rest,
    };
  // `[As-built](…) → [Proposed simplification](…). [Scope qualification:] Note`,
  // with legacy `, not like for like` also understood as a qualification,
  // `[As-built](…) → none. Note` or `none. Note`.
  const beforeAt = rest.findIndex(link);
  let before: Element | null = null;
  let after: Element | null = null;
  let note = rest;
  if (beforeAt >= 0) {
    before = rest[beforeAt] as Element;
    const second = rest.findIndex((n, i) => i > beforeAt && link(n));
    const between = textOf(
      rest.slice(beforeAt + 1, second < 0 ? undefined : second),
    );
    if (second >= 0 && /→|->/.test(between) && !/\bnone\b/i.test(between)) {
      after = rest[second] as Element;
      note = rest.slice(second + 1);
    } else
      note = trimLeading(rest.slice(beforeAt + 1), /^\s*(?:→|->)?\s*none\b/i);
  } else note = trimLeading(rest, /^\s*none\b/i);
  const likeForLike =
    !/^\s*[.,]?\s*(?:not like for like|scope qualification:)/i.test(
      textOf(note.slice(0, 1)),
    );
  note = trimLeading(note, /^\s*,?\s*(?:not like for like)?\s*[.,;:]?\s*/i);
  return {
    type: "element",
    tagName: "g5-finding-evidence",
    properties: {
      kind,
      entryIndex,
      before: before ? JSON.stringify(describeLink(before)) : "",
      after: after ? JSON.stringify(describeLink(after)) : "",
      likeForLike: String(likeForLike),
    },
    children: note,
  };
}

/** Drop a leading match from the first text node. */
function trimLeading(
  nodes: ElementContent[],
  pattern: RegExp,
): ElementContent[] {
  const [first, ...others] = nodes;
  if (!first || first.type !== "text") return nodes;
  const value = first.value.replace(pattern, "");
  return value ? [{ ...first, value }, ...others] : others;
}

/** A simplification's comparison closes its entry, after the text it supports. */
function comparisonLast(group: Element) {
  const comparison = (n: ElementContent) =>
    n.type === "element" &&
    n.tagName === "g5-finding-evidence" &&
    n.properties.kind === "comparison";
  if (group.children.some(comparison))
    group.children = [
      ...group.children.filter((n) => !comparison(n)),
      ...group.children.filter(comparison),
    ];
}

/** An actionable bug shows its Reproduced by even when nothing reproduced it. */
function withReproduction(group: Element, entry: FindingEntry | undefined) {
  if (!entry || entry.decision === null) return;
  const evidence = (kind: string) =>
    group.children.findIndex(
      (n) =>
        n.type === "element" &&
        n.tagName === "g5-finding-evidence" &&
        n.properties.kind === kind,
    );
  if (evidence("reproduced-by") >= 0) return;
  const status = group.children.findIndex(
    (n) => n.type === "element" && n.tagName === "g5-finding-status",
  );
  // The `Run:` line stays directly under the status line.
  const next = group.children.findIndex(
    (n, i) => i > status && n.type === "element",
  );
  const run =
    entry.run !== null &&
    next >= 0 &&
    textOf((group.children[next] as Element).children)
      .trimStart()
      .startsWith("Run:")
      ? next
      : -1;
  const after = Math.max(evidence("found-by"), status, run);
  group.children.splice(after + 1, 0, {
    type: "element",
    tagName: "g5-finding-evidence",
    properties: {
      kind: "reproduced-by",
      entryIndex: Number(group.properties.entryIndex),
      links: "[]",
    },
    children: [],
  });
}

/** Drop a paragraph's content before `end`; false when nothing remains. */
function keepAfter(node: Element, end: number): boolean {
  const kept: Element["children"] = [];
  for (const child of node.children) {
    const from = child.position?.start.offset;
    const to = child.position?.end.offset;
    if (to !== undefined && to <= end) continue;
    if (child.type === "text" && from !== undefined && from < end) {
      const cut = child.value.indexOf("\n", Math.max(0, end - from - 1));
      if (cut < 0) continue;
      kept.push({ ...child, value: child.value.slice(cut + 1) });
      continue;
    }
    kept.push(child);
  }
  node.children = kept;
  return kept.some(
    (child) => child.type !== "text" || child.value.trim() !== "",
  );
}
