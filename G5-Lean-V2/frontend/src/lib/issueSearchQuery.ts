// =============================================================================
// Home issue search text
// =============================================================================

/**
 * Typing searches once the text has been still this long; Enter searches at
 * once. Each search spends one request per domain from the installation's
 * shared GitHub search limit (about 30 a minute), so partly typed words and
 * qualifiers (`author:oct`) should not each become a search.
 */
export const ISSUE_SEARCH_DEBOUNCE_MS = 1500;

const SEARCH_OPERATORS = new Set(["OR", "AND", "NOT"]);

/** GitHub issue-search qualifiers whose value may still be being typed. */
const QUALIFIERS = new Set([
  "assignee",
  "author",
  "closed",
  "commenter",
  "comments",
  "created",
  "in",
  "interactions",
  "involves",
  "is",
  "label",
  "language",
  "linked",
  "mentions",
  "milestone",
  "no",
  "org",
  "project",
  "reactions",
  "reason",
  "repo",
  "sort",
  "state",
  "team",
  "type",
  "updated",
  "user",
]);

/**
 * The text stops mid-qualifier: the last term is a known qualifier without a
 * value (`author:`), an operator or a lone `-`, or a quote is still open.
 * GitHub would refuse or misread it, so a typing pause does not search it;
 * Enter still does. Other text ending in `:` (`Error:`) is ordinary text.
 */
export function isUnfinishedQuery(text: string): boolean {
  if ((text.match(/"/g)?.length ?? 0) % 2 === 1) return true;
  const last = text.trim().split(/\s+/).at(-1) ?? "";
  if (last === "") return false;
  const bare = last.replace(/^-/, "");
  if (bare === "" || SEARCH_OPERATORS.has(bare)) return true;
  return bare.endsWith(":") && QUALIFIERS.has(bare.slice(0, -1).toLowerCase());
}

/**
 * `label:bug label:"needs triage"` — the qualifiers both trackers' searches
 * honor, for a search started from the Home Issues tile's selected labels.
 * A name with a space, comma, colon or quote is quoted, so it keeps its
 * exact meaning: unquoted, a comma separates alternatives (`label:a,b`)
 * and a space ends the term.
 */
export function labelQualifiers(labels: string[]): string {
  return labels
    .map((label) =>
      /[\s",:]/.test(label) ? `label:"${label}"` : `label:${label}`,
    )
    .join(" ");
}
