// =============================================================================
// Findings: the bugs/issues and design simplifications a verification run left
// =============================================================================
// `g5.documents.findings.files`: the viewer's reader of
// `TLA_verification/bugs_issues.md` and `TLA_verification/design_simplification.md`.
// Each entry is an anchor line, a heading and a status line
// (`Severity: High · Decision: Open`), optionally followed by a `Run:` line
// naming the run that first recorded it. It mirrors the agents' format checker,
// `.agents/guidance/scripts/check_findings.py`; both are held to the fixtures
// in `tests/fixtures/findings/`. The decision written here is the only record
// of the owner's choice (`g5.documents.findings.decision`).
// =============================================================================

import { linkPathCandidates, resolveOntologyLink } from "./currentOntology";

export const FINDINGS_FILES = [
  "bugs_issues.md",
  "design_simplification.md",
] as const;
export type FindingsFile = (typeof FINDINGS_FILES)[number];

/** Where G5 looks for the files: beside the ontology, under `.g5`. */
export const FINDINGS_PATHS: readonly string[] = FINDINGS_FILES.map(
  (name) => `.g5/TLA_verification/${name}`,
);

export const FINDINGS_LABEL: Record<FindingsFile, string> = {
  "bugs_issues.md": "Bugs and issues",
  "design_simplification.md": "Design simplifications",
};

export const SEVERITIES = [
  "Critical",
  "High",
  "Medium",
  "Low",
  "Unranked",
] as const;
export type Severity = (typeof SEVERITIES)[number];

/** A decision the owner can make; `Open` is what agents write. */
export type DecidedKind = "Need fix" | "Defer" | "Irrelevant";
export type Decision = "Open" | DecidedKind | "?";

export interface IssueRef {
  key: string;
  url: string;
}

export interface FindingEntry {
  id: string;
  title: string;
  level: number;
  /** 1-based line of the anchor. */
  line: number;
  /** 1-based line after the entry. */
  end: number;
  /** 1-based line of the status line, when there is one. */
  statusLine: number | null;
  severity: Severity;
  /** `null` for an entry that is not actionable. */
  decision: Decision | null;
  issue: IssueRef | null;
  /** The run that first recorded the entry, from its `Run:` line, or `null`. */
  run: string | null;
  problems: string[];
  /** Another entry in the file has the same ID. */
  duplicate: boolean;
  /** Offsets in the source: the anchor, the heading, the status line, the end. */
  offsets: {
    start: number;
    heading: number;
    status: number | null;
    statusEnd: number | null;
    end: number;
  };
}

export interface ParsedFindings {
  entries: FindingEntry[];
  problems: { line: number; message: string }[];
}

const ANCHOR_RE = /^<a id="([^"]*)"><\/a>\s*$/;
const HEADING_RE = /^(#{1,6})[ \t]+([^\n]*?)(?:[ \t]+#+)?[ \t]*$/;
const STATUS_RE =
  /^Severity:[ \t]*([^\n]*?)(?:[ \t]+·[ \t]+Decision:[ \t]*([^\n]*?))?[ \t]*$/;
const DECISION_RE =
  /^(Open|Need fix|Defer|Irrelevant)(?=[ \t—–-]|$)[ \t]*(?:[—–-][ \t]*)?([^\n]*)$/;
const ISSUE_LINK_RE = /\[([^\]\n]+)\]\(([^)\s]+)\)/;
const FENCE_RE = /^ {0,3}(`{3,}|~{3,})/;
const STATUS_DECISION_RE = /([ \t]+·[ \t]+Decision:[ \t]*)([^\n]*?)([ \t]*)$/;
const RUN_LABEL = "Run:";

/**
 * Which findings file a repository path is, or `null`: only the files beside
 * the ontology, in `.g5/TLA_verification/` or, in a checkout rooted at `.g5`,
 * `TLA_verification/`.
 */
export function findingsFileOf(path: string): FindingsFile | null {
  for (const name of FINDINGS_FILES)
    if (
      path === `.g5/TLA_verification/${name}` ||
      path === `TLA_verification/${name}`
    )
      return name;
  return null;
}

/** Whether each line can carry structure: it is outside a fenced block. */
function structureLines(lines: string[]): boolean[] {
  const usable: boolean[] = [];
  let fence: string | null = null;
  for (const line of lines) {
    const match = FENCE_RE.exec(line);
    if (fence === null) {
      if (match) {
        fence = match[1][0].repeat(match[1].length);
        usable.push(false);
        continue;
      }
      usable.push(true);
    } else {
      usable.push(false);
      const stripped = line.trim();
      if (
        match &&
        stripped.startsWith(fence) &&
        [...stripped].every((c) => c === fence![0])
      )
        fence = null;
    }
  }
  return usable;
}

function parseStatus(entry: FindingEntry, text: string): void {
  const match = STATUS_RE.exec(text);
  if (!match) {
    entry.problems.push(
      "the line after the heading is not a status line `Severity: …`",
    );
    return;
  }
  const rawSeverity = match[1].trim();
  const canonical = SEVERITIES.find(
    (s) => s.toLowerCase() === rawSeverity.toLowerCase(),
  );
  if (!canonical)
    entry.problems.push(
      `unknown severity \`${rawSeverity}\`; use ${SEVERITIES.join(", ")}`,
    );
  else entry.severity = canonical;
  if (match[2] === undefined) return;
  const decision = DECISION_RE.exec(match[2].trim());
  if (!decision) {
    entry.decision = "?";
    entry.problems.push(
      `unknown decision \`${match[2].trim()}\`; agents write \`Open\``,
    );
    return;
  }
  entry.decision = decision[1] as Decision;
  const rest = decision[2];
  const link = ISSUE_LINK_RE.exec(rest);
  if (link) entry.issue = { key: link[1], url: link[2] };
  if (entry.decision === "Open" && rest.trim())
    entry.problems.push("an Open decision has nothing after it");
  if ((entry.decision === "Need fix" || entry.decision === "Defer") && !link)
    entry.problems.push(`a \`${entry.decision}\` decision must link its issue`);
}

