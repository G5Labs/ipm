// =============================================================================
// G5 Client — Guided journey marks (workbench g5.workbench.journey.marks)
// =============================================================================
// `GET /api/v1/workspaces/:id/journey` is the latest mark per step; `PUT
// …/journey/steps/:stepId {done, skipped}` is the owner's mark, undo or skip.
// Loaded marks survive a transient refresh failure (the step states keep
// their evidence; `g5.workspace.page.refresh`). A mark binds the workspace it
// was made in at mutate time — React Query runs a paused-then-resumed
// mutation with the latest `mutationFn`, so the URL and the invalidation
// never read the render's workspace id (the phase move does the same). A
// mark is a timeline entry too, so the activity feed is invalidated with it.
// =============================================================================

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../lib/api";
import type {
  JourneyMarkDTO,
  MutationHook,
  WorkspaceJourneyDTO,
} from "../lib/types";
import { externalIssueKeys } from "./useExternalIssues";
import {
  type RefreshingState,
  toMutationHook,
  toRefreshingState,
} from "./hookUtils";

export const journeyKey = (workspaceId: string) =>
  ["journey", workspaceId] as const;

export function journeyMarksQueryOptions(workspaceId: string) {
  return {
    queryKey: journeyKey(workspaceId),
    queryFn: async ({ signal }: { signal: AbortSignal }) =>
      (
        await api.get<WorkspaceJourneyDTO>(
          `/api/v1/workspaces/${workspaceId}/journey`,
          { signal },
        )
      ).marks,
  };
}

export function useJourneyMarks(
  workspaceId: string,
  enabled: boolean,
): RefreshingState<JourneyMarkDTO[]> {
  const query = useQuery<JourneyMarkDTO[], Error>({
    ...journeyMarksQueryOptions(workspaceId),
    enabled: enabled && !!workspaceId,
  });
  return toRefreshingState(query);
}

export interface JourneyMarkInput {
  /** Bound at mutate time; never the hook's current workspace. */
  workspaceId: string;
  stepId: string;
  done: boolean;
  /** Skip instead of done (g5.ui.guided-panel); never with `done`. */
  skipped?: boolean;
}

export function useSetJourneyMark(): MutationHook<
  JourneyMarkInput,
  JourneyMarkDTO[]
> {
  const queryClient = useQueryClient();
  const mutation = useMutation<JourneyMarkDTO[], Error, JourneyMarkInput>({
    mutationFn: async ({ workspaceId, stepId, done, skipped = false }) =>
      (
        await api.put<WorkspaceJourneyDTO>(
          `/api/v1/workspaces/${workspaceId}/journey/steps/${encodeURIComponent(stepId)}`,
          { done, skipped },
        )
      ).marks,
    // Overlapping marks and an in-flight read must not overwrite each other
    // with a stale list: the read is cancelled while a mark is in flight and
    // the authoritative list is refetched once it settles.
    onMutate: async ({ workspaceId }) => {
      await queryClient.cancelQueries({ queryKey: journeyKey(workspaceId) });
    },
    onSettled: (_data, _error, { workspaceId }) => {
      void queryClient.invalidateQueries({ queryKey: journeyKey(workspaceId) });
      void queryClient.invalidateQueries({
        queryKey: externalIssueKeys.activity(workspaceId),
      });
    },
  });
  return toMutationHook(mutation);
}
