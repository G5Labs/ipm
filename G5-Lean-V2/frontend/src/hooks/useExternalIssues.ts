// =============================================================================
// G5 Client — Issues (GitHub issue domains and the native G5 tracker)
// =============================================================================
// Backend contract (issue domains, links and reads):
//   Org admin:
//     POST   /api/v1/orgs/:orgId/issue-domains              → IssueDomainDTO
//     PATCH  /api/v1/orgs/:orgId/issue-domains/:domainId    → IssueDomainDTO
//     DELETE /api/v1/orgs/:orgId/issue-domains/:domainId    → 204
//     GET    /api/v1/orgs/:orgId/issue-domains/:domainId/export → native JSON
//   Org member:
//     GET /api/v1/orgs/:orgId/issue-domains                 → { domains }
//     GET /api/v1/orgs/:orgId/issues/search?q=&state=[&assignee=me|none] → search response
//     GET /api/v1/orgs/:orgId/issues/github/:repoId/:number → issue detail
//     GET /api/v1/orgs/:orgId/issues/g5/:number             → native issue detail
//     GET  /api/v1/orgs/:orgId/issues/create-target          → creation target
//     POST /api/v1/orgs/:orgId/issues                        → issue creation
//   Issue page (g5.issue.native.page), ISSUE = issues/g5/:number or
//   issues/github/:repositoryId/:number:
//     PATCH /api/v1/orgs/:orgId/issues/g5/:number            → summary
//     GET   /api/v1/orgs/:orgId/issues/github/:repoId/:number/write-target
//     PATCH /api/v1/orgs/:orgId/issues/github/:repoId/:number → issue edit
//     POST  /api/v1/orgs/:orgId/ISSUE/state                  → state change
//     POST  /api/v1/orgs/:orgId/ISSUE/comments               → comment result
//     PATCH /api/v1/orgs/:orgId/ISSUE/comments/:commentId    → comment result
//     POST  /api/v1/orgs/:orgId/ISSUE/labels                 → label change
//   Label catalog (g5.issue.labels.contract), DOMAIN = issue-domains/:domainId:
//     GET    /api/v1/orgs/:orgId/DOMAIN/labels?page=N        → label page
//     POST   /api/v1/orgs/:orgId/DOMAIN/labels               → label write (201)
//     PATCH  /api/v1/orgs/:orgId/DOMAIN/labels/:labelId      → label write
//     DELETE /api/v1/orgs/:orgId/DOMAIN/labels/:labelId      → label write
//   Workspace:
//     GET  /api/v1/workspaces/:id/issue-links               → { links }
//     POST /api/v1/workspaces/:id/issue-links               → link (201)
//     POST /api/v1/workspaces/:id/issue-links/:linkId/unlink
// Search, detail and link summaries are live tracker reads; typed failures
// carry `error.details.issueError` and must never render as empty results.
// =============================================================================

import { useEffect, useMemo, useRef } from "react";
import { useParams } from "react-router-dom";
import {
  useInfiniteQuery,
  useMutation,
  useQuery,
  useQueryClient,
  type QueryClient,
} from "@tanstack/react-query";

import { ApiError, api } from "../lib/api";
import { routes } from "../lib/routes";
import type {
  AsyncState,
  CreateIssueDomainInput,
  CreateWorkspaceIssueLinkInput,
  ExternalIssueAssigneeFilter,
  ExternalIssueDetailDTO,
  ExternalIssueSearchResponseDTO,
  ExternalIssueStateFilter,
  ExternalIssueSummaryDTO,
  IssueDomainDTO,
  IssueDomainListDTO,
  MutationHook,
  UpdateIssueDomainInput,
  CommentResultDTO,
  CreateIssueInput,
  CreateLabelInput,
  DeleteLabelInput,
  EditLabelInput,
  ExternalIssueLabelDTO,
  IssueLabelChangeDTO,
  IssueLabelChangeInput,
  LabelPageDTO,
  LabelWriteDTO,
  EditGithubIssueInput,
  EditIssueCommentInput,
  EditIssuePageCommentInput,
  EditNativeIssueInput,
  ExternalIssueLocatorDTO,
  IssueEditDTO,
  IssuePageCommentInput,
  IssueWriteTargetDTO,
  IssueCreateTargetDTO,
  IssueCreationDTO,
  IssueStateChangeDTO,
  IssueStateChangeInput,
  PostIssueCommentInput,
  WorkspaceActivityCurrentDTO,
  WorkspaceActivityDTO,
  WorkspaceIssueLinkDTO,
  WorkspaceIssueLinkListDTO,
} from "../lib/types";
import {
  isExternalIssuesEnabled,
  isOrgExternalIssuesEnabled,
} from "../lib/externalIssuesFlags";
import { NATIVE_ISSUE_REPOSITORY_ID } from "../lib/issuePage";
import { MAX_LABEL_CATALOG_PAGES } from "../lib/issueLabels";
import { useOrgScopedConfig } from "./useSettings";
import { ACTIVE_TASK_POLL_INTERVAL } from "./useTasks";
import { useAuth } from "./useAuth";
import { toAsyncState, toMutationHook } from "./hookUtils";
import { useInstanceInfo } from "./useSettings";