/** The entries of a findings file, read as the format checker reads them. */
export function parseFindings(text: string): ParsedFindings {
  const raw = text.split("\n");
  const starts: number[] = [];
  for (let i = 0, at = 0; i < raw.length; at += raw[i].length + 1, i++)
    starts.push(at);
  const lines = raw.map((line) => line.replace(/\r$/, ""));
  const usable = structureLines(lines);
  const problems: ParsedFindings["problems"] = [];
  const heads: { start: number; id: string; level: number; title: string }[] =
    [];
  lines.forEach((line, index) => {
    if (!usable[index]) return;
    const anchor = ANCHOR_RE.exec(line);
    if (!anchor) return;
    const heading =
      index + 1 < lines.length ? HEADING_RE.exec(lines[index + 1]) : null;
    if (!heading || heading[1].length < 2 || !usable[index + 1]) {
      problems.push({
        line: index + 1,
        message: `anchor \`${anchor[1]}\` is not directly followed by a heading`,
      });
      return;
    }
    heads.push({
      start: index,
      id: anchor[1],
      level: heading[1].length,
      title: heading[2].trim(),
    });
  });
  const offsetOf = (line: number) =>
    line < starts.length ? starts[line] : text.length;
  const entries = heads.map((head, position) => {
    const limit =
      position + 1 < heads.length ? heads[position + 1].start : lines.length;
    let end = limit;
    for (let i = head.start + 2; i < limit; i++) {
      const heading = usable[i] ? HEADING_RE.exec(lines[i]) : null;
      if (heading && heading[1].length <= head.level) {
        end = i;
        break;
      }
    }
    const entry: FindingEntry = {
      id: head.id,
      title: head.title,
      level: head.level,
      line: head.start + 1,
      end: end + 1,
      statusLine: null,
      severity: "Unranked",
      decision: null,
      issue: null,
      run: null,
      problems: [],
      duplicate: false,
      offsets: {
        start: offsetOf(head.start),
        heading: offsetOf(head.start + 1),
        status: null,
        statusEnd: null,
        end: offsetOf(end),
      },
    };
    let status = -1;
    for (let i = head.start + 2; i < end; i++)
      if (lines[i].trim()) {
        status = i;
        break;
      }
    if (
      status >= 0 &&
      usable[status] &&
      lines[status].startsWith("Severity:")
    ) {
      entry.statusLine = status + 1;
      entry.offsets.status = starts[status];
      entry.offsets.statusEnd = starts[status] + lines[status].length;
      parseStatus(entry, lines[status].trim());
      // Its own line, because a decision rewrites the end of the status line.
      let next = -1;
      for (let i = status + 1; i < end; i++)
        if (lines[i].trim()) {
          next = i;
          break;
        }
      if (next >= 0 && usable[next] && lines[next].startsWith(RUN_LABEL)) {
        const label = lines[next].slice(RUN_LABEL.length).trim();
        if (label) entry.run = label;
        else entry.problems.push("the `Run:` line names no run");
      }
    } else {
      entry.problems.push(
        "missing status line `Severity: …` after the heading",
      );
    }
    return entry;
  });
  const counts = new Map<string, number>();
  for (const e of entries) counts.set(e.id, (counts.get(e.id) ?? 0) + 1);
  for (const e of entries) e.duplicate = (counts.get(e.id) ?? 0) > 1;
  return { entries, problems };
}

