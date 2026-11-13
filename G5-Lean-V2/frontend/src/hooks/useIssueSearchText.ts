import { useEffect, useState } from "react";

import {
  ISSUE_SEARCH_DEBOUNCE_MS,
  isUnfinishedQuery,
} from "../lib/issueSearchQuery";

/**
 * Issue search text and the query it has become: typing searches after a
 * pause, never mid-qualifier; `searchNow` (Enter) searches at once. Every
 * issue search field uses it (g5.issue.search.one-path).
 */
export function useIssueSearchText(initial = "") {
  const [text, setText] = useState(initial);
  const [query, setQuery] = useState(initial.trim());
  useEffect(() => {
    if (isUnfinishedQuery(text)) return;
    const next = text.trim();
    const timer = window.setTimeout(
      () => setQuery(next),
      ISSUE_SEARCH_DEBOUNCE_MS,
    );
    return () => window.clearTimeout(timer);
  }, [text]);
  return {
    text,
    setText,
    query,
    setQuery,
    searchNow: () => setQuery(text.trim()),
  };
}
