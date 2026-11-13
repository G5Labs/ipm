// =============================================================================
// useIntentLog — React Query hook for the read-only Intent Log projection.
// Spec: g5/client/data-layer/intent-log-api. Backend: GET /api/v1/intent-log.
// =============================================================================

import { useQuery } from "@tanstack/react-query";

import { api } from "../lib/api";
import type { AsyncState } from "../lib/types";
import { toAsyncState } from "./hookUtils";

export type IntentKind = "external_doc" | "chat" | "view" | "ontology";

export type AuthorType =
  | "human"
  | "agent"
  | "supervisor_agent"
  | "system"
  | "unknown";

export type TraceStatus =
  | "traceable"
  | "unlinked"
  | "unregistered"
  | "not_applicable"
  | "unknown";

export type ProvenanceSource = "record" | "git_blame" | "unknown";

export interface IntentLogEntry {
  id: string;
  occurredAt: string | null;
  occurredAtSource: ProvenanceSource;
  actor: string | null;
  actorSource: ProvenanceSource;
  sourceKind: "intent_record" | "git_attribution";
  intentKind: IntentKind;
  authorType: AuthorType;
  artifactType: string | null;
  artifactPath: string | null;
  blockId: string | null;
  contentPreview: string;
  content: string | null;
  /** Block title from canonical extraction (server metadata; null for chat
   * rows and unresolvable sources). Never derived by parsing `content`. */
  title?: string | null;
  /** Normalized block label from canonical extraction. */
  blockLabel?: string | null;
  isHuman: boolean;
  traceKey: { fileType: string; fileName: string; id: string } | null;
  traceStatus: TraceStatus;
  rawSource: {
    type: string;
    id: string;
    repoId?: string;
    repoName?: string;
    repoSlug?: string;
    sessionId?: string;
    commitSha?: string;
    duplicatePaths?: string[];
  };
}

export interface SourceDiagnostic {
  sourcePath: string;
  line?: number;
  reason: string;
}

export interface FacetBucket {
  value: string;
  count: number;
}

export interface IntentLogMeta {
  facets: Record<string, FacetBucket[]>;
  cursor: string | null;
  hasMore: boolean;
  diagnostics: SourceDiagnostic[];
}

export interface IntentLogResponse {
  rows: IntentLogEntry[];
  meta: IntentLogMeta;
}

export interface IntentLogFilters {
  q?: string;
  sourceKind?: string[];
  intentKind?: IntentKind[];
  authorType?: AuthorType[];
  actor?: string[];
  timeRange?: "all" | "day" | "week" | "month" | "custom";
  from?: string;
  to?: string;
  traceStatus?: TraceStatus[];
  artifactType?: string[];
  repo?: string;
  sort?: string;
  group?: string;
}

/**
 * Query the Intent Log projection for one workspace branch. Array filters are
 * sent comma-separated (the API splits them). Returns one page plus meta
 * (facets, cursor, diagnostics); the caller accumulates pages for "load more".
 */
export function useIntentLog(
  workspaceId: string,
  filters: IntentLogFilters,
  cursor?: string | null,
): AsyncState<IntentLogResponse> {
  const result = useQuery<IntentLogResponse, Error>({
    queryKey: ["intent-log", workspaceId, filters, cursor ?? null],
    queryFn: ({ signal }) =>
      api.get<IntentLogResponse>("/api/v1/intent-log", {
        signal,
        params: {
          workspaceId,
          q: filters.q || undefined,
          sourceKind: filters.sourceKind,
          intentKind: filters.intentKind,
          authorType: filters.authorType,
          actor: filters.actor,
          timeRange: filters.timeRange,
          from: filters.from,
          to: filters.to,
          traceStatus: filters.traceStatus,
          artifactType: filters.artifactType,
          repo: filters.repo,
          sort: filters.sort,
          group: filters.group,
          cursor: cursor || undefined,
        },
      }),
    enabled: workspaceId.length > 0,
  });
  return toAsyncState(result);
}
