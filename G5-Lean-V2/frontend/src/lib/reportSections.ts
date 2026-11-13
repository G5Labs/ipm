// =============================================================================
// Governing prompt section of an agent review report
// (g5.governance.approval.report-view)
// =============================================================================
// The rendered report shows its top-level `Governing prompt` section as a
// collapsed disclosure. This only splits the Markdown body for display; the
// stored bytes and the Raw view are untouched, and a report without that
// heading renders as one body.
// =============================================================================

export interface GoverningPromptSplit {
  /** Markdown before the section (usually empty). */
  before: string;
  /** The section's content, without its heading line. */
  prompt: string;
  /** Everything from the next heading of the same or a higher level. */
  after: string;
}

const HEADING = /^ {0,3}(#{1,6})[ \t]+(.*?)[ \t#]*$/;
const FENCE = /^ {0,3}(`{3,}|~{3,})/;

/** Split a report body at its first level 1–2 `Governing prompt` heading. */
export function splitGoverningPrompt(
  body: string,
): GoverningPromptSplit | null {
  const lines = body.split(/(?<=\n)/);
  let fence: string | null = null;
  let start = -1;
  let level = 0;
  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index].replace(/\r?\n$/, "");
    const marker = line.match(FENCE)?.[1];
    if (fence) {
      if (
        marker &&
        marker[0] === fence[0] &&
        marker.length >= fence.length &&
        line.trim() === marker
      )
        fence = null;
      continue;
    }
    if (marker) {
      fence = marker;
      continue;
    }
    const heading = line.match(HEADING);
    if (!heading) continue;
    const depth = heading[1].length;
    if (start < 0) {
      if (
        depth <= 2 &&
        heading[2].trim().toLowerCase() === "governing prompt"
      ) {
        start = index;
        level = depth;
      }
    } else if (depth <= level) {
      return {
        before: lines.slice(0, start).join(""),
        prompt: lines.slice(start + 1, index).join(""),
        after: lines.slice(index).join(""),
      };
    }
  }
  if (start < 0) return null;
  return {
    before: lines.slice(0, start).join(""),
    prompt: lines.slice(start + 1).join(""),
    after: "",
  };
}
