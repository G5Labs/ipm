import { describe, expect, it } from "vitest";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import {
  findButtonTestidViolations,
  findRawButtonTags,
} from "./buttonTestidScanner";

// Repository rule (client tests spec D16; test_contract/workspace_header_layout
// I6, chat 4f381fe9): EVERY raw `<button` JSX element in client source carries
// a `data-testid`. There is deliberately NO baseline allowlist — the sweep
// that introduced this suite fixed every existing violation, and a new
// unlabeled button is a hard failure with file and line. Components built on
// the shared `Button` primitive are out of scope (the primitive forwards
// `data-testid`; callers add one where a stable anchor is needed).

const SRC_ROOT = join(__dirname, "..");

function collectTsxFiles(dir: string, acc: string[] = []): string[] {
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) {
      collectTsxFiles(full, acc);
    } else if (full.endsWith(".tsx") && !/\.test\.tsx$/.test(full)) {
      acc.push(full);
    }
  }
  return acc;
}

describe("raw <button> testid coverage (tests spec D16)", () => {
  it("every raw <button> in client source carries a data-testid", () => {
    const failures: string[] = [];
    for (const file of collectTsxFiles(SRC_ROOT)) {
      const source = readFileSync(file, "utf8");
      for (const violation of findButtonTestidViolations(source)) {
        failures.push(
          `${relative(SRC_ROOT, file)}:${violation.line} — raw <button> without data-testid`,
        );
      }
    }
    expect(
      failures,
      `Raw <button> elements missing data-testid (add one, or build on the shared Button primitive with an explicit testid):\n${failures.join("\n")}`,
    ).toEqual([]);
  });

  // Regression proof for the matcher itself (contract I6 flow step 2): a
  // violating fixture must be flagged, and the tricky JSX shapes the scanner
  // exists for must not produce false negatives or false positives.
  it("the matcher flags an unlabeled button", () => {
    const fixture = `<button type="button" onClick={() => act()}>Go</button>`;
    expect(findButtonTestidViolations(fixture)).toHaveLength(1);
  });

  it("the matcher accepts a labeled button whose attributes contain arrows and nested braces", () => {
    const fixture = [
      `<button`,
      `  type="button"`,
      `  onClick={() => run({ deep: { value: ">" } })}`,
      `  data-testid="labeled-button"`,
      `>`,
      `  Go`,
      `</button>`,
    ].join("\n");
    expect(findRawButtonTags(fixture)).toHaveLength(1);
    expect(findButtonTestidViolations(fixture)).toHaveLength(0);
  });

  it("the matcher does not stop at the > of an arrow function before data-testid", () => {
    // data-testid AFTER the arrow: a naive [^>]*? matcher would truncate the
    // tag at `=>` and report a false violation.
    const fixture = `<button onClick={() => act()} data-testid="after-arrow">Go</button>`;
    expect(findButtonTestidViolations(fixture)).toHaveLength(0);
  });

  it("the matcher ignores a literal <button inside a string literal", () => {
    // The AST walk sees only real JSX elements — `<button` text inside
    // string/template literals must never be reported as an element.
    const fixture = [
      `const example = "<button>";`,
      `const tpl = \`docs say <button type="button"> needs a testid\`;`,
      `export const usage = { example, tpl };`,
    ].join("\n");
    expect(findRawButtonTags(fixture)).toHaveLength(0);
    expect(findButtonTestidViolations(fixture)).toHaveLength(0);
  });

  it("the matcher ignores a literal <button inside comments", () => {
    const fixture = [
      `// a raw <button onClick={x}> in a line comment`,
      `/* and <button type="button"> in a block comment */`,
      `export const noop = 1;`,
    ].join("\n");
    expect(findRawButtonTags(fixture)).toHaveLength(0);
  });

  it("the matcher still flags a button whose only testid arrives via a spread", () => {
    // The floor requires a statically visible data-testid in the opening
    // tag (tests spec D16); `{...props}` cannot prove one exists.
    const fixture = `<button type="button" {...sharedProps}>Go</button>`;
    expect(findButtonTestidViolations(fixture)).toHaveLength(1);
  });
});
