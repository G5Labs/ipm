// =============================================================================
// G5 Client — Explainer Hooks
// =============================================================================
// Data hooks for the explainer creation flow:
// - useExplainerTemplates: GET /branches/:branchId/explainer-templates
// - useSaveExplainerTemplate: POST /branches/:branchId/explainer-templates
// Wire DTO types live locally in this hook file.
// Also exports the pure recent-templates localStorage helpers used by the
// Create Explainer modal's template combobox Recent group.
// =============================================================================

import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { api, ApiError } from "../lib/api";
import type { AsyncState, MutationHook } from "../lib/types";
import { toAsyncState, toMutationHook } from "./hookUtils";

// ── Wire Types ──────────────────────────────────────────────────────────

export type ExplainerFormat = "page" | "slides" | "markdown";

/** One template row from GET /api/v1/branches/:branchId/explainer-templates. */
export interface ExplainerTemplate {
  name: string;
  slug: string;
  prompt: string;
  source: "builtin" | "saved";
}

/** Request body for POST /api/v1/branches/:repoId/explainer-templates. */
export interface SaveExplainerTemplateInput {
  name: string;
  slug: string;
  prompt: string;
  overwrite?: boolean;
}

/** 200 response data for the save-template endpoint. */
export interface SaveExplainerTemplateResult {
  headCommit: string;
  template: ExplainerTemplate;
}

// ── Mutation / Query Hooks ──────────────────────────────────────────────

/**
 * List explainer templates committed on a branch (six builtins first, then
 * saved templates). `repoId` is the workspace docs-branch id.
 */
export function useExplainerTemplates(
  workspaceId: string,
  repoId: string,
): AsyncState<ExplainerTemplate[]> {
  const query = useQuery<ExplainerTemplate[], Error>({
    queryKey: ["explainer-templates", workspaceId, repoId],
    queryFn: ({ signal }) =>
      api.get<ExplainerTemplate[]>(
        `/api/v1/workspaces/${workspaceId}/repos/${repoId}/explainer-templates`,
        { signal },
      ),
    enabled: !!workspaceId && !!repoId,
    staleTime: 30_000,
  });
  return toAsyncState(query);
}

/**
 * Save (commit) an explainer template on a branch.
 *
 * A duplicate slug without `overwrite: true` fails with HTTP 409 CONFLICT —
 * detect it with `isExplainerTemplateConflict` so the modal can show an
 * inline overwrite confirmation instead of a generic error.
 */
export function useSaveExplainerTemplate(
  workspaceId: string,
  repoId: string,
): MutationHook<SaveExplainerTemplateInput, SaveExplainerTemplateResult> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    SaveExplainerTemplateResult,
    Error,
    SaveExplainerTemplateInput
  >({
    mutationFn: (input) =>
      api.post<SaveExplainerTemplateResult>(
        `/api/v1/workspaces/${workspaceId}/repos/${repoId}/explainer-templates`,
        input,
      ),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: ["explainer-templates", workspaceId, repoId],
      });
    },
  });
  return toMutationHook(mutation);
}

/**
 * True when a save-template failure is the duplicate-slug 409 CONFLICT that
 * should be answered with an overwrite confirmation.
 */
export function isExplainerTemplateConflict(error: unknown): boolean {
  return error instanceof ApiError && error.status === 409;
}

// ── Pure Modal Helpers ──────────────────────────────────────────────────

// Loud realtime length limits (spec D12, user decision 2026-08-28): the modal
// never uses a silently-truncating native `maxLength`; instead each field
// shows a live counter and an over-limit error while the user can still edit.
// These keep assembled chat requests bounded while preserving editable text.

/** Max characters for the Instruction field. */
export const EXPLAINER_INSTRUCTION_MAX_CHARS = 10_000;

/** Max characters for the generation prompt. */
export const EXPLAINER_PROMPT_MAX_CHARS = 20_000;