// GitHub-backed reads: a short stale window keeps tab switches and focus
// changes from re-contacting GitHub, and failures surface immediately rather
// than after silent retries against a 5 s upstream budget.
const GITHUB_READ_STALE_MS = 30_000;

export const externalIssueKeys = {
  domains: (orgId: string) => ["issue-domains", orgId] as const,
  searchAll: ["external-issue-search"] as const,
  search: (
    orgId: string,
    q: string,
    state: ExternalIssueStateFilter,
    assignee: ExternalIssueAssigneeFilter,
  ) => ["external-issue-search", orgId, q, state, assignee] as const,
  resolve: (orgId: string, reference: string) =>
    ["external-issue-resolve", orgId, reference] as const,
  createTarget: (orgId: string) => ["issue-create-target", orgId] as const,
  writeTarget: (orgId: string, locator: ExternalIssueLocatorDTO) =>
    [
      "issue-write-target",
      orgId,
      locator.repositoryId,
      locator.number,
    ] as const,
  detailAll: ["external-issue"] as const,
  detail: (orgId: string, repositoryId: string, number: number) =>
    ["external-issue", orgId, repositoryId, number] as const,
  linksAll: ["workspace-issue-links"] as const,
  activity: (workspaceId: string) =>
    ["workspace-activity", workspaceId] as const,
  activityCurrent: (workspaceId: string) =>
    ["workspace-activity-current", workspaceId] as const,
  links: (workspaceId: string) =>
    ["workspace-issue-links", workspaceId] as const,
  labelsAll: ["issue-labels"] as const,
  labels: (orgId: string, domainId: string) =>
    ["issue-labels", orgId, domainId] as const,
};

/**
 * The organization named by the `:org` route segment, from the caller's
 * memberships. Workspace center tabs render under `/:org/:workspace`.
 */
export function useRouteOrgMembership() {
  const { org } = useParams<{ org: string }>();
  const { user } = useAuth();
  return useMemo(
    () => user?.orgs.find((candidate) => candidate.orgSlug === org) ?? null,
    [org, user],
  );
}

export type ExternalIssuesFeatureState =
  | { status: "loading" }
  | { status: "enabled" }
  /**
   * `switchedOff`: the deployment offers external issues but the
   * organization has them off. Cleanup (unlinking, removing a domain) stays
   * available then, without any issue content.
   */
  | { status: "disabled"; switchedOff?: boolean }
  | { status: "error"; error: Error; retry: () => void };

/**
 * Issues for an organization: the deployment offers them (the `issues`
 * instance feature, or `external_issues` from an older server) and the
 * organization turned them on (its own `external_issues_enabled` setting). A failed instance-info request
 * is reported as `error` (with a retry). Fail-closed: an unreadable org
 * setting means disabled. Organization reads wait for the active tenant.
 */
export function useExternalIssuesFeature(
  orgId: string | null | undefined,
): ExternalIssuesFeatureState {
  const queryClient = useQueryClient();
  const instanceInfo = useInstanceInfo();
  const { user } = useAuth();
  const orgActive = !!orgId && user?.activeOrgId === orgId;
  const offered =
    instanceInfo.status === "ok" && isExternalIssuesEnabled(instanceInfo.data);
  const orgConfig = useOrgScopedConfig(offered && orgActive ? orgId : "");
  if (instanceInfo.status === "loading") return { status: "loading" };
  if (instanceInfo.status === "error") {
    return {
      status: "error",
      error: instanceInfo.error,
      retry: () =>
        void queryClient.refetchQueries({ queryKey: ["instance-info"] }),
    };
  }
  if (!offered || !orgId) return { status: "disabled" };
  if (!orgActive) return { status: "loading" };
  if (orgConfig.status === "loading") return { status: "loading" };
  if (orgConfig.status !== "ok") return { status: "disabled" };
  return isOrgExternalIssuesEnabled(orgConfig.data)
    ? { status: "enabled" }
    : { status: "disabled", switchedOff: true };
}

// ---------------------------------------------------------------------------
// Issue domains
// ---------------------------------------------------------------------------

export function useIssueDomains(
  orgId: string,
  enabled = true,
): AsyncState<IssueDomainListDTO> & { refetch: () => void } {
  const query = useQuery<IssueDomainListDTO, Error>({
    queryKey: externalIssueKeys.domains(orgId),
    queryFn: ({ signal }) =>
      api.get<IssueDomainListDTO>(routes.orgs.issueDomains(orgId), { signal }),
    enabled: !!orgId && enabled,
  });
  return { ...toAsyncState(query), refetch: () => void query.refetch() };
}

function invalidateAfterDomainChange(queryClient: QueryClient, orgId: string) {
  void queryClient.invalidateQueries({
    queryKey: externalIssueKeys.domains(orgId),
  });
  // Domain membership and trust feed search results, issue reads and link
  // summaries.
  invalidateIssueProjections(queryClient);
  void queryClient.invalidateQueries({ queryKey: externalIssueKeys.linksAll });
}

