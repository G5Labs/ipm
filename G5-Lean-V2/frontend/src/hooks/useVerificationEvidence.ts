// =============================================================================
// useVerificationEvidence — whether linked state machines have saved evidence
// =============================================================================
// One query decides the evidence for every state machine a file links
// (`g5.documents.ontology.verification.status`): each linked manifest is read,
// trying its verification roots in order, then the run index of each root
// where a manifest was found. Files are read directly, never listed, so
// folders hidden from Files work too. Missing files and failed reads stay
// distinct (`tools/tlc/verification-artifact-reader.md`).
// =============================================================================

import { useEffect, useRef } from "react";
import {
  useQueries,
  useQuery,
  useQueryClient,
  type QueryClient,
} from "@tanstack/react-query";
import { ApiError } from "../lib/api";
import type { StateMachineLink } from "../lib/currentOntology";
import {
  artifactPath,
  parseRunIndex,
  type RunIndex,
} from "../lib/verificationArtifacts";
import type { LinkEvidence } from "../lib/verificationStatus";
import { fetchWorkspaceFile } from "./useFiles";
import { useWorkspaceReadAuthority } from "../lib/workspaceLiveGate";

// Evidence older than this is read again when the file changes, as when the
// link helper or a new run has just written it, and when a panel opens.
const STALE_MS = 30_000;

type Read<T> =
  | { status: "loading" }
  | { status: "ok"; data: T }
  | { status: "absent" }
  | { status: "error"; message: string };

const absent = (error: unknown) =>
  error instanceof ApiError && error.status === 404;
const message = (error: unknown) =>
  error instanceof Error ? error.message : String(error);

function toRead<T>(query: {
  isPending: boolean;
  isError: boolean;
  error: unknown;
  data: T | undefined;
}): Read<T> {
  if (query.isPending) return { status: "loading" };
  if (query.isError)
    return absent(query.error)
      ? { status: "absent" }
      : { status: "error", message: message(query.error) };
  return { status: "ok", data: query.data as T };
}

/** A file's UTF-8 text through the workspace read facade. */
export async function readText(
  workspaceId: string,
  repoId: string,
  path: string,
  signal?: AbortSignal,
): Promise<string> {
  const file = await fetchWorkspaceFile(workspaceId, repoId, path, signal);
  if (file.encoding !== "utf-8") throw new Error(`${path} is not a text file`);
  return file.content;
}

/** Query keys shared by the evidence check and the verification panel. */
function useKeys(workspaceId: string, repoId: string) {
  const authority = useWorkspaceReadAuthority();
  const key = (...parts: unknown[]) => [
    "verification-evidence",
    workspaceId,
    repoId,
    ...parts,
    ...(authority ? [authority] : []),
  ];
  return {
    text: (path: string | null) => key("text", path),
    index: (root: string | null) => key("index", root),
    evidence: (links: string) => key("links", links),
  };
}

const linkKey = (link: StateMachineLink) =>
  `${link.concern}\u0000${link.roots.join("\u0000")}`;

function readTextQuery(
  client: QueryClient,
  queryKey: unknown[],
  read: () => Promise<string>,
) {
  return client.fetchQuery({ queryKey, queryFn: read, staleTime: STALE_MS });
}

/**
 * Evidence for each linked state machine, and a way to read it again. Nothing
 * is read when `links` is empty. `revision` is the ontology file's content:
 * when it changes, evidence older than `STALE_MS` is read again.
 */
