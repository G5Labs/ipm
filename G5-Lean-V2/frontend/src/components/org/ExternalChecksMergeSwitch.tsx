// =============================================================================
// Organization switch — block merge on failing/pending external PR checks
// =============================================================================
// When on, a workspace pull request cannot merge while any external provider
// check (commit status or check run) has failed or was cancelled. A check the
// base branch requires that is pending or not yet reported blocks too, so a
// just-opened PR cannot merge before its required checks report; other pending
// checks do not block
// (governance g5.governance.merge.external-policy). Off by default. The switch
// is the organization's own Runtime Config setting; only an org admin
// may change it (the API enforces this regardless of the disabled state here).
// =============================================================================

import { useId } from "react";

import {
  useOrgScopedConfig,
  useUpdateOrgScopedConfig,
} from "../../hooks/useSettings";
import {
  EXTERNAL_CHECKS_BLOCK_MERGE_SETTING,
  isOrgExternalChecksBlockMerge,
} from "../../lib/externalChecksFlags";

export function ExternalChecksMergeSwitch({
  orgId,
  canEdit,
}: {
  orgId: string;
  canEdit: boolean;
}) {
  const switchId = useId();
  const config = useOrgScopedConfig(orgId);
  const update = useUpdateOrgScopedConfig(orgId);
  const enabled =
    config.status === "ok" && isOrgExternalChecksBlockMerge(config.data);

  return (
    <section
      data-testid="external-checks-merge-switch-section"
      className="mb-5 rounded-xl border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-5 py-4"
    >
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-sm font-semibold text-[var(--g5-text0)]">
            Require passing external checks to merge
          </h2>
          <p className="mt-1 max-w-2xl text-xs text-[var(--g5-text2)]">
            Block merging a pull request while any external provider check
            (commit status or check run) has failed or was cancelled. Checks the
            base branch requires on GitHub also block while pending or not yet
            reported, so a newly opened PR cannot merge before they report;
            other pending checks do not block.
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
            data-testid="external-checks-merge-switch"
            checked={enabled}
            disabled={config.status !== "ok" || update.isLoading || !canEdit}
            onChange={(event) =>
              update.mutate({
                settings: {
                  [EXTERNAL_CHECKS_BLOCK_MERGE_SETTING]: event.target.checked,
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
          data-testid="external-checks-merge-switch-error"
          className="mt-2 text-xs text-[var(--g5-blocked)]"
        >
          {update.error?.message ||
            "The organization setting could not be loaded."}
        </p>
      )}
    </section>
  );
}
