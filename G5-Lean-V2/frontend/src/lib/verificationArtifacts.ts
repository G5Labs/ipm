// =============================================================================
// verificationArtifacts — reading a project's saved TLA+/TLC artifacts
// =============================================================================
// The only code that interprets artifact formats and paths for the ontology
// viewer (`g5.documents.ontology.verification.details`). The reader contract is
// `tools/tlc/verification-artifact-reader.md`: concern manifests under
// `models/<concern>/`, the run index `results/index.jsonl`, and receipts at
// `results/<concern>/<run_id>/receipt.json`. Saved values are shown as saved;
// nothing here summarizes or re-runs anything.
// =============================================================================

/** One row of `results/index.jsonl`, as far as the viewer reads it. */
export interface RunRow {
  runId: string;
  concern: string;
  label: string;
  kind: string | null;
  variant: string | null;
  expect: string | null;
  outcome: string;
  distinctStates: number | null;
  statesGenerated: number | null;
  depth: number | null;
}

export interface RunIndex {
  rows: RunRow[];
  /** Lines that are not JSON objects with `concern`, `label` and `outcome`. */
  unreadable: number;
}

/** Paths of a verification root's saved files. */
export const artifactPath = {
  manifest: (root: string, concern: string) =>
    `${root}/models/${concern}/manifest.json`,
  folder: (root: string, concern: string) => `${root}/models/${concern}`,
  bindings: (root: string, concern: string) =>
    `${root}/models/${concern}/BINDINGS.md`,
  index: (root: string) => `${root}/results/index.jsonl`,
  receipt: (root: string, concern: string, runId: string) =>
    `${root}/results/${concern}/${runId}/receipt.json`,
};

const text = (value: unknown) =>
  typeof value === "string" && value !== "" ? value : null;
// Editors on Windows may save a byte-order mark.
const withoutBom = (source: string) => source.replace(/^\uFEFF/, "");
const count = (value: unknown) =>
  typeof value === "number" && Number.isFinite(value) ? value : null;

export function parseRunIndex(source: string): RunIndex {
  const rows: RunRow[] = [];
  let unreadable = 0;
  for (const line of withoutBom(source).split("\n")) {
    if (!line.trim()) continue;
    let row: unknown;
    try {
      row = JSON.parse(line);
    } catch {
      unreadable++;
      continue;
    }
    const record = row as Record<string, unknown>;
    const concern = text(record?.concern);
    const label = text(record?.label);
    const outcome = text(record?.outcome);
    if (!row || typeof row !== "object" || !concern || !label || !outcome) {
      unreadable++;
      continue;
    }
    rows.push({
      runId: text(record.run_id) ?? "",
      concern,
      label,
      kind: text(record.kind),
      variant: text(record.variant),
      expect: text(record.expect),
      outcome,
      distinctStates: count(record.distinct_states),
      statesGenerated: count(record.states_generated),
      depth: count(record.depth),
    });
  }
  return { rows, unreadable };
}

/** A declared configuration of a concern manifest. */
export interface DeclaredRun {
  label: string;
  spec: string | null;
  cfg: string | null;
  kind: string | null;
  variant: string | null;
  expect: string | null;
}

export interface Manifest {
  title: string | null;
  question: string;
  runs: DeclaredRun[];
}

/** The manifest fields the viewer reads, or why it cannot read them. */
export function parseManifest(
  source: string,
): { manifest: Manifest } | { unsupported: string } {
  let value: unknown;
  try {
    value = JSON.parse(withoutBom(source));
  } catch {
    return { unsupported: "The manifest is not valid JSON." };
  }
  if (!value || typeof value !== "object" || Array.isArray(value))
    return { unsupported: "The manifest is not a JSON object." };
  const manifest = value as Record<string, unknown>;
  const question = text(manifest.question)?.trim();
  if (!question) return { unsupported: "The manifest has no question." };
  if (!Array.isArray(manifest.runs))
    return { unsupported: "The manifest has no list of runs." };
  const title = text(manifest.title)?.trim() || null;
  return {
    manifest: {
      title,
      question,
      runs: manifest.runs.flatMap((run) => {
        const item = run as Record<string, unknown>;
        const label = text(item?.label);
        return label
          ? [
              {
                label,
                spec: text(item.spec),
                cfg: text(item.cfg),
                kind: text(item.kind),
                variant: text(item.variant),
                expect: text(item.expect),
              },
            ]
          : [];
      }),
    },
  };
}

/** A concern folder's name as a readable title: dashes become spaces. */
/** The run a receipt path names: `<root>/results/<concern>/<run_id>/receipt.json`. */
export function receiptRun(
  path: string,
): { root: string; concern: string; runId: string } | null {
  const match = path.match(/^(.+)\/results\/([^/]+)\/([^/]+)\/receipt\.json$/);
  return match ? { root: match[1], concern: match[2], runId: match[3] } : null;
}

