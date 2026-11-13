// =============================================================================
// Workspace landing — Work Description (formerly Implementation description)
// =============================================================================
// The workspace's non-stacked G5 PR (ui g5.workbench.landing.implementation):
// its open PR, else its latest. The G5 lifecycle sits beside the title, the
// opening of the canonical description follows, then one row per repository
// of the PR's recorded vector, with each published repository's own provider
// status and numbered link. Read more opens the reader with the full
// description and the same rows; when provider bodies differ from what G5
// would publish, or could not be compared, the reader offers each source
// (git g5.git.pr-description). Nothing here creates, updates or synchronizes
// a PR: Prepare to Ship and Pull Requests own those actions.
// =============================================================================

import { useId, useRef, useState, type ReactNode } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  useWorkspacePullRequestDescription,
  workspacePullRequestsQueryOptions,
} from "../../hooks/useWorkspace";
import { useWorkspaceReviewFlow } from "../../hooks/useWorkspaceReviewFlow";
import {
  g5PullRequestLifecycle,
  providerPullRequestLifecycle,
  type LifecycleState,
} from "../../lib/landingStatus";
import type {
  AsyncState,
  PullRequestDescriptionDTO,
  ProviderPullRequestDescriptionDTO,
  RepositoryPullRequestSummary,
  ReviewHead,
  WorkspacePullRequestProviderRepoSummary,
  WorkspacePullRequestSummary,
} from "../../lib/types";
import { githubPublications } from "../../lib/types";
import { GithubRenderedHtml } from "../shared/GithubRenderedHtml";
import { MarkdownRenderer } from "../shared/MarkdownRenderer";
import {
  ClippedPreview,
  ExternalRecordLink,
  LandingReader,
  LandingRow,
  LandingSection,
  LifecycleStatus,
  ReadMoreButton,
  ShortSha,
} from "./LandingParts";

/** The PR the landing presents: open first (toward the workspace's target
 *  when several are open), else the most recent. */
export function landingPullRequest(
  pulls: WorkspacePullRequestSummary[],
  targetWorkspaceId: string | null | undefined,
): WorkspacePullRequestSummary | null {
  const newest = [...pulls].sort((a, b) =>
    b.createdAt.localeCompare(a.createdAt),
  );
  const open = newest.filter((p) => p.status === "open");
  return (
    open.find((p) => p.targetWorkspaceId === targetWorkspaceId) ??
    open[0] ??
    newest[0] ??
    null
  );
}

function recordedHeads(value: unknown): ReviewHead[] {
  if (!Array.isArray(value)) return [];
  return value.filter(
    (h): h is ReviewHead =>
      !!h &&
      typeof h === "object" &&
      typeof (h as ReviewHead).repoId === "string" &&
      typeof (h as ReviewHead).headCommit === "string",
  );
}

interface RepositoryRow {
  repoId: string;
  name: string;
  sha: string | null;
  provider: WorkspacePullRequestProviderRepoSummary | null;
  /** The member's local repository pull request (g5.git.pr-repository). */
  local: RepositoryPullRequestSummary | null;
  /** The PR does not change this member, so it has no repository PR. */
  unchanged: boolean;
  status: LifecycleState | null;
}

/** One row per repository of the PR's recorded vector, provider rows joined
 *  by repository; a provider row outside the vector is still listed. */
function repositoryRows(
  pull: WorkspacePullRequestSummary,
  repoNames: Map<string, string>,
  live: ProviderPullRequestDescriptionDTO[],
): RepositoryRow[] {
  const providers = githubPublications(pull);
  const status = (provider: WorkspacePullRequestProviderRepoSummary | null) =>
    provider
      ? providerPullRequestLifecycle(
          provider.providerStatus,
          live.find((r) => r.repoId === provider.repoId)?.lifecycle,
        )
      : null;
  // Older servers send no repository pull requests; then nothing reads as
  // unchanged.
  const repositoryPulls = pull.repositoryPullRequests;
  const rows: RepositoryRow[] = recordedHeads(pull.sourceHeads).map((head) => {
    const provider = providers.find((p) => p.repoId === head.repoId) ?? null;
    const repositoryPull =
      repositoryPulls?.find((r) => r.repoId === head.repoId) ?? null;
    const local = repositoryPull?.adapter === "g5" ? repositoryPull : null;
    return {
      repoId: head.repoId,
      name:
        provider?.providerRepoFullName ??
        repoNames.get(head.repoId) ??
        (local?.repository || head.repoId),
      sha: head.headCommit,
      provider,
      local,
      unchanged: !!repositoryPulls && !repositoryPull && !provider,
      status: provider
        ? status(provider)
        : local
          ? g5PullRequestLifecycle(pull.status)
          : null,
    };
  });
  for (const provider of providers) {
    if (rows.some((r) => r.repoId === provider.repoId)) continue;
    rows.push({
      repoId: provider.repoId,
      name: provider.providerRepoFullName,
      sha: provider.sourceHeadSha,
      provider,
      local: null,
      unchanged: false,
      status: status(provider),
    });
  }
  // A local repository PR of a member listed only in the preflight evidence.
  for (const local of repositoryPulls ?? []) {
    if (local.adapter !== "g5") continue;
    if (rows.some((r) => r.repoId === local.repoId)) continue;
    rows.push({
      repoId: local.repoId,
      name: repoNames.get(local.repoId) ?? (local.repository || local.repoId),
      sha: local.headRefOid,
      provider: null,
      local,
      unchanged: false,
      status: g5PullRequestLifecycle(pull.status),
    });
  }
  return rows;
}

