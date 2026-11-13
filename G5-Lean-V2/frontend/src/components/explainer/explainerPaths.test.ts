// Managed explainer path predicate — TS mirror of the g5-common
// `is_managed_explainer_path` rules (core D25). The traversal cases pin the
// 2026-08-10 review finding: `..`/`.`/empty components must never count as
// managed, in either root shape.
import { describe, expect, it } from "vitest";
import { isManagedExplainerPath } from "./explainerPaths";

describe("isManagedExplainerPath", () => {
  it("accepts both managed shapes", () => {
    expect(
      isManagedExplainerPath(
        "docs/0e1f2a3b-0000-0000-0000-000000000001/explainers/topic-2026-08-10-0100.html",
      ),
    ).toBe(true);
    expect(
      isManagedExplainerPath(
        "docs/0e1f2a3b-0000-0000-0000-000000000001/explainers/topic-2026-08-10-0100.md",
      ),
    ).toBe(true);
    expect(isManagedExplainerPath("explainers/topic.html")).toBe(true);
  });

  it("rejects lookalikes", () => {
    expect(isManagedExplainerPath("explainers/")).toBe(false);
    expect(isManagedExplainerPath("docs/ws/explainers/")).toBe(false);
    expect(isManagedExplainerPath("docs/explainers/topic.html")).toBe(false);
    expect(isManagedExplainerPath("docs/ws/notes/topic.html")).toBe(false);
    expect(isManagedExplainerPath("src/explainers/nope.html")).toBe(false);
    expect(isManagedExplainerPath("docs/ws/sub/explainers/topic.html")).toBe(
      false,
    );
  });

  it("rejects traversal and degenerate components (review 2026-08-10)", () => {
    expect(isManagedExplainerPath("explainers/../outside.md")).toBe(false);
    expect(isManagedExplainerPath("docs/ws/explainers/../outside.md")).toBe(
      false,
    );
    expect(isManagedExplainerPath("docs/../explainers/escape.html")).toBe(
      false,
    );
    expect(isManagedExplainerPath("explainers//double.md")).toBe(false);
    expect(isManagedExplainerPath("explainers/./dot.md")).toBe(false);
    expect(isManagedExplainerPath("docs/ws/explainers/a/../b.html")).toBe(
      false,
    );
  });
});
