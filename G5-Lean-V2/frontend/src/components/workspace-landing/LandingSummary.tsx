// =============================================================================
// Workspace landing — summary line
// =============================================================================
// One compact line above the content (ui g5.workbench.landing.summary): the
// workspace-wide approval summary and the changed-file counts under the
// complete pushed comparison, independent of Source Control filters.
// =============================================================================

import { useMemo } from "react";
import { countChangedFiles } from "../../lib/changeFiles";
import { Link } from "react-router-dom";
import { Files, ShieldCheck } from "lucide-react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../lib/api";
import {
  reviewUrl,
  useWorkspaceReviewFlow,
  type ReviewChanges,
} from "../../hooks/useWorkspaceReviewFlow";
import { ContextualHelp } from "../ui/ContextualHelp";
import type { ApprovalSummary } from "../../lib/landingStatus";

const LINK =
  "text-[var(--g5-info)] hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]";

const APPROVAL_TONE: Record<ApprovalSummary["kind"], string> = {
  loading: "text-[var(--g5-text2)]",
  unavailable: "text-[var(--g5-warn)]",
  "none-required": "text-[var(--g5-text1)]",
  // Requirement counts carry their status in words; the theme foreground keeps
  // them readable in every theme (g5.workbench.landing.summary).
  pending: "text-[var(--g5-text1)]",
  rejected: "text-[var(--g5-text1)]",
  approved: "text-[var(--g5-text1)]",
};

export function LandingSummary({
  workspaceId,
  approvals,
}: {
  workspaceId: string;
  /** `null` where the workspace offers no Approvals (Base, personal). */
  approvals: ApprovalSummary | null;
}) {
  const overview = useWorkspaceReviewFlow(workspaceId);
  const changes = useQuery({
    queryKey: [
      "workspace-review-changes",
      workspaceId,
      "base",
      overview.data?.heads,
    ],
    queryFn: () =>
      api.get<ReviewChanges>(`${reviewUrl(workspaceId)}/changes?mode=base`),
    enabled:
      !!workspaceId &&
      approvals !== null &&
      (!!overview.data || overview.isError),
    staleTime: 0,
    refetchOnMount: "always",
  });
  const files =
    !changes.error && changes.data?.baselineAvailable
      ? changes.data.files
      : null;
  const counts = useMemo(() => countChangedFiles(files ?? []), [files]);
  const total = counts.code + counts.ontology + counts.docs;
  const unavailable = changes.data?.unavailableRepositories ?? [];
  const repoName = (id: string) =>
    overview.data?.repos.find((r) => r.id === id)?.name ?? id;
  return (
    <div
      data-testid="landing-summary"
      className="mb-6 flex flex-wrap items-center gap-x-5 gap-y-2 border-b border-[var(--g5-stroke)] pb-3 text-sm"
    >
      {approvals && (
        <Link
          to="?view=approvals"
          data-testid="landing-summary-approvals"
          data-state={approvals.kind}
          className="inline-flex items-center gap-1.5 hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]"
        >
          <ShieldCheck size={15} aria-hidden="true" />
          <span className="font-semibold text-[var(--g5-text0)]">
            Approvals
          </span>
          <span className={APPROVAL_TONE[approvals.kind]}>
            · {approvals.text}
          </span>
        </Link>
      )}
      {approvals !== null && (
        <span
          data-testid="workspace-change-summary"
          className="inline-flex flex-wrap items-center gap-x-2 gap-y-1"
        >
          <Link
            to="?view=source-control"
            state={{ sourceControlSelection: "all" }}
            data-testid="landing-summary-changed"
            className="inline-flex items-center gap-1.5 font-semibold text-[var(--g5-text0)] hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]"
          >
            <Files size={15} aria-hidden="true" />
            {files
              ? `${total}${unavailable.length ? "+" : ""} changed ${total === 1 && !unavailable.length ? "file" : "files"}`
              : "Changed files"}
          </Link>
          {files && unavailable.length > 0 && (
            <ContextualHelp label="Unavailable repository comparisons">
              {unavailable.map((repo) => (
                <p key={repo.repoId}>
                  {repo.repoName ?? repoName(repo.repoId)}: {repo.reason}
                </p>
              ))}
            </ContextualHelp>
          )}
          {files?.length !== 0 &&
            (
              [
                { kind: "code", label: "Code", view: "source-control" },
                { kind: "ontology", label: "Ontology", view: "ontology" },
                { kind: "docs", label: "Docs", view: "docs" },
              ] as const
            ).map((item, index) => (
              <span key={item.kind} className="inline-flex items-center gap-2">
                {index > 0 && (
                  <span aria-hidden="true" className="text-[var(--g5-text2)]">
                    ·
                  </span>
                )}
                <Link
                  to={`?view=${item.view}`}
                  state={
                    item.kind === "code"
                      ? { sourceControlSelection: "code" }
                      : undefined
                  }
                  data-testid={`landing-summary-${item.kind}`}
                  className={LINK}
                >
                  {item.label}
                  {files ? ` ${counts[item.kind]}` : ""}
                </Link>
              </span>
            ))}
        </span>
      )}
      {changes.error && (
        <span role="alert" className="text-xs text-[var(--g5-warn)]">
          Change counts unavailable.{" "}
          <button
            type="button"
            data-testid="review-changes-retry"
            className={LINK}
            onClick={() => void changes.refetch()}
          >
            Retry
          </button>
        </span>
      )}
      {changes.data && !changes.data.baselineAvailable && (
        <span className="text-xs text-[var(--g5-text2)]">
          No recorded comparison baseline is available.
        </span>
      )}
    </div>
  );
}
