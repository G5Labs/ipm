// =============================================================================
// Suggested workspace name for an issue
// =============================================================================
// `issue-<number>-<title-slug>`: lowercase ASCII letters, digits and hyphens,
// always a valid exact Git branch name. The title slug is cut at a word
// boundary so names stay readable.
// =============================================================================

const MAX_TITLE_SLUG_CHARS = 48;

export function workspaceNameFromIssue(issue: {
  locator: { number: number };
  title: string;
}): string {
  const slug = issue.title
    .normalize("NFKD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  let short = slug;
  if (slug.length > MAX_TITLE_SLUG_CHARS) {
    short = slug.slice(0, MAX_TITLE_SLUG_CHARS);
    // Drop a partial last word unless the cut already falls between words.
    const wordEnd = short.lastIndexOf("-");
    if (slug[MAX_TITLE_SLUG_CHARS] !== "-" && wordEnd > 0) {
      short = short.slice(0, wordEnd);
    }
  }
  short = short.replace(/-+$/, "");
  const prefix = `issue-${issue.locator.number}`;
  return short ? `${prefix}-${short}` : prefix;
}
