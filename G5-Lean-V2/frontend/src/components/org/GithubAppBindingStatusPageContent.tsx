/**
 * GitHub App binding — organization-admin center-panel tab
 * (`?view=github-app`).
 *
 * Connects the organization to the G5 GitHub App installation, chooses which
 * GitHub repositories it covers, and disconnects it. The binding card shows
 * the latest recorded snapshot, which is not a live GitHub check; only the
 * repository picker and the "Manage in GitHub" link contact GitHub. The
 * browser never receives a credential or callback secret — only
 * server-built GitHub URLs.
 */

import { useMemo, useState } from "react";
import { useParams, useSearchParams } from "react-router-dom";
import {
  CircleAlert,
  CircleCheck,
  CircleX,
  ExternalLink,
  GitBranch,
  Github,
  LoaderCircle,
  RefreshCw,
  ShieldCheck,
  Unplug,
} from "lucide-react";

import { Button } from "../ui/UI";
import { useAuth } from "../../hooks/useAuth";
import { useInstanceInfo } from "../../hooks/useSettings";
import { isGithubAppEnabled } from "../../lib/githubAppFlags";
import { isExternalIssuesEnabled } from "../../lib/externalIssuesFlags";
import { IssueDomainSettingsSection } from "../external-issues/IssueDomainSettingsSection";
import { ExternalIssuesSwitch } from "../external-issues/ExternalIssuesSwitch";
import { useExternalIssuesFeature } from "../../hooks/useExternalIssues";
import {
  useDisconnectGithubAppBinding,
  useRefreshGithubAppBinding,
  useGithubAppBindingStatus,
  useGithubAppConnectionAttempt,
  useGithubAppInstallationRepositories,
  useGithubAppManagementUrl,
  useGithubAppReconnectAttempt,
  useUpdateGithubAppCoverage,
} from "../../hooks/useGithubAppBinding";
import type {
  GithubAppBindingDTO,
  GithubAppInstallationRepositoriesDTO,
} from "../../lib/types";

/** Leave the app for a server-built GitHub URL. */
function navigateBrowser(url: string) {
  window.location.href = url;
}

function formatRecordedAt(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;
  return date.toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
}

function issuesPermissionLabel(permissions: Record<string, string>): string {
  switch (permissions.issues) {
    case "write":
      return "Read and write";
    case "read":
      return "Read only";
    default:
      return "Not granted";
  }
}