/**
 * The runs a file's entries name, in order of first appearance, and whether
 * some entries name none.
 */
export function findingRuns(entries: readonly Pick<FindingEntry, "run">[]): {
  labels: string[];
  unnamed: boolean;
} {
  const labels = new Set<string>();
  let unnamed = false;
  for (const { run } of entries)
    if (run === null) unnamed = true;
    else labels.add(run);
  return { labels: [...labels], unnamed };
}

/** The decision text the viewer writes after `Decision: `. */
export function decisionText(
  kind: DecidedKind,
  issue: IssueRef | null,
  file: FindingsFile,
): string {
  const link = issue
    ? `[${issue.key.replace(/[[\]]/g, "")}](${issue.url.replace(/[\s()]/g, (c) => `%${c.charCodeAt(0).toString(16).toUpperCase().padStart(2, "0")}`)})`
    : "";
  if (kind === "Irrelevant") {
    const subject =
      file === "design_simplification.md" ? "simplification" : "issue";
    const sentence = `Irrelevant — user decided this ${subject} is not important.`;
    return link ? `${sentence} Closed ${link}.` : sentence;
  }
  if (!issue) throw new Error(`${kind} needs its issue`);
  return `${kind} — ${link}`;
}

/**
 * The file with one entry's decision replaced. Throws when the entry is
 * missing, ambiguous or has no decision to replace, so a stale press writes
 * nothing.
 */
export function setDecision(
  text: string,
  id: string,
  kind: DecidedKind,
  issue: IssueRef | null,
  file: FindingsFile,
): string {
  const matches = parseFindings(text).entries.filter((e) => e.id === id);
  if (matches.length !== 1)
    throw new Error(
      matches.length
        ? `Entry ${id} appears more than once in the file.`
        : `Entry ${id} is no longer in the file.`,
    );
  const [entry] = matches;
  const { status, statusEnd } = entry.offsets;
  if (status === null || statusEnd === null || entry.decision === null)
    throw new Error(`Entry ${id} has no decision line.`);
  const line = text.slice(status, statusEnd);
  const match = STATUS_DECISION_RE.exec(line);
  if (!match) throw new Error(`Entry ${id} has no decision line.`);
  const at = status + match.index + match[1].length;
  return (
    text.slice(0, at) +
    decisionText(kind, issue, file) +
    text.slice(at + match[2].length)
  );
}

/** GitHub's title limit, which G5 enforces for every tracker. */
const MAX_TITLE = 256;
const PRIORITY_PREFIX = /^\[(?:High|Low)\][ \t]*/i;

/** The priority a decision gives its issue. */
export function priorityOf(kind: "Need fix" | "Defer"): "High" | "Low" {
  return kind === "Need fix" ? "High" : "Low";
}

/** `[High] <title>` or `[Low] <title>`, keeping any other wording. */
export function prioritizedTitle(
  title: string,
  kind: "Need fix" | "Defer",
): string {
  const full = `[${priorityOf(kind)}] ${title.replace(PRIORITY_PREFIX, "")}`;
  return full.length > MAX_TITLE ? `${full.slice(0, MAX_TITLE - 1)}…` : full;
}

/** The per-entry creation key: one issue per entry and author. */
export function creationKey(repoId: string, path: string, id: string): string {
  return `finding:${repoId}:${path}#${id}`;
}

