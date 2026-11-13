// =============================================================================
// Issue people and the native G5 tracker (g5.issue.native)
// =============================================================================
// How tracker users are named (a G5 member by display name, a GitHub
// account by login) and the native tracker's issue key rule. Why an issue
// page write was refused lives in `issueCommentErrors.ts`.
// =============================================================================

import type { ExternalIssueUserDTO } from "./types";

/**
 * A tracker user by name: a G5 member's display name, else the login
 * (`octocat`). `null` without a user.
 */
export function issueUserName(
  user: ExternalIssueUserDTO | null | undefined,
): string | null {
  if (!user) return null;
  return user.displayName?.trim() || user.login;
}

/**
 * A tracker user as a mention: a G5 member's display name, else `@login`.
 * `null` without a user.
 */
export function issueUserLabel(
  user: ExternalIssueUserDTO | null | undefined,
): string | null {
  if (!user) return null;
  return user.displayName?.trim() || `@${user.login}`;
}

/** Most characters a native issue key may have. */
export const NATIVE_ISSUE_KEY_MAX = 10;

/**
 * The suggested native issue key for an organization slug: its letters and
 * digits uppercased, without leading digits, at most 10 characters; `G5`
 * when nothing usable is left.
 */
export function suggestNativeIssueKey(orgSlug: string): string {
  const key = orgSlug
    .toUpperCase()
    .replace(/[^A-Z0-9]/g, "")
    .replace(/^[0-9]+/, "")
    .slice(0, NATIVE_ISSUE_KEY_MAX);
  return key.length >= 2 ? key : "G5";
}

/** The server's key rule: 2–10 of A–Z and 0–9, starting with a letter. */
export function isValidNativeIssueKey(key: string): boolean {
  return /^[A-Z][A-Z0-9]{1,9}$/.test(key);
}
