// G5 Client — useWorkspaceNames
//
// Shared access to the workspace UUID → display-name map used to relabel
// `docs/<workspace_id>` segments in human-facing doc paths (sidebar doc rows,
// create-doc preview, commit modal file list, chat citation tooltips) — see
// `workspace_docs_folders.pm_view.md` D8. Exposed via context so cross-cutting
// consumers can read it without prop-drilling through intermediaries. The map
// is built once in WorkspacePage (from the org workspaces list already fetched
// for the switcher) and provided to the subtree. Default is an empty map, so
// any consumer rendered outside the provider safely falls back to the raw UUID.

import { createContext, useContext } from "react";

const WorkspaceNamesContext = createContext<Record<string, string>>({});

export const WorkspaceNamesProvider = WorkspaceNamesContext.Provider;

/**
 * Returns the workspace UUID → display-name map for the active org. An empty
 * map (the default outside a provider) means callers fall back to the raw UUID.
 * Pair with `labelWorkspaceFolderInPath` from `lib/utils` to relabel doc paths.
 */
export function useWorkspaceNames(): Record<string, string> {
  return useContext(WorkspaceNamesContext);
}
