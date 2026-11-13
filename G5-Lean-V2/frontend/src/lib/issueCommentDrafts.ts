// =============================================================================
// Issue comment drafts in this browser
// =============================================================================
// Unsent issue comments are kept in localStorage per person and issue: from
// a workspace's Activity also per workspace, from the issue page once per
// issue. They never leave the browser and are cleared on sign-out.
// =============================================================================

export const ISSUE_COMMENT_DRAFT_PREFIX = "g5:issue-comment-draft:";

/** Remove every issue comment draft stored in this browser. */
export function clearIssueCommentDrafts(): void {
  try {
    const keys: string[] = [];
    for (let index = 0; index < window.localStorage.length; index += 1) {
      const key = window.localStorage.key(index);
      if (key?.startsWith(ISSUE_COMMENT_DRAFT_PREFIX)) keys.push(key);
    }
    for (const key of keys) window.localStorage.removeItem(key);
  } catch {
    // Storage unavailable: there is nothing to clear.
  }
}
