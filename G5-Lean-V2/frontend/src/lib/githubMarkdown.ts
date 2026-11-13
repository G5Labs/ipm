// =============================================================================
// GitHub-flavoured Markdown for G5's renderer
// =============================================================================
// GitHub never displays HTML comments (`<!-- ... -->`), which issue templates
// and G5's own comment reference line use; G5's renderer shows raw HTML as
// text. Remove them before rendering, except inside fenced code blocks, where
// GitHub shows them literally.
// =============================================================================

const FENCE = /^ {0,3}(`{3,}|~{3,})/;
const HTML_COMMENT = /<!--[\s\S]*?(?:-->|$)/g;

/** Markdown as GitHub displays it: HTML comments outside code fences removed. */
export function stripHtmlComments(markdown: string): string {
  const lines = markdown.split("\n");
  const out: string[] = [];
  let prose: string[] = [];
  let fence: string | null = null;
  const flushProse = () => {
    if (prose.length > 0) {
      out.push(...prose.join("\n").replace(HTML_COMMENT, "").split("\n"));
      prose = [];
    }
  };
  for (const line of lines) {
    const marker = FENCE.exec(line)?.[1];
    if (fence === null) {
      if (marker) {
        flushProse();
        fence = marker;
        out.push(line);
      } else {
        prose.push(line);
      }
    } else {
      out.push(line);
      if (marker && marker[0] === fence[0] && marker.length >= fence.length) {
        fence = null;
      }
    }
  }
  flushProse();
  return out.join("\n").trimEnd();
}

/**
 * The first paragraph of a Markdown document as one bounded line: headings,
 * HTML comments and blank lines before it are skipped and its lines joined.
 * Used for one-line purposes (the workbench timeline header); the Markdown
 * marks inside the paragraph are kept as written.
 */
export function firstParagraph(
  markdown: string | null | undefined,
  maxChars = 280,
): string {
  if (!markdown) return "";
  const blocks = stripHtmlComments(markdown)
    .replace(/\r\n/g, "\n")
    .split(/\n\s*\n/);
  for (const block of blocks) {
    const lines = block
      .split("\n")
      .map((line) => line.trim())
      .filter((line) => line.length > 0 && !line.startsWith("#"));
    if (lines.length === 0) continue;
    const text = lines.join(" ");
    return text.length > maxChars
      ? `${text.slice(0, maxChars - 1).trimEnd()}…`
      : text;
  }
  return "";
}