export function useCreateIssueDomain(
  orgId: string,
): MutationHook<CreateIssueDomainInput, IssueDomainDTO> {
  const queryClient = useQueryClient();
  const mutation = useMutation<IssueDomainDTO, Error, CreateIssueDomainInput>({
    mutationFn: (input) =>
      api.post<IssueDomainDTO>(routes.orgs.issueDomains(orgId), input),
    onSuccess: () => invalidateAfterDomainChange(queryClient, orgId),
  });
  return toMutationHook(mutation);
}

export function useUpdateIssueDomain(
  orgId: string,
): MutationHook<UpdateIssueDomainInput, IssueDomainDTO> {
  const queryClient = useQueryClient();
  const mutation = useMutation<IssueDomainDTO, Error, UpdateIssueDomainInput>({
    mutationFn: ({ domainId, contentTrusted, commentsEnabled }) =>
      api.patch<IssueDomainDTO>(routes.orgs.issueDomain(orgId, domainId), {
        contentTrusted,
        commentsEnabled,
      }),
    onSuccess: () => invalidateAfterDomainChange(queryClient, orgId),
  });
  return toMutationHook(mutation);
}

export function useDeleteIssueDomain(
  orgId: string,
): MutationHook<string, void> {
  const queryClient = useQueryClient();
  const mutation = useMutation<void, Error, string>({
    mutationFn: (domainId) =>
      api.delete<void>(routes.orgs.issueDomain(orgId, domainId)),
    onSuccess: () => invalidateAfterDomainChange(queryClient, orgId),
  });
  return toMutationHook(mutation);
}

// ---------------------------------------------------------------------------
// Search and read
// ---------------------------------------------------------------------------

export interface ExternalIssueSearchState {
  state: AsyncState<ExternalIssueSearchResponseDTO>;
  isFetching: boolean;
  refetch: () => void;
}

/**
 * `assignee` "me" limits results to issues assigned to the caller: their
 * connected GitHub account on GitHub domains (the API refuses it with
 * `github_connection_required` when the caller has not connected one), the
 * caller on the native tracker; "none" limits them to unassigned issues.
 */
export function useExternalIssueSearch(
  orgId: string,
  q: string,
  stateFilter: ExternalIssueStateFilter,
  assignee: ExternalIssueAssigneeFilter,
  enabled = true,
  options: { staleTime?: number } = {},
): ExternalIssueSearchState {
  const query = useQuery<ExternalIssueSearchResponseDTO, Error>({
    queryKey: externalIssueKeys.search(orgId, q, stateFilter, assignee),
    queryFn: ({ signal }) =>
      api.get<ExternalIssueSearchResponseDTO>(routes.orgs.issueSearch(orgId), {
        signal,
        params: {
          q,
          state: stateFilter,
          assignee: assignee === "any" ? undefined : assignee,
        },
      }),
    enabled: !!orgId && enabled,
    retry: false,
    staleTime: options.staleTime ?? GITHUB_READ_STALE_MS,
  });
  return {
    state: toAsyncState(query),
    isFetching: query.isFetching,
    refetch: () => void query.refetch(),
  };
}

/**
 * Resolve a URL, `owner/name#n` or native `ACME-12` reference inside the
 * org's issue domains.
 */
export function useResolveExternalIssue(
  orgId: string,
  reference: string,
  enabled = true,
): AsyncState<ExternalIssueSummaryDTO> {
  const query = useQuery<ExternalIssueSummaryDTO, Error>({
    queryKey: externalIssueKeys.resolve(orgId, reference),
    queryFn: ({ signal }) =>
      api.get<ExternalIssueSummaryDTO>(routes.orgs.issueResolve(orgId), {
        signal,
        params: { ref: reference },
      }),
    enabled: !!orgId && !!reference && enabled,
    retry: false,
    staleTime: GITHUB_READ_STALE_MS,
  });
  return toAsyncState(query);
}

export function useExternalIssueDetail(
  orgId: string,
  repositoryId: string,
  number: number,
  enabled = true,
): AsyncState<ExternalIssueDetailDTO> & { refetch: () => void } {
  const query = useQuery<ExternalIssueDetailDTO, Error>({
    queryKey: externalIssueKeys.detail(orgId, repositoryId, number),
    queryFn: ({ signal }) =>
      api.get<ExternalIssueDetailDTO>(
        repositoryId === NATIVE_ISSUE_REPOSITORY_ID
          ? routes.orgs.nativeIssue(orgId, number)
          : routes.orgs.githubIssue(orgId, repositoryId, number),
        { signal },
      ),
    enabled: !!orgId && !!repositoryId && number > 0 && enabled,
    retry: false,
    staleTime: GITHUB_READ_STALE_MS,
  });
  return { ...toAsyncState(query), refetch: () => void query.refetch() };
}

// ---------------------------------------------------------------------------
// Workspace links
// ---------------------------------------------------------------------------

export function useWorkspaceIssueLinks(
  workspaceId: string,
  enabled = true,
): AsyncState<WorkspaceIssueLinkListDTO> & { refetch: () => void } {
  const query = useQuery<WorkspaceIssueLinkListDTO, Error>({
    queryKey: externalIssueKeys.links(workspaceId),
    queryFn: ({ signal }) =>
      api.get<WorkspaceIssueLinkListDTO>(
        routes.workspaces.issueLinks(workspaceId),
        { signal },
      ),
    enabled: !!workspaceId && enabled,
    retry: false,
    staleTime: GITHUB_READ_STALE_MS,
  });
  return { ...toAsyncState(query), refetch: () => void query.refetch() };
}

