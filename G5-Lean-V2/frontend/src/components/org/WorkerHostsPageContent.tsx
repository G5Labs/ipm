import { useEffect, useMemo, useState } from "react";
import { AlertCircle, Plus, Server, Trash2 } from "lucide-react";
import { Link, useParams } from "react-router-dom";

import { useAuth } from "../../hooks/useAuth";
import { useInstanceInfo } from "../../hooks/useSettings";
import {
  useDeallocateWorkerHost,
  useWorkerHosts,
  useWorkerHostWorkspaces,
} from "../../hooks/useWorkerHosts";
import {
  getWorkspaceUiCapabilities,
  type WorkspaceUiCapabilities,
} from "../../lib/capabilities";
import { ApiError } from "../../lib/api";
import type { WorkerHost } from "../../lib/types";
import { AddWorkerHostDialog } from "./AddWorkerHostDialog";
import { Button, Chip, IconButton } from "../ui/UI";

/**
 * Hello capability of a local process-isolation host
 * (ontology g5.runtime.boot.backend.process-local): its workspaces are host
 * processes without a VM boundary, so operators see that on the row.
 */
const PROCESS_ISOLATION_CAPABILITY = "isolation:process";

/**
 * A host whose published runtime is older than the control plane's takes no
 * new workspace VMs (ontology g5.runtime.placement.runtime-identity). The
 * chip says only that; the owner asked for no further detail on the page.
 */
function runtimeStateChip(host: WorkerHost) {
  if (host.runtimeState !== "stale") return null;
  return (
    <div className="mt-1" data-testid="worker-host-runtime-state">
      <Chip text="stale" state="blocked" />
    </div>
  );
}

function vmCount(count: number): string {
  return `${count} workspace VM${count === 1 ? "" : "s"}`;
}

export interface WorkerHostsPageContentProps {
  capabilities?: WorkspaceUiCapabilities;
}

const ACTIVE_STATUSES = new Set([
  "provisioning",
  "ready",
  "unreachable",
  "draining",
]);

