import type { Block, BlockTag, RecordSpan } from "./utils";

export const CURRENT_ONTOLOGY_ROOT = ".g5/ontology/";
/** Ontology files are `*.g5.md` at any depth under `.g5/ontology/`; other
 * files there are ordinary files (g5.documents.ontology.current-format). Keep
 * in step with `g5_contracts::g5_paths::is_ontology_path`. */
export const isCurrentOntologyPath = (path: string) => {
  if (!path.startsWith(CURRENT_ONTOLOGY_ROOT) || /[\\\0]/.test(path))
    return false;
  const parts = path.slice(CURRENT_ONTOLOGY_ROOT.length).split("/");
  const name = parts[parts.length - 1];
  return (
    parts.every((part) => !["", ".", "..", ".git", ".g5"].includes(part)) &&
    name.length > ".g5.md".length &&
    name.endsWith(".g5.md")
  );
};
export const CURRENT_MARKER = /^\[([IDQ]) · ([A-Za-z0-9][A-Za-z0-9._-]*)\]$/;
export const CURRENT_ANCHOR = /^<a\s+(?:id|name)=["']([^"']+)["']\s*><\/a>$/;
export interface CurrentRecord {
  anchorIndex: number;
  /** Alias anchor blocks directly above the statement's own anchor, in order. */
  aliasIndices: number[];
  /** Former IDs named by those alias anchors. */
  aliases: string[];
  sourceStart: number;
  headingIndex: number;
  metadataStart: number;
  metadataEnd: number;
  nested: boolean;
  diagnostic?: string;
}