/** A link or summary names its issue by tracker and external id. */
type IssueIdentity = { trackerId: string; externalIssueId: string };

function linksSameIssue(
  list: WorkspaceIssueLinkListDTO | undefined,
  issue: IssueIdentity,
): boolean {
  return !!list?.links.some(
    (link) =>
      link.trackerId === issue.trackerId &&
      link.externalIssueId === issue.externalIssueId,
  );
}

/**
 * Cancel in-flight reads of the cached link lists a state write can change:
 * the submitting workspace's and any that links the same issue.
 */
async function cancelLinkReads(
  queryClient: QueryClient,
  workspaceId: string,
  issue: IssueIdentity | null,
) {
  const touched = queryClient
    .getQueriesData<WorkspaceIssueLinkListDTO>({
      queryKey: externalIssueKeys.linksAll,
    })
    .filter(
      ([key, list]) =>
        key[1] === workspaceId ||
        (issue !== null && linksSameIssue(list, issue)),
    );
  await Promise.all(
    touched.map(([queryKey]) =>
      queryClient.cancelQueries({ queryKey, exact: true }),
    ),
  );
}

/** Issue search rows and detail pages project link and state facts. */
function invalidateIssueProjections(queryClient: QueryClient) {
  void queryClient.invalidateQueries({ queryKey: externalIssueKeys.searchAll });
  void queryClient.invalidateQueries({ queryKey: externalIssueKeys.detailAll });
}

function invalidateAfterLinkChange(
  queryClient: QueryClient,
  workspaceId: string,
) {
  void queryClient.invalidateQueries({
    queryKey: externalIssueKeys.links(workspaceId),
  });
  // The Issues composer is bound to the current primary issue, while each
  // browser draft stays bound to its original destination.
  void queryClient.invalidateQueries({
    queryKey: externalIssueKeys.activity(workspaceId),
  });
  // Primary ownership and link lists appear on search results and details.
  invalidateIssueProjections(queryClient);
}

export function useLinkWorkspaceIssue(
  workspaceId: string,
): MutationHook<CreateWorkspaceIssueLinkInput, WorkspaceIssueLinkDTO> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    WorkspaceIssueLinkDTO,
    Error,
    CreateWorkspaceIssueLinkInput
  >({
    mutationFn: (input) =>
      api.post<WorkspaceIssueLinkDTO>(
        routes.workspaces.issueLinks(workspaceId),
        input,
      ),
    onSuccess: () => invalidateAfterLinkChange(queryClient, workspaceId),
  });
  return toMutationHook(mutation);
}

/**
 * Close (as completed) or reopen a linked issue: a GitHub issue as the
 * caller's own GitHub account (g5.external-issues.transition), a native one
 * in G5 (g5.issue.native.writes). An applied change updates the
 * cached link's summary from GitHub's answer; an unknown outcome re-reads
 * the links, whose live summaries show the state.
 */
export function useChangeIssueState(
  workspaceId: string,
): MutationHook<IssueStateChangeInput, IssueStateChangeDTO> {
  const queryClient = useQueryClient();
  // The workspace is bound at submission: a request still in flight after
  // the page moves to another workspace updates the workspace it was sent
  // for, not the one now rendered.
  const mutation = useMutation<
    IssueStateChangeDTO,
    Error,
    IssueStateChangeInput & { workspaceId: string }
  >({
    mutationFn: ({ workspaceId, linkId, state }) =>
      api.post<IssueStateChangeDTO>(
        routes.workspaces.issueLinkState(workspaceId, linkId),
        { state },
      ),
    // A links read started before the write must not land after it and
    // restore the old state in any workspace that links the issue: in-flight
    // reads of the lists the write can change are cancelled on both sides of
    // the write before the answer is installed. Other lists (a workspace
    // whose first load is in flight) are left alone.
    onMutate: async ({ workspaceId, linkId }) => {
      const link = queryClient
        .getQueryData<WorkspaceIssueLinkListDTO>(
          externalIssueKeys.links(workspaceId),
        )
        ?.links.find((candidate) => candidate.id === linkId);
      await cancelLinkReads(queryClient, workspaceId, link ?? null);
    },
    onSuccess: async (result, { workspaceId, linkId }) => {
      await cancelLinkReads(queryClient, workspaceId, result.issue);
      const answered = result.issue;
      if (result.outcome === "applied" && answered) {
        // Every cached list that links this issue, in any workspace, shows
        // GitHub's answer; a link is matched by its issue, not its id.
        queryClient.setQueriesData<WorkspaceIssueLinkListDTO>(
          { queryKey: externalIssueKeys.linksAll },
          (list) =>
            list && {
              links: list.links.map((link) =>
                link.id === linkId ||
                (link.trackerId === answered.trackerId &&
                  link.externalIssueId === answered.externalIssueId)
                  ? {
                      ...link,
                      issueError: null,
                      issue: {
                        ...answered,
                        // The list attaches the primary owner; the write's
                        // answer does not carry it.
                        primaryWorkspace:
                          link.issue?.primaryWorkspace ??
                          answered.primaryWorkspace,
                      },
                    }
                  : link,
              ),
            },
        );
      } else {
        void queryClient.invalidateQueries({
          queryKey: externalIssueKeys.links(workspaceId),
        });
      }
      // The issue page and search rows show the state; the timeline gains
      // the closed or reopened event.
      invalidateIssueProjections(queryClient);
      invalidateActivity(queryClient, workspaceId);
    },
    // A refusal or a lost answer re-reads the row: a refusal may mean the
    // issue or its domain changed (the row's summary is stale), and a lost
    // answer (network failure, 5xx) may hide an applied change.
    onError: (_error, { workspaceId }) => {
      invalidateAfterLinkChange(queryClient, workspaceId);
      invalidateActivity(queryClient, workspaceId);
    },
  });
  const bound = toMutationHook(mutation);
  return useMemo(
    () => ({
      ...bound,
      mutate: (input: IssueStateChangeInput) =>
        bound.mutate({ ...input, workspaceId }),
      mutateAsync: (input: IssueStateChangeInput) =>
        bound.mutateAsync({ ...input, workspaceId }),
    }),
    // eslint-disable-next-line react-hooks/exhaustive-deps -- `bound` is a fresh object each render; its fields are stable
    [bound.isLoading, bound.error, bound.data, workspaceId],
  );
}

