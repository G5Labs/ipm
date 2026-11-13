// =============================================================================
// G5 Client — Organization Management Hooks
// =============================================================================

import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "../lib/api";
import type {
  ApiKey,
  AsyncState,
  CreateApiKeyInput,
  CreateApiKeyResult,
  CurrentUserDTO,
  InviteMemberResponseDTO,
  MutationHook,
  OrgAdminSettingsDTO,
  OrgInvitationDTO,
  OrgMemberDTO,
} from "../lib/types";
import { toAsyncState, toMutationHook } from "./hookUtils";

// ---------------------------------------------------------------------------
// Query Hooks
// ---------------------------------------------------------------------------

/**
 * Derives org data from the useCurrentUser cache via `select`.
 * No separate API call — uses the current-user query.
 */
export function useOrgs(): AsyncState<CurrentUserDTO["orgs"]> {
  const query = useQuery<CurrentUserDTO, Error, CurrentUserDTO["orgs"]>({
    queryKey: ["current-user"],
    queryFn: ({ signal }) =>
      api.get<CurrentUserDTO>("/api/v1/users/me", { signal }),
    staleTime: 300_000, // 5 minutes
    select: (data) => data.orgs,
  });
  return toAsyncState(query);
}

export function useOrgMembers(orgId: string): AsyncState<OrgMemberDTO[]> {
  const query = useQuery<OrgMemberDTO[], Error>({
    queryKey: ["org-members", orgId],
    queryFn: ({ signal }) =>
      api.get<OrgMemberDTO[]>(`/api/v1/orgs/${orgId}/members`, { signal }),
    enabled: !!orgId,
    refetchInterval: (query) =>
      query.state.data?.some((member) => member.shutdownPending) ? 3000 : false,
  });
  return toAsyncState(query);
}

export function useOrgInvitations(
  orgId: string,
): AsyncState<OrgInvitationDTO[]> {
  const query = useQuery<OrgInvitationDTO[], Error>({
    queryKey: ["org-invitations", orgId],
    queryFn: ({ signal }) =>
      api.get<OrgInvitationDTO[]>(`/api/v1/orgs/${orgId}/invitations`, {
        signal,
      }),
    enabled: !!orgId,
  });
  return toAsyncState(query);
}

export function useApiKeys(
  orgId: string,
  enabled = true,
): AsyncState<ApiKey[]> {
  const query = useQuery<ApiKey[], Error>({
    queryKey: ["org-api-keys", orgId],
    queryFn: ({ signal }) =>
      api.get<ApiKey[]>(`/api/v1/orgs/${orgId}/api-keys`, { signal }),
    enabled: !!orgId && enabled,
  });
  return toAsyncState(query);
}

/** Admin page organization settings; the API refuses non-administrators. */
export function useOrgAdminSettings(
  orgId: string,
  enabled = true,
): AsyncState<OrgAdminSettingsDTO> {
  const query = useQuery<OrgAdminSettingsDTO, Error>({
    queryKey: ["org-admin-settings", orgId],
    queryFn: ({ signal }) =>
      api.get<OrgAdminSettingsDTO>(`/api/v1/orgs/${orgId}/admin-settings`, {
        signal,
      }),
    enabled: !!orgId && enabled,
  });
  return toAsyncState(query);
}

// ---------------------------------------------------------------------------
// Mutation Hooks
// ---------------------------------------------------------------------------

export function useInviteOrgMember(): MutationHook<
  { orgId: string; email: string; role: string },
  InviteMemberResponseDTO
> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    InviteMemberResponseDTO,
    Error,
    { orgId: string; email: string; role: string }
  >({
    mutationFn: ({ orgId, email, role }) =>
      api.post<InviteMemberResponseDTO>(`/api/v1/orgs/${orgId}/members`, {
        email,
        role,
      }),
    onSuccess: (_data, { orgId }) => {
      queryClient.invalidateQueries({ queryKey: ["org-members", orgId] });
      queryClient.invalidateQueries({ queryKey: ["org-invitations", orgId] });
    },
  });
  return toMutationHook(mutation);
}

export function useCreateApiKey(
  orgId: string,
): MutationHook<CreateApiKeyInput, CreateApiKeyResult> {
  const queryClient = useQueryClient();
  const mutation = useMutation<CreateApiKeyResult, Error, CreateApiKeyInput>({
    mutationFn: (input) =>
      api.post<CreateApiKeyResult>(`/api/v1/orgs/${orgId}/api-keys`, input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["org-api-keys", orgId] });
    },
  });
  return toMutationHook(mutation);
}

export function useRevokeApiKey(
  orgId: string,
): MutationHook<{ keyId: string }, void> {
  const queryClient = useQueryClient();
  const mutation = useMutation<void, Error, { keyId: string }>({
    mutationFn: ({ keyId }) =>
      api.delete<void>(`/api/v1/orgs/${orgId}/api-keys/${keyId}`),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["org-api-keys", orgId] });
    },
  });
  return toMutationHook(mutation);
}

export function useUpdateMemberRole(): MutationHook<
  { orgId: string; userId: string; role: string },
  void
> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    void,
    Error,
    { orgId: string; userId: string; role: string }
  >({
    mutationFn: ({ orgId, userId, role }) =>
      api.patch<void>(`/api/v1/orgs/${orgId}/members/${userId}`, { role }),
    onSuccess: (_data, { orgId }) => {
      queryClient.invalidateQueries({ queryKey: ["org-members", orgId] });
    },
  });
  return toMutationHook(mutation);
}

export function useRemoveMember(): MutationHook<
  { orgId: string; userId: string },
  void
> {
  const queryClient = useQueryClient();
  const mutation = useMutation<void, Error, { orgId: string; userId: string }>({
    mutationFn: ({ orgId, userId }) =>
      api.delete<void>(`/api/v1/orgs/${orgId}/members/${userId}`),
    onSuccess: (_data, { orgId }) => {
      queryClient.invalidateQueries({ queryKey: ["org-members", orgId] });
    },
  });
  return toMutationHook(mutation);
}

export function useResendInvitation(): MutationHook<
  { orgId: string; invitationId: string },
  void
> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    void,
    Error,
    { orgId: string; invitationId: string }
  >({
    mutationFn: ({ orgId, invitationId }) =>
      api.post<void>(
        `/api/v1/orgs/${orgId}/invitations/${invitationId}/resend`,
      ),
    onSuccess: (_data, { orgId }) => {
      queryClient.invalidateQueries({ queryKey: ["org-invitations", orgId] });
    },
  });
  return toMutationHook(mutation);
}

export function useRevokeInvitation(): MutationHook<
  { orgId: string; invitationId: string },
  void
> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    void,
    Error,
    { orgId: string; invitationId: string }
  >({
    mutationFn: ({ orgId, invitationId }) =>
      api.delete<void>(`/api/v1/orgs/${orgId}/invitations/${invitationId}`),
    onSuccess: (_data, { orgId }) => {
      queryClient.invalidateQueries({ queryKey: ["org-invitations", orgId] });
    },
  });
  return toMutationHook(mutation);
}

export function useUpdateOrgAdminSettings(): MutationHook<
  { orgId: string; settings: Partial<OrgAdminSettingsDTO> },
  OrgAdminSettingsDTO
> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    OrgAdminSettingsDTO,
    Error,
    { orgId: string; settings: Partial<OrgAdminSettingsDTO> }
  >({
    mutationFn: ({ orgId, settings }) =>
      api.put<OrgAdminSettingsDTO>(
        `/api/v1/orgs/${orgId}/admin-settings`,
        settings,
      ),
    onSuccess: (data, { orgId }) => {
      queryClient.setQueryData(["org-admin-settings", orgId], data);
    },
  });
  return toMutationHook(mutation);
}

export function useSetMemberActive(): MutationHook<
  { orgId: string; userId: string; active: boolean; activityRevision: number },
  OrgMemberDTO
> {
  const queryClient = useQueryClient();
  const mutation = useMutation({
    mutationFn: ({
      orgId,
      userId,
      active,
      activityRevision,
    }: {
      orgId: string;
      userId: string;
      active: boolean;
      activityRevision: number;
    }) =>
      api.post<OrgMemberDTO>(
        `/api/v1/orgs/${orgId}/members/${userId}/${active ? "reactivate" : "deactivate"}`,
        { activityRevision },
      ),
    onSettled: (_data, _error, { orgId }) => {
      // Conflicts refresh the observed revision too.
      void queryClient.invalidateQueries({ queryKey: ["org-members", orgId] });
    },
  });
  return toMutationHook(mutation);
}

/** One workspace's state while a member's workspaces are being stopped. */
export interface MemberShutdownWorkspace {
  workspaceId: string;
  name: string;
  state: "pending" | "stopping" | "stopped";
  /** The VM's last lifecycle problem while not yet stopped, if any. */
  blocker?: string | null;
}

/**
 * Stop every workspace a member owns in the organization
 * (`g5.lifecycle.stop.by-user`): the member themself or an admin. Access is
 * unchanged; the hold clears once everything has stopped.
 */
export function useStopMemberWorkspaces(): MutationHook<
  { orgId: string; userId: string },
  { holdId: string; joined: boolean; workspaces: MemberShutdownWorkspace[] }
> {
  const queryClient = useQueryClient();
  const mutation = useMutation({
    mutationFn: ({ orgId, userId }: { orgId: string; userId: string }) =>
      api.post<{
        holdId: string;
        joined: boolean;
        workspaces: MemberShutdownWorkspace[];
      }>(`/api/v1/orgs/${orgId}/members/${userId}/stop-workspaces`, {}),
    onSettled: (_data, _error, { orgId, userId }) => {
      void queryClient.invalidateQueries({ queryKey: ["org-members", orgId] });
      void queryClient.invalidateQueries({
        queryKey: ["member-shutdown", orgId, userId],
      });
    },
  });
  return toMutationHook(mutation);
}

export function useMemberShutdown(
  orgId: string,
  userId: string,
): AsyncState<MemberShutdownWorkspace[]> {
  return toAsyncState(
    useQuery({
      queryKey: ["member-shutdown", orgId, userId],
      queryFn: ({ signal }) =>
        api.get<MemberShutdownWorkspace[]>(
          `/api/v1/orgs/${orgId}/members/${userId}/shutdown`,
          { signal },
        ),
      enabled: !!orgId && !!userId,
      refetchInterval: 3000,
    }),
  );
}