/** Consume parsed Markdown blocks, so fenced examples are never declarations. */
export function currentRecordSpans(blocks: Block[]): RecordSpan[] | null {
  const footer = blocks.findIndex(
    (b) => b.type === "heading" && b.level === 2 && b.content === "Metadata",
  );
  const limit = footer < 0 ? blocks.length : footer;
  const footerEnd =
    footer < 0
      ? blocks.length
      : blocks.findIndex(
          (b, i) => i > footer && b.type === "heading" && b.level <= 2,
        );
  const metadataLimit = footerEnd < 0 ? blocks.length : footerEnd;
  const candidates: {
    index: number;
    heading: number;
    anchor: number;
    /** Consecutive anchor-only blocks above `anchor`: the statement's aliases. */
    aliases: number[];
    kind: "I" | "D" | "Q";
    id: string;
  }[] = [];
  const anchorAt = (i: number) =>
    i >= 0 &&
    blocks[i].type === "paragraph" &&
    CURRENT_ANCHOR.test((blocks[i] as { content: string }).content.trim());
  for (let i = 0; i < limit; i++) {
    const b = blocks[i];
    if (b.type !== "paragraph") continue;
    const match = b.content.trim().match(CURRENT_MARKER);
    if (!match) continue;
    const anchor = anchorAt(i - 1) ? i - 1 : -1;
    const aliases: number[] = [];
    for (let a = anchor - 1; anchor >= 0 && anchorAt(a); a--)
      aliases.unshift(a);
    candidates.push({
      index: i,
      anchor,
      aliases,
      heading: i > 0 && blocks[i - 1].type === "heading" ? i - 1 : -1,
      kind: match[1] as "I" | "D" | "Q",
      id: match[2],
    });
  }
  const anchorId = (i: number) =>
    CURRENT_ANCHOR.exec((blocks[i] as { content: string }).content.trim())?.[1];
  if (!candidates.length) return null;
  // Reuse indexes across records. Comparison can parse thousands of statements;
  // rescanning every anchor/footer for each record made valid capped inputs stall.
  const counts = new Map<string, number>();
  for (const c of candidates) counts.set(c.id, (counts.get(c.id) ?? 0) + 1);
  const anchorCounts = new Map<string, number>();
  for (const a of ontologyAnchors(blocks))
    anchorCounts.set(a.id, (anchorCounts.get(a.id) ?? 0) + 1);
  const metadataById = new Map<string, number[]>();
  const metadataEnds = new Map<number, number>();
  let nextMetadataHeading = metadataLimit;
  for (let i = metadataLimit - 1; footer >= 0 && i > footer; i--) {
    const b = blocks[i];
    if (b.type !== "heading" || b.level > 3) continue;
    metadataEnds.set(i, nextMetadataHeading);
    nextMetadataHeading = i;
    if (b.level === 3) {
      const entries = metadataById.get(b.content) ?? [];
      entries.push(i);
      metadataById.set(b.content, entries);
    }
  }
  const sectionEnds = new Map<number, number[]>();
  const candidateIndices = new Set(candidates.map((c) => c.index));
  const nextHeading = Array<number>(7).fill(limit);
  for (let i = limit - 1; i >= 0; i--) {
    if (candidateIndices.has(i)) sectionEnds.set(i, [...nextHeading]);
    const b = blocks[i];
    if (b.type === "heading")
      for (let level = b.level; level <= 6; level++) nextHeading[level] = i;
  }
  return candidates.map((c, candidateIndex) => {
    const h = blocks[c.heading];
    const level = h?.type === "heading" ? h.level : 6;
    const sectionEnd = sectionEnds.get(c.index)![level];
    const next = candidates[candidateIndex + 1];
    const end = Math.min(
      sectionEnd,
      next
        ? next.aliases.length
          ? next.aliases[0]
          : next.anchor >= 0
            ? next.anchor
            : next.heading >= 0
              ? next.heading
              : next.index
        : limit,
    );
    const metadata = metadataById.get(c.id) ?? [];
    const metadataStart = metadata.length === 1 ? metadata[0] : -1;
    const metadataEnd = metadataEnds.get(metadataStart) ?? metadataLimit;
    const aliases = c.aliases.map((a) => anchorId(a)!);
    const diagnostic =
      c.anchor >= 0 &&
      (anchorId(c.anchor) !== c.id ||
        anchorCounts.get(c.id) !== 1 ||
        aliases.some(
          (alias) => anchorCounts.get(alias) !== 1 || counts.has(alias),
        ))
        ? "Mismatched or ambiguous statement anchor"
        : c.heading < 0 && c.anchor < 0
          ? "Statement is missing its authored anchor or heading"
          : counts.get(c.id) !== 1
            ? "Duplicate statement ID"
            : metadata.length !== 1
              ? "Missing or ambiguous footer metadata"
              : metadataLimit < blocks.length
                ? "Metadata must be the final section"
                : undefined;
    // Only whole metadata bullets made of known flags establish provenance or
    // uncertainty. Do not infer flags from source descriptions or code examples.
    const flags = new Set(
      metadataStart < 0
        ? []
        : blocks.slice(metadataStart + 1, metadataEnd).flatMap((block) =>
            block.type !== "bullet-list"
              ? []
              : block.items.flatMap((item) => {
                  const tokens = item
                    .split(/[·;,]/)
                    .map((token) => token.trim());
                  return tokens.every((token) =>
                    /^(human-provided|human-verified|agent|uncertain)$/.test(
                      token,
                    ),
                  )
                    ? tokens
                    : [];
                }),
          ),
    );
    const metadataTags: BlockTag[] = [];
    if (flags.has("human-provided") || flags.has("human-verified"))
      metadataTags.push({ kind: "human", syntax: "inline" });
    if (flags.has("uncertain"))
      metadataTags.push({ kind: "uncertainty", syntax: "inline" });
    return {
      id: c.id,
      kind: c.kind,
      title: "",
      spanStart: c.index,
      spanEnd: Math.max(c.index + 1, end),
      metadataTags,
      current: {
        headingIndex: c.heading,
        anchorIndex: c.anchor,
        aliasIndices: c.aliases,
        aliases,
        sourceStart: c.aliases.length
          ? c.aliases[0]
          : c.anchor >= 0
            ? c.anchor
            : c.heading >= 0
              ? c.heading
              : c.index,
        metadataStart,
        metadataEnd,
        nested:
          c.anchor < 0 &&
          c.heading >= 0 &&
          !!next &&
          next.anchor < 0 &&
          next.index < sectionEnd,
        diagnostic,
      },
    };
  });
}

/** Metadata shown in statement details need not repeat in the document body. */
export function disclosedCurrentMetadataIndices(
  blocks: Block[],
  records: Iterable<RecordSpan>,
): Set<number> {
  const hidden = new Set<number>();
  for (const record of records) {
    const current = record.current;
    if (!current || current.diagnostic || current.metadataStart < 0) continue;
    for (let i = current.metadataStart; i < current.metadataEnd; i++)
      hidden.add(i);
  }
  const footer = blocks.findIndex(
    (b) => b.type === "heading" && b.level === 2 && b.content === "Metadata",
  );
  // Keep ordinary or partially disclosed sections visible, including orphaned
  // entries and diagnostic content that has no statement-details destination.
  if (
    hidden.size > 0 &&
    footer >= 0 &&
    blocks.slice(footer + 1).every((_, i) => hidden.has(footer + 1 + i))
  ) {
    hidden.add(footer);
  }
  return hidden;
}

