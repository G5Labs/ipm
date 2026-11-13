// =============================================================================
// G5 Client — Settings, Instance & Health Queries
// =============================================================================

import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "../lib/api";
import type {
  ScopedConfig,
  WorkspaceSettings,
  SubscriptionUsage,
  InstanceInfoDTO,
  AsyncState,
  MutationHook,
  UpdateScopedConfigInput,
} from "../lib/types";
import { toAsyncState, toMutationHook } from "./hookUtils";
import { isProviderAccountsEnabled } from "../lib/providerFlags";
import { isGuidedJourneyEnabled } from "../lib/instanceFeatures";
import { scopedConfigSignal } from "../lib/scopedConfigAbort";

// ---------------------------------------------------------------------------
// Query Hooks
// ---------------------------------------------------------------------------

export function useWorkspaceSettings(
  workspaceId: string,
): AsyncState<WorkspaceSettings> {
  const query = useQuery<WorkspaceSettings, Error>({
    queryKey: ["settings", workspaceId],
    queryFn: ({ signal }) =>
      api.get<WorkspaceSettings>(`/api/v1/workspaces/${workspaceId}/settings`, {
        signal,
      }),
    enabled: !!workspaceId,
  });
  return toAsyncState(query);
}

/**
 * Read the workspace's display-only subscription-usage snapshot for the
 * composer usage chip's PRE-session render (agent_interaction_ux D8; provider
 * quota snapshot D13). Returns the snapshot or `null` when none has been
 * captured. Workspace-scoped and member-readable
 * (security_policy/role_based_access_control D10); the same key the host LLM
 * proxy stamps (`claude_code` / `anthropic_oauth`).
 *
 * `enabled` is threaded from the caller so the fetch is skipped entirely when
 * the per-user window setting is `off`. A few-minute `staleTime` keeps the
 * pre-session read cheap; the active-session chip continues to source its
 * snapshot from the session detail payload rather than this query.
 *
 * `accountId` (the caller's effective Claude assignment, settings-page D18)
 * selects the account-keyed snapshot row — D8: the chip reads the snapshot
 * keyed by the session's current account. It is part of the query key so a
 * changed assignment refetches; absent, the server answers its default
 * (nil-account) row.
 */
export function useWorkspaceProviderQuota(
  workspaceId: string,
  enabled: boolean,
  accountId?: string | null,
): AsyncState<SubscriptionUsage | null> {
  const accountKey = accountId ? accountId : null;
  const query = useQuery<SubscriptionUsage | null, Error>({
    queryKey: ["provider-quota", workspaceId, accountKey],
    queryFn: ({ signal }) =>
      api.get<SubscriptionUsage | null>(
        `/api/v1/workspaces/${workspaceId}/provider-quota?provider=claude_code&authMode=anthropic_oauth${
          accountKey ? `&accountId=${encodeURIComponent(accountKey)}` : ""
        }`,
        { signal },
      ),
    enabled: enabled && !!workspaceId,
    staleTime: 180_000, // 3 minutes — passive quota snapshot, refreshed lazily
  });
  return toAsyncState(query);
}

export function useInstanceInfo(): AsyncState<InstanceInfoDTO> {
  const query = useQuery<InstanceInfoDTO, Error>({
    queryKey: ["instance-info"],
    queryFn: ({ signal }) =>
      api.get<InstanceInfoDTO>("/api/v1/instance/info", { signal }),
    staleTime: 300_000, // 5 minutes — instance info rarely changes
  });
  return toAsyncState(query);
}




/**
 * Runtime Guided-section gate (ui g5.ui.guided-toggle): the `guided_journey`
 * instance feature, fail-closed.
 */
export function useGuidedJourneyEnabled(): boolean {
  const instanceInfo = useInstanceInfo();
  return isGuidedJourneyEnabled(
    instanceInfo.status === "ok" ? instanceInfo.data : null,
  );
}

/**
 * Runtime provider-accounts kill-switch (external_agent_provider_contract
 * D14; session_provider_controls D29): resolves the `provider_accounts`
 * instance feature from the cached instance-info query. The server advertises
 * it by default (instance spec D16); the client still fails closed while the
 * query is loading or errored.
 */
export function useProviderAccountsEnabled(): boolean {
  const instanceInfo = useInstanceInfo();
  return isProviderAccountsEnabled(
    instanceInfo.status === "ok" ? instanceInfo.data : null,
  );
}