/** `g5Number` is the G5 PR's name (`PR3fa9c1`), which also names each local
 *  repository pull request (g5.git.pr-local-number). */
function RepositoryRows({
  rows,
  g5Number,
}: {
  rows: RepositoryRow[];
  g5Number: string | null;
}) {
  if (rows.length === 0) return null;
  return (
    <ul data-testid="landing-repository-rows" className="mt-3 @container">
      {rows.map((row) => (
        <LandingRow
          key={row.repoId}
          testId="landing-repository-row"
          repoId={row.repoId}
          lead={
            <span className="block truncate font-mono text-xs text-[var(--g5-text0)]">
              {row.name}
            </span>
          }
          middle={<ShortSha sha={row.sha} />}
          trailing={
            row.provider ? (
              <>
                {row.status && (
                  <LifecycleStatus
                    state={row.status}
                    testId="landing-repository-status"
                  />
                )}
                {row.provider.providerPrNumber != null &&
                  row.provider.providerPrUrl && (
                    <ExternalRecordLink
                      href={row.provider.providerPrUrl}
                      label={`GitHub PR #${row.provider.providerPrNumber}`}
                      testId="landing-repository-github"
                    />
                  )}
              </>
            ) : row.local ? (
              <>
                {row.status && (
                  <LifecycleStatus
                    state={row.status}
                    testId="landing-repository-status"
                  />
                )}
                {g5Number && (
                  <span
                    data-testid="landing-repository-g5"
                    className="font-mono text-xs text-[var(--g5-text1)]"
                  >
                    {g5Number}
                  </span>
                )}
              </>
            ) : row.unchanged ? (
              <span
                data-testid="landing-repository-unchanged"
                className="text-xs text-[var(--g5-text2)]"
              >
                Unchanged
              </span>
            ) : undefined
          }
        />
      ))}
    </ul>
  );
}

/** `none`: nothing published, or the comparison is still being read. */
type Comparison = "none" | "matches" | "differs" | "unavailable";

/** All publications match, any differs, or the comparison is incomplete. */
function overallComparison(
  description: AsyncState<PullRequestDescriptionDTO>,
  published: boolean,
): Comparison {
  if (!published || description.status === "loading") return "none";
  if (description.status === "error") return "unavailable";
  const repos = description.data.repositories;
  if (repos.length === 0) return "unavailable";
  if (repos.some((r) => r.comparison === "differs")) return "differs";
  if (repos.every((r) => r.comparison === "matches")) return "matches";
  return "unavailable";
}

const COMPARISON_TEXT: Record<Exclude<Comparison, "none">, string> = {
  matches: "Matches GitHub",
  differs: "GitHub description differs",
  unavailable: "Not compared with GitHub",
};

function ComparisonHint({ comparison }: { comparison: Comparison }) {
  if (comparison === "none") return null;
  return (
    <span
      data-testid="landing-description-comparison"
      data-state={comparison}
      className={`shrink-0 text-xs ${comparison === "differs" ? "text-[var(--g5-warn)]" : "text-[var(--g5-text2)]"}`}
    >
      {COMPARISON_TEXT[comparison]}
    </span>
  );
}

/** The canonical description exactly as the server resolved it; the client
 *  never applies the saved-else-Project-Doc precedence itself. */
function G5Description({
  description,
}: {
  description: PullRequestDescriptionDTO;
}) {
  if (description.descriptionHtml) {
    return <GithubRenderedHtml html={description.descriptionHtml} />;
  }
  const text = description.description;
  if (text?.trim()) {
    return <MarkdownRenderer content={text} trustedContent={false} />;
  }
  return (
    <p
      data-testid="landing-implementation-no-description"
      className="text-sm italic text-[var(--g5-text2)]"
    >
      No description saved for this PR.
    </p>
  );
}