/** A descriptive comparison of the two recorded state counts. */
export type StateComparison =
  | { kind: "ratio"; percent: number; factor: number; qualifications: string[] }
  | { kind: "none"; reason: string };

/**
 * Show the arithmetic whenever both counts exist. Outcomes and differences in
 * scope qualify the score; they do not hide it (`g5.documents.findings.evidence`).
 * A visited-state count is neither a correctness score nor runtime performance.
 */
export function compareStates(
  before: RunRow | null | undefined,
  after: RunRow | null | undefined,
  {
    linked,
    likeForLike,
  }: { linked: { before: boolean; after: boolean }; likeForLike: boolean },
): StateComparison {
  if (!linked.before)
    return { kind: "none", reason: "No as-built run is linked." };
  if (!linked.after)
    return { kind: "none", reason: "No proposed-model run is linked." };
  if (!before || !after)
    return { kind: "none", reason: "A linked run is not in the run index." };
  if (before.distinctStates == null || after.distinctStates == null)
    return { kind: "none", reason: "A run records no state count." };
  if (
    !Number.isFinite(before.distinctStates) ||
    !Number.isFinite(after.distinctStates) ||
    before.distinctStates < 0 ||
    after.distinctStates < 0
  )
    return { kind: "none", reason: "A run records an invalid state count." };
  if (before.distinctStates === 0)
    return {
      kind: "none",
      reason: "The baseline state count is zero; a ratio is undefined.",
    };
  const qualifications: string[] = [];
  if (!likeForLike)
    qualifications.push(
      "The runs differ in scope or behavior; this score compares their recorded counts.",
    );
  if (before.outcome !== "PASS" || after.outcome !== "PASS")
    qualifications.push(
      "At least one run did not complete a passing exploration; its count covers only the states it visited.",
    );
  const factor = after.distinctStates / before.distinctStates;
  return {
    kind: "ratio",
    percent: Math.round((1 - factor) * 1000) / 10,
    factor: Math.round(factor * 1000) / 1000,
    qualifications,
  };
}

export function concernTitle(concern: string): string {
  const words = concern.replace(/[-_]+/g, " ").trim();
  return words.charAt(0).toUpperCase() + words.slice(1);
}

/**
 * Whether a run's saved outcome matched its expectation: the index row's, else
 * the manifest's; a run with neither is expected to pass. `matched` is not a
 * verification claim: a matched control or divergence means the deliberate
 * violation was found.
 */
export type OutcomeMatch = "matched" | "incomplete" | "mismatch";
export function outcomeMatch(
  outcome: string,
  expect: string | null,
): OutcomeMatch {
  if (outcome.startsWith("EXPECTED_")) return "matched";
  if (outcome === "PASS" && (expect ?? "pass") === "pass") return "matched";
  if (["ERROR", "TIMEOUT", "RESOURCE"].includes(outcome)) return "incomplete";
  return "mismatch";
}

const RUN_STAMP = /^(\d{4})(\d{2})(\d{2})T(\d{2})(\d{2})(\d{2})Z/;

/** The run's date from a leading `YYYYMMDDTHHMMSSZ` in its ID, if any. */
export function runDate(runId: string): string | null {
  const m = RUN_STAMP.exec(runId);
  return m ? `${m[1]}-${m[2]}-${m[3]} ${m[4]}:${m[5]}` : null;
}

export interface ConcernRuns {
  /** Each declared run in manifest order, with its latest execution. */
  declared: { run: DeclaredRun; latest: RunRow | null }[];
  /** Earlier executions of declared runs and runs no longer declared. */
  earlier: RunRow[];
}

/**
 * The latest execution of each label: the greatest run ID when every run ID
 * of that label starts with a timestamp, otherwise the last row in the index.
 */
export function concernRuns(
  rows: readonly RunRow[],
  declared: readonly DeclaredRun[],
): ConcernRuns {
  const byLabel = new Map<string, RunRow[]>();
  for (const row of rows) {
    const list = byLabel.get(row.label) ?? [];
    list.push(row);
    byLabel.set(row.label, list);
  }
  const latestOf = (list: RunRow[]) =>
    list.every((row) => RUN_STAMP.test(row.runId))
      ? list.reduce((a, b) => (b.runId > a.runId ? b : a))
      : list[list.length - 1];
  const latest = new Set<RunRow>();
  const result = declared.map((run) => {
    const list = byLabel.get(run.label);
    const row = list?.length ? latestOf(list) : null;
    if (row) latest.add(row);
    return { run, latest: row };
  });
  return {
    declared: result,
    earlier: rows.filter((row) => !latest.has(row)),
  };
}
