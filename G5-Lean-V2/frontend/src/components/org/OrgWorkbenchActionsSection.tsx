/**
 * OrgWorkbenchActionsSection — the organization's workbench action switches
 * on the Admin page (ui g5.workbench.review-prompts).
 *
 * Holds the Re-run Verification switch: off until an org admin turns
 * it on, and while off every workspace in the organization shows the action
 * greyed with the reason. The switch is the organization's own Runtime Config
 * setting; the API refuses anyone else's change and any narrower scope.
 *
 * @module org/OrgWorkbenchActionsSection
 */

import { useId } from "react";

import {
  useOrgScopedConfig,
  useUpdateOrgScopedConfig,
} from "../../hooks/useSettings";
import {
  VERIFICATION_ACTION_ENABLED_SETTING,
  isOrgVerificationActionEnabled,
} from "../../lib/verificationActionFlags";
import { SettingsRow, SettingsSection } from "../settings/SettingsPageShell";

/** Mounted only after the Admin page's organization-role gate. */
export function OrgWorkbenchActionsSection({ orgId }: { orgId: string }) {
  const switchId = useId();
  const config = useOrgScopedConfig(orgId);
  const update = useUpdateOrgScopedConfig(orgId);
  const enabled =
    config.status === "ok" && isOrgVerificationActionEnabled(config.data);

  // A section of the Admin sheet (ui g5.ui.settings-page).
  return (
    <SettingsSection
      id="workbench"
      title="Workbench actions"
      description="Workbench actions that only an administrator can offer to this organization's workspaces."
      testId="org-admin-workbench-section"
    >
      <SettingsRow
        title="Re-run Verification"
        description="Integration Engineering's Re-run Verification seeds a chat that builds or updates formal state machines and runs formal verification for the workspace's pull request, and Home offers the same for Base as a whole. Both are very expensive. While off, workspaces show the action greyed and Home shows no Base re-verification."
        testId="org-admin-verification-action-row"
      >
        <label
          htmlFor={switchId}
          className="inline-flex cursor-pointer items-center gap-2 text-sm text-[var(--g5-text1)]"
        >
          <input
            id={switchId}
            type="checkbox"
            role="switch"
            data-testid="org-admin-verification-action-switch"
            checked={enabled}
            disabled={config.status !== "ok" || update.isLoading}
            onChange={(event) =>
              update.mutate({
                settings: {
                  [VERIFICATION_ACTION_ENABLED_SETTING]: event.target.checked,
                },
              })
            }
            className="h-4 w-4 accent-[var(--g5-info)]"
          />
          {enabled ? "On" : "Off"}
        </label>
        {(config.status === "error" || update.error) && (
          <p
            role="alert"
            data-testid="org-admin-verification-action-error"
            className="mt-2 text-xs text-[var(--g5-blocked)]"
          >
            {update.error?.message ||
              "The organization setting could not be loaded."}
          </p>
        )}
      </SettingsRow>
    </SettingsSection>
  );
}