export function useUnlinkWorkspaceIssue(
  workspaceId: string,
): MutationHook<string, { unlinked: boolean }> {
  const queryClient = useQueryClient();
  const mutation = useMutation<{ unlinked: boolean }, Error, string>({
    mutationFn: (linkId) =>
      api.post<{ unlinked: boolean }>(
        routes.workspaces.unlinkIssueLink(workspaceId, linkId),
        {},
      ),
    onSuccess: () => invalidateAfterLinkChange(queryClient, workspaceId),
  });
  return toMutationHook(mutation);
}

// ---------------------------------------------------------------------------
// Workspace Activity
// ---------------------------------------------------------------------------

/**
 * Activity pages, newest page first: page 0 is the most recent entries and
 * each further page is older. Entries within a page are newest first too, so
 * the pages flatten in load order (ui g5.workbench.landing).
 */
export function useWorkspaceActivity(workspaceId: string, enabled = true) {
  return useInfiniteQuery<
    WorkspaceActivityDTO,
    Error,
    { pages: WorkspaceActivityDTO[] },
    readonly unknown[],
    string | null
  >({
    queryKey: externalIssueKeys.activity(workspaceId),
    queryFn: ({ pageParam, signal }) =>
      api.get<WorkspaceActivityDTO>(routes.workspaces.activity(workspaceId), {
        signal,
        params: pageParam ? { before: pageParam } : undefined,
      }),
    initialPageParam: null,
    getNextPageParam: (page) => page.earlierCursor,
    enabled: !!workspaceId && enabled,
    retry: false,
    staleTime: GITHUB_READ_STALE_MS,
  });
}

/**
 * Ongoing conversations and tasks (ui g5.ui.activity-current), newest first.
 * Polled while a consumer is mounted; one request serves the page and the
 * rail. When an entry leaves the set (its work ended) the record is
 * invalidated so the entry reappears there at its last-activity position.
 */
export function useWorkspaceActivityCurrent(
  workspaceId: string,
  enabled = true,
) {
  const queryClient = useQueryClient();
  const query = useQuery<WorkspaceActivityCurrentDTO, Error>({
    queryKey: externalIssueKeys.activityCurrent(workspaceId),
    queryFn: ({ signal }) =>
      api.get<WorkspaceActivityCurrentDTO>(
        routes.workspaces.activityCurrent(workspaceId),
        { signal },
      ),
    enabled: !!workspaceId && enabled,
    retry: false,
    // The task list's cadence, so the badge and the Tasks section agree.
    refetchInterval: ACTIVE_TASK_POLL_INTERVAL,
  });
  const keys = query.data?.entries.map((entry) => entry.key).join("\n");
  // The last set seen, per workspace, so a switch never reads another
  // workspace's entries as having left.
  const previousKeys = useRef<{ workspaceId: string; keys: string } | null>(
    null,
  );
  useEffect(() => {
    if (keys === undefined) return;
    const previous =
      previousKeys.current?.workspaceId === workspaceId
        ? previousKeys.current.keys
        : undefined;
    previousKeys.current = { workspaceId, keys };
    if (previous === undefined || previous === keys) return;
    const now = new Set(keys.split("\n"));
    const left = previous.split("\n").some((key) => key && !now.has(key));
    if (left) invalidateActivity(queryClient, workspaceId);
  }, [keys, queryClient, workspaceId]);
  return query;
}

// ---------------------------------------------------------------------------
// Primary-issue comment publications
// ---------------------------------------------------------------------------

function invalidateActivity(queryClient: QueryClient, workspaceId: string) {
  void queryClient.invalidateQueries({
    queryKey: externalIssueKeys.activity(workspaceId),
  });
}

/**
 * Post a comment on the primary issue: as the caller's GitHub account, or
 * stored in G5 for a native issue. An `outcome_unknown` answer is retried
 * with the same idempotency key; the server looks for the comment first.
 */
