import { describe, expect, it } from "vitest";
import {
  compareStates,
  concernRuns,
  concernTitle,
  outcomeMatch,
  parseManifest,
  parseRunIndex,
  runDate,
} from "./verificationArtifacts";

const row = (fields: Record<string, unknown>) =>
  JSON.stringify({
    concern: "receiving-numbering",
    label: "safety",
    outcome: "PASS",
    expect: "pass",
    ...fields,
  });

describe("verification artifact reader", () => {
  it("reads index rows and counts unreadable lines", () => {
    const index = parseRunIndex(
      [
        row({
          run_id: "20261024T014713Z-a",
          distinct_states: 26827,
          states_generated: 86478,
          depth: 13,
        }),
        "",
        "{not json",
        JSON.stringify({ concern: "x" }),
        row({
          run_id: "r2",
          kind: "control",
          variant: "control",
          expect: "violation:NoClash",
          outcome: "EXPECTED_VIOLATION",
        }),
      ].join("\n"),
    );
    expect(index.unreadable).toBe(2);
    expect(index.rows).toHaveLength(2);
    expect(index.rows[0]).toMatchObject({
      distinctStates: 26827,
      statesGenerated: 86478,
      depth: 13,
    });
    expect(index.rows[1]).toMatchObject({
      kind: "control",
      outcome: "EXPECTED_VIOLATION",
    });
  });

  it("matches outcomes to expectations without reinterpreting them", () => {
    expect(outcomeMatch("PASS", "pass")).toBe("matched");
    expect(outcomeMatch("EXPECTED_VIOLATION", "violation:X")).toBe("matched");
    expect(outcomeMatch("EXPECTED_DEADLOCK", "deadlock")).toBe("matched");
    expect(outcomeMatch("TIMEOUT", "pass")).toBe("incomplete");
    expect(outcomeMatch("ERROR", "pass")).toBe("incomplete");
    expect(outcomeMatch("NOT_DETECTED", "violation:X")).toBe("mismatch");
    expect(outcomeMatch("UNEXPECTED(deadlock)", "pass")).toBe("mismatch");
    expect(outcomeMatch("PASS", "violation:X")).toBe("mismatch");
    // No expectation anywhere: a run is expected to pass.
    expect(outcomeMatch("PASS", null)).toBe("matched");
  });

  it("reads files saved with a byte-order mark and ignores empty values", () => {
    const index = parseRunIndex(`\uFEFF${row({ run_id: "r1", expect: "" })}`);
    expect(index.unreadable).toBe(0);
    expect(index.rows[0].expect).toBeNull();
    const parsed = parseManifest(
      `\uFEFF${JSON.stringify({ question: "Q?", runs: [] })}`,
    );
    expect(parsed).toEqual({
      manifest: { title: null, question: "Q?", runs: [] },
    });
  });

  it("selects each declared run's latest execution and keeps the rest", () => {
    const index = parseRunIndex(
      [
        row({ run_id: "20261025T000000Z-safety-b" }),
        row({ run_id: "20261024T000000Z-safety-a" }),
        row({
          run_id: "20261023T000000Z-old",
          label: "retired",
          outcome: "ERROR",
        }),
      ].join("\n"),
    );
    const runs = concernRuns(index.rows, [
      {
        label: "safety",
        spec: "N.tla",
        cfg: "s.cfg",
        kind: "safety",
        variant: "as-built",
        expect: "pass",
      },
      {
        label: "never-run",
        spec: null,
        cfg: null,
        kind: null,
        variant: null,
        expect: null,
      },
    ]);
    expect(runs.declared[0].latest?.runId).toBe("20261025T000000Z-safety-b");
    expect(runs.declared[1].latest).toBeNull();
    expect(runs.earlier.map((r) => r.runId)).toEqual([
      "20261024T000000Z-safety-a",
      "20261023T000000Z-old",
    ]);
  });

  it("takes the last row when run IDs carry no timestamp", () => {
    const index = parseRunIndex(
      [row({ run_id: "z-first" }), row({ run_id: "a-second" })].join("\n"),
    );
    const runs = concernRuns(index.rows, [
      {
        label: "safety",
        spec: null,
        cfg: null,
        kind: null,
        variant: null,
        expect: null,
      },
    ]);
    expect(runs.declared[0].latest?.runId).toBe("a-second");
    expect(runDate("a-second")).toBeNull();
    expect(runDate("20261024T014713Z-x")).toBe("2026-10-24 01:47");
  });

  it("reads manifests and reports unsupported ones", () => {
    expect(
      parseManifest(
        JSON.stringify({
          question: "Unique?",
          ontology_ids: ["a"],
          runs: [{ label: "s", spec: "A.tla", cfg: "a.cfg" }],
          bindings: [{ path: "x" }],
        }),
      ),
    ).toEqual({
      manifest: {
        title: null,
        question: "Unique?",
        runs: [
          {
            label: "s",
            spec: "A.tla",
            cfg: "a.cfg",
            kind: null,
            variant: null,
            expect: null,
          },
        ],
      },
    });
    expect(parseManifest("{")).toEqual({
      unsupported: "The manifest is not valid JSON.",
    });
    expect(parseManifest(JSON.stringify({ runs: [] }))).toEqual({
      unsupported: "The manifest has no question.",
    });
    expect(parseManifest(JSON.stringify({ question: "Q" }))).toEqual({
      unsupported: "The manifest has no list of runs.",
    });
    expect(concernTitle("receiving-numbering")).toBe("Receiving numbering");
  });
});

describe("recorded state-count score", () => {
  const run = (states: number | null, outcome = "PASS") =>
    parseRunIndex(row({ distinct_states: states, outcome })).rows[0];
  const linked = { before: true, after: true };
  it("shows a score even when bounds or behavior differ", () => {
    expect(
      compareStates(run(100), run(60), { linked, likeForLike: false }),
    ).toMatchObject({
      kind: "ratio",
      percent: 40,
      factor: 0.6,
      qualifications: [expect.stringContaining("differ in scope or behavior")],
    });
  });
  it.each(["EXPECTED_VIOLATION", "ERROR", "TIMEOUT", "RESOURCE"])(
    "retains %s and qualifies its visited-state count",
    (outcome) => {
      const before = run(100, outcome);
      expect(
        compareStates(before, run(60), { linked, likeForLike: true }),
      ).toMatchObject({
        kind: "ratio",
        percent: 40,
        qualifications: [
          expect.stringContaining("did not complete a passing exploration"),
        ],
      });
      expect(before.outcome).toBe(outcome);
    },
  );
  it("does not invent a ratio for missing, zero-baseline or invalid counts", () => {
    for (const states of [null, 0, -1])
      expect(
        compareStates(run(states), run(20), { linked, likeForLike: true }).kind,
      ).toBe("none");
    expect(
      compareStates(run(100), run(0), { linked, likeForLike: true }),
    ).toMatchObject({ kind: "ratio", percent: 100, factor: 0 });
  });
});
