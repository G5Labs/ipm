import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../lib/api";
import { routes } from "../lib/routes";
import { useAuth } from "./useAuth";

interface GithubConnectionStatus {
  connected: boolean;
  githubLogin: string | null;
}

/**
 * The person's GitHub App connection. `enabled` lets a surface that only
 * sometimes needs it (a workspace page's issue actions) skip the read.
 */
export function useGithubConnection(enabled = true) {
  const { user } = useAuth();
  const queryClient = useQueryClient();
  const queryKey = ["github-user-connection", user?.id];
  const status = useQuery({
    queryKey,
    queryFn: ({ signal }) =>
      api.get<GithubConnectionStatus>(routes.githubApp.userConnection, {
        signal,
      }),
    enabled: !!user && enabled,
    staleTime: 0,
  });
  const connect = useMutation({
    mutationFn: (returnPath: string) =>
      api.post<{ redirectUrl: string }>(routes.githubApp.userConnection, {
        returnPath,
      }),
  });
  const disconnect = useMutation({
    mutationFn: () =>
      api.post<GithubConnectionStatus>(
        routes.githubApp.userConnectionDisconnect,
        {},
      ),
    onSuccess: (data) => {
      queryClient.setQueryData(queryKey, data);
      // Other consumers (including issue composers) must re-evaluate the grant.
      void queryClient.invalidateQueries({ queryKey: ["workspace-activity"] });
    },
  });
  return { status, connect, disconnect };
}

/**
 * Whether the login landing should offer the shared connection for the
 * organization the person is entering (g5.settings.github-connection). The
 * server decides from the connection, the person's role there and whether
 * that organization uses GitHub; only the landing asks, so other connection
 * reads stay organization-free. No retry: an unanswered offer must not hold
 * up sign-in.
 */
export function useGithubAfterLoginOffer(orgId: string) {
  const { user } = useAuth();
  return useQuery({
    queryKey: ["github-after-login-offer", user?.id, orgId],
    queryFn: ({ signal }) =>
      api.get<GithubConnectionStatus & { offerAfterLogin?: boolean }>(
        `${routes.githubApp.userConnection}?afterLoginOrgId=${encodeURIComponent(orgId)}`,
        { signal },
      ),
    staleTime: 0,
    retry: false,
  });
}
