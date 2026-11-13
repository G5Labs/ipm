// =============================================================================
// G5 Client — Platform Operator Console Hooks
// =============================================================================
// The operator organization's Home (g5.organization.platform-operator.home):
// every tenant organization's platform facts, and creating an organization
// for a named initial Admin. The API checks operator authority on each call.
// =============================================================================

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../lib/api";
import { routes } from "../lib/routes";
import type {
  AsyncState,
  CreatePlatformOrgInput,
  CreatePlatformOrgResponse,
  MutationHook,
  PlatformOrgDTO,
} from "../lib/types";
import { toAsyncState, toMutationHook } from "./hookUtils";

const PLATFORM_ORGS_KEY = ["platform-orgs"] as const;

// ---------------------------------------------------------------------------
// Query Hooks
// ---------------------------------------------------------------------------

/** Every tenant organization, by name; refused unless the caller is an operator. */
export function usePlatformOrgs(enabled = true): AsyncState<PlatformOrgDTO[]> {
  const query = useQuery<PlatformOrgDTO[], Error>({
    queryKey: PLATFORM_ORGS_KEY,
    queryFn: ({ signal }) =>
      api.get<PlatformOrgDTO[]>(routes.platform.orgs, { signal }),
    enabled,
  });
  return toAsyncState(query);
}

// ---------------------------------------------------------------------------
// Mutation Hooks
// ---------------------------------------------------------------------------

/** Creates an organization administered by `adminEmail`; the operator gains no membership. */
export function useCreatePlatformOrg(): MutationHook<
  CreatePlatformOrgInput,
  CreatePlatformOrgResponse
> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    CreatePlatformOrgResponse,
    Error,
    CreatePlatformOrgInput
  >({
    mutationFn: (input) =>
      api.post<CreatePlatformOrgResponse>(routes.platform.orgs, input),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: PLATFORM_ORGS_KEY });
    },
  });
  return toMutationHook(mutation);
}
