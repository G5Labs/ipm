/** Instance feature g5-api advertises when the G5 GitHub App is configured. */
export const GITHUB_APP_FEATURE = "github_app";

/**
 * Whether the organization GitHub App binding surface is available.
 * Fail-closed: unloaded or malformed instance info means hidden.
 */
export function isGithubAppEnabled(
  instanceInfo: { features?: readonly string[] | null } | null | undefined,
): boolean {
  const features = instanceInfo?.features;
  return Array.isArray(features) && features.includes(GITHUB_APP_FEATURE);
}