function ProviderBody({ repo }: { repo: ProviderPullRequestDescriptionDTO }) {
  if (repo.bodyHtml) return <GithubRenderedHtml html={repo.bodyHtml} />;
  if (repo.body?.trim()) {
    return <MarkdownRenderer content={repo.body} trustedContent={false} />;
  }
  return (
    <p role="status" className="text-sm italic text-[var(--g5-text2)]">
      {repo.error ?? "No description on GitHub."}
    </p>
  );
}

/**
 * One tab per description source with roving focus: arrow keys, Home and End
 * move between tabs and select them; Enter and Space work natively.
 */
function DescriptionSources({
  sources,
  selected,
  onSelect,
  children,
}: {
  sources: { id: string; label: string }[];
  selected: string;
  onSelect: (id: string) => void;
  children: ReactNode;
}) {
  const prefix = useId();
  const tabs = useRef<(HTMLButtonElement | null)[]>([]);
  const selectedIndex = Math.max(
    0,
    sources.findIndex((source) => source.id === selected),
  );
  const move = (index: number) => {
    const next = (index + sources.length) % sources.length;
    onSelect(sources[next].id);
    tabs.current[next]?.focus();
  };
  return (
    <>
      <div
        role="tablist"
        aria-label="Description source"
        className="mb-4 flex flex-wrap gap-x-4 gap-y-1 border-b border-[var(--g5-stroke)]"
        onKeyDown={(event) => {
          const target = {
            ArrowRight: selectedIndex + 1,
            ArrowLeft: selectedIndex - 1,
            Home: 0,
            End: sources.length - 1,
          }[event.key];
          if (target === undefined) return;
          event.preventDefault();
          move(target);
        }}
      >
        {sources.map((source, index) => (
          <button
            key={source.id}
            ref={(element) => {
              tabs.current[index] = element;
            }}
            id={`${prefix}-tab-${index}`}
            type="button"
            role="tab"
            data-testid="landing-description-tab"
            aria-selected={index === selectedIndex}
            aria-controls={`${prefix}-panel`}
            tabIndex={index === selectedIndex ? 0 : -1}
            onClick={() => onSelect(source.id)}
            className={`-mb-px border-b-2 px-0.5 pb-1.5 text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)] ${index === selectedIndex ? "border-[var(--g5-info)] text-[var(--g5-text0)]" : "border-transparent text-[var(--g5-text2)] hover:text-[var(--g5-text0)]"}`}
          >
            {source.label}
          </button>
        ))}
      </div>
      <div
        id={`${prefix}-panel`}
        role="tabpanel"
        aria-labelledby={`${prefix}-tab-${selectedIndex}`}
      >
        {children}
      </div>
    </>
  );
}

function shortRepository(name: string): string {
  return name.split("/").pop() ?? name;
}

/**
 * The PR the Work Description presents (open, else latest) and its list read.
 * The landing reads the same selection to decide whether Work Description has
 * content (g5.workbench.landing.task), never the rendered text.
 */
export function useLandingPullRequest(workspaceId: string) {
  const overview = useWorkspaceReviewFlow(workspaceId);
  // A push refreshes the open PR's recorded vector in the background; read
  // the list again on a slow cadence so the rows follow it.
  const pulls = useQuery({
    ...workspacePullRequestsQueryOptions(workspaceId, "source", "all"),
    refetchInterval: 15_000,
  });
  const pull = pulls.data
    ? landingPullRequest(pulls.data, overview.data?.workspace.targetWorkspaceId)
    : null;
  return { overview, pulls, pull };
}