export function usePostIssueComment(
  workspaceId: string,
): MutationHook<PostIssueCommentInput, CommentResultDTO> {
  const queryClient = useQueryClient();
  const mutation = useMutation<CommentResultDTO, Error, PostIssueCommentInput>({
    mutationFn: (input) =>
      api.post<CommentResultDTO>(
        routes.workspaces.activityComments(workspaceId),
        input,
      ),
    onSettled: () => invalidateActivity(queryClient, workspaceId),
  });
  return toMutationHook(mutation);
}

/**
 * Replace the text of the caller's own comment on the primary issue; a
 * native comment's edit goes to the native route.
 */
export function useEditIssueComment(
  workspaceId: string,
): MutationHook<EditIssueCommentInput, CommentResultDTO> {
  const queryClient = useQueryClient();
  const mutation = useMutation<CommentResultDTO, Error, EditIssueCommentInput>({
    mutationFn: ({ commentId, provider, ...input }) =>
      api.patch<CommentResultDTO>(
        provider === "g5"
          ? routes.workspaces.activityNativeComment(workspaceId, commentId)
          : routes.workspaces.activityGithubComment(workspaceId, commentId),
        input,
      ),
    onSettled: () => invalidateActivity(queryClient, workspaceId),
  });
  return toMutationHook(mutation);
}

// ---------------------------------------------------------------------------
// Issue creation (g5.external-issues.creation)
// ---------------------------------------------------------------------------

/**
 * Whether the caller can create an issue now, where it would go, as which
 * GitHub account, and who it can be assigned to. A live GitHub read (the
 * repository's visibility); creation repeats every check at send time.
 */
export function useIssueCreateTarget(
  orgId: string,
  enabled = true,
): AsyncState<IssueCreateTargetDTO> & { refetch: () => void } {
  const query = useQuery<IssueCreateTargetDTO, Error>({
    queryKey: externalIssueKeys.createTarget(orgId),
    queryFn: ({ signal }) =>
      api.get<IssueCreateTargetDTO>(routes.orgs.issueCreateTarget(orgId), {
        signal,
      }),
    enabled: !!orgId && enabled,
    retry: false,
    staleTime: GITHUB_READ_STALE_MS,
  });
  return { ...toAsyncState(query), refetch: () => void query.refetch() };
}

/**
 * Create an issue in the organization's default creation target as the
 * caller's GitHub account, or (`checkOnly`) look for the one an earlier
 * attempt with the same idempotency key created. Every request with a key
 * looks before it sends, so a retry never creates a second issue once the
 * first is listed.
 */
export function useCreateIssue(
  orgId: string,
): MutationHook<CreateIssueInput, IssueCreationDTO> {
  const queryClient = useQueryClient();
  const mutation = useMutation<IssueCreationDTO, Error, CreateIssueInput>({
    mutationFn: (input) =>
      api.post<IssueCreationDTO>(routes.orgs.issues(orgId), input),
    onSuccess: (result) => {
      // A new open issue belongs in Home's slice and the full search.
      if (result.state === "created") {
        void queryClient.invalidateQueries({
          queryKey: externalIssueKeys.searchAll,
        });
      }
    },
    onError: (error) => {
      // A refusal of the destination (changed target, switch off, now
      // public, no GitHub account) means the target the person saw is stale.
      if (error instanceof ApiError && error.status === 409) {
        void queryClient.invalidateQueries({
          queryKey: externalIssueKeys.createTarget(orgId),
        });
      }
    },
  });
  return toMutationHook(mutation);
}

// ---------------------------------------------------------------------------
// Issue page writes (g5.issue.native.page): G5 and GitHub issues
// ---------------------------------------------------------------------------

/**
 * An issue changed on its page: its detail, search rows, every workspace's
 * link summary and Activity show it.
 */
function invalidateAfterIssuePageChange(queryClient: QueryClient) {
  invalidateIssueProjections(queryClient);
  void queryClient.invalidateQueries({ queryKey: externalIssueKeys.linksAll });
  void queryClient.invalidateQueries({ queryKey: ["workspace-activity"] });
}

/**
 * After a refused GitHub write the target the person saw may be stale (a
 * switch, the repository's audience or their connection changed).
 */
function invalidateWriteTargetOnRefusal(
  queryClient: QueryClient,
  orgId: string,
  locator: ExternalIssueLocatorDTO,
  error: Error,
) {
  if (error instanceof ApiError && error.status >= 400 && error.status < 500) {
    void queryClient.invalidateQueries({
      queryKey: externalIssueKeys.writeTarget(orgId, locator),
    });
  }
}

/**
 * Whether the caller can change a GitHub issue from its page, as which
 * GitHub account, and who it can be assigned to. A live GitHub read; every
 * write repeats its checks.
 */
export function useIssueWriteTarget(
  orgId: string,
  locator: ExternalIssueLocatorDTO,
  enabled = true,
): AsyncState<IssueWriteTargetDTO> & { refetch: () => void } {
  const query = useQuery<IssueWriteTargetDTO, Error>({
    queryKey: externalIssueKeys.writeTarget(orgId, locator),
    queryFn: ({ signal }) =>
      api.get<IssueWriteTargetDTO>(
        routes.orgs.issueWriteTarget(orgId, locator),
        {
          signal,
        },
      ),
    enabled: !!orgId && enabled,
    retry: false,
    staleTime: GITHUB_READ_STALE_MS,
  });
  return { ...toAsyncState(query), refetch: () => void query.refetch() };
}

