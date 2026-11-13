import { useMutation } from "@tanstack/react-query";
import { api } from "../lib/api";
import type {
  AgentLaunchPreferencesV1,
  MutationHook,
  ResolveAgentLaunchRequestV1,
  ResolvedAgentExecutionV1,
} from "../lib/types";
import { toMutationHook } from "./hookUtils";

interface ResolveAgentLaunchResponse {
  resolved: ResolvedAgentExecutionV1;
}

async function resolveAgentLaunchRequest(
  workspaceId: string,
  request: ResolveAgentLaunchRequestV1,
): Promise<ResolvedAgentExecutionV1> {
  const response = await api.post<ResolveAgentLaunchResponse>(
    `/api/v1/workspaces/${workspaceId}/config/agent-launch/resolve`,
    request,
  );
  return response.resolved;
}

/** Resolve a preference draft against effective scoped Runtime Config. */
export function useResolveAgentLaunch(
  workspaceId: string,
): MutationHook<AgentLaunchPreferencesV1, ResolvedAgentExecutionV1> {
  const mutation = useMutation<
    ResolvedAgentExecutionV1,
    Error,
    AgentLaunchPreferencesV1
  >({
    mutationFn: (preferences) =>
      resolveAgentLaunchRequest(workspaceId, { preferences }),
  });
  return toMutationHook(mutation);
}
