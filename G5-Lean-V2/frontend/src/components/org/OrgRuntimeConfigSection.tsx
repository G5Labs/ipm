import {
  useOrgScopedConfig,
  useProviderAccountsEnabled,
  useUpdateOrgScopedConfig,
} from "../../hooks/useSettings";
import { RuntimeEnvConfig } from "../settings/RuntimeEnvConfig";
import { ProviderAccountsSection } from "../settings/ProviderAccountsSection";
import { OrgApiKeyManager } from "../settings/OrgApiKeyManager";
import { SettingsSection } from "../settings/SettingsPageShell";

/** Mounted only after the Admin page's organization-role gate. */
export function OrgRuntimeConfigSection({
  orgId,
  orgName,
}: {
  orgId: string;
  orgName: string;
}) {
  const config = useOrgScopedConfig(orgId);
  const updateConfig = useUpdateOrgScopedConfig(orgId);
  // Provider-accounts kill-switch: the Accounts section exists only while
  // the `provider_accounts` instance feature is advertised.
  const providerAccountsEnabled = useProviderAccountsEnabled();

  // Three sections of the Admin sheet (ui g5.ui.settings-page); the page
  // lists them in its index under the same ids.
  return (
    <div data-testid="org-admin-runtime-config">
      <SettingsSection
        id="defaults"
        title="Defaults"
        description={`What every workspace and member of ${orgName} starts with: the default agent, each agent's model and account, and custom variables. The first shared account becomes the default for its agent; anyone can override these for a workspace or for themselves.`}
      >
        <RuntimeEnvConfig
          context="organization"
          orgId={orgId}
          org={config}
          updateOrg={updateConfig.mutateAsync}
          savingOrg={updateConfig.isLoading}
          canEditOrg
          canEditWorkspace={false}
        />
      </SettingsSection>
      {providerAccountsEnabled && (
        <SettingsSection
          id="accounts"
          title="Accounts"
          description={`Subscriptions and API keys shared by everyone in ${orgName}.`}
        >
          <ProviderAccountsSection
            context="organization"
            orgId={orgId}
            canEditOrg
            canEditWorkspace={false}
          />
        </SettingsSection>
      )}
      <SettingsSection
        id="api-keys"
        title="API keys"
        description={`Keys that let tools act for ${orgName}.`}
      >
        <OrgApiKeyManager orgId={orgId} orgName={orgName} canManage />
      </SettingsSection>
    </div>
  );
}
