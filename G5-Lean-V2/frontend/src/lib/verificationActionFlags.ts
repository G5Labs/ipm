/**
 * Organization switch for the workbench's Re-run Verification action and
 * Home's Base re-verification (ui g5.workbench.review-prompts.switch).
 *
 * Like the external-checks switch this is the organization's own Runtime
 * Config setting, set only by an org admin. Off unless exactly
 * `true`, so the action stays greyed until an administrator turns it on.
 */
export const VERIFICATION_ACTION_ENABLED_SETTING =
  "verification_action_enabled";

export function isOrgVerificationActionEnabled(
  orgConfig: { settings?: Record<string, unknown> | null } | null | undefined,
): boolean {
  return orgConfig?.settings?.[VERIFICATION_ACTION_ENABLED_SETTING] === true;
}

/** Why Re-run Verification is greyed while the switch is off. */
export const VERIFICATION_ACTION_OFF_TITLE =
  "Your organization admin has disabled this feature. An admin can turn it on in Admin → Workbench actions.";
