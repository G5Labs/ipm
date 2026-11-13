// Scanner for the raw-button testid floor (client tests spec D16;
// test_contract/workspace_header_layout I6): every raw `<button` JSX element
// in client source must carry a `data-testid` attribute inside its opening
// tag, so e2e and unit suites can target controls by stable testid instead
// of DOM order or display text (experience:
// editor/technical/global_text_in_topbar_collides_with_e2e_getbytext).
//
// Elements are found by parsing each file with the TypeScript compiler API
// (`ts.createSourceFile` with the TSX script kind — `typescript` is already
// a client dev dependency) and walking the AST for `<button …>` opening and
// self-closing JSX elements. The parser sees only real JSX, so a literal
// `<button` inside a comment or a string literal never counts, and
// multi-line opening tags whose attribute expressions contain `=>` arrows or
// nested braces terminate correctly. A `data-testid` supplied only through a
// `{...spread}` attribute deliberately does NOT satisfy the floor — the
// testid must be statically visible in the opening tag (tests spec D16).

import ts from "typescript";

export interface RawButtonTag {
  /** 0-based offset of the `<button` token in the source. */
  start: number;
  /** 1-based line number of the `<button` token. */
  line: number;
  /** The full opening tag text, `<button … >`. */
  tag: string;
  /** Whether the opening tag carries an explicit `data-testid` attribute. */
  hasTestid: boolean;
}

export interface ButtonTestidViolation {
  line: number;
  tag: string;
}

function hasExplicitTestid(attributes: ts.JsxAttributes): boolean {
  return attributes.properties.some(
    (property) =>
      ts.isJsxAttribute(property) &&
      ts.isIdentifier(property.name) &&
      property.name.text === "data-testid",
  );
}

/** Find every raw `<button` opening tag in a TSX source string. */
export function findRawButtonTags(rawSource: string): RawButtonTag[] {
  const sourceFile = ts.createSourceFile(
    "scan.tsx",
    rawSource,
    ts.ScriptTarget.Latest,
    /* setParentNodes */ false,
    ts.ScriptKind.TSX,
  );
  const tags: RawButtonTag[] = [];

  const record = (
    node: ts.JsxOpeningElement | ts.JsxSelfClosingElement,
  ): void => {
    if (!ts.isIdentifier(node.tagName) || node.tagName.text !== "button") {
      return;
    }
    const start = node.getStart(sourceFile);
    tags.push({
      start,
      line: sourceFile.getLineAndCharacterOfPosition(start).line + 1,
      tag: node.getText(sourceFile),
      hasTestid: hasExplicitTestid(node.attributes),
    });
  };

  const visit = (node: ts.Node): void => {
    if (ts.isJsxOpeningElement(node) || ts.isJsxSelfClosingElement(node)) {
      record(node);
    }
    ts.forEachChild(node, visit);
  };
  visit(sourceFile);

  return tags;
}

/** Raw `<button` opening tags missing a `data-testid` attribute. */
export function findButtonTestidViolations(
  source: string,
): ButtonTestidViolation[] {
  return findRawButtonTags(source)
    .filter((tag) => !tag.hasTestid)
    .map(({ line, tag }) => ({ line, tag }));
}
