// =============================================================================
// G5 Client — Provider Accounts Queries & Mutations
// =============================================================================
// Named provider accounts (external_agent_provider_contract D14; settings-page
// D16): subscriptions and API keys stored as scoped rows and listed for the
// Settings Accounts section.
//
// The list rows carry bounded secret hints (secret_custody_and_redaction D9),
// so the query is keyed UNDER the `scoped-config` prefix: the auth
// principal-transition seam in useAuth cancels + resets every `scoped-config`
// query and aborts every in-flight scoped-config request, and this hook
// inherits that eviction without extra wiring (settings-page D9/D16).
// Mutations attach the shared abort signal and DISCARD their response bodies
// (resolve void) exactly like the scoped-config PUT hooks, so no hint-bearing
// mutation result ever lives in the mutation cache; the section consumes
// fresh rows through list refetch only. No account data touches localStorage.
// =============================================================================

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../lib/api";
import type {
  AsyncState,
  CreateProviderAccountInput,
  MutationHook,
  ProviderAccountDTO,
  UpdateProviderAccountInput,
} from "../lib/types";
import { toAsyncState, toMutationHook } from "./hookUtils";
import { scopedConfigSignal } from "../lib/scopedConfigAbort";

/** Query key for the accounts list — deliberately under the scoped-config prefix. */
export function providerAccountsQueryKey(
  orgId: string,
  workspaceId?: string | null,
): readonly [string, string, string, string | null] {
  return ["scoped-config", "provider-accounts", orgId, workspaceId ?? null];
}

function accountsPath(orgId: string): string {
  return `/api/v1/orgs/${encodeURIComponent(orgId)}/provider-accounts`;
}

function accountPath(orgId: string, accountId: string): string {
  return `${accountsPath(orgId)}/${encodeURIComponent(accountId)}`;
}

/**
 * List the provider accounts visible to the caller: org accounts, the
 * workspace's accounts when `workspaceId` is given, and the caller's own user
 * accounts. Disabled until an org id is known.
 */
export function useProviderAccounts(
  orgId: string,
  workspaceId?: string | null,
): AsyncState<ProviderAccountDTO[]> {
  const query = useQuery<ProviderAccountDTO[], Error>({
    queryKey: providerAccountsQueryKey(orgId, workspaceId),
    queryFn: ({ signal }) =>
      api.get<ProviderAccountDTO[]>(accountsPath(orgId), {
        signal,
        params: workspaceId ? { workspaceId } : undefined,
      }),
    enabled: !!orgId,
  });
  return toAsyncState(query);
}

function useInvalidateProviderAccounts(orgId: string): () => void {
  const queryClient = useQueryClient();
  return () => {
    void queryClient.invalidateQueries({
      queryKey: ["scoped-config", "provider-accounts", orgId],
    });
  };
}

/**
 * Creating or removing an account also writes the per-agent assignment at
 * the account's own scope (chat g5.chat.account-defaults), so every scoped
 * config in the cache is stale too; the whole prefix covers both.
 */
function useInvalidateAccountsAndAssignments(): () => void {
  const queryClient = useQueryClient();
  return () => {
    void queryClient.invalidateQueries({ queryKey: ["scoped-config"] });
  };
}

/**
 * Create an account; kind and scope are fixed by this call (D14). The server
 * makes it the default for its agents at its scope where none is set yet.
 */
export function useCreateProviderAccount(
  orgId: string,
): MutationHook<CreateProviderAccountInput, void> {
  const invalidate = useInvalidateAccountsAndAssignments();
  const mutation = useMutation<void, Error, CreateProviderAccountInput>({
    mutationFn: async (input) => {
      await api.post<ProviderAccountDTO>(accountsPath(orgId), input, {
        signal: scopedConfigSignal(),
      });
    },
    onSuccess: invalidate,
  });
  return toMutationHook(mutation);
}

export interface UpdateProviderAccountArgs {
  accountId: string;
  /** Only the changed fields; callers omit a blank secret. */
  patch: UpdateProviderAccountInput;
}

/** Patch name / baseUrl / secret; kind and scope are immutable. */
export function useUpdateProviderAccount(
  orgId: string,
): MutationHook<UpdateProviderAccountArgs, void> {
  const invalidate = useInvalidateProviderAccounts(orgId);
  const mutation = useMutation<void, Error, UpdateProviderAccountArgs>({
    mutationFn: async ({ accountId, patch }) => {
      await api.patch<ProviderAccountDTO>(
        accountPath(orgId, accountId),
        patch,
        {
          signal: scopedConfigSignal(),
        },
      );
    },
    onSuccess: invalidate,
  });
  return toMutationHook(mutation);
}

/**
 * Remove an account. The server clears the assignments at the account's own
 * scope with it and refuses (409 `account_assigned`) while another scope
 * still assigns it.
 */
export function useDeleteProviderAccount(
  orgId: string,
): MutationHook<string, void> {
  const invalidate = useInvalidateAccountsAndAssignments();
  const mutation = useMutation<void, Error, string>({
    mutationFn: async (accountId) => {
      await api.delete(accountPath(orgId, accountId), {
        signal: scopedConfigSignal(),
      });
    },
    onSuccess: invalidate,
  });
  return toMutationHook(mutation);
}
