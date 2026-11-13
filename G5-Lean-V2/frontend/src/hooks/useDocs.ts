import { useWorkspaceReadAuthority } from "../lib/workspaceLiveGate";
// =============================================================================
// G5 Client — Document Queries
// =============================================================================

import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "../lib/api";
import { routes } from "../lib/routes";
import type { AsyncState, DocNode, MutationHook } from "../lib/types";
import { toAsyncState, toMutationHook } from "./hookUtils";

// ---------------------------------------------------------------------------
// Query Hooks
// ---------------------------------------------------------------------------

/**
 * Fetch the list of documents for a workspace.
 * Returns all DocNode summaries for the sidebar document list.
 */
export function useDocList(
  workspaceId: string,
  authorityKey?: string,
): AsyncState<DocNode[]> {
  const contextAuthority = useWorkspaceReadAuthority();
  const authority = authorityKey ?? contextAuthority;
  const query = useQuery<DocNode[] | { docs: DocNode[] }, Error, DocNode[]>({
    queryKey: [
      "docs",
      workspaceId,
      "workspace",
      ...(authority ? [authority] : []),
    ],
    queryFn: ({ signal }) =>
      api.get<DocNode[] | { docs: DocNode[] }>(
        `/api/v1/workspaces/${workspaceId}/docs?surface=workspace`,
        { signal },
      ),
    enabled: !!workspaceId,
    // Derive `folder` from `filePath` so DocsSection groups by directory.
    // Do NOT deduplicate — the backend allows duplicate names/paths and the
    // sidebar should show all of them (each has a unique id).
    select: (response) => {
      const docs = Array.isArray(response) ? response : response.docs;
      return docs.map((d) => ({
        ...d,
        folder:
          d.folder ??
          (d.filePath
            ? d.filePath.split("/").slice(0, -1).join("/") || undefined
            : undefined),
      }));
    },
  });
  return toAsyncState(query);
}

// ---------------------------------------------------------------------------
// Mutation Hooks
// ---------------------------------------------------------------------------

/**
 * Delete a non-view Project Doc from a workspace.
 * DELETE /api/v1/workspaces/:wsId/docs/project-docs/:docId
 */
export function useDeleteDoc(
  workspaceId: string,
): MutationHook<{ docId: string }, void> {
  const queryClient = useQueryClient();
  const mutation = useMutation<void, Error, { docId: string }>({
    mutationFn: ({ docId }) =>
      api.delete<void>(routes.docs.deleteProjectDoc(workspaceId, docId)),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["docs", workspaceId] });
      queryClient.invalidateQueries({ queryKey: ["views", workspaceId] });
      queryClient.invalidateQueries({
        queryKey: ["workspace-changes", workspaceId],
      });
    },
  });
  return toMutationHook(mutation);
}

/**
 * Delete a retained View doc from a workspace.
 * DELETE /api/v1/workspaces/:wsId/docs/:docId
 *
 * Views were dropped from the sidebar in the Create HTML Explainer cutover
 * but remain first-class docs; this is the type-gated delete affordance's
 * backend call (`delete_view` server-side — the project-docs route rejects
 * view types).
 */
export function useDeleteViewDoc(
  workspaceId: string,
): MutationHook<{ docId: string }, void> {
  const queryClient = useQueryClient();
  const mutation = useMutation<void, Error, { docId: string }>({
    mutationFn: ({ docId }) =>
      api.delete<void>(routes.docs.deleteViewDoc(workspaceId, docId)),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["docs", workspaceId] });
      queryClient.invalidateQueries({ queryKey: ["views", workspaceId] });
    },
  });
  return toMutationHook(mutation);
}

export function parseRepositoryDocHandle(
  docId: string,
): { workspaceId: string; repoId: string; path: string } | null {
  if (!docId.startsWith("overlay:")) return null;
  const parts = docId.split(":");
  if (
    parts.length !== 4 ||
    !UUID_PATTERN.test(parts[1]) ||
    !UUID_PATTERN.test(parts[2])
  )
    return null;
  const encodedPath = parts[3];
  if (!encodedPath) return null;
  try {
    const padded = encodedPath.padEnd(
      encodedPath.length + ((4 - (encodedPath.length % 4)) % 4),
      "=",
    );
    const base64 = padded.replace(/-/g, "+").replace(/_/g, "/");
    const binary = atob(base64);
    const bytes = Uint8Array.from(binary, (char) => char.charCodeAt(0));
    return {
      workspaceId: parts[1],
      repoId: parts[2],
      path: new TextDecoder("utf-8", { fatal: true }).decode(bytes),
    };
  } catch {
    return null;
  }
}

export function parseRepositoryDocPath(docId: string): string | null {
  return parseRepositoryDocHandle(docId)?.path ?? null;
}

const UUID_PATTERN =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

export function repositoryDocHandle(
  doc: Pick<DocNode, "id" | "filePath" | "workspaceId" | "repoId">,
): string | null {
  if (parseRepositoryDocPath(doc.id)) return doc.id;
  const { workspaceId, repoId } = doc;
  const path = doc.filePath;
  if (!workspaceId || !repoId || !path) return null;

  const bytes = new TextEncoder().encode(path);
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  const encodedPath = btoa(binary)
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/g, "");
  return `overlay:${workspaceId}:${repoId}:${encodedPath}`;
}
