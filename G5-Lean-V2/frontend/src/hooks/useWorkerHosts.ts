// =============================================================================
// G5 Client — Worker-Host (ORG-scoped cloud VM) Queries & Mutations
// =============================================================================
// Data layer for the org-scoped `worker_hosts` control plane (add/remove cloud
// worker hosts). DISTINCT from the per-session `workers` concept in
// useWorkers.ts — do NOT conflate the two.
//
// Backend contract (PRs #128/#129/#130/#139/#148):
//   GET    /api/v1/orgs/:orgId/worker-hosts        → { hosts, total }
//   POST   /api/v1/orgs/:orgId/worker-hosts        → 201 ProvisionWorkerHostResult
//   DELETE /api/v1/orgs/:orgId/worker-hosts/:hostId → 204 (idempotent; 404 if unknown)
//
// Provisioning is async: POST returns status "provisioning"; a reconciler flips
// it to "ready" (~30s, heartbeat-gated) only once a cloud backend is wired. In
// dev it stays "provisioning" indefinitely — that is NOT a failure. The UI polls
// GET while any host is in a transient state (`provisioning`/`draining`).
// =============================================================================

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../lib/api";
import { routes } from "../lib/routes";
import type {
  AsyncState,
  ListWorkerHostsResponse,
  MutationHook,
  ProvisionWorkerHostInput,
  ProvisionWorkerHostResult,
  WorkerHost,
  WorkerHostWorkspace,
  WorkerHostStatus,
  UpdateWorkerImagePolicyInput,
  WorkerImagePolicyResult,
} from "../lib/types";
import { toAsyncState, toMutationHook } from "./hookUtils";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/** Stable React Query key for the org's worker-host list. */
function workerHostsKey(orgId: string): [string, string] {
  return ["org-worker-hosts", orgId];
}

/**
 * Host statuses that are still settling and warrant polling. `provisioning`
 * flips to `ready` via the server-side reconciler; `unreachable` may recover
 * after a reconnect; `draining` flips to `terminated` once the host finishes
 * draining. `ready`/`terminated` are stable.
 */
const TRANSIENT_HOST_STATUSES: ReadonlySet<WorkerHostStatus> =
  new Set<WorkerHostStatus>(["provisioning", "unreachable", "draining"]);

/**
 * Polling cadence (ms) while at least one host is in a transient state. ~6s is
 * a balance between observing the ~30s provisioning→ready flip promptly and not
 * hammering the org-scoped list endpoint.
 */
const TRANSIENT_HOST_POLL_INTERVAL = 6_000;

/**
 * Wire shape of the provision `201` response. It is `ProvisionWorkerHostResult`
 * (including the echoed runtime/profile fields) plus the one-time `hostJwt` host
 * credential, which this layer strips before returning so it never reaches a
 * consumer or React Query's mutation cache (see `useProvisionWorkerHost`).
 * `hostJwt` is optional here because the strip is by rest-spread — a response
 * that omits it (it should not, but the type must not assume it) still
 * typechecks and round-trips the runtime/profile echo untouched.
 */
interface ProvisionWorkerHostApiResponse extends ProvisionWorkerHostResult {
  hostJwt?: string;
}

/**
 * Polling decision for the worker-host list: poll (~6s) while any host is in a
 * transient state (`provisioning`/`draining`), otherwise stop. Extracted as a
 * pure function so the cadence/stop logic is unit-testable without fake timers.
 */
export function workerHostsRefetchInterval(
  data: ListWorkerHostsResponse | undefined,
): number | false {
  if (!data) return false;
  return data.hosts.some((host) => TRANSIENT_HOST_STATUSES.has(host.status))
    ? TRANSIENT_HOST_POLL_INTERVAL
    : false;
}

// ---------------------------------------------------------------------------
// Query Hooks
// ---------------------------------------------------------------------------

/**
 * List the org's worker hosts. Polls (~6s) while any host is `provisioning` or
 * `draining`, otherwise idle. `enabled` lets a caller defer the query (e.g.
 * until a panel opens) without dropping the orgId guard.
 */
export function useWorkerHosts(
  orgId: string,
  enabled = true,
): AsyncState<ListWorkerHostsResponse> {
  const query = useQuery<ListWorkerHostsResponse, Error>({
    queryKey: workerHostsKey(orgId),
    queryFn: ({ signal }) =>
      api.get<ListWorkerHostsResponse>(
        `${routes.orgs.workerHosts(orgId)}?activeOnly=true`,
        {
          signal,
        },
      ),
    enabled: !!orgId && enabled,
    refetchInterval: (query) => workerHostsRefetchInterval(query.state.data),
  });
  return toAsyncState(query);
}