const LINK_DEST_RE = /(\]\()(<[^>\n]*>|[^)\s]+)((?:\s+"[^"]*")?\))/g;

/**
 * An entry as displayed, for an issue description: its text after the status
 * line, with every relative link made absolute by `absolute`, followed by a
 * link to the entry itself.
 */
export function issueBody(
  text: string,
  entry: FindingEntry,
  filePath: string,
  file: FindingsFile,
  absolute: (path: string, fragment: string) => string,
): string {
  const headingEnd = text.indexOf("\n", entry.offsets.heading);
  const from =
    entry.offsets.statusEnd ??
    (headingEnd < 0 ? entry.offsets.end : headingEnd + 1);
  const absoluteLinks = (part: string) =>
    part.replace(LINK_DEST_RE, (whole, open, dest, close) => {
      const href = dest.replace(/^<|>$/g, "");
      if (/^[a-z][a-z0-9+.-]*:/i.test(href) || href.startsWith("//"))
        return whole;
      try {
        const target = resolveOntologyLink(filePath, href);
        if (!target) return whole;
        return `${open}${absolute(target.path, target.fragment)}${close}`;
      } catch {
        return whole;
      }
    });
  const raw = text.slice(from, entry.offsets.end);
  const lines = raw.split("\n");
  const usable = structureLines(lines);
  const body = lines
    .map((line, i) =>
      usable[i]
        ? // Code spans keep their text.
          line
            .split(/(`+[^`\n]*?`+)/)
            .map((part, k) => (k % 2 ? part : absoluteLinks(part)))
            .join("")
        : line,
    )
    .join("\n")
    .trim();
  const source = `From [${FINDINGS_LABEL[file]}: ${entry.title}](${absolute(filePath, entry.id)})`;
  return body ? `${body}\n\n${source}` : source;
}

/**
 * The severity a Design verified status marks with a dot: the most severe of
 * the linked findings still open or tracked, if Critical, High or Medium
 * (`g5.documents.ontology.verification.status`). Irrelevant, other and Low or
 * Unranked findings show none.
 */
export function findingsDot(
  entries: readonly Pick<FindingEntry, "severity" | "decision">[],
): Severity | null {
  const tracked = entries.filter(
    (e) =>
      e.decision === "Open" ||
      e.decision === "Need fix" ||
      e.decision === "Defer",
  );
  for (const level of ["Critical", "High", "Medium"] as const)
    if (tracked.some((e) => e.severity === level)) return level;
  return null;
}

/** A findings entry linked from ontology metadata. */
export interface FindingLink {
  file: FindingsFile;
  /** Candidate paths of the file, in link resolution order. */
  paths: string[];
  id: string;
}

/**
 * Findings entries a statement's metadata links (`#<id>` links into a
 * findings file), in first-linked order.
 */
export function findingLinks(
  destinations: readonly string[],
  sourcePath: string,
): FindingLink[] {
  const links = new Map<string, FindingLink>();
  for (const href of destinations) {
    if (/^[a-z][a-z0-9+.-]*:/i.test(href) || href.startsWith("#")) continue;
    const hash = href.indexOf("#");
    if (hash < 0) continue;
    let id: string;
    try {
      id = decodeURIComponent(href.slice(hash + 1));
    } catch {
      continue;
    }
    if (!id) continue;
    let candidates: string[];
    try {
      candidates = linkPathCandidates(sourcePath, href);
    } catch {
      continue;
    }
    const paths = candidates.filter((path) => findingsFileOf(path));
    if (!paths.length) continue;
    const file = findingsFileOf(paths[0])!;
    const key = `${file}\u0000${id}`;
    if (!links.has(key)) links.set(key, { file, paths, id });
  }
  return [...links.values()];
}

/** The text a chat seed may carry: no control characters or backticks. */
function seedText(value: string, limit = 200): string {
  return Array.from(value)
    .filter((c) => c >= " " && c !== "\u007f" && c !== "`")
    .join("")
    .replace(/\s+/g, " ")
    .trim()
    .slice(0, limit);
}

/** The chat prompt for one entry (`g5.workbench.chat-actions`). */
export function findingDiscussPrompt(
  entry: FindingEntry,
  file: FindingsFile,
  filePath: string,
): string {
  const kind = file === "bugs_issues.md" ? "bug" : "design simplification";
  const facts = [seedText(entry.id, 120), `severity ${entry.severity}`];
  if (entry.decision && entry.decision !== "?")
    facts.push(
      `decision ${entry.decision}${entry.issue ? ` with ${seedText(entry.issue.key, 80)}` : ""}`,
    );
  return `Let's discuss the ${kind} "${seedText(entry.title)}" (${facts.join(", ")}) in ${seedText(filePath, 300)}.`;
}

/**
 * An absolute G5 link to a file of this workspace, for issue descriptions:
 * the workspace's route is the current page's path before `/file/`.
 */
export function workspaceFileUrl(
  pathname: string,
  repoId: string,
  path: string,
  fragment: string,
): string {
  const { origin } = window.location;
  const marker = pathname.indexOf("/file/");
  const root =
    marker >= 0 ? pathname.slice(0, marker) : pathname.replace(/\/$/, "");
  const encoded = path.split("/").map(encodeURIComponent).join("/");
  return `${origin}${root}/file/${encoded}?branch=${encodeURIComponent(repoId)}${fragment ? `#${encodeURIComponent(fragment)}` : ""}`;
}
