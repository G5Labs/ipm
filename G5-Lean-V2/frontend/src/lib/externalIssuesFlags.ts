/**
 * Instance feature g5-api always advertises: issue surfaces (the native G5
 * tracker, plus GitHub repositories where the App is configured).
 */
export const ISSUES_FEATURE = "issues";

/**
 * Instance feature g5-api advertises when external GitHub issues are
 * available (the G5 GitHub App is configured). Only GitHub-specific
 * affordances (adding GitHub repositories, Connect GitHub) depend on it.
 */
export const EXTERNAL_ISSUES_FEATURE = "external_issues";

function advertises(
  instanceInfo: { features?: readonly string[] | null } | null | undefined,
  feature: string,
): boolean {
  const features = instanceInfo?.features;
  return Array.isArray(features) && features.includes(feature);
}

/**
 * Whether issue domains, search, workspace links and the issue view are
 * available. A server from before the native tracker advertises only
 * `external_issues`, which counts too. Fail-closed: unloaded or malformed
 * instance info means hidden.
 */
export function isExternalIssuesEnabled(
  instanceInfo: { features?: readonly string[] | null } | null | undefined,
): boolean {
  return (
    advertises(instanceInfo, ISSUES_FEATURE) ||
    advertises(instanceInfo, EXTERNAL_ISSUES_FEATURE)
  );
}

/** Whether GitHub repositories can be issue domains on this deployment. */
export function isGithubIssuesEnabled(
  instanceInfo: { features?: readonly string[] | null } | null | undefined,
): boolean {
  return advertises(instanceInfo, EXTERNAL_ISSUES_FEATURE);
}

/**
 * The organization's own Issues switch (Runtime Config, set by an Admin).
 * The stored key predates the native tracker. Only the
 * organization scope counts; on unless exactly `false`, so an organization
 * that never touched the switch tracks issues with G5 issues from the start
 * (issues g5.issue.native.domain).
 */
export const EXTERNAL_ISSUES_ENABLED_SETTING = "external_issues_enabled";

export function isOrgExternalIssuesEnabled(
  orgConfig: { settings?: Record<string, unknown> | null } | null | undefined,
): boolean {
  return orgConfig?.settings?.[EXTERNAL_ISSUES_ENABLED_SETTING] !== false;
}