export function useWorkerHostWorkspaces(
  orgId: string,
  enabled = true,
): AsyncState<WorkerHostWorkspace[]> {
  return toAsyncState(
    useQuery<WorkerHostWorkspace[], Error>({
      queryKey: ["org-worker-host-workspaces", orgId],
      queryFn: ({ signal }) =>
        api.get<WorkerHostWorkspace[]>(
          `${routes.orgs.workerHosts(orgId)}/workspaces`,
          { signal },
        ),
      enabled: !!orgId && enabled,
      // Keep polling even when empty: a VM may start or move to another host.
      refetchInterval: 5_000,
    }),
  );
}

// ---------------------------------------------------------------------------
// Mutation Hooks
// ---------------------------------------------------------------------------

/**
 * Provision a new worker host for the org. On success, invalidates the org's
 * worker-host list so the new `provisioning` row appears and polling begins.
 *
 * The 201 body carries a one-time `hostJwt` host credential. It is stripped at
 * RUNTIME below (not merely via the static type), so this hook never returns or
 * stores the credential — even `JSON.stringify`-ing the mutation result is safe.
 */
export function useProvisionWorkerHost(
  orgId: string,
): MutationHook<ProvisionWorkerHostInput, ProvisionWorkerHostResult> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    ProvisionWorkerHostResult,
    Error,
    ProvisionWorkerHostInput
  >({
    mutationFn: async (input) => {
      const response = await api.post<ProvisionWorkerHostApiResponse>(
        routes.orgs.provisionWorkerHost(orgId),
        input,
      );
      // Strip ONLY the one-time `hostJwt` credential at RUNTIME (rest-spread), so
      // it never lands in the mutation cache or reaches a consumer — while the
      // echoed runtime/profile fields (baseImageRef, baseImageDigest,
      // baseImageSourceKind, baseImageSourceValue,
      // runtimeSchema, runtimeLabels, runtimeLabelsRefreshedAt, workerProfile)
      // survive for the add-flow's onProvisioned consumer.
      const { hostJwt: _hostJwt, ...result } = response;
      return result;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: workerHostsKey(orgId) });
    },
  });
  return toMutationHook(mutation);
}

/**
 * Deallocate (terminate) a worker host. The backend transitions the row to
 * `terminated` (it is never deleted) and is idempotent — re-deallocating an
 * already-terminated host still returns 204; an unknown host returns 404. A
 * host that still owns active workspace VMs is refused with 409 unless
 * `force` is set, which stops those VMs
 * (g5.architecture.host_forced_removal). On success, invalidates the org's
 * worker-host list.
 */
export function useDeallocateWorkerHost(
  orgId: string,
): MutationHook<{ hostId: string; force?: boolean }, void> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    void,
    Error,
    { hostId: string; force?: boolean }
  >({
    mutationFn: ({ hostId, force }) =>
      api.delete<void>(routes.orgs.deallocateWorkerHost(orgId, hostId), {
        params: force ? { force: true } : undefined,
      }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: workerHostsKey(orgId) });
    },
  });
  return toMutationHook(mutation);
}

/**
 * Refresh a worker host's base-image OCI runtime labels (PR3). POSTs (no body)
 * the explicit refresh endpoint; the backend re-reads the image's
 * `com.g5labs.runtime.*` labels and returns the UPDATED existing `WorkerHost`
 * DTO (no new response struct). On success, invalidates the org's worker-host
 * list so the refreshed row (new labels + `runtimeLabelsRefreshedAt`, or the
 * absent-schema success state where `runtimeSchema` is null and
 * `runtimeLabels` is `{}`) re-renders.
 *
 * Failure modes are surfaced by the caller as typed `ApiError`s: `403` (missing
 * `workers:admin`), `422 unsupported_runtime_schema`, and `503`
 * (registry/network/auth/timeout/oversize). A blank/absent runtime schema is a
 * SUCCESS, not an error.
 */
export function useRefreshWorkerHostRuntimeLabels(
  orgId: string,
): MutationHook<{ hostId: string }, WorkerHost> {
  const queryClient = useQueryClient();
  const mutation = useMutation<WorkerHost, Error, { hostId: string }>({
    mutationFn: ({ hostId }) =>
      api.post<WorkerHost>(
        routes.orgs.refreshWorkerHostRuntimeLabels(orgId, hostId),
      ),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: workerHostsKey(orgId) });
    },
  });
  return toMutationHook(mutation);
}

// Re-export for consumers that want the row type without reaching into types.ts.
export type { WorkerHost };
