// =============================================================================
// G5 Client — GitHub App Organization Binding
// =============================================================================
// Backend contract (routes/github_app.rs), organization admins only:
//   GET  /api/v1/orgs/:orgId/github-app/binding               → recorded snapshot
//   POST /api/v1/orgs/:orgId/github-app/connection-attempt    → GitHub install URL
//   POST /api/v1/orgs/:orgId/github-app/reconnect-attempt     → GitHub OAuth URL
//   GET  /api/v1/orgs/:orgId/github-app/binding/repositories  → live GitHub list
//   PUT  /api/v1/orgs/:orgId/github-app/binding/coverage      → snapshot N+1
//   POST /api/v1/orgs/:orgId/github-app/binding/refresh       → snapshot N+1 if changed
//   POST /api/v1/orgs/:orgId/github-app/binding/disconnect    → snapshot N+1
//   GET  /api/v1/orgs/:orgId/github-app/binding/management-url
// The binding read is recorded state; the repository list, coverage change,
// refresh and management URL contact GitHub.
// =============================================================================

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { api } from "../lib/api";
import { routes } from "../lib/routes";
import type {
  AsyncState,
  DisconnectGithubAppBindingInput,
  GithubAppBindingStatusResponseDTO,
  GithubAppInstallationRepositoriesDTO,
  GithubAppManagementUrlDTO,
  GithubAppRedirectDTO,
  MutationHook,
  RefreshGithubAppBindingInput,
  UpdateGithubAppCoverageInput,
} from "../lib/types";
import { toAsyncState, toMutationHook } from "./hookUtils";

function bindingKey(orgId: string): [string, string] {
  return ["github-app-binding", orgId];
}

function repositoriesKey(orgId: string): [string, string] {
  return ["github-app-repositories", orgId];
}

export function useGithubAppBindingStatus(
  orgId: string,
  enabled = true,
): AsyncState<GithubAppBindingStatusResponseDTO> {
  const query = useQuery<GithubAppBindingStatusResponseDTO, Error>({
    queryKey: bindingKey(orgId),
    queryFn: ({ signal }) =>
      api.get<GithubAppBindingStatusResponseDTO>(
        routes.orgs.githubAppBinding(orgId),
        { signal },
      ),
    enabled: !!orgId && enabled,
  });
  return toAsyncState(query);
}

/** Request the server-built GitHub App installation URL. */
export function useGithubAppConnectionAttempt(
  orgId: string,
): MutationHook<void, GithubAppRedirectDTO> {
  const mutation = useMutation<GithubAppRedirectDTO, Error, void>({
    mutationFn: () =>
      api.post<GithubAppRedirectDTO>(
        routes.orgs.githubAppConnectionAttempt(orgId),
        {},
      ),
  });
  return toMutationHook(mutation);
}

/** Request the server-built OAuth URL that reconnects an installed App. */
export function useGithubAppReconnectAttempt(
  orgId: string,
): MutationHook<void, GithubAppRedirectDTO> {
  const mutation = useMutation<GithubAppRedirectDTO, Error, void>({
    mutationFn: () =>
      api.post<GithubAppRedirectDTO>(
        routes.orgs.githubAppReconnectAttempt(orgId),
        {},
      ),
  });
  return toMutationHook(mutation);
}

/** Live list of repositories the installation can access (asks GitHub). */
export function useGithubAppInstallationRepositories(
  orgId: string,
  enabled = false,
): AsyncState<GithubAppInstallationRepositoriesDTO> {
  const query = useQuery<GithubAppInstallationRepositoriesDTO, Error>({
    queryKey: repositoriesKey(orgId),
    queryFn: ({ signal }) =>
      api.get<GithubAppInstallationRepositoriesDTO>(
        routes.orgs.githubAppRepositories(orgId),
        { signal },
      ),
    enabled: !!orgId && enabled,
  });
  return toAsyncState(query);
}

function useInvalidateBinding(orgId: string) {
  const queryClient = useQueryClient();
  return () => {
    void queryClient.invalidateQueries({ queryKey: bindingKey(orgId) });
    void queryClient.invalidateQueries({ queryKey: repositoriesKey(orgId) });
  };
}

/** Replace coverage with exactly the selected GitHub repositories. */
export function useUpdateGithubAppCoverage(
  orgId: string,
): MutationHook<
  UpdateGithubAppCoverageInput,
  GithubAppBindingStatusResponseDTO
> {
  const invalidate = useInvalidateBinding(orgId);
  const mutation = useMutation<
    GithubAppBindingStatusResponseDTO,
    Error,
    UpdateGithubAppCoverageInput
  >({
    mutationFn: (input) =>
      api.put<GithubAppBindingStatusResponseDTO>(
        routes.orgs.githubAppCoverage(orgId),
        input,
      ),
    onSuccess: invalidate,
  });
  return toMutationHook(mutation);
}

/** Append a disconnected snapshot; earlier snapshots remain as history. */
export function useDisconnectGithubAppBinding(
  orgId: string,
): MutationHook<
  DisconnectGithubAppBindingInput,
  GithubAppBindingStatusResponseDTO
> {
  const invalidate = useInvalidateBinding(orgId);
  const mutation = useMutation<
    GithubAppBindingStatusResponseDTO,
    Error,
    DisconnectGithubAppBindingInput
  >({
    mutationFn: (input) =>
      api.post<GithubAppBindingStatusResponseDTO>(
        routes.orgs.githubAppDisconnect(orgId),
        input,
      ),
    onSuccess: invalidate,
  });
  return toMutationHook(mutation);
}

/**
 * Re-read the installation from GitHub and record changed permissions or
 * account (e.g. after an owner accepts new permissions) as a new snapshot.
 */
export function useRefreshGithubAppBinding(
  orgId: string,
): MutationHook<
  RefreshGithubAppBindingInput,
  GithubAppBindingStatusResponseDTO
> {
  const invalidate = useInvalidateBinding(orgId);
  const mutation = useMutation<
    GithubAppBindingStatusResponseDTO,
    Error,
    RefreshGithubAppBindingInput
  >({
    mutationFn: (input) =>
      api.post<GithubAppBindingStatusResponseDTO>(
        routes.orgs.githubAppRefresh(orgId),
        input,
      ),
    onSuccess: invalidate,
  });
  return toMutationHook(mutation);
}

/** Fetch GitHub's validated installation settings URL. */
export function useGithubAppManagementUrl(
  orgId: string,
): MutationHook<void, GithubAppManagementUrlDTO> {
  const mutation = useMutation<GithubAppManagementUrlDTO, Error, void>({
    mutationFn: () =>
      api.get<GithubAppManagementUrlDTO>(
        routes.orgs.githubAppManagementUrl(orgId),
      ),
  });
  return toMutationHook(mutation);
}
