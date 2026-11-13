// =============================================================================
// G5 Client — Grounding API Hooks
// =============================================================================
// React Query hooks for querying intent traceability from the public API.
// Used by the IntentTraceabilityPanel to display parent/child intents.
// =============================================================================

import { useQuery } from "@tanstack/react-query";
import { api } from "../lib/api";
import { toAsyncState } from "./hookUtils";
import type { AsyncState } from "../lib/types";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type TraceRequest = {
  workspaceId: string;
  repoId?: string;
  fileType: string;
  fileName: string;
  intentRecordNumber: string;
};

export interface TraceResult {
  intentRecordId: string;
  workspaceId: string;
  repoId: string;
  fileType: string;
  fileName: string | null;
  intentRecordNumber: string | null;
  codeBlockName: string | null;
  content: string | null;
  /** Block title from canonical extraction (server metadata; null for chat
   * records and unresolvable/stale sources). Clients render titles from this
   * field — never by parsing `content`, which arrives label-line-free. */
  title?: string | null;
  /** Normalized block label / code symbol from canonical extraction. */
  blockLabel?: string | null;
  /** Stored chat session identity; null for non-chat records. */
  chatSessionId?: string | null;
  /** Immutable stored chat context. */
  chatContext?: string | null;
  /** Verbatim stored user message. */
  userContent?: string | null;
  /** Intent-record creation author when stored. */
  createdBy?: string | null;
  /** Intent-record creation timestamp when stored. */
  createdAt?: string | null;
  /** Existing physical code path returned by the server for navigation. */
  sourceFilePath?: string | null;
  /** True when a non-chat source or block cannot be resolved. */
  stale?: boolean;
  isHuman: boolean;
  priority: number | null;
  certainty: number | null;
  relationship?: string | null;
  relevance?: number | null;
}

export interface TraceResponse {
  availability?: "recorded" | "not_recorded";
  totalCount: number;
  results: TraceResult[];
}

// ---------------------------------------------------------------------------
// Hooks
// ---------------------------------------------------------------------------

/**
 * Trace parent intents for a given intent block.
 * Returns all intent records that ground *into* the specified block.
 */
export function useTraceParents(
  request: TraceRequest | null,
): AsyncState<TraceResponse> {
  const query = useQuery<TraceResponse, Error>({
    queryKey: ["grounding", "parents", request],
    queryFn: () =>
      api.get<TraceResponse>("/api/v1/grounding/trace-parents", {
        params: request ?? undefined,
      }),
    enabled: !!request,
  });
  return toAsyncState(query);
}

/**
 * Trace child intents for a given intent block.
 * Returns all intent records grounded *from* the specified block.
 */
export function useTraceChildren(
  request: TraceRequest | null,
): AsyncState<TraceResponse> {
  const query = useQuery<TraceResponse, Error>({
    queryKey: ["grounding", "children", request],
    queryFn: () =>
      api.get<TraceResponse>("/api/v1/grounding/trace-children", {
        params: request ?? undefined,
      }),
    enabled: !!request,
  });
  return toAsyncState(query);
}