export function resolveOntologyLink(
  sourcePath: string,
  href: string,
): { path: string; fragment: string } | null {
  if (/^[a-z][a-z0-9+.-]*:/i.test(href) || href.startsWith("//")) return null;
  const hash = href.indexOf("#");
  const rawPath = hash < 0 ? href : href.slice(0, hash);
  const fragment = decodeURIComponent(hash < 0 ? "" : href.slice(hash + 1));
  if (rawPath.includes("?"))
    throw new Error("Repository links with query parameters are unsupported");
  if (!rawPath) return { path: sourcePath, fragment };
  const decoded = decodeURIComponent(rawPath);
  if (decoded.includes("\\") || decoded.includes("\0"))
    throw new Error("Invalid repository link");
  const segments = decoded.startsWith("/")
    ? []
    : sourcePath.split("/").slice(0, -1);
  for (const part of decoded.split("/")) {
    if (!part || part === ".") continue;
    if (part === "..") {
      if (!segments.length) throw new Error("Link escapes the repository");
      segments.pop();
    } else segments.push(part);
  }
  return { path: segments.join("/"), fragment };
}

export function createMarkdownHeadingSlug(content: string): string {
  return content
    .toLowerCase()
    .normalize("NFKD")
    .replace(/[\u0300-\u036f]/g, "")
    .replace(/&/g, " and ")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

/** Keep the complete authored heading order, including hidden statement headings. */
export function ontologyHeadingIds(blocks: Block[]): Map<number, string> {
  const ids = new Map<number, string>();
  const counts = new Map<string, number>();
  blocks.forEach((b, i) => {
    if (b.type !== "heading") return;
    const base = createMarkdownHeadingSlug(b.content) || `section-${i}`;
    const n = counts.get(base) ?? 0;
    counts.set(base, n + 1);
    ids.set(i, n ? `${base}-${n + 1}` : base);
  });
  return ids;
}

/** Authored HTML anchors outside fences/inline code, plus generated headings.
 * An empty anchor immediately before a heading names that heading's block. */
export function ontologyAnchors(
  blocks: Block[],
): { id: string; index: number; explicit: boolean }[] {
  const footer = blocks.findIndex(
    (b) => b.type === "heading" && b.level === 2 && b.content === "Metadata",
  );
  // Footer entry headings are disclosure data, not duplicate navigation targets
  // for simple stable IDs such as "first".
  const anchors = [...ontologyHeadingIds(blocks)]
    .filter(([index]) => footer < 0 || index <= footer)
    .map(([index, id]) => ({
      id,
      index,
      explicit: false,
    }));
  blocks.forEach((block, index) => {
    if (block.type === "code" || !("content" in block)) return;
    const content = block.content.replace(/(`+)[\s\S]*?\1/g, "");
    for (const match of content.matchAll(
      /<a\b[^>]*?\s(?:id|name)\s*=\s*["']([^"']+)["'][^>]*>/gi,
    )) {
      const anchorOnly = /^\s*<a\s+[^>]+>\s*<\/a>\s*$/i.test(content);
      const paragraphMatches = (i: number, pattern: RegExp) =>
        blocks[i]?.type === "paragraph" &&
        pattern.test((blocks[i] as { content: string }).content.trim());
      let target =
        anchorOnly &&
        (blocks[index + 1]?.type === "heading" ||
          paragraphMatches(index + 1, CURRENT_MARKER))
          ? index + 1
          : index;
      // An alias anchor stacks above a statement's own anchor, by the record
      // parser's strict rule, and names the same statement.
      if (target === index && CURRENT_ANCHOR.test(content.trim())) {
        let next = index + 1;
        while (paragraphMatches(next, CURRENT_ANCHOR)) next++;
        if (next > index + 1 && paragraphMatches(next, CURRENT_MARKER))
          target = next;
      }
      anchors.push({ id: match[1], index: target, explicit: true });
    }
  });
  return anchors;
}

export function ontologyFragmentIndex(
  blocks: Block[],
  fragment: string,
): number {
  const matches = ontologyAnchors(blocks).filter((a) => a.id === fragment);
  return matches.length === 1 ? matches[0].index : -1;
}

export function currentOntologyDiagnostics(blocks: Block[]): string[] {
  const records = currentRecordSpans(blocks) ?? [];
  const recordCounts = new Map<string, number>();
  for (const r of records)
    recordCounts.set(r.id, (recordCounts.get(r.id) ?? 0) + 1);
  const footer = blocks.findIndex(
    (b) => b.type === "heading" && b.level === 2 && b.content === "Metadata",
  );
  const diagnostics: string[] = [];
  blocks.forEach((b, i) => {
    if (
      b.type === "paragraph" &&
      /^\[[IDQ]\s*·/.test(b.content.trim()) &&
      !CURRENT_MARKER.test(b.content.trim())
    )
      diagnostics.push(
        "Malformed statement marker; repair in Files or with an agent.",
      );
    if (
      footer >= 0 &&
      i > footer &&
      b.type === "heading" &&
      b.level === 3 &&
      recordCounts.get(b.content) !== 1
    )
      diagnostics.push(`Unpaired footer metadata: ${b.content}`);
  });
  return diagnostics;
}

/** Verification tags (`.agents/guidance/ontology-format.md`), at most one per
 * statement (`g5.documents.ontology.verification.status`). `future` sets none. */
export const VERIFICATION_TAGS = [
  "test-coverage",
  "manual-check-review",
  "design-verified",
  "implementation-verified",
] as const;
export type VerificationTag = (typeof VERIFICATION_TAGS)[number];

/**
 * Each record's parent record index, or -1 (`g5.documents.ontology.disclosure`):
 * a statement is the child of the nearest earlier statement whose ID it extends
 * with `.`, when only that statement's descendants and no section heading come
 * between them.
 */
export function statementParents(
  blocks: Block[],
  records: readonly RecordSpan[],
): number[] {
  const parents = records.map(() => -1);
  const chain: number[] = [];
  records.forEach((record, i) => {
    const start = record.current?.sourceStart ?? record.spanStart;
    for (let b = i > 0 ? records[i - 1].spanEnd : start; b < start; b++)
      if (blocks[b]?.type === "heading" && b !== record.current?.headingIndex) {
        chain.length = 0;
        break;
      }
    while (
      chain.length &&
      !record.id.startsWith(`${records[chain[chain.length - 1]].id}.`)
    )
      chain.pop();
    if (chain.length) parents[i] = chain[chain.length - 1];
    chain.push(i);
  });
  return parents;
}

const isQuestionFence = (block: Block | undefined) =>
  block?.type === "code" && /^g5-question/.test(block.language);

/**
 * The statement's lead: its first content block after the marker, skipping
 * question cards, which stay with the lead anyway. -1 when it has no body.
 */
export function statementLeadBlock(
  blocks: Block[],
  record: RecordSpan,
): number {
  for (let i = record.spanStart + 1; i < record.spanEnd; i++)
    if (!isQuestionFence(blocks[i])) return i;
  return -1;
}

/** Blocks that stay with the lead when a fold is closed. */
export function staysWithLead(
  blocks: Block[],
  record: RecordSpan,
  index: number,
): boolean {
  return (
    index <= record.spanStart ||
    index === statementLeadBlock(blocks, record) ||
    isQuestionFence(blocks[index])
  );
}

/**
 * Verification tags written on bullets of their own in the statement's footer
 * entry. Like provenance, only whole bullets count, with no period or note.
 */
export function statementVerificationTags(
  blocks: Block[],
  record: RecordSpan,
): VerificationTag[] {
  const current = record.current;
  if (!current || current.metadataStart < 0) return [];
  const tags = new Set<VerificationTag>();
  for (const block of blocks.slice(
    current.metadataStart + 1,
    current.metadataEnd,
  )) {
    if (block.type !== "bullet-list") continue;
    for (const item of block.items) {
      const token = item.trim();
      if ((VERIFICATION_TAGS as readonly string[]).includes(token))
        tags.add(token as VerificationTag);
    }
  }
  return [...tags];
}

/**
 * A statement's metadata details, as the verification details show them for
 * tags other than design-verified: the footer entry without its bare tag
 * bullets and its `Expands:`/`Children:` structure lines.
 */
export function metadataDetails(metadata: string): string {
  return metadata
    .split("\n")
    .filter(
      (line) =>
        !/^\s*[-*+]\s+`?[a-z][a-z-]*`?(?:\s*[,;·]\s*`?[a-z][a-z-]*`?)*\.?\s*$/.test(
          line,
        ) && !/^\s*(?:[-*+]\s+)?(?:Expands|Children):/.test(line),
    )
    .join("\n")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
}

/** A state machine linked from a statement's metadata. */
export interface StateMachineLink {
  /** The concern folder's name under `models/`. */
  concern: string;
  /**
   * Candidate verification roots (folders holding `models/`), in the
   * [link resolution](#g5.documents.ontology.links) order: as authored, then
   * from the repository root, then from the `.g5` folder.
   */
  roots: string[];
}

const MODELS_PATH = /^(.*?)(?:^|\/)TLA_verification\/models\/([^/]+)(?:\/|$)/;

/**
 * Link destinations in Markdown text, including ones with spaces; links in
 * code examples do not count.
 */
export function markdownLinkDestinations(text: string): string[] {
  const destinations: string[] = [];
  const prose = text
    .replace(
      /^ {0,3}(`{3,}|~{3,})[^\n]*\n[\s\S]*?(?:^ {0,3}\1[^\n]*$|(?![\s\S]))/gm,
      "",
    )
    .replace(/(`+)[\s\S]*?\1/g, "");
  for (const match of prose.matchAll(/\]\((?:<([^>\n]*)>|([^)\n]*))\)/g)) {
    const raw = (match[1] ?? match[2] ?? "").trim().replace(/\s+"[^"]*"$/, "");
    if (raw) destinations.push(raw);
  }
  return destinations;
}

/**
 * Repository paths a relative link may mean, in resolution order: as authored,
 * then without its leading `./`, `../` and `/` from the repository root, then
 * the same from the `.g5` folder. Throws the authored link's own error for a
 * query string or an invalid path.
 */
export function linkPathCandidates(sourcePath: string, href: string): string[] {
  const candidates: string[] = [];
  const add = (path: string | null) => {
    if (path && !candidates.includes(path)) candidates.push(path);
  };
  try {
    add(resolveOntologyLink(sourcePath, href)?.path ?? null);
  } catch (error) {
    // A link that climbs above the repository root still has fallbacks.
    if (!String(error).includes("escapes the repository")) throw error;
  }
  const hash = href.indexOf("#");
  let raw = hash < 0 ? href : href.slice(0, hash);
  try {
    raw = decodeURIComponent(raw);
  } catch {
    // keep the raw path
  }
  if (/^[a-z][a-z0-9+.-]*:/i.test(href) || href.startsWith("//"))
    return candidates;
  const parts = raw.split("/");
  while (parts.length && ["", ".", ".."].includes(parts[0])) parts.shift();
  // Later `.` and `..` segments resolve; one that climbs out drops the fallback.
  const resolved: string[] = [];
  for (const part of parts) {
    if (!part || part === ".") continue;
    if (part !== "..") resolved.push(part);
    else if (!resolved.pop()) return candidates;
  }
  const stripped = resolved.join("/");
  if (!stripped) return candidates;
  add(stripped);
  const g5 = sourcePath.match(/^(.*?(?:^|\/)\.g5)\//)?.[1];
  if (g5) add(`${g5}/${stripped}`);
  return candidates;
}

/** State machines linked from a statement's metadata, in first-linked order. */
export function stateMachineLinks(
  metadata: string,
  sourcePath: string,
): StateMachineLink[] {
  const links = new Map<string, StateMachineLink>();
  for (const href of markdownLinkDestinations(metadata)) {
    if (/^[a-z][a-z0-9+.-]*:/i.test(href) || href.startsWith("#")) continue;
    let candidates: string[];
    try {
      candidates = linkPathCandidates(sourcePath, href);
    } catch {
      continue;
    }
    for (const path of candidates) {
      const match = path.match(MODELS_PATH);
      if (!match) continue;
      const root = `${match[1] ? `${match[1]}/` : ""}TLA_verification`;
      const link = links.get(match[2]) ?? { concern: match[2], roots: [] };
      if (!link.roots.includes(root)) link.roots.push(root);
      links.set(match[2], link);
    }
  }
  return [...links.values()];
}