export function useUserScopedConfig(orgId: string): AsyncState<ScopedConfig> {
  const query = useQuery<ScopedConfig, Error>({
    queryKey: ["scoped-config", "user", orgId],
    queryFn: ({ signal }) =>
      api.get<ScopedConfig>("/api/v1/users/me/config", {
        signal,
        params: { orgId },
      }),
    enabled: !!orgId,
  });
  return toAsyncState(query);
}

export function useOrgScopedConfig(orgId: string): AsyncState<ScopedConfig> {
  const query = useQuery<ScopedConfig, Error>({
    queryKey: ["scoped-config", "org", orgId],
    queryFn: ({ signal }) =>
      api.get<ScopedConfig>(`/api/v1/orgs/${orgId}/config`, { signal }),
    enabled: !!orgId,
  });
  return toAsyncState(query);
}

export function useWorkspaceScopedConfig(
  workspaceId: string,
  orgId: string,
): AsyncState<ScopedConfig> {
  const query = useQuery<ScopedConfig, Error>({
    queryKey: ["scoped-config", "workspace", orgId, workspaceId],
    queryFn: ({ signal }) =>
      api.get<ScopedConfig>(`/api/v1/workspaces/${workspaceId}/config`, {
        signal,
        params: { orgId },
      }),
    enabled: !!workspaceId && !!orgId,
  });
  return toAsyncState(query);
}

// ---------------------------------------------------------------------------
// Mutation Hooks
// ---------------------------------------------------------------------------

export function useUpdateSettings(
  workspaceId: string,
): MutationHook<Partial<WorkspaceSettings>, WorkspaceSettings> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    WorkspaceSettings,
    Error,
    Partial<WorkspaceSettings>
  >({
    mutationFn: (input) =>
      api.put<WorkspaceSettings>(
        `/api/v1/workspaces/${workspaceId}/settings`,
        input,
      ),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["settings", workspaceId] });
      queryClient.invalidateQueries({ queryKey: ["spend", workspaceId] });
    },
  });
  return toMutationHook(mutation);
}

export function useUpdateUserScopedConfig(
  orgId: string,
): MutationHook<UpdateScopedConfigInput, void> {
  const queryClient = useQueryClient();
  const mutation = useMutation<void, Error, UpdateScopedConfigInput>({
    // Attached to the auth-transition abort seam, and the hint-bearing
    // response body is DISCARDED (settings-page D9): settings surfaces
    // consume hints exclusively through query refetch, so no mutation
    // state ever holds secret-derived material.
    mutationFn: async (input) => {
      await api.put<ScopedConfig>("/api/v1/users/me/config", input, {
        params: { orgId },
        signal: scopedConfigSignal(),
      });
    },
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: ["scoped-config", "user", orgId],
      });
      queryClient.invalidateQueries({
        queryKey: ["scoped-config", "workspace", orgId],
      });
    },
  });
  return toMutationHook(mutation);
}

export function useUpdateOrgScopedConfig(
  orgId: string,
): MutationHook<UpdateScopedConfigInput, void> {
  const queryClient = useQueryClient();
  const mutation = useMutation<void, Error, UpdateScopedConfigInput>({
    // Response body discarded — see useUpdateUserScopedConfig.
    mutationFn: async (input) => {
      await api.put<ScopedConfig>(`/api/v1/orgs/${orgId}/config`, input, {
        signal: scopedConfigSignal(),
      });
    },
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: ["scoped-config", "org", orgId],
      });
      queryClient.invalidateQueries({
        queryKey: ["scoped-config", "user", orgId],
      });
      queryClient.invalidateQueries({
        queryKey: ["scoped-config", "workspace", orgId],
      });
    },
  });
  return toMutationHook(mutation);
}

export function useUpdateWorkspaceScopedConfig(
  workspaceId: string,
  orgId: string,
): MutationHook<UpdateScopedConfigInput, void> {
  const queryClient = useQueryClient();
  const mutation = useMutation<void, Error, UpdateScopedConfigInput>({
    // Response body discarded — see useUpdateUserScopedConfig.
    mutationFn: async (input) => {
      await api.put<ScopedConfig>(
        `/api/v1/workspaces/${workspaceId}/config`,
        input,
        {
          params: { orgId },
          signal: scopedConfigSignal(),
        },
      );
    },
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: ["scoped-config", "workspace", orgId, workspaceId],
      });
    },
  });
  return toMutationHook(mutation);
}