/** Max characters for the optional file-name field. */
export const EXPLAINER_FILE_NAME_MAX_CHARS = 120;

/** Max characters for a save-as-template display name. */
export const EXPLAINER_TEMPLATE_NAME_MAX_CHARS = 80;

/**
 * Server-side max length for a template SLUG (mirrors the backend's
 * `EXPLAINER_TEMPLATE_SLUG_MAX_LEN`). The modal validates the slug DERIVED
 * from the template name against this in realtime: a 65-80 character ASCII
 * name passes the name counter but its slug would only fail after Save with
 * a server 422 (PR #2299 review finding).
 */
export const EXPLAINER_TEMPLATE_SLUG_MAX_CHARS = 64;

/**
 * Count characters as Unicode code points, matching the server's Rust
 * `chars().count()` on the same limits: `String.length` counts UTF-16 code
 * units and double-counts non-BMP characters such as emoji, which would make
 * the client and server disagree about the mirrored limits (spec D12;
 * PR #2299 review finding).
 */
export function explainerCharCount(value: string): number {
  let count = 0;
  for (const _codePoint of value) count += 1;
  return count;
}

/** Derive a template slug from a display name (kebab-case). */
export function explainerTemplateSlug(name: string): string {
  return name
    .trim()
    .toLowerCase()
    .replace(/\s+/g, "-")
    .replace(/[^a-z0-9-]/g, "")
    .replace(/-+/g, "-")
    .replace(/^-+|-+$/g, "");
}

/**
 * Focus-node picker match: case-insensitive substring over the node name AND
 * over the componentId segments.
 */
export function focusNodeMatches(
  node: { title: string; componentId: string | null },
  query: string,
): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return false;
  if (node.title.toLowerCase().includes(q)) return true;
  return (node.componentId ?? "")
    .split("/")
    .some((segment) => segment.toLowerCase().includes(q));
}

// ── Recent-Templates Helpers (pure + localStorage) ──────────────────────

/** Most-recently-used template slugs are capped at this length. */
export const RECENT_EXPLAINER_TEMPLATES_CAP = 5;

/** Per-workspace localStorage key for recently used template slugs. */
export function recentExplainerTemplatesKey(workspaceId: string): string {
  return `g5:explainerTemplates:recent:${workspaceId}`;
}

/**
 * Pure list update: put `slug` first, dedupe, cap at
 * RECENT_EXPLAINER_TEMPLATES_CAP. Does not touch storage.
 */
export function pushRecentExplainerTemplate(
  recents: readonly string[],
  slug: string,
): string[] {
  return [slug, ...recents.filter((existing) => existing !== slug)].slice(
    0,
    RECENT_EXPLAINER_TEMPLATES_CAP,
  );
}

/**
 * Read the most-recent-first slug list for a workspace from localStorage.
 * Malformed or missing entries yield an empty list.
 */
export function readRecentExplainerTemplates(workspaceId: string): string[] {
  if (typeof localStorage === "undefined") return [];
  try {
    const raw = localStorage.getItem(recentExplainerTemplatesKey(workspaceId));
    if (!raw) return [];
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed
      .filter((entry): entry is string => typeof entry === "string")
      .slice(0, RECENT_EXPLAINER_TEMPLATES_CAP);
  } catch {
    return [];
  }
}

/**
 * Record a template use: prepend `slug` (deduped, capped) and persist.
 * Returns the updated list. Storage failures are non-fatal.
 */
export function recordRecentExplainerTemplate(
  workspaceId: string,
  slug: string,
): string[] {
  const next = pushRecentExplainerTemplate(
    readRecentExplainerTemplates(workspaceId),
    slug,
  );
  if (typeof localStorage !== "undefined") {
    try {
      localStorage.setItem(
        recentExplainerTemplatesKey(workspaceId),
        JSON.stringify(next),
      );
    } catch {
      // Quota/serialization failures must not break the create flow.
    }
  }
  return next;
}