/**
 * Edit a native issue's title, description or assignees. The server applies
 * it only to the version the editor saw (`expectedUpdatedAt`), refusing a
 * changed issue with 409 `issue_changed`.
 */
export function useEditNativeIssue(
  orgId: string,
  number: number,
): MutationHook<EditNativeIssueInput, ExternalIssueSummaryDTO> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    ExternalIssueSummaryDTO,
    Error,
    EditNativeIssueInput
  >({
    mutationFn: (input) =>
      api.patch<ExternalIssueSummaryDTO>(
        routes.orgs.nativeIssue(orgId, number),
        input,
      ),
    onSettled: () => invalidateAfterIssuePageChange(queryClient),
  });
  return toMutationHook(mutation);
}

/**
 * Edit a GitHub issue's title, description or assignees as the caller's
 * GitHub account. The server refuses (409 `issue_changed`) when GitHub's
 * value of a changed field differs from what the form showed (`seen`).
 */
export function useEditGithubIssue(
  orgId: string,
  locator: ExternalIssueLocatorDTO,
): MutationHook<EditGithubIssueInput, IssueEditDTO> {
  const queryClient = useQueryClient();
  const mutation = useMutation<IssueEditDTO, Error, EditGithubIssueInput>({
    mutationFn: (input) =>
      api.patch<IssueEditDTO>(routes.orgs.issue(orgId, locator), input),
    onError: (error) =>
      invalidateWriteTargetOnRefusal(queryClient, orgId, locator, error),
    onSettled: () => invalidateAfterIssuePageChange(queryClient),
  });
  return toMutationHook(mutation);
}

/**
 * Close (as completed) or reopen an issue from its page: in G5, or as the
 * caller's GitHub account. Repeating it is harmless.
 */
export function useChangeIssuePageState(
  orgId: string,
  locator: ExternalIssueLocatorDTO,
): MutationHook<"closed" | "open", IssueStateChangeDTO> {
  const queryClient = useQueryClient();
  const mutation = useMutation<IssueStateChangeDTO, Error, "closed" | "open">({
    mutationFn: (state) =>
      api.post<IssueStateChangeDTO>(routes.orgs.issueState(orgId, locator), {
        state,
      }),
    onError: (error) =>
      invalidateWriteTargetOnRefusal(queryClient, orgId, locator, error),
    onSettled: () => invalidateAfterIssuePageChange(queryClient),
  });
  return toMutationHook(mutation);
}

/**
 * Comment on an issue from its page. The idempotency key makes a retry of
 * the same submission return the first comment; on GitHub it is looked for
 * before posting, so retrying an `outcome_unknown` answer never posts twice.
 */
export function usePostIssuePageComment(
  orgId: string,
  locator: ExternalIssueLocatorDTO,
): MutationHook<IssuePageCommentInput, CommentResultDTO> {
  const queryClient = useQueryClient();
  const mutation = useMutation<CommentResultDTO, Error, IssuePageCommentInput>({
    mutationFn: (input) =>
      api.post<CommentResultDTO>(
        routes.orgs.issueComments(orgId, locator),
        input,
      ),
    onError: (error) =>
      invalidateWriteTargetOnRefusal(queryClient, orgId, locator, error),
    onSettled: () => invalidateAfterIssuePageChange(queryClient),
  });
  return toMutationHook(mutation);
}

/**
 * Edit the caller's own comment from the issue page, only if it is
 * unchanged since the editor read it (409 `comment_changed` otherwise).
 */
export function useEditIssuePageComment(
  orgId: string,
  locator: ExternalIssueLocatorDTO,
): MutationHook<EditIssuePageCommentInput, CommentResultDTO> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    CommentResultDTO,
    Error,
    EditIssuePageCommentInput
  >({
    mutationFn: ({ commentId, ...input }) =>
      api.patch<CommentResultDTO>(
        routes.orgs.issueComment(orgId, locator, commentId),
        input,
      ),
    onError: (error) =>
      invalidateWriteTargetOnRefusal(queryClient, orgId, locator, error),
    onSettled: () => invalidateAfterIssuePageChange(queryClient),
  });
  return toMutationHook(mutation);
}

// ---------------------------------------------------------------------------
// Labels (g5.issue.labels): a domain's catalog and an issue's labels
// ---------------------------------------------------------------------------

/** A domain's label catalog as the picker and the catalog editor read it. */
export interface IssueLabelCatalog {
  domainId: string;
  displayKey: string;
  /** Ordered by name. */
  labels: ExternalIssueLabelDTO[];
  /** More labels exist than the first {@link MAX_LABEL_CATALOG_PAGES} pages hold. */
  truncated: boolean;
}

/**
 * A domain's label catalog, every page up to {@link MAX_LABEL_CATALOG_PAGES}
 * (100 labels each). Any member, viewers included, may read it; a GitHub
 * domain's is a live GitHub read.
 */
