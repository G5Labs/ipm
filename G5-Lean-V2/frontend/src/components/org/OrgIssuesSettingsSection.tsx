/**
 * OrgIssuesSettingsSection — the organization's Issues settings on the Admin
 * page (g5.issue.native.domain).
 *
 * Holds the organization's Issues switch and every issue domain. Issues are
 * on and the native tracker is the organization's domain from the start;
 * "Use G5 issues" adds the native tracker again after a removal, and GitHub
 * repositories can be added when the deployment has the GitHub App (the
 * covered repositories come from the organization's binding). None of it
 * needs the GitHub App. While the organization has issues off, existing
 * domains stay listed for removal.
 *
 * @module org/OrgIssuesSettingsSection
 */

import { useExternalIssuesFeature } from "../../hooks/useExternalIssues";
import { useGithubAppBindingStatus } from "../../hooks/useGithubAppBinding";
import { useInstanceInfo } from "../../hooks/useSettings";
import {
  isExternalIssuesEnabled,
  isGithubIssuesEnabled,
} from "../../lib/externalIssuesFlags";
import { isGithubAppEnabled } from "../../lib/githubAppFlags";
import { ExternalIssuesSwitch } from "../external-issues/ExternalIssuesSwitch";
import { IssueDomainSettingsSection } from "../external-issues/IssueDomainSettingsSection";
import { SettingsSection } from "../settings/SettingsPageShell";

export function OrgIssuesSettingsSection({
  orgId,
  orgSlug,
}: {
  orgId: string;
  orgSlug: string;
}) {
  const instanceInfo = useInstanceInfo();
  const info = instanceInfo.status === "ok" ? instanceInfo.data : null;
  const offered = isExternalIssuesEnabled(info);
  // GitHub repositories need the App and the deployment's GitHub issues.
  const githubRepositories =
    isGithubAppEnabled(info) && isGithubIssuesEnabled(info);
  const feature = useExternalIssuesFeature(orgId);
  const enabled = feature.status === "enabled";
  const switchedOff =
    feature.status === "disabled" && feature.switchedOff === true;
  const bindingStatus = useGithubAppBindingStatus(
    orgId,
    githubRepositories && enabled,
  );
  const binding =
    bindingStatus.status === "ok" ? bindingStatus.data.binding : null;

  if (!offered) return null;
  // A section of the Admin sheet (ui g5.ui.settings-page); the page lists it
  // in its index only while offered, by the same gate.
  return (
    <SettingsSection
      id="issues"
      title="Issues"
      description="The issue tracker this organization works from."
      testId="org-admin-issues-section"
    >
      <ExternalIssuesSwitch orgId={orgId} />
      {(enabled || switchedOff) && (
        <IssueDomainSettingsSection
          orgId={orgId}
          orgSlug={orgSlug}
          binding={binding}
          githubRepositories={githubRepositories}
          removeOnly={!enabled}
        />
      )}
    </SettingsSection>
  );
}
