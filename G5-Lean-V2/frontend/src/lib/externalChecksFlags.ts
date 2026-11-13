/**
 * Organization merge policy: whether a workspace pull request's external
 * provider checks must be passing before it can merge
 * (governance g5.governance.merge.external-policy).
 *
 * Like the external-issues switch this is the organization's own Runtime Config
 * setting, set only by an org admin. Off unless exactly `true`.
 */
export const EXTERNAL_CHECKS_BLOCK_MERGE_SETTING =
  "external_checks_block_merge";

export function isOrgExternalChecksBlockMerge(
  orgConfig: { settings?: Record<string, unknown> | null } | null | undefined,
): boolean {
  return orgConfig?.settings?.[EXTERNAL_CHECKS_BLOCK_MERGE_SETTING] === true;
}