function BindingCard({ binding }: { binding: GithubAppBindingDTO }) {
  const isActive = binding.disposition === "active";
  const StatusIcon = isActive ? CircleCheck : CircleX;

  return (
    <div
      data-testid="github-app-binding-card"
      className="rounded-xl border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] p-5"
    >
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <h2 className="text-[15px] font-semibold text-[var(--g5-text0)]">
            Latest recorded binding
          </h2>
          <p className="mt-1 text-xs text-[var(--g5-text2)]">
            Version {binding.version} · recorded{" "}
            {formatRecordedAt(binding.recordedAt)}
          </p>
        </div>
        <span
          data-testid="github-app-binding-disposition"
          className={`inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-[11px] font-semibold ${
            isActive
              ? "border-[rgb(113,201,136,0.35)] bg-[rgb(113,201,136,0.10)] text-[var(--g5-ok)]"
              : "border-[rgb(255,107,107,0.35)] bg-[rgb(255,107,107,0.10)] text-[var(--g5-blocked)]"
          }`}
        >
          <StatusIcon className="h-3.5 w-3.5" />
          {isActive ? "Connected" : "Disconnected"}
        </span>
      </div>

      <dl className="mt-5 grid gap-3 border-y border-[var(--g5-stroke)] py-4 text-sm sm:grid-cols-3">
        <div>
          <dt className="text-[11px] font-medium uppercase tracking-wide text-[var(--g5-text2)]">
            GitHub account
          </dt>
          <dd className="mt-1 font-medium text-[var(--g5-text0)]">
            @{binding.accountLogin}
          </dd>
        </div>
        <div>
          <dt className="text-[11px] font-medium uppercase tracking-wide text-[var(--g5-text2)]">
            Installation
          </dt>
          <dd
            data-testid="github-app-binding-installation"
            className="mt-1 font-mono text-xs text-[var(--g5-text0)]"
          >
            {binding.installationId}
          </dd>
        </div>
        <div>
          <dt className="text-[11px] font-medium uppercase tracking-wide text-[var(--g5-text2)]">
            Issues permission
          </dt>
          <dd
            data-testid="github-app-binding-issues-permission"
            className="mt-1 text-[var(--g5-text0)]"
          >
            {issuesPermissionLabel(binding.permissions)}
          </dd>
        </div>
      </dl>

      <section
        className="mt-5"
        aria-labelledby="github-app-covered-repositories"
      >
        <div className="flex items-center gap-2">
          <GitBranch className="h-4 w-4 text-[var(--g5-text2)]" />
          <h3
            id="github-app-covered-repositories"
            className="text-[13px] font-semibold text-[var(--g5-text0)]"
          >
            Covered repositories
          </h3>
          <span className="text-[11px] text-[var(--g5-text2)]">
            {binding.coveredRepositories.length}
          </span>
        </div>
        {binding.coveredRepositories.length === 0 ? (
          <p
            data-testid="github-app-binding-no-repositories"
            className="mt-2 text-xs text-[var(--g5-text2)]"
          >
            G5 does not use this installation for any repository yet.
          </p>
        ) : (
          <ul className="mt-3 grid gap-2 sm:grid-cols-2">
            {binding.coveredRepositories.map((repository) => (
              <li
                key={repository.githubRepoId}
                className="truncate rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-3 py-2 text-xs font-medium text-[var(--g5-text0)]"
              >
                {repository.fullName}
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}

function RepositoryCoverageManager({
  inventory,
  selected,
  onToggle,
  onSave,
  isSaving,
  canSave,
}: {
  inventory: GithubAppInstallationRepositoriesDTO;
  selected: Set<number>;
  onToggle: (githubRepoId: number) => void;
  onSave: () => void;
  isSaving: boolean;
  canSave: boolean;
}) {
  return (
    <div data-testid="github-app-coverage-manager" className="mt-4">
      {inventory.repositories.length === 0 ? (
        <p className="rounded-lg border border-dashed border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-3 py-3 text-xs text-[var(--g5-text2)]">
          GitHub reports no repositories for this installation. Grant it
          repository access in GitHub first.
        </p>
      ) : (
        <fieldset className="grid gap-2 sm:grid-cols-2">
          <legend className="sr-only">
            Repositories the installation can access
          </legend>
          {inventory.repositories.map((repository) => {
            const checked = selected.has(repository.githubRepoId);
            return (
              <label
                key={repository.githubRepoId}
                className={`flex cursor-pointer items-center gap-3 rounded-lg border px-3 py-2.5 text-sm transition-colors ${
                  checked
                    ? "border-[rgb(103,164,255,0.45)] bg-[rgb(103,164,255,0.08)]"
                    : "border-[var(--g5-stroke)] bg-[var(--g5-bg0)]"
                }`}
              >
                <input
                  type="checkbox"
                  aria-label={repository.fullName}
                  checked={checked}
                  onChange={() => onToggle(repository.githubRepoId)}
                  className="h-4 w-4 accent-[var(--g5-info)]"
                />
                <span className="min-w-0 truncate font-medium text-[var(--g5-text0)]">
                  {repository.fullName}
                </span>
                {!repository.private && (
                  <span className="ml-auto shrink-0 rounded-full border border-[var(--g5-stroke)] px-1.5 text-[10px] text-[var(--g5-text2)]">
                    Public
                  </span>
                )}
              </label>
            );
          })}
        </fieldset>
      )}
      <div className="mt-4 flex justify-end">
        <Button
          kind="primary"
          onClick={onSave}
          disabled={isSaving || !canSave}
          data-testid="github-app-save-coverage"
        >
          {isSaving && (
            <LoaderCircle className="mr-1.5 h-3.5 w-3.5 animate-spin" />
          )}
          Save repository coverage
        </Button>
      </div>
    </div>
  );
}

const CONNECTION_OUTCOMES: Record<string, { ok: boolean; message: string }> = {
  connected: { ok: true, message: "GitHub App connected." },
  failed: {
    ok: false,
    message:
      "GitHub App connection could not be verified. Start the connection again.",
  },
};

export function GithubAppBindingStatusPageContent() {
  const { org } = useParams<{ org: string }>();
  const [searchParams] = useSearchParams();
  const { user, hasScope } = useAuth();
  const currentOrg = useMemo(
    () => user?.orgs.find((candidate) => candidate.orgSlug === org),
    [org, user],
  );
  const orgId = currentOrg?.orgId ?? "";
  const isActiveOrg = !!currentOrg && currentOrg.orgId === user?.activeOrgId;
  // `hasScope` reflects only the active org; the role is per target org.
  const canView =
    currentOrg?.role === "admin" || (isActiveOrg && hasScope("org:admin"));

  const instanceInfo = useInstanceInfo();
  const featureEnabled = isGithubAppEnabled(
    instanceInfo.status === "ok" ? instanceInfo.data : null,
  );
  const featureUnavailable =
    instanceInfo.status !== "loading" && !featureEnabled;
  // The deployment offers external issues; the organization turns them on.
  const externalIssuesOffered = isExternalIssuesEnabled(
    instanceInfo.status === "ok" ? instanceInfo.data : null,
  );
  const externalIssuesFeature = useExternalIssuesFeature(orgId);
  const externalIssuesEnabled = externalIssuesFeature.status === "enabled";
  // Off for the organization: domains can still be removed.
  const externalIssuesSwitchedOff =
    externalIssuesFeature.status === "disabled" &&
    externalIssuesFeature.switchedOff === true;

  const status = useGithubAppBindingStatus(orgId, canView && featureEnabled);
  const connectionAttempt = useGithubAppConnectionAttempt(orgId);
  const reconnectAttempt = useGithubAppReconnectAttempt(orgId);
  const coverage = useUpdateGithubAppCoverage(orgId);
  const disconnect = useDisconnectGithubAppBinding(orgId);
  const refresh = useRefreshGithubAppBinding(orgId);
  const managementUrl = useGithubAppManagementUrl(orgId);
  const [pickerOpen, setPickerOpen] = useState(false);
  const [selection, setSelection] = useState<{
    bindingVersion: number;
    ids: Set<number>;
  } | null>(null);
  const [confirmDisconnect, setConfirmDisconnect] = useState(false);

  const binding = status.status === "ok" ? status.data.binding : null;
  const isActiveBinding = binding?.disposition === "active";
  const inventory = useGithubAppInstallationRepositories(
    orgId,
    canView && pickerOpen && isActiveBinding,
  );

  const coveredIds = useMemo(
    () =>
      new Set(
        inventory.status === "ok"
          ? inventory.data.repositories
              .filter((repository) => repository.covered)
              .map((repository) => repository.githubRepoId)
          : [],
      ),
    [inventory],
  );
  const selectedIds =
    inventory.status === "ok" &&
    selection?.bindingVersion === inventory.data.bindingVersion
      ? selection.ids
      : coveredIds;
  const hasChanges =
    selectedIds.size !== coveredIds.size ||
    [...selectedIds].some((id) => !coveredIds.has(id));

  const outcome =
    CONNECTION_OUTCOMES[searchParams.get("githubAppConnection") ?? ""];
  const actionError =
    connectionAttempt.error ||
    reconnectAttempt.error ||
    coverage.error ||
    disconnect.error ||
    refresh.error ||
    managementUrl.error ||
    (inventory.status === "error" ? inventory.error : null);

  const navigateTo = async (
    request: () => Promise<{ redirectUrl: string }>,
  ) => {
    try {
      const { redirectUrl } = await request();
      // The server built this GitHub URL; the client only follows it.
      navigateBrowser(redirectUrl);
    } catch {
      // The generic error notice below is the only feedback.
    }
  };

  const toggleRepository = (githubRepoId: number) => {
    if (inventory.status !== "ok") return;
    const ids = new Set(selectedIds);
    if (ids.has(githubRepoId)) ids.delete(githubRepoId);
    else ids.add(githubRepoId);
    setSelection({ bindingVersion: inventory.data.bindingVersion, ids });
  };

  const saveCoverage = async () => {
    if (inventory.status !== "ok") return;
    try {
      await coverage.mutateAsync({
        bindingVersion: inventory.data.bindingVersion,
        githubRepoIds: [...selectedIds].sort((a, b) => a - b),
      });
      setSelection(null);
      setPickerOpen(false);
    } catch {
      // Keep the selection so the admin can retry after reloading.
    }
  };

  const confirmDisconnectBinding = async () => {
    if (!binding) return;
    try {
      await disconnect.mutateAsync({ bindingVersion: binding.version });
      setConfirmDisconnect(false);
      setPickerOpen(false);
      setSelection(null);
    } catch {
      // The generic error notice below is the only feedback.
    }
  };

  const refreshFromGithub = async () => {
    if (!binding) return;
    try {
      await refresh.mutateAsync({ bindingVersion: binding.version });
    } catch {
      // The generic error notice below is the only feedback.
    }
  };

  const openInGithub = async () => {
    try {
      const { managementUrl: url } = await managementUrl.mutateAsync();
      window.open(url, "_blank", "noopener,noreferrer");
    } catch {
      // The generic error notice below is the only feedback.
    }
  };

  if (canView && featureUnavailable) {
    return (
      <div
        data-testid="github-app-binding-unavailable"
        className="mx-auto w-full max-w-4xl px-6 py-8"
      >
        <div className="rounded-xl border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-5 py-6">
          <h1 className="text-lg font-semibold text-[var(--g5-text0)]">
            GitHub App
          </h1>
          <p className="mt-2 text-sm text-[var(--g5-text1)]">
            The GitHub App is not configured for this G5 deployment.
          </p>
        </div>
      </div>
    );
  }

  if (!canView) {
    return (
      <div
        data-testid="github-app-binding-access-denied"
        className="mx-auto w-full max-w-4xl px-6 py-8"
      >
        <div className="rounded-xl border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-5 py-6">
          <h1 className="text-lg font-semibold text-[var(--g5-text0)]">
            GitHub App
          </h1>
          <p className="mt-2 text-sm text-[var(--g5-text1)]">
            Only organization admins can configure the GitHub App.
          </p>
        </div>
      </div>
    );
  }

  const connectButtons = (primary: "install" | "reconnect") => (
    <div className="flex flex-wrap gap-3">
      <Button
        kind={primary === "install" ? "primary" : "secondary"}
        onClick={() => void navigateTo(connectionAttempt.mutateAsync)}
        disabled={connectionAttempt.isLoading}
        data-testid="github-app-install"
      >
        {connectionAttempt.isLoading && (
          <LoaderCircle className="mr-1.5 h-3.5 w-3.5 animate-spin" />
        )}
        Install GitHub App
      </Button>
      <Button
        kind={primary === "reconnect" ? "primary" : "secondary"}
        onClick={() => void navigateTo(reconnectAttempt.mutateAsync)}
        disabled={reconnectAttempt.isLoading}
        data-testid="github-app-reconnect"
      >
        {reconnectAttempt.isLoading && (
          <LoaderCircle className="mr-1.5 h-3.5 w-3.5 animate-spin" />
        )}
        Connect existing installation
      </Button>
    </div>
  );

  return (
    <div
      data-testid="github-app-binding-page"
      className="mx-auto w-full max-w-4xl px-6 py-8"
    >
      <div className="mb-5 flex items-start gap-3">
        <span className="mt-0.5 rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] p-2 text-[var(--g5-text0)]">
          <Github className="h-5 w-5" />
        </span>
        <div>
          <h1 className="text-xl font-semibold text-[var(--g5-text0)]">
            GitHub App
          </h1>
          <p className="mt-1 max-w-2xl text-[13px] leading-relaxed text-[var(--g5-text1)]">
            Connect this organization to the G5 GitHub App and choose which
            GitHub repositories G5 may use it for.
          </p>
        </div>
      </div>

      {outcome && (
        <div
          data-testid="github-app-connection-outcome"
          className={`mb-5 flex items-center gap-2 rounded-lg border px-3.5 py-3 text-sm text-[var(--g5-text1)] ${
            outcome.ok
              ? "border-[rgb(113,201,136,0.35)] bg-[rgb(113,201,136,0.08)]"
              : "border-[rgb(255,107,107,0.3)] bg-[rgb(255,107,107,0.08)]"
          }`}
        >
          {outcome.ok ? (
            <CircleCheck className="h-4 w-4 shrink-0 text-[var(--g5-ok)]" />
          ) : (
            <CircleAlert className="h-4 w-4 shrink-0 text-[var(--g5-blocked)]" />
          )}
          {outcome.message}
        </div>
      )}

      <div
        data-testid="github-app-binding-notice"
        className="mb-5 flex items-start gap-2.5 rounded-lg border border-[rgb(103,164,255,0.30)] bg-[rgb(103,164,255,0.08)] px-3.5 py-3 text-xs leading-relaxed text-[var(--g5-text1)]"
      >
        <ShieldCheck className="mt-0.5 h-4 w-4 shrink-0 text-[var(--g5-info)]" />
        <p>
          <span className="font-semibold text-[var(--g5-text0)]">
            Recorded binding — not live verified.
          </span>{" "}
          The binding card shows G5's saved configuration. Choosing repositories
          asks GitHub which repositories the installation can access.
        </p>
      </div>

      {actionError && (
        <div
          data-testid="github-app-binding-action-error"
          className="mb-5 flex items-center gap-2 rounded-lg border border-[rgb(255,107,107,0.3)] bg-[rgb(255,107,107,0.08)] px-3.5 py-3 text-sm text-[var(--g5-text1)]"
        >
          <CircleAlert className="h-4 w-4 shrink-0 text-[var(--g5-blocked)]" />
          The GitHub App request could not be completed. Reload and try again.
        </div>
      )}

      {/* The organization's own switch, whatever the binding's state. */}
      {externalIssuesOffered && (
        <div className="mb-5">
          <ExternalIssuesSwitch orgId={orgId} />
        </div>
      )}

      {status.status === "loading" && (
        <p
          data-testid="github-app-binding-loading"
          className="text-sm text-[var(--g5-text2)]"
        >
          Loading GitHub App binding…
        </p>
      )}
      {status.status === "error" && (
        <div
          data-testid="github-app-binding-error"
          className="flex items-center gap-2 rounded-lg border border-[rgb(255,107,107,0.3)] bg-[rgb(255,107,107,0.08)] px-3.5 py-3 text-sm text-[var(--g5-text1)]"
        >
          <CircleAlert className="h-4 w-4 shrink-0 text-[var(--g5-blocked)]" />
          Failed to load the GitHub App binding.
        </div>
      )}

      {status.status === "ok" && binding === null && (
        <div
          data-testid="github-app-binding-empty"
          className="rounded-xl border border-dashed border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-6 py-10 text-center"
        >
          <Github className="mx-auto h-6 w-6 text-[var(--g5-text2)]" />
          <h2 className="mt-3 text-[15px] font-semibold text-[var(--g5-text0)]">
            GitHub App not connected
          </h2>
          <p className="mx-auto mt-2 max-w-lg text-xs leading-relaxed text-[var(--g5-text2)]">
            Install the App on your GitHub organization, or connect an
            installation that already exists. GitHub confirms that you can
            access the installation before G5 records it.
          </p>
          <div className="mt-5 flex justify-center">
            {connectButtons("install")}
          </div>
        </div>
      )}

      {status.status === "ok" && binding !== null && (
        <>
          <BindingCard binding={binding} />

          {isActiveBinding ? (
            <section className="mt-5 rounded-xl border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] p-5">
              <div className="flex flex-wrap items-start justify-between gap-3">
                <div>
                  <h2 className="text-[15px] font-semibold text-[var(--g5-text0)]">
                    Repository coverage
                  </h2>
                  <p className="mt-1 max-w-2xl text-xs leading-relaxed text-[var(--g5-text2)]">
                    Choose the repositories G5 may use this installation for.
                    This changes G5's record only; GitHub access is unchanged.
                  </p>
                </div>
                {!pickerOpen && (
                  <Button
                    kind="secondary"
                    onClick={() => setPickerOpen(true)}
                    data-testid="github-app-choose-repositories"
                  >
                    <RefreshCw className="mr-1.5 h-3.5 w-3.5" />
                    Choose repositories
                  </Button>
                )}
              </div>
              {pickerOpen && inventory.status === "loading" && (
                <p className="mt-4 inline-flex items-center gap-2 text-xs text-[var(--g5-text2)]">
                  <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
                  Loading repositories from GitHub…
                </p>
              )}
              {pickerOpen && inventory.status === "ok" && (
                <RepositoryCoverageManager
                  inventory={inventory.data}
                  selected={selectedIds}
                  onToggle={toggleRepository}
                  onSave={() => void saveCoverage()}
                  isSaving={coverage.isLoading}
                  canSave={hasChanges}
                />
              )}
            </section>
          ) : (
            <div className="mt-5 rounded-xl border border-dashed border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-5 py-4">
              <p className="text-sm font-medium text-[var(--g5-text0)]">
                This binding is disconnected.
              </p>
              <p className="mb-4 mt-1 text-xs text-[var(--g5-text2)]">
                G5 no longer uses this installation. Connect the existing
                installation again, or install the App on another account.
              </p>
              {connectButtons("reconnect")}
            </div>
          )}

          {(externalIssuesEnabled || externalIssuesSwitchedOff) && (
            <IssueDomainSettingsSection
              orgId={orgId}
              orgSlug={currentOrg?.orgSlug ?? ""}
              binding={binding}
              removeOnly={!externalIssuesEnabled}
            />
          )}

          <section className="mt-5 rounded-xl border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-5 py-4">
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div>
                <h2 className="text-sm font-semibold text-[var(--g5-text0)]">
                  Manage the installation in GitHub
                </h2>
                <p className="mt-1 max-w-2xl text-xs text-[var(--g5-text2)]">
                  Change repository access or uninstall the App on GitHub.
                  Uninstalling there does not disconnect it here. After
                  accepting new permissions on GitHub, refresh so G5 records
                  them.
                </p>
              </div>
              <div className="flex items-center gap-2">
                {isActiveBinding && (
                  <Button
                    kind="secondary"
                    onClick={() => void refreshFromGithub()}
                    disabled={refresh.isLoading}
                    data-testid="github-app-refresh"
                  >
                    {refresh.isLoading ? (
                      <LoaderCircle className="mr-1.5 h-3.5 w-3.5 animate-spin" />
                    ) : (
                      <RefreshCw className="mr-1.5 h-3.5 w-3.5" />
                    )}
                    Refresh from GitHub
                  </Button>
                )}
                <Button
                  kind="secondary"
                  onClick={() => void openInGithub()}
                  disabled={managementUrl.isLoading}
                  data-testid="github-app-manage"
                >
                  {managementUrl.isLoading ? (
                    <LoaderCircle className="mr-1.5 h-3.5 w-3.5 animate-spin" />
                  ) : (
                    <ExternalLink className="mr-1.5 h-3.5 w-3.5" />
                  )}
                  Manage in GitHub
                </Button>
              </div>
            </div>
          </section>

          {isActiveBinding && (
            <section className="mt-5 rounded-xl border border-[rgb(255,107,107,0.22)] bg-[rgb(255,107,107,0.04)] px-5 py-4">
              {!confirmDisconnect ? (
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <div>
                    <h2 className="text-sm font-semibold text-[var(--g5-text0)]">
                      Disconnect the GitHub App
                    </h2>
                    <p className="mt-1 text-xs text-[var(--g5-text2)]">
                      G5 stops using this installation for every repository. Its
                      history is kept.
                    </p>
                  </div>
                  <Button
                    kind="secondary"
                    onClick={() => setConfirmDisconnect(true)}
                    data-testid="github-app-disconnect"
                  >
                    <Unplug className="mr-1.5 h-3.5 w-3.5" />
                    Disconnect
                  </Button>
                </div>
              ) : (
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <p className="text-sm text-[var(--g5-text1)]">
                    Stop using this GitHub App installation for all
                    repositories?
                  </p>
                  <div className="flex items-center gap-2">
                    <Button
                      kind="secondary"
                      onClick={() => setConfirmDisconnect(false)}
                    >
                      Cancel
                    </Button>
                    <Button
                      kind="primary"
                      onClick={() => void confirmDisconnectBinding()}
                      disabled={disconnect.isLoading}
                      data-testid="github-app-confirm-disconnect"
                    >
                      {disconnect.isLoading && (
                        <LoaderCircle className="mr-1.5 h-3.5 w-3.5 animate-spin" />
                      )}
                      Confirm disconnect
                    </Button>
                  </div>
                </div>
              )}
            </section>
          )}
        </>
      )}
    </div>
  );
}
