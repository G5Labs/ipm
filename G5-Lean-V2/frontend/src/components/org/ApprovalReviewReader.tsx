import { useState } from "react";
import { ChevronRight } from "lucide-react";
import { formatApprovalDateTime } from "../../lib/dateTime";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../lib/api";
import { routes } from "../../lib/routes";
import { useAuth } from "../../hooks/useAuth";
import { MarkdownRenderer } from "../shared/MarkdownRenderer";
import { splitGoverningPrompt } from "../../lib/reportSections";
import { LandingReader } from "../workspace-landing/LandingParts";
import {
  FormalReviewInspection,
  type FormalReviewReport,
} from "./FormalReviewInspection";

/**
 * The rendered report body: a top-level Governing prompt section is a
 * disclosure, collapsed by default, so the verdict reads first
 * (g5.governance.approval.report-view). Other sections stay open.
 */
function ReportBody({ body }: { body: string }) {
  const split = splitGoverningPrompt(body);
  if (!split) return <MarkdownRenderer trustedContent={false} content={body} />;
  return (
    <>
      {split.before.trim() && (
        <MarkdownRenderer trustedContent={false} content={split.before} />
      )}
      <details
        data-testid="approval-report-governing-prompt"
        className="group mb-6"
      >
        {/* Still a section heading, styled by the report's own stylesheet. */}
        <summary className="cursor-pointer list-none focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)] [&::-webkit-details-marker]:hidden">
          <div className="markdown-body g5-markdown-preview">
            <h2 className="flex items-center gap-1.5">
              <ChevronRight
                size={18}
                aria-hidden="true"
                className="flex-none transition-transform group-open:rotate-90"
              />
              Governing prompt
            </h2>
          </div>
        </summary>
        <div className="mt-4">
          <MarkdownRenderer trustedContent={false} content={split.prompt} />
        </div>
      </details>
      {split.after.trim() && (
        <MarkdownRenderer trustedContent={false} content={split.after} />
      )}
    </>
  );
}

export function ApprovalReportDocument({ document }: { document: string }) {
  const [mode, setMode] = useState<"view" | "raw">("view");
  const frontmatter = document.match(/^---\r?\n([\s\S]*?)\r?\n---(?:\r?\n|$)/);
  return (
    <>
      <div role="group" aria-label="Report display" className="mb-4 flex gap-2">
        {(["view", "raw"] as const).map((value) => (
          <button
            key={value}
            type="button"
            data-testid={`approval-report-${value}`}
            aria-pressed={mode === value}
            onClick={() => setMode(value)}
            className="rounded border border-[var(--g5-stroke)] px-3 py-1 text-sm aria-pressed:bg-[var(--g5-bg2)]"
          >
            {value === "view" ? "View" : "Raw"}
          </button>
        ))}
      </div>
      <div data-testid="agent-review-report-view" className="break-words">
        {mode === "raw" ? (
          <pre
            data-testid="approval-report-source"
            className="whitespace-pre-wrap text-sm"
          >
            {document}
          </pre>
        ) : (
          <>
            <ReportBody body={document.slice(frontmatter?.[0].length ?? 0)} />
            {frontmatter && (
              <details className="mt-6 text-xs">
                <summary>Report frontmatter</summary>
                <pre className="whitespace-pre-wrap">{frontmatter[1]}</pre>
              </details>
            )}
          </>
        )}
      </div>
    </>
  );
}

export function ApprovalReviewReader({
  workspaceId,
  reviewId,
  kind,
  onClose,
}: {
  workspaceId: string;
  reviewId: string;
  kind: "report" | "inspection";
  onClose: () => void;
}) {
  const { user } = useAuth();
  const report = useQuery({
    queryKey: [
      "approval-report",
      user?.id,
      user?.activeOrgId,
      workspaceId,
      reviewId,
    ],
    queryFn: ({ signal }) =>
      api.get<FormalReviewReport>(
        `${routes.workspaceApprovals.requestAgentReview(workspaceId)}/${encodeURIComponent(reviewId)}/report`,
        { signal },
      ),
  });
  const data = report.data;
  const resolution = data?.assignment?.resolution;
  const title = kind === "report" ? "Agent review report" : "Inspect review";
  return (
    <LandingReader
      ariaLabel={title}
      contentLabel={title}
      heading={<h2 className="text-lg font-semibold">{title}</h2>}
      testId="approval-review-reader"
      onClose={onClose}
    >
      {report.isPending && <p role="status">Loading review…</p>}
      {report.error && <p role="alert">{report.error.message}</p>}
      {data && (
        <>
          {data.failureCause && (
            <p role="status" className="mb-4">
              Review failed
              {data.failureStage ? ` during ${data.failureStage}` : ""}:{" "}
              {data.failureCause}
            </p>
          )}
          {kind === "report" && data.document != null ? (
            <ApprovalReportDocument document={data.document} />
          ) : (
            <div className="space-y-4 text-sm">
              <section aria-label="Captured request">
                <h3 className="font-semibold">Request</h3>
                <p>
                  {data.request?.requestedAt
                    ? formatApprovalDateTime(data.request.requestedAt)
                    : null}
                </p>
                <p className="whitespace-pre-wrap">
                  {data.request?.message || data.request?.note || "—"}
                </p>
              </section>
              <section aria-label="Review policy">
                <h3 className="font-semibold">Review policy</h3>
                <p className="whitespace-pre-wrap">
                  {data.definition?.agent_prompt || "—"}
                </p>
              </section>
              <section aria-label="Captured revisions">
                <h3 className="font-semibold">Revisions</h3>
                {!resolution ? (
                  <p>Not captured yet</p>
                ) : (
                  resolution.sourceHeads.map((source) => (
                    <div key={source.repoId} className="mt-2 break-all">
                      <h4 className="font-medium">
                        {resolution.repositoryNames?.[source.repoId] ??
                          source.repoId}
                      </h4>
                      <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 text-xs">
                        {(
                          [
                            ["Source", source.headCommit],
                            [
                              "Parent",
                              resolution.targetHeads?.find(
                                (h) => h.repoId === source.repoId,
                              )?.headCommit,
                            ],
                            [
                              "Merge base",
                              resolution.mergeBase?.repositories.find(
                                (h) => h.repoId === source.repoId,
                              )?.gitSha,
                            ],
                            [
                              "Workspace base",
                              resolution.workspaceBaseHeads?.find(
                                (h) => h.repoId === source.repoId,
                              )?.headCommit,
                            ],
                          ] as const
                        )
                          .filter(([, sha]) => sha != null)
                          .map(([label, sha]) => (
                            <div className="contents" key={label}>
                              <dt>{label}</dt>
                              <dd>
                                <code>{sha}</code>
                              </dd>
                            </div>
                          ))}
                      </dl>
                    </div>
                  ))
                )}
              </section>
              <details className="text-xs">
                <summary>Technical details</summary>
                <pre className="whitespace-pre-wrap break-all">
                  {JSON.stringify(
                    {
                      reviewId,
                      requestId: data.requestId,
                      assignment: data.assignment,
                    },
                    null,
                    2,
                  )}
                </pre>
              </details>
              <details className="text-xs">
                <summary>Transcript</summary>
                <FormalReviewInspection
                  workspaceId={workspaceId}
                  reviewId={reviewId}
                />
              </details>
            </div>
          )}
        </>
      )}
    </LandingReader>
  );
}
