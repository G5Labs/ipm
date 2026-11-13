// The findings format, read the way the agents' checker reads it
// (`g5.documents.findings.files`). `check_findings.py` runs the same fixtures.
import { readdirSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { describe, expect, it } from "vitest";
import {
  creationKey,
  decisionText,
  findingLinks,
  findingRuns,
  findingsFileOf,
  issueBody,
  parseFindings,
  prioritizedTitle,
  setDecision,
} from "./findings";

const FIXTURES = resolve(__dirname, "../../../tests/fixtures/findings");

interface Expected {
  fileProblems: { line: number; message: string }[];
  entries: {
    id: string;
    title: string;
    level: number;
    line: number;
    end: number;
    statusLine: number | null;
    severity: string;
    decision: string | null;
    issue: { key: string; url: string } | null;
    run: string | null;
    problems: string[];
  }[];
}

const entry = (
  id: string,
  status = "Severity: High · Decision: Open",
  body = "What goes wrong. See [the run](results/r/receipt.json).",
) => `<a id="${id}"></a>\n### Entry ${id}\n\n${status}\n\n${body}\n`;

describe("parseFindings", () => {
  for (const folder of readdirSync(FIXTURES, { withFileTypes: true })) {
    if (!folder.isDirectory()) continue;
    it(`matches the checker on ${folder.name}`, () => {
      const dir = join(FIXTURES, folder.name);
      const expected = JSON.parse(
        readFileSync(join(dir, "expected.json"), "utf8"),
      ) as Expected;
      const source = readdirSync(dir).find((name) => name.endsWith(".md"))!;
      const parsed = parseFindings(readFileSync(join(dir, source), "utf8"));
      expect(parsed.problems).toEqual(expected.fileProblems);
      expect(
        parsed.entries.map((e) => ({
          id: e.id,
          title: e.title,
          level: e.level,
          line: e.line,
          end: e.end,
          statusLine: e.statusLine,
          severity: e.severity,
          decision: e.decision,
          issue: e.issue,
          run: e.run,
          problems: e.problems,
        })),
      ).toEqual(expected.entries);
    });
  }

  it("reads Windows line endings like Unix ones", () => {
    const unix = entry("a") + entry("b", "Severity: Low");
    const parsed = parseFindings(unix.replace(/\n/g, "\r\n"));
    expect(parsed.entries.map((e) => [e.id, e.severity, e.decision])).toEqual([
      ["a", "High", "Open"],
      ["b", "Low", null],
    ]);
  });

  it("marks duplicate IDs", () => {
    const parsed = parseFindings(entry("a") + entry("a"));
    expect(parsed.entries.map((e) => e.duplicate)).toEqual([true, true]);
  });
});

describe("runs", () => {
  const status = "Severity: High · Decision: Open";
  const ran = (label: string, body = "Found by: source review") =>
    `Run: ${label}\n\n${body}`;

  it("reads the Run: line directly after the status line", () => {
    const parsed = parseFindings(
      entry("a", status, ran("Uplift 2026-10-24")) +
        entry("b", status) +
        // On the next line, too, with the space around the label trimmed.
        `<a id="c"></a>\n### Entry c\n\nSeverity: Low\nRun:  Base re-verification 2026-11-06 (incremental) \n\nDrift.\n`,
    );
    expect(
      parsed.entries.map((e) => [e.id, e.run, e.decision, e.problems]),
    ).toEqual([
      ["a", "Uplift 2026-10-24", "Open", []],
      ["b", null, "Open", []],
      ["c", "Base re-verification 2026-11-06 (incremental)", null, []],
    ]);
  });

  it("reports an empty label and reads no Run: line elsewhere", () => {
    const parsed = parseFindings(
      entry("a", status, "Run:   \n\nWhy.") +
        entry(
          "b",
          status,
          "Found by: source review\n\nRun: Uplift 2026-10-24",
        ) +
        entry("c", status, "```text\nRun: Uplift 2026-10-24\n```") +
        `<a id="d"></a>\n### Entry d\n\nRun: Uplift 2026-10-24\n\n${status}\n`,
    );
    expect(parsed.entries.map((e) => [e.id, e.run, e.problems])).toEqual([
      ["a", null, ["the `Run:` line names no run"]],
      ["b", null, []],
      ["c", null, []],
      ["d", null, ["missing status line `Severity: …` after the heading"]],
    ]);
  });

  it("keeps the file's order and everything else an entry reads", () => {
    const read = (parts: string[]) =>
      parseFindings(parts.join("")).entries.map((e) => ({
        id: e.id,
        title: e.title,
        severity: e.severity,
        decision: e.decision,
        line: e.line,
        statusLine: e.statusLine,
        problems: e.problems,
      }));
    const plain = read([
      entry("a", "Severity: Critical · Decision: Open"),
      entry("b", "Severity: High · Decision: Open"),
      entry("q", "Severity: Medium"),
    ]);
    const withRuns = read([
      entry(
        "a",
        "Severity: Critical · Decision: Open",
        ran("Uplift 2026-10-24"),
      ),
      entry("b", "Severity: High · Decision: Open"),
      entry("q", "Severity: Medium", ran("Base re-verification 2026-11-06")),
    ]);
    // Each Run: paragraph moves the entries after it down two lines.
    const moved = [0, 2, 2];
    expect(withRuns).toEqual(
      plain.map((e, i) => ({
        ...e,
        line: e.line + moved[i],
        statusLine: e.statusLine! + moved[i],
      })),
    );
  });

  it("lists the runs in order of first appearance, and whether some name none", () => {
    const base = "Base re-verification 2026-11-06 (incremental)";
    const parsed = parseFindings(
      entry("a", status, ran(base)) +
        entry("b", status, ran("Uplift 2026-10-24")) +
        entry("c", status) +
        entry("d", status, ran(base)),
    );
    expect(findingRuns(parsed.entries)).toEqual({
      labels: [base, "Uplift 2026-10-24"],
      unnamed: true,
    });
    expect(findingRuns(parsed.entries.filter((e) => e.run))).toEqual({
      labels: [base, "Uplift 2026-10-24"],
      unnamed: false,
    });
    expect(findingRuns(parseFindings(entry("a")).entries)).toEqual({
      labels: [],
      unnamed: true,
    });
  });
});

describe("setDecision", () => {
  const text = `# Bugs\n\n${entry("a")}\n${entry("b", "Severity: Low · Decision: Open")}`;

  it("replaces only the decision of one entry", () => {
    const next = setDecision(
      text,
      "b",
      "Defer",
      { key: "ACME-3", url: "https://g5.example/i/3" },
      "bugs_issues.md",
    );
    expect(next).toBe(
      text.replace(
        "Severity: Low · Decision: Open",
        "Severity: Low · Decision: Defer — [ACME-3](https://g5.example/i/3)",
      ),
    );
    expect(parseFindings(next).entries[1]).toMatchObject({
      decision: "Defer",
      issue: { key: "ACME-3", url: "https://g5.example/i/3" },
      problems: [],
    });
  });

  it("keeps a closed issue on Irrelevant and says which kind of entry", () => {
    expect(
      decisionText(
        "Irrelevant",
        { key: "acme/api#7", url: "https://github.com/acme/api/issues/7" },
        "bugs_issues.md",
      ),
    ).toBe(
      "Irrelevant — user decided this issue is not important. Closed [acme/api#7](https://github.com/acme/api/issues/7).",
    );
    expect(decisionText("Irrelevant", null, "design_simplification.md")).toBe(
      "Irrelevant — user decided this simplification is not important.",
    );
    const next = setDecision(text, "a", "Irrelevant", null, "bugs_issues.md");
    expect(parseFindings(next).entries[0].decision).toBe("Irrelevant");
  });

  it("leaves the Run: line as it is", () => {
    const run = "Run: Uplift 2026-10-24";
    const withRun = `# Bugs\n\n${entry("a", "Severity: High · Decision: Open", `${run}\n\nFound by: source review`)}`;
    const next = setDecision(
      withRun,
      "a",
      "Need fix",
      { key: "ACME-4", url: "https://g5.example/i/4" },
      "bugs_issues.md",
    );
    expect(next).toBe(
      withRun.replace(
        "Decision: Open\n",
        "Decision: Need fix — [ACME-4](https://g5.example/i/4)\n",
      ),
    );
    expect(parseFindings(next).entries[0]).toMatchObject({
      decision: "Need fix",
      run: "Uplift 2026-10-24",
      problems: [],
    });
    // A Run: line straight under the status line stays on its own line.
    const joined = withRun.replace(`Open\n\n${run}`, `Open\n${run}`);
    const decided = setDecision(
      joined,
      "a",
      "Irrelevant",
      null,
      "bugs_issues.md",
    );
    expect(decided).toContain(
      `Decision: Irrelevant — user decided this issue is not important.\n${run}\n`,
    );
    expect(parseFindings(decided).entries[0].run).toBe("Uplift 2026-10-24");
  });

  it("keeps Windows line endings", () => {
    const crlf = text.replace(/\n/g, "\r\n");
    const next = setDecision(crlf, "a", "Irrelevant", null, "bugs_issues.md");
    expect(next.split("\r\n").length).toBe(crlf.split("\r\n").length);
    expect(next).toContain("not important.\r\n");
  });

  it("refuses a missing, duplicated or undecidable entry", () => {
    expect(() =>
      setDecision(text, "gone", "Irrelevant", null, "bugs_issues.md"),
    ).toThrow("no longer in the file");
    expect(() =>
      setDecision(
        entry("a") + entry("a"),
        "a",
        "Irrelevant",
        null,
        "bugs_issues.md",
      ),
    ).toThrow("more than once");
    expect(() =>
      setDecision(
        entry("q", "Severity: Low"),
        "q",
        "Irrelevant",
        null,
        "bugs_issues.md",
      ),
    ).toThrow("no decision line");
    expect(() => decisionText("Defer", null, "bugs_issues.md")).toThrow();
  });
});

describe("issues", () => {
  it("prefixes the priority and keeps other wording", () => {
    expect(prioritizedTitle("Ceiling bypass", "Need fix")).toBe(
      "[High] Ceiling bypass",
    );
    expect(prioritizedTitle("[High] Ceiling bypass (edited)", "Defer")).toBe(
      "[Low] Ceiling bypass (edited)",
    );
    expect(prioritizedTitle("x".repeat(300), "Defer")).toHaveLength(256);
    expect(creationKey("r1", ".g5/TLA_verification/bugs_issues.md", "a")).toBe(
      "finding:r1:.g5/TLA_verification/bugs_issues.md#a",
    );
  });

  it("builds the body from the displayed entry with absolute links", () => {
    const path = ".g5/TLA_verification/bugs_issues.md";
    const text = entry(
      "a",
      "Severity: High · Decision: Open",
      "Over-receipt. See [rule](../ontology/r.g5.md#x), [code](../../src/a.cs#L3-L5), [web](https://example.com) and `[not](a.md)`.\n\n```text\n[kept](b.md)\n```",
    );
    const [parsed] = parseFindings(text).entries;
    const body = issueBody(
      text,
      parsed,
      path,
      "bugs_issues.md",
      (p, f) => `https://g5.example/w/file/${p}${f ? `#${f}` : ""}`,
    );
    expect(body).toBe(
      [
        "Over-receipt. See [rule](https://g5.example/w/file/.g5/ontology/r.g5.md#x), [code](https://g5.example/w/file/src/a.cs#L3-L5), [web](https://example.com) and `[not](a.md)`.",
        "",
        "```text",
        "[kept](b.md)",
        "```",
        "",
        "From [Bugs and issues: Entry a](https://g5.example/w/file/.g5/TLA_verification/bugs_issues.md#a)",
      ].join("\n"),
    );
  });
});

describe("links", () => {
  it("recognizes the two files under any TLA_verification folder", () => {
    expect(findingsFileOf(".g5/TLA_verification/bugs_issues.md")).toBe(
      "bugs_issues.md",
    );
    expect(findingsFileOf("TLA_verification/design_simplification.md")).toBe(
      "design_simplification.md",
    );
    expect(findingsFileOf(".g5/TLA_verification/models/bugs_issues.md")).toBe(
      null,
    );
    expect(findingsFileOf("docs/bugs_issues.md")).toBe(null);
    // Only beside the ontology, not in Project Docs or other folders.
    expect(findingsFileOf("docs/ws/TLA_verification/bugs_issues.md")).toBe(
      null,
    );
  });

  it("finds entries linked from ontology metadata", () => {
    expect(
      findingLinks(
        [
          "../../TLA_verification/bugs_issues.md#bug-001",
          "../../TLA_verification/bugs_issues.md#bug-001",
          "../../TLA_verification/design_simplification.md#simp-1",
          "../../TLA_verification/bugs_issues.md",
          "../../TLA_verification/models/a/manifest.json",
          "https://example.com/bugs_issues.md#x",
        ],
        ".g5/ontology/features/receiving.g5.md",
      ),
    ).toEqual([
      {
        file: "bugs_issues.md",
        paths: [
          ".g5/TLA_verification/bugs_issues.md",
          "TLA_verification/bugs_issues.md",
        ],
        id: "bug-001",
      },
      {
        file: "design_simplification.md",
        paths: [
          ".g5/TLA_verification/design_simplification.md",
          "TLA_verification/design_simplification.md",
        ],
        id: "simp-1",
      },
    ]);
  });
});
