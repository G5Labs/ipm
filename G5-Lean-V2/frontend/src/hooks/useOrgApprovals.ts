// =============================================================================
// G5 Client — Org Approval-Definition Queries & Mutations
// =============================================================================
// Data layer for the org-wide approval-definition library (B6 governance
// PR-1; `approval_definitions`, migration 186). "org_approvals", NOT bare
// "approvals" — that name means in-chat agent tool-approvals
// (lib/agentApprovals.ts). Definitions gate nothing by themselves in PR-1:
// workspace attachment (`approval_configs`) ships in a later PR.
//
// Backend contract (routes/org_approvals.rs):
//   GET  /api/v1/orgs/:orgId/approval-definitions                     → OrgApprovalDefinitionDTO[]
//   POST /api/v1/orgs/:orgId/approval-definitions                     → 201 created v1
//   POST /api/v1/orgs/:orgId/approval-definitions/:definitionId/versions
//        → 200 next version; 409 when a concurrent publish won the race
// =============================================================================

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, ApiError } from "../lib/api";
import { routes } from "../lib/routes";
import type {
  AsyncState,
  CreateOrgApprovalDefinitionInput,
  MutationHook,
  OrgApprovalDefinitionDTO,
  PublishOrgApprovalDefinitionVersionInput,
} from "../lib/types";
import { toAsyncState, toMutationHook } from "./hookUtils";

/** Stable React Query key for the org's approval-definition library. */
function orgApprovalDefinitionsKey(orgId: string): [string, string] {
  return ["org-approval-definitions", orgId];
}

/** List the org's approval definitions (latest version each, name ASC). */
export function useOrgApprovalDefinitions(
  orgId: string,
  enabled = true,
): AsyncState<OrgApprovalDefinitionDTO[]> {
  const query = useQuery<OrgApprovalDefinitionDTO[], Error>({
    queryKey: orgApprovalDefinitionsKey(orgId),
    queryFn: ({ signal }) =>
      api.get<OrgApprovalDefinitionDTO[]>(
        routes.orgs.approvalDefinitions(orgId),
        { signal },
      ),
    enabled: !!orgId && enabled,
  });
  return toAsyncState(query);
}

/** Create version 1 of a new definition; invalidation is not a v1 concept. */
export function useCreateOrgApprovalDefinition(
  orgId: string,
): MutationHook<CreateOrgApprovalDefinitionInput, OrgApprovalDefinitionDTO> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    OrgApprovalDefinitionDTO,
    Error,
    CreateOrgApprovalDefinitionInput
  >({
    mutationFn: (input) =>
      api.post<OrgApprovalDefinitionDTO>(
        routes.orgs.approvalDefinitions(orgId),
        input,
      ),
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: orgApprovalDefinitionsKey(orgId),
      });
    },
  });
  return toMutationHook(mutation);
}

/**
 * Publish the next append-only version of an existing definition. A 409 means
 * a concurrent publish won the race — the caller must surface the conflict
 * and refetch rather than silently retrying (the raced edit was never
 * reviewed against the winner's version).
 */
export function usePublishOrgApprovalDefinitionVersion(
  orgId: string,
): MutationHook<
  { definitionId: string } & PublishOrgApprovalDefinitionVersionInput,
  OrgApprovalDefinitionDTO
> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    OrgApprovalDefinitionDTO,
    Error,
    { definitionId: string } & PublishOrgApprovalDefinitionVersionInput
  >({
    mutationFn: ({ definitionId, ...input }) =>
      api.post<OrgApprovalDefinitionDTO>(
        routes.orgs.publishApprovalDefinitionVersion(orgId, definitionId),
        input,
      ),
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: orgApprovalDefinitionsKey(orgId),
      });
    },
    onError: (error) => {
      // A 409 means a concurrent publish won; refresh the library so the
      // conflict notice is truthful about showing the latest version.
      if (error instanceof ApiError && error.status === 409) {
        void queryClient.invalidateQueries({
          queryKey: orgApprovalDefinitionsKey(orgId),
        });
      }
    },
  });
  return toMutationHook(mutation);
}
