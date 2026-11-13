import type { Element, Root, RootContent } from "hast";
import {
  ID_BLOCK_LABEL_CANDIDATE_RE,
  parseMarkdownToBlocks,
  type RecordSpan,
} from "../../lib/utils";
import {
  disclosedCurrentMetadataIndices,
  statementParents,
  staysWithLead,
} from "../../lib/currentOntology";

/**
 * Source coordinates decorate a sanitized AST. They never become write offsets.
 *
 * In an ontology file each statement is a `g5-record` holding its lead, then a
 * `g5-details` element with the rest of its own content and a `g5-kids`
 * element with its child statements' records (`g5.documents.ontology.disclosure`).
 */
export function documentMarkup(source: string, ontology = false) {
  const blocks = parseMarkdownToBlocks(source, ontology);
  const hidden = disclosedCurrentMetadataIndices(blocks, blocks.recordSpans);
  const offset = (i: number) => blocks[i]?.sourceRange?.start ?? source.length;
  const records = blocks.recordSpans;
  const metadata = (r: RecordSpan) =>
    r.current && r.current.metadataStart >= 0
      ? source.slice(
          offset(r.current.metadataStart + 1),
          offset(r.current.metadataEnd),
        )
      : "";
  for (const r of records)
    if (r.current) {
      hidden.add(r.current.anchorIndex);
      hidden.add(r.current.headingIndex);
      for (const alias of r.current.aliasIndices) hidden.add(alias);
    }
  const blockAtOffset = new Map(
    blocks.map((b, i) => [b.sourceRange?.start, i]),
  );
  // Identity lines (aliases, anchor, marker) end at their marker line. Text
  // written directly under the marker shares a rendered paragraph with them;
  // only the identity part is dropped and the rest is the statement's body.
  const identityOf = new Map<number, { record: RecordSpan; end: number }>();
  for (const r of records) {
    const marker = blocks[r.spanStart]?.sourceRange;
    if (!r.current || !marker) continue;
    const end = marker.start + marker.length;
    for (const index of [
      ...r.current.aliasIndices,
      r.current.anchorIndex,
      r.spanStart,
    ])
      if (index >= 0) identityOf.set(index, { record: r, end });
  }
  const lineStarts = [0];
  for (let i = source.indexOf("\n"); i >= 0; i = source.indexOf("\n", i + 1))
    lineStarts.push(i + 1);
  const lineAt = (at: number) => {
    let low = 0;
    let high = lineStarts.length;
    while (low < high) {
      const mid = (low + high) >> 1;
      if (lineStarts[mid] <= at) low = mid + 1;
      else high = mid;
    }
    return low;
  };
  const nests = ontology && records.some((r) => r.current);
  // Each record's parent record index, or -1.
  const parents = nests
    ? statementParents(blocks, records)
    : records.map(() => -1);
  function plugin() {
    return (tree: Root) => {
      const groups = new Map<RecordSpan, Element>();
      const children: RootContent[] = [];
      const groupOf = (record: RecordSpan) => {
        let group = groups.get(record);
        if (!group) {
          group = {
            type: "element",
            tagName: "g5-record",
            properties: {
              recordIndex: records.indexOf(record),
              "data-block-index": record.spanStart,
            },
            children: [],
          };
          groups.set(record, group);
          const parent = parents[records.indexOf(record)];
          const parentGroup = parent >= 0 ? groups.get(records[parent]) : null;
          if (parentGroup) kids(parentGroup).push(group);
          else children.push(group);
        }
        return group;
      };
      for (const node of tree.children) {
        let start = node.position?.start.offset;
        if (start === undefined) {
          children.push(node);
          continue;
        }
        let index = blockAtOffset.get(start);
        const identity =
          index === undefined ? undefined : identityOf.get(index);
        if (identity) {
          // A statement keeps its frame even when its identity lines are all
          // there is.
          groupOf(identity.record);
          if (node.type !== "element" || !trimIdentity(node, identity.end))
            continue;
          // What remains is the first body block.
          index = identity.record.spanStart + 1;
          start = offset(index);
        } else if (index !== undefined && hidden.has(index)) continue;
        const at = start;
        const record = records.find(
          (r) => at >= offset(r.spanStart) && at < offset(r.spanEnd),
        );
        if (node.type === "element") {
          node.properties["data-source-line"] = identity
            ? lineAt(at)
            : node.position?.start.line;
          if (index !== undefined) node.properties["data-block-index"] = index;
        }
        if (!record) {
          children.push(node);
          continue;
        }
        const group = groupOf(record);
        if (index === record.spanStart) {
          const block = blocks[index];
          if (record.current) continue;
          // A record span already validated this first label. Reuse its
          // matcher so nested references in a title have the same boundary.
          const labelLine = source.slice(start).split("\n", 1)[0];
          if (ID_BLOCK_LABEL_CANDIDATE_RE.test(labelLine.trim())) {
            if (
              block.type === "paragraph" &&
              block.content.trim() === labelLine.trim()
            )
              continue;
            if (node.type !== "element") continue;
            const afterLabel = start + labelLine.length + 1;
            node.children = node.children.filter(
              (child) =>
                (child.position?.end.offset ?? afterLabel) > afterLabel,
            );
            const first = node.children[0];
            if (
              first?.type === "text" &&
              (first.position?.start.offset ?? afterLabel) < afterLabel
            )
              first.value = first.value.slice(first.value.indexOf("\n") + 1);
          }
        }
        if (node.type !== "doctype") group.children.push(node);
      }
      if (nests)
        for (const [record, group] of groups) splitStatement(record, group);
      tree.children = children;
    };
  }
  /** Drop a paragraph's content before `end`; false when nothing remains. */
  function trimIdentity(node: Element, end: number): boolean {
    const kept: Element["children"] = [];
    for (const child of node.children) {
      const from = child.position?.start.offset;
      const to = child.position?.end.offset;
      if (to !== undefined && to <= end) continue;
      if (child.type === "text" && from !== undefined && from < end) {
        // The marker line ends the identity; the text after its break stays.
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
  // Child records collected per parent until the parent's content is split.
  const kidsOf = new Map<Element, Element[]>();
  const kids = (group: Element) => {
    let list = kidsOf.get(group);
    if (!list) kidsOf.set(group, (list = []));
    return list;
  };
  /** Lead nodes stay in the record; the rest moves to `g5-details`. */
  function splitStatement(record: RecordSpan, group: Element) {
    const lead: Element["children"] = [];
    const details: Element["children"] = [];
    let bucket = lead;
    for (const node of group.children) {
      if (node.type === "element") {
        const index = Number(node.properties["data-block-index"]);
        bucket =
          Number.isInteger(index) && !staysWithLead(blocks, record, index)
            ? details
            : lead;
      }
      bucket.push(node);
    }
    const recordIndex = records.indexOf(record);
    group.children = [
      ...lead,
      ...(details.some((node) => node.type === "element")
        ? [element("g5-details", recordIndex, details)]
        : []),
      ...(kidsOf.get(group)?.length
        ? [element("g5-kids", recordIndex, kidsOf.get(group)!)]
        : []),
    ];
    kidsOf.delete(group);
  }
  const element = (
    tagName: string,
    recordIndex: number,
    children: Element["children"],
  ): Element => ({
    type: "element",
    tagName,
    properties: { recordIndex },
    children,
  });
  return { blocks, records, hidden, metadata, plugin, parents };
}
