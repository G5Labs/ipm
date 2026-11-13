import { describe, expect, it } from "vitest";
import { findLineMatches, linePatternAt } from "./reviewLineSearch";

describe("review line matching", () => {
  it("preserves overlapping exact occurrences and rejects incomplete patterns", () => {
    expect(findLineMatches(["x", "x", "x"], ["x", "x"])).toEqual([0, 1]);
    expect(findLineMatches(["a", "b", "a", "b", "a"], ["a", "b", "a"])).toEqual(
      [0, 2],
    );
    expect(findLineMatches(["a"], [])).toEqual([]);
    expect(findLineMatches([], ["a"])).toEqual([]);
    expect(findLineMatches(["a"], ["a", "b"])).toEqual([]);
    expect(linePatternAt(["a", "b"], ["b"], 1)).toBe(true);
    expect(linePatternAt(["a", "b"], ["b"], 0)).toBe(false);
    expect(linePatternAt(["a", "b"], [], 2)).toBe(true);
    expect(linePatternAt(["a", "b"], [], 3)).toBe(false);
    expect(linePatternAt(["a", "b"], ["a"], -1)).toBe(false);
  });

  it("keeps exact-line reads linear for a large repeated document and near-match pattern", () => {
    let reads = 0;
    const counted = (values: string[]) =>
      new Proxy(values, {
        get(target, key, receiver) {
          if (typeof key === "string" && /^\d+$/.test(key)) reads++;
          return Reflect.get(target, key, receiver);
        },
      });
    const lines = counted(Array(100_000).fill("repeat"));
    const pattern = counted([...Array(49_999).fill("repeat"), "missing"]);
    expect(findLineMatches(lines, pattern)).toEqual([]);
    expect(reads).toBeLessThan(8 * (lines.length + pattern.length));
  });
});