export function useIssueLabelCatalog(
  orgId: string,
  domainId: string,
  enabled = true,
): AsyncState<IssueLabelCatalog> & {
  refetch: () => void;
  isFetching: boolean;
} {
  const query = useQuery<IssueLabelCatalog, Error>({
    queryKey: externalIssueKeys.labels(orgId, domainId),
    queryFn: async ({ signal }) => {
      const labels: ExternalIssueLabelDTO[] = [];
      let displayKey = "";
      let page: number | null = 1;
      let read = 0;
      while (page !== null && read < MAX_LABEL_CATALOG_PAGES) {
        const result: LabelPageDTO = await api.get<LabelPageDTO>(
          routes.orgs.issueDomainLabels(orgId, domainId),
          { signal, params: { page } },
        );
        labels.push(...result.labels);
        displayKey = result.displayKey;
        page = result.nextPage;
        read += 1;
      }
      return { domainId, displayKey, labels, truncated: page !== null };
    },
    enabled: !!orgId && !!domainId && enabled,
    retry: false,
    staleTime: GITHUB_READ_STALE_MS,
  });
  return {
    ...toAsyncState(query),
    refetch: () => void query.refetch(),
    isFetching: query.isFetching,
  };
}

/**
 * A catalog write may have changed the catalog whatever its answer (an
 * unknown outcome, a conflict naming a label the editor had not seen).
 */
function invalidateLabelCatalog(
  queryClient: QueryClient,
  orgId: string,
  domainId: string,
) {
  void queryClient.invalidateQueries({
    queryKey: externalIssueKeys.labels(orgId, domainId),
  });
}

/**
 * Add a label to a domain's catalog. A name already taken by a different
 * label is refused (409 `label_exists`, carrying it); a retry of the same
 * create returns the label. The domain is bound at submission.
 */
export function useCreateIssueLabel(
  orgId: string,
): MutationHook<CreateLabelInput, LabelWriteDTO> {
  const queryClient = useQueryClient();
  const mutation = useMutation<LabelWriteDTO, Error, CreateLabelInput>({
    mutationFn: ({ domainId, ...input }) =>
      api.post<LabelWriteDTO>(
        routes.orgs.issueDomainLabels(orgId, domainId),
        input,
      ),
    onSettled: (_result, _error, { domainId }) =>
      invalidateLabelCatalog(queryClient, orgId, domainId),
  });
  return toMutationHook(mutation);
}

/**
 * Edit a catalog label's name, colour or description (last writer wins).
 * Issues read the catalog's name and colour, so every displayed issue is
 * re-read too.
 */
export function useEditIssueLabel(
  orgId: string,
): MutationHook<EditLabelInput, LabelWriteDTO> {
  const queryClient = useQueryClient();
  const mutation = useMutation<LabelWriteDTO, Error, EditLabelInput>({
    mutationFn: ({ domainId, labelId, ...input }) =>
      api.patch<LabelWriteDTO>(
        routes.orgs.issueDomainLabel(orgId, domainId, labelId),
        input,
      ),
    onSettled: (_result, _error, { domainId }) => {
      invalidateLabelCatalog(queryClient, orgId, domainId);
      invalidateAfterIssuePageChange(queryClient);
    },
  });
  return toMutationHook(mutation);
}

/**
 * Delete a catalog label, which also removes it from every issue in the
 * domain; every displayed issue is re-read.
 */
export function useDeleteIssueLabel(
  orgId: string,
): MutationHook<DeleteLabelInput, LabelWriteDTO> {
  const queryClient = useQueryClient();
  const mutation = useMutation<LabelWriteDTO, Error, DeleteLabelInput>({
    mutationFn: ({ domainId, labelId }) =>
      api.delete<LabelWriteDTO>(
        routes.orgs.issueDomainLabel(orgId, domainId, labelId),
      ),
    onSettled: (_result, _error, { domainId }) => {
      invalidateLabelCatalog(queryClient, orgId, domainId);
      invalidateAfterIssuePageChange(queryClient);
    },
  });
  return toMutationHook(mutation);
}

/**
 * Add and remove labels on an issue, named by name: in G5, or as the
 * caller's GitHub account. Each change reports its own outcome; the issue's
 * page shows the labels as re-read at once, and every other cached copy of
 * the issue (search rows, Home's tile, workspace links) is re-read. The
 * issue is bound at submission.
 */
export function useChangeIssueLabels(
  orgId: string,
): MutationHook<IssueLabelChangeInput, IssueLabelChangeDTO> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    IssueLabelChangeDTO,
    Error,
    IssueLabelChangeInput
  >({
    mutationFn: ({ locator, addLabels, removeLabels }) =>
      api.post<IssueLabelChangeDTO>(routes.orgs.issueLabels(orgId, locator), {
        addLabels,
        removeLabels,
      }),
    onSuccess: (result, { locator }) => {
      const answered = result.issue;
      if (!answered) return;
      queryClient.setQueryData<ExternalIssueDetailDTO>(
        externalIssueKeys.detail(orgId, locator.repositoryId, locator.number),
        (detail) => detail && { ...detail, labels: answered.labels },
      );
    },
    onError: (error, { locator }) =>
      invalidateWriteTargetOnRefusal(queryClient, orgId, locator, error),
    onSettled: () => invalidateAfterIssuePageChange(queryClient),
  });
  return toMutationHook(mutation);
}