export function ImplementationSection({
  workspaceId,
}: {
  workspaceId: string;
}) {
  const [readerOpen, setReaderOpen] = useState(false);
  const [tab, setTab] = useState("g5");
  const { overview, pulls, pull } = useLandingPullRequest(workspaceId);
  const description = useWorkspacePullRequestDescription(workspaceId, pull);

  let content: ReactNode;
  if (pulls.isPending) {
    content = (
      <p role="status" className="text-sm text-[var(--g5-text2)]">
        Loading implementation…
      </p>
    );
  } else if (pulls.isError && !pulls.data) {
    content = (
      <p role="alert" className="text-sm">
        Could not load the implementation.{" "}
        <button
          type="button"
          data-testid="review-implementation-retry"
          className="text-[var(--g5-info)] hover:underline"
          onClick={() => void pulls.refetch()}
        >
          Retry
        </button>
      </p>
    );
  } else if (!pull) {
    content = (
      <p
        data-testid="landing-implementation-empty"
        className="text-sm text-[var(--g5-text1)]"
      >
        No implementation description yet.
      </p>
    );
  } else {
    const repoNames = new Map(
      (overview.data?.repos ?? []).map((r) => [r.id, r.name]),
    );
    const read = description.status === "ok" ? description.data : null;
    const rows = repositoryRows(pull, repoNames, read?.repositories ?? []);
    const published = githubPublications(pull).some(
      (r) => r.providerPrNumber != null,
    );
    const comparison = overallComparison(description, published);
    const status = g5PullRequestLifecycle(pull.status);
    const titleRow = (level: "h2" | "h3", aside?: ReactNode) => {
      const Heading = level;
      return (
        <div className="flex flex-wrap items-baseline gap-x-2.5 gap-y-1">
          <LifecycleStatus
            state={status}
            testId="landing-implementation-status"
          />
          <Heading
            data-testid="landing-implementation-title"
            className="min-w-0 flex-1 break-words text-lg font-semibold text-[var(--g5-text0)]"
          >
            {pull.title}
          </Heading>
          {aside}
        </div>
      );
    };
    const body =
      description.status === "loading" ? (
        <p role="status" className="text-sm italic text-[var(--g5-text2)]">
          Loading description…
        </p>
      ) : description.status === "error" && pull.descriptionOverride?.trim() ? (
        // The server's read failed, but the PR record already carries the
        // saved text: show it, marked as possibly stale. The precedence stays
        // on the server (g5.git.pr-description); this is only the last copy.
        <>
          <p
            role="status"
            data-testid="landing-implementation-description-stale"
            className="mb-2 text-xs italic text-[var(--g5-text2)]"
          >
            The current description could not be loaded; showing the saved
            description, which may be out of date.
          </p>
          <MarkdownRenderer
            content={pull.descriptionOverride}
            trustedContent={false}
          />
        </>
      ) : description.status === "error" ? (
        <p
          role="status"
          data-testid="landing-implementation-description-error"
          className="text-sm italic text-[var(--g5-text2)]"
        >
          The description could not be loaded.
        </p>
      ) : (
        <G5Description description={description.data} />
      );
    const sources =
      comparison === "differs" || comparison === "unavailable"
        ? (read?.repositories ?? [])
        : [];
    const activeSource =
      sources.find((r) => `${r.repoId}:${r.number}` === tab) ?? null;
    content = (
      <>
        {titleRow("h3")}
        <div className="mt-2">
          <ClippedPreview testId="landing-implementation-preview">
            {body}
          </ClippedPreview>
        </div>
        <RepositoryRows rows={rows} g5Number={pull.number ?? null} />
        {comparison !== "none" && comparison !== "matches" && (
          <p
            data-testid="landing-implementation-divergence"
            data-state={comparison}
            className={`mt-2 rounded border-l-2 px-3 py-1.5 text-xs ${comparison === "differs" ? "border-[var(--g5-warn)] bg-[var(--g5-bg2)] text-[var(--g5-text1)]" : "border-[var(--g5-stroke)] text-[var(--g5-text2)]"}`}
          >
            {comparison === "differs"
              ? "GitHub description differs."
              : "The GitHub description could not be compared."}
          </p>
        )}
        <ReadMoreButton
          testId="workspace-read-implementation"
          onClick={() => {
            setTab("g5");
            setReaderOpen(true);
          }}
        />
        {readerOpen && (
          <LandingReader
            ariaLabel="Implementation description"
            contentLabel="Full implementation description"
            onClose={() => setReaderOpen(false)}
            testId="landing-implementation-reader"
            heading={titleRow("h2", <ComparisonHint comparison={comparison} />)}
          >
            {sources.length > 0 ? (
              <DescriptionSources
                sources={[
                  { id: "g5", label: "G5 Description" },
                  ...sources.map((r) => ({
                    id: `${r.repoId}:${r.number}`,
                    label: `GitHub · ${shortRepository(r.repository)} #${r.number}`,
                  })),
                ]}
                selected={activeSource ? tab : "g5"}
                onSelect={setTab}
              >
                {activeSource ? <ProviderBody repo={activeSource} /> : body}
              </DescriptionSources>
            ) : (
              body
            )}
            <RepositoryRows rows={rows} g5Number={pull.number ?? null} />
          </LandingReader>
        )}
      </>
    );
  }
  return (
    <LandingSection label="Work Description" testId="landing-implementation">
      {content}
    </LandingSection>
  );
}
