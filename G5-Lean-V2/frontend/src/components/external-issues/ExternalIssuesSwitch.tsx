// =============================================================================
// Organization switch for issues
// =============================================================================
// Issues (G5 issues and GitHub repositories, search, workspace links,
// Activity discussion and comments, the agent's `g5d issue` commands) are on
// for an organization until an admin turns them off here
// (g5.issue.native.domain). The switch is the organization's own Runtime
// Config setting (its stored key predates the native tracker).
// =============================================================================

import { useId } from "react";

import {
  useOrgScopedConfig,
  useUpdateOrgScopedConfig,
} from "../../hooks/useSettings";
import {
  EXTERNAL_ISSUES_ENABLED_SETTING,
  isOrgExternalIssuesEnabled,
} from "../../lib/externalIssuesFlags";

export function ExternalIssuesSwitch({ orgId }: { orgId: string }) {
  const switchId = useId();
  const config = useOrgScopedConfig(orgId);
  const update = useUpdateOrgScopedConfig(orgId);
  const enabled =
    config.status === "ok" && isOrgExternalIssuesEnabled(config.data);

  return (
    <section
      data-testid="external-issues-switch-section"
      className="mt-5 rounded-xl border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-5 py-4"
    >
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-sm font-semibold text-[var(--g5-text0)]">
            Issues
          </h2>
          <p className="mt-1 max-w-2xl text-xs text-[var(--g5-text2)]">
            Search, link and discuss issues in this organization&apos;s
            workspaces, and let workspace agents read and comment on them.
          </p>
        </div>
        <label
          htmlFor={switchId}
          className="inline-flex cursor-pointer items-center gap-2 text-sm text-[var(--g5-text1)]"
        >
          <input
            id={switchId}
            type="checkbox"
            role="switch"
            data-testid="external-issues-switch"
            checked={enabled}
            disabled={config.status !== "ok" || update.isLoading}
            onChange={(event) =>
              update.mutate({
                settings: {
                  [EXTERNAL_ISSUES_ENABLED_SETTING]: event.target.checked,
                },
              })
            }
            className="h-4 w-4 accent-[var(--g5-info)]"
          />
          {enabled ? "On" : "Off"}
        </label>
      </div>
      {(config.status === "error" || update.error) && (
        <p
          role="alert"
          data-testid="external-issues-switch-error"
          className="mt-2 text-xs text-[var(--g5-blocked)]"
        >
          {update.error?.message ||
            "The organization setting could not be loaded."}
        </p>
      )}
    </section>
  );
}
