// =============================================================================
// spacedLinks — `[label](path with spaces)` as a link in rendered ontology
// =============================================================================
// Markdown does not accept unencoded spaces in a link destination, so such a
// link renders as plain text. In ontology files it is a link like any other
// (`g5.documents.ontology.links`); this display step never changes the source.
// It runs after sanitizing and only creates relative links.
// =============================================================================

import type { Element, ElementContent, Root, Text } from "hast";

const SPACED = /\[([^\]\n]+)\]\(([^()\n]*\s[^()\n]*)\)/g;
const SKIP = new Set(["a", "code", "pre", "script", "style"]);

function split(node: Text): ElementContent[] | null {
  const parts: ElementContent[] = [];
  let last = 0;
  for (const match of node.value.matchAll(SPACED)) {
    const destination = match[2].trim();
    // Relative paths only: a scheme or protocol-relative link stays text.
    if (
      /^[a-z][a-z0-9+.-]*:/i.test(destination) ||
      destination.startsWith("//")
    )
      continue;
    if (match.index > last)
      parts.push({ type: "text", value: node.value.slice(last, match.index) });
    parts.push({
      type: "element",
      tagName: "a",
      properties: { href: encodeURI(destination) },
      children: [{ type: "text", value: match[1] }],
    });
    last = match.index + match[0].length;
  }
  if (!parts.length) return null;
  if (last < node.value.length)
    parts.push({ type: "text", value: node.value.slice(last) });
  return parts;
}

function visit(parent: Root | Element) {
  const children: (typeof parent.children)[number][] = [];
  for (const child of parent.children) {
    if (child.type === "text") {
      children.push(...(split(child) ?? [child]));
      continue;
    }
    if (child.type === "element" && !SKIP.has(child.tagName)) visit(child);
    children.push(child);
  }
  parent.children = children as typeof parent.children;
}

/** Rehype plugin: link `[label](path with spaces)` written in text. */
export function spacedLinks() {
  return (tree: Root) => visit(tree);
}