export function useVerificationEvidence(
  workspaceId: string,
  repoId: string,
  links: readonly StateMachineLink[],
  revision: string,
): { evidenceOf: (link: StateMachineLink) => LinkEvidence; retry: () => void } {
  const client = useQueryClient();
  const keys = useKeys(workspaceId, repoId);
  const distinct = [
    ...new Map(links.map((link) => [linkKey(link), link])).values(),
  ].sort((a, b) => linkKey(a).localeCompare(linkKey(b)));
  const id = distinct.map(linkKey).join("\u0001");
  const query = useQuery({
    queryKey: keys.evidence(id),
    queryFn: async () => {
      const text = (path: string) =>
        readTextQuery(client, keys.text(path), () =>
          readText(workspaceId, repoId, path),
        );
      const runIndex = (root: string) =>
        client.fetchQuery({
          queryKey: keys.index(root),
          queryFn: async () =>
            parseRunIndex(
              await readText(workspaceId, repoId, artifactPath.index(root)),
            ),
          staleTime: STALE_MS,
        });
      // The run index is the slow read: start it beside the manifests, for
      // each link's root as written, where its manifest nearly always is.
      // The read below joins it; its outcome is decided there.
      for (const root of new Set(distinct.map((link) => link.roots[0])))
        if (root) void runIndex(root).catch(() => {});
      // The first root holding the manifest decides; a missing manifest tries
      // the next root and a failed read stops there.
      const manifests = await Promise.all(
        distinct.map(async (link) => {
          for (const root of link.roots) {
            try {
              await text(artifactPath.manifest(root, link.concern));
              return { root };
            } catch (error) {
              if (!absent(error)) return { error: message(error) };
            }
          }
          return {};
        }),
      );
      const roots = [
        ...new Set(manifests.flatMap((m) => ("root" in m ? [m.root!] : []))),
      ];
      const indexes = new Map<string, Read<RunIndex>>(
        await Promise.all(
          roots.map(async (root) => {
            try {
              const index = await runIndex(root);
              return [root, { status: "ok", data: index }] as const;
            } catch (error) {
              return [
                root,
                absent(error)
                  ? ({ status: "absent" } as const)
                  : ({ status: "error", message: message(error) } as const),
              ] as const;
            }
          }),
        ),
      );
      const result: Record<string, LinkEvidence> = {};
      distinct.forEach((link, i) => {
        const found = manifests[i];
        const concern = link.concern;
        if ("error" in found) {
          result[linkKey(link)] = {
            state: "unreadable",
            reason: `${concern}: its manifest could not be read (${found.error})`,
          };
          return;
        }
        if (!("root" in found)) {
          result[linkKey(link)] = {
            state: "missing",
            reason: `${concern}: no state machine folder with a manifest`,
          };
          return;
        }
        const root = found.root!;
        const index = indexes.get(root)!;
        if (index.status === "error")
          result[linkKey(link)] = {
            state: "unreadable",
            reason: `${concern}: the run index could not be read (${index.message})`,
            root,
          };
        else if (
          index.status === "ok" &&
          index.data.rows.some((row) => row.concern === concern)
        )
          result[linkKey(link)] = { state: "found", root };
        else
          result[linkKey(link)] = {
            state: "missing",
            reason: `${concern}: no recorded run`,
            root,
          };
      });
      return result;
    },
    enabled: !!workspaceId && !!repoId && distinct.length > 0,
    staleTime: STALE_MS,
    retry: false,
  });
  // An edited file reads evidence again once it is stale.
  const seen = useRef(revision);
  useEffect(() => {
    if (seen.current === revision) return;
    seen.current = revision;
    if (query.isStale) void query.refetch();
  }, [revision, query]);
  return {
    evidenceOf: (link) => {
      if (query.isError)
        return {
          state: "unreadable",
          reason: `${link.concern}: evidence could not be read (${message(query.error)})`,
        };
      return query.data?.[linkKey(link)] ?? { state: "checking" };
    },
    retry: () => {
      for (const link of distinct)
        for (const root of link.roots) {
          void client.invalidateQueries({
            queryKey: keys.text(artifactPath.manifest(root, link.concern)),
            refetchType: "none",
          });
          void client.invalidateQueries({
            queryKey: keys.index(root),
            refetchType: "none",
          });
        }
      void query.refetch();
    },
  };
}

/** A text file for the verification panel, cached with the evidence reads. */
export function useVerificationText(
  workspaceId: string,
  repoId: string,
  path: string | null,
) {
  const keys = useKeys(workspaceId, repoId);
  const query = useQuery({
    queryKey: keys.text(path),
    queryFn: ({ signal }) => readText(workspaceId, repoId, path!, signal),
    enabled: !!workspaceId && !!repoId && !!path,
    staleTime: STALE_MS,
    retry: false,
  });
  return { read: toRead(query), refetch: query.refetch };
}

/**
 * The first of several candidate paths that exists, with its text, for a
 * findings file linked from metadata (`g5.documents.findings`). A missing
 * file falls through to the next candidate; another failure stops there.
 */
export function useFirstVerificationText(
  workspaceId: string,
  repoId: string,
  paths: readonly string[],
) {
  const keys = useKeys(workspaceId, repoId);
  const query = useQuery(firstTextQuery(keys, workspaceId, repoId, paths));
  return { read: toRead(query), refetch: query.refetch };
}

/**
 * The same reads for several findings files at once, under the same cache
 * entries, for the severity of a statement's linked findings
 * (`g5.documents.ontology.verification.status`).
 */
export function useFirstVerificationTexts(
  workspaceId: string,
  repoId: string,
  pathSets: readonly (readonly string[])[],
): Read<{ path: string; text: string }>[] {
  const keys = useKeys(workspaceId, repoId);
  return useQueries({
    queries: pathSets.map((paths) =>
      firstTextQuery(keys, workspaceId, repoId, paths),
    ),
  }).map(toRead);
}

function firstTextQuery(
  keys: ReturnType<typeof useKeys>,
  workspaceId: string,
  repoId: string,
  paths: readonly string[],
) {
  return {
    queryKey: keys.text(paths.join("\u0000")),
    queryFn: async ({ signal }: { signal: AbortSignal }) => {
      let missing: unknown = null;
      for (const path of paths) {
        try {
          return {
            path,
            text: await readText(workspaceId, repoId, path, signal),
          };
        } catch (error) {
          if (!absent(error)) throw error;
          missing = error;
        }
      }
      throw missing;
    },
    enabled: !!workspaceId && !!repoId && paths.length > 0,
    staleTime: STALE_MS,
    retry: false,
  };
}

/** The run index of a verification root, shared with the evidence reads. */
export function useRunIndex(
  workspaceId: string,
  repoId: string,
  root: string | null,
) {
  const keys = useKeys(workspaceId, repoId);
  const query = useQuery({
    queryKey: keys.index(root),
    queryFn: async ({ signal }) =>
      parseRunIndex(
        await readText(workspaceId, repoId, artifactPath.index(root!), signal),
      ),
    enabled: !!workspaceId && !!repoId && !!root,
    staleTime: STALE_MS,
    retry: false,
  });
  return { read: toRead(query), refetch: query.refetch };
}

export type { Read };