function relativeTime(value: string | null | undefined, now: number): string {
  if (!value) return "Never";
  const elapsed = now - Date.parse(value);
  if (!Number.isFinite(elapsed)) return value;
  const minutes = Math.max(0, Math.floor(elapsed / 60_000));
  if (minutes < 1) return "Just now";
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.floor(hours / 24)}d ago`;
}

function HostId({ id }: { id: string | null }) {
  return id ? (
    <span title={id} className="font-mono whitespace-nowrap">
      {id.length > 8 ? `${id.slice(0, 8)}…` : id}
    </span>
  ) : (
    <span className="text-[var(--g5-text2)]">Unassigned</span>
  );
}

/// The workspace's name, truncated to its column, linking to the workspace.
function WorkspaceName({ name, href }: { name: string; href: string }) {
  return (
    <Link
      to={href}
      data-testid="workspace-name-link"
      className="block w-full truncate text-left hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]"
    >
      {name}
    </Link>
  );
}

function VmTime({ value, now }: { value: string | null; now: number }) {
  return value && Number.isFinite(Date.parse(value)) ? (
    <time dateTime={value} title={new Date(value).toLocaleString()}>
      {relativeTime(value, now)}
    </time>
  ) : (
    <span aria-label="Not available">—</span>
  );
}

export function WorkerHostsPageContent({
  capabilities = getWorkspaceUiCapabilities(),
}: WorkerHostsPageContentProps = {}) {
  const { org } = useParams<{ org: string }>();
  const { user, hasScope } = useAuth();
  const currentOrg = useMemo(
    () => user?.orgs.find((candidate) => candidate.orgSlug === org),
    [org, user],
  );
  const orgId = currentOrg?.orgId ?? "";
  const instanceInfo = useInstanceInfo();
  const isCloud =
    instanceInfo.status === "ok" &&
    instanceInfo.data.selfHosted === false &&
    capabilities.cloudWorkspaceWorkflows;
  const isActiveOrg = currentOrg?.orgId === user?.activeOrgId;
  const canManage =
    currentOrg?.role === "admin" || (isActiveOrg && hasScope("workers:admin"));
  const controlsEnabled = isCloud && canManage;
  const hostsState = useWorkerHosts(orgId, controlsEnabled);
  const workspacesState = useWorkerHostWorkspaces(orgId, controlsEnabled);
  const deallocateHost = useDeallocateWorkerHost(orgId);
  const [addDialogOpen, setAddDialogOpen] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  // A plain removal the backend refused because the host still owns VMs
  // (409). The operator decides here whether to force it.
  const [refusedRemoval, setRefusedRemoval] = useState<{
    host: WorkerHost;
    message: string;
  } | null>(null);
  const [now, setNow] = useState(Date.now);
  useEffect(() => {
    // Unchanged poll responses retain their identity in React Query. Advance
    // relative times even when no VM activity or host state has changed.
    const timer = window.setInterval(() => setNow(Date.now()), 60_000);
    return () => window.clearInterval(timer);
  }, []);

  // The backend refuses to remove a host that owns active VMs unless forced
  // (g5.architecture.host_forced_removal). A row that shows occupancy asks for
  // that consent up front; a refusal of a plain removal offers it afterwards.
  const deallocate = async (host: WorkerHost, force: boolean) => {
    setActionError(null);
    setRefusedRemoval(null);
    try {
      await deallocateHost.mutateAsync({ hostId: host.hostId, force });
    } catch (error) {
      const message =
        error instanceof Error ? error.message : "Failed to remove host";
      if (!force && error instanceof ApiError && error.status === 409) {
        setRefusedRemoval({ host, message });
      } else {
        setActionError(message);
      }
    }
  };

  const removeHost = async (host: WorkerHost) => {
    const active = host.occupancy ?? 0;
    const question =
      active > 0
        ? `Host ${host.hostId} still runs ${vmCount(active)}. Removing it stops ${active === 1 ? "that VM" : "them"}. Remove anyway?`
        : `Remove host ${host.hostId}?`;
    if (!window.confirm(question)) return;
    await deallocate(host, active > 0);
  };

  if (!orgId) {
    return (
      <p className="p-6 text-sm text-[var(--g5-text2)]">
        Organization not found.
      </p>
    );
  }
  if (instanceInfo.status === "loading") {
    return (
      <p className="p-6 text-sm text-[var(--g5-text2)]">
        Loading host controls…
      </p>
    );
  }
  if (!isCloud) {
    return (
      <div className="p-6 text-sm text-[var(--g5-text1)]">
        Cloud workspace hosts are not enabled for this installation.
      </div>
    );
  }
  if (!canManage) {
    return (
      <div className="p-6 text-sm text-[var(--g5-text1)]">
        The <code>workers:admin</code> scope is required to manage hosts.
      </div>
    );
  }

  // The API retains terminated records for audit history. This page shows
  // active hosts only, including for its empty state after a refresh.
  const hosts =
    hostsState.status === "ok"
      ? hostsState.data.hosts.filter((host) => ACTIVE_STATUSES.has(host.status))
      : [];
  const workspaces =
    workspacesState.status === "ok"
      ? workspacesState.data
          .filter((workspace) => workspace.status !== "stopped")
          .sort((a, b) => {
            if (a.hostId !== b.hostId) {
              if (!a.hostId) return 1;
              if (!b.hostId) return -1;
              return a.hostId.localeCompare(b.hostId);
            }
            return (
              a.workspaceName.localeCompare(b.workspaceName) ||
              a.workspaceId.localeCompare(b.workspaceId)
            );
          })
      : [];

  return (
    <div
      className="mx-auto w-full max-w-6xl p-6"
      data-testid="worker-hosts-page-content"
    >
      <div className="mb-6 flex items-start justify-between gap-4">
        <div>
          <h1 className="text-xl font-semibold text-[var(--g5-text0)]">
            Hosts
          </h1>
          <p className="mt-1 text-sm text-[var(--g5-text2)]">
            Hosts run the workspace VMs for this organization.
          </p>
        </div>
        <Button kind="primary" onClick={() => setAddDialogOpen(true)}>
          <Plus className="mr-1 inline h-4 w-4" /> Add host
        </Button>
      </div>

      {actionError && (
        <div
          role="alert"
          className="mb-4 flex items-center gap-2 rounded-md border border-[var(--g5-blocked)] p-3 text-sm text-[var(--g5-blocked)]"
        >
          <AlertCircle className="h-4 w-4" /> {actionError}
        </div>
      )}
      {refusedRemoval && (
        <div
          role="alert"
          data-testid="worker-host-removal-refused"
          className="mb-4 flex flex-wrap items-center gap-3 rounded-md border border-[var(--g5-blocked)] p-3 text-sm text-[var(--g5-blocked)]"
        >
          <AlertCircle className="h-4 w-4 shrink-0" />
          <span className="flex-1">
            {refusedRemoval.message} Removing it anyway stops its workspace VMs.
          </span>
          <Button
            kind="secondary"
            onClick={() => void deallocate(refusedRemoval.host, true)}
            disabled={deallocateHost.isLoading}
          >
            Remove anyway
          </Button>
          <Button kind="ghost" onClick={() => setRefusedRemoval(null)}>
            Keep host
          </Button>
        </div>
      )}
      {hostsState.status === "loading" && (
        <p className="py-8 text-center text-sm text-[var(--g5-text2)]">
          Loading hosts…
        </p>
      )}
      {hostsState.status === "error" && (
        <p
          role="alert"
          className="py-8 text-center text-sm text-[var(--g5-blocked)]"
        >
          Failed to load workspace hosts.
        </p>
      )}
      {hostsState.status === "ok" && hosts.length === 0 && (
        <div className="flex flex-col items-center gap-2 rounded-md border border-[var(--g5-stroke)] py-10 text-center">
          <Server className="h-7 w-7 text-[var(--g5-text2)]" />
          <p className="text-sm text-[var(--g5-text1)]">
            No workspace hosts yet.
          </p>
        </div>
      )}
      {hosts.length > 0 && (
        <div className="overflow-x-auto rounded-md border border-[var(--g5-stroke)]">
          <table aria-label="Hosts" className="w-full border-collapse text-sm">
            <thead className="bg-[var(--g5-bg2)] text-left text-xs uppercase text-[var(--g5-text2)]">
              <tr>
                <th className="px-4 py-3">Status</th>
                <th className="px-4 py-3 whitespace-nowrap">Host ID</th>
                <th className="px-4 py-3">Region</th>
                <th className="px-4 py-3">Instance</th>
                <th className="px-4 py-3">Slots</th>
                <th className="px-4 py-3">Resources per VM</th>
                <th className="px-4 py-3">Heartbeat</th>
                <th className="px-4 py-3">Age</th>
                <th className="px-4 py-3">Actions</th>
              </tr>
            </thead>
            <tbody>
              {hosts.map((host) => (
                <tr
                  key={host.hostId}
                  className="border-t border-[var(--g5-stroke)] bg-[var(--g5-bg1)]"
                  data-testid="worker-host-row"
                  data-host-id={host.hostId}
                >
                  <td className="px-4 py-3">
                    <Chip
                      text={host.status}
                      state={host.status === "ready" ? "ok" : "warn"}
                    />
                    {runtimeStateChip(host)}
                    {host.capabilities?.includes(
                      PROCESS_ISOLATION_CAPABILITY,
                    ) && (
                      <div
                        className="mt-1"
                        title="This host runs workspaces as plain host processes with no VM boundary. Localhost development only."
                      >
                        <Chip text="process (unisolated)" state="warn" />
                      </div>
                    )}
                  </td>
                  <td className="px-4 py-3 text-[var(--g5-text1)]">
                    <HostId id={host.hostId} />
                  </td>
                  <td className="px-4 py-3 text-[var(--g5-text1)]">
                    {host.region}
                  </td>
                  <td className="px-4 py-3 text-[var(--g5-text1)]">
                    {host.instanceType}
                  </td>
                  <td className="px-4 py-3 text-[var(--g5-text1)]">
                    {host.occupancy ?? 0} / {host.slotsTotal ?? 0}
                  </td>
                  <td className="px-4 py-3 whitespace-nowrap text-[var(--g5-text1)]">
                    {host.slotSizeReported && host.slotCpus && host.slotMemMib
                      ? `${host.slotCpus} vCPU / ${(host.slotMemMib / 1024).toFixed(1)} GiB`
                      : "Pending"}
                  </td>
                  <td className="px-4 py-3 text-[var(--g5-text2)]">
                    {relativeTime(host.lastHeartbeat, now)}
                  </td>
                  <td className="px-4 py-3 whitespace-nowrap text-[var(--g5-text2)]">
                    <time
                      dateTime={host.createdAt}
                      title={new Date(host.createdAt).toLocaleString()}
                    >
                      {relativeTime(host.createdAt, now)}
                    </time>
                  </td>
                  <td className="px-4 py-3">
                    <IconButton
                      title="Remove host"
                      onClick={() => void removeHost(host)}
                    >
                      <Trash2 className="h-4 w-4" />
                    </IconButton>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      <section className="mt-10" aria-labelledby="worker-workspaces-heading">
        <h2
          id="worker-workspaces-heading"
          className="text-xl font-semibold text-[var(--g5-text0)]"
        >
          Workspace
        </h2>
        <p className="mt-1 mb-6 text-sm text-[var(--g5-text2)]">
          Workspaces with non-stopped VMs.
        </p>
        {workspacesState.status === "loading" && (
          <p className="py-8 text-center text-sm text-[var(--g5-text2)]">
            Loading workspace VMs…
          </p>
        )}
        {workspacesState.status === "error" && (
          <p
            role="alert"
            className="py-8 text-center text-sm text-[var(--g5-blocked)]"
          >
            Failed to load workspace VMs.
          </p>
        )}
        {workspacesState.status === "ok" && workspaces.length === 0 && (
          <p className="rounded-md border border-[var(--g5-stroke)] py-10 text-center text-sm text-[var(--g5-text1)]">
            No workspace VMs are active.
          </p>
        )}
        {workspaces.length > 0 && (
          <div className="overflow-x-auto rounded-md border border-[var(--g5-stroke)]">
            <table
              aria-label="Workspaces with non-stopped VMs"
              className="w-full min-w-[920px] table-fixed border-collapse text-sm"
            >
              <colgroup>
                <col className="w-[118px]" />
                <col className="w-[240px]" />
                <col className="w-[226px]" />
                <col className="w-[112px]" />
                <col className="w-[96px]" />
                <col />
              </colgroup>
              <thead className="bg-[var(--g5-bg2)] text-left text-xs uppercase text-[var(--g5-text2)]">
                <tr>
                  <th className="px-4 py-3 whitespace-nowrap">Host ID</th>
                  <th className="px-4 py-3">Name</th>
                  <th className="px-4 py-3">Owner</th>
                  <th className="px-4 py-3">Status</th>
                  <th
                    className="px-4 py-3"
                    title="Time since the current VM became ready"
                    aria-description="Time since the current VM became ready"
                  >
                    Age
                  </th>
                  <th className="px-4 py-3 whitespace-nowrap">Last activity</th>
                </tr>
              </thead>
              <tbody>
                {workspaces.map((workspace) => (
                  <tr
                    key={workspace.workspaceId}
                    className="border-t border-[var(--g5-stroke)] bg-[var(--g5-bg1)]"
                  >
                    <td className="px-4 py-3 text-[var(--g5-text1)]">
                      <HostId id={workspace.hostId} />
                    </td>
                    <td className="px-4 py-3 text-[var(--g5-text1)]">
                      <WorkspaceName
                        name={workspace.workspaceName}
                        href={`/${org}/${encodeURIComponent(workspace.workspaceName)}`}
                      />
                    </td>
                    <td className="px-4 py-3 break-words text-[var(--g5-text1)]">
                      <div>
                        {workspace.ownerDisplayName ||
                          workspace.ownerEmail ||
                          "Unknown owner"}
                      </div>
                      {workspace.ownerDisplayName && workspace.ownerEmail && (
                        <div className="mt-1 text-xs text-[var(--g5-text2)]">
                          {workspace.ownerEmail}
                        </div>
                      )}
                    </td>
                    <td className="px-4 py-3 whitespace-nowrap">
                      <Chip
                        text={workspace.status}
                        state={
                          workspace.status === "running"
                            ? "ok"
                            : workspace.status === "failed"
                              ? "blocked"
                              : "warn"
                        }
                      />
                    </td>
                    <td className="px-4 py-3 whitespace-nowrap text-[var(--g5-text2)]">
                      <VmTime value={workspace.startedAt} now={now} />
                    </td>
                    <td className="px-4 py-3 whitespace-nowrap text-[var(--g5-text2)]">
                      <VmTime value={workspace.lastActivityAt} now={now} />
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>

      <AddWorkerHostDialog
        open={addDialogOpen}
        onClose={() => setAddDialogOpen(false)}
        orgId={orgId}
        onProvisioned={() => setAddDialogOpen(false)}
        defaultBaseImageRef={
          hostsState.status === "ok"
            ? hostsState.data.orgDefaultBaseImageRef
            : undefined
        }
        bootDiskSizing={
          hostsState.status === "ok"
            ? hostsState.data.bootDiskSizing
            : undefined
        }
      />
    </div>
  );
}
