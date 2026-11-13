import { useState } from "react";
import { api } from "../../lib/api";
import { routes } from "../../lib/routes";
import { Button } from "../ui/UI";

export interface FormalReviewReport {
  requestId?: string;
  request?: {
    message?: string;
    note?: string;
    requestedAt?: string;
    requestedBy?: string;
  } | null;
  definition?: { agent_prompt?: string } | null;
  document: string | null;
  sha256: string | null;
  failureStage?: string | null;
  failureCause?: string | null;
  assignment?: {
    resolution: {
      assignedAt?: string;
      mergeBase?: {
        repositories: { repoId: string; repoName: string; gitSha: string }[];
      };
      sourceWorkspaceId?: string;
      targetWorkspaceId?: string;
      targetHeads?: {
        repoId: string;
        branchName: string;
        headCommit: string;
      }[];
      repositoryNames?: Record<string, string>;
      workspaceBaseHeads?: {
        repoId: string;
        branchName: string;
        headCommit: string;
      }[];
      sourceHeads: { repoId: string; branchName: string; headCommit: string }[];
    };
  } | null;
}
interface TranscriptItem {
  id: string;
  kind: "input" | "message" | "event";
  createdAt: string;
  entryType: string;
  content: string;
  contentTruncated: boolean;
}
interface TranscriptPage {
  items: TranscriptItem[];
  nextCursor: string | null;
  eventsTrimmed: boolean;
}

/** Retained data only: this surface grants no live review-workspace access. */
export function FormalReviewInspection({
  workspaceId,
  reviewId,
}: {
  workspaceId: string;
  reviewId: string;
}) {
  const endpoint = `${routes.workspaceApprovals.requestAgentReview(workspaceId)}/${encodeURIComponent(reviewId)}`;
  const [transcript, setTranscript] = useState<TranscriptPage | null>(null);
  const [transcriptBusy, setTranscriptBusy] = useState(false);
  const [transcriptError, setTranscriptError] = useState<string | null>(null);
  async function loadTranscript(restart = false) {
    setTranscriptBusy(true);
    setTranscriptError(null);
    try {
      const page = await api.get<TranscriptPage>(`${endpoint}/transcript`, {
        params: {
          cursor: restart ? undefined : (transcript?.nextCursor ?? undefined),
        },
      });
      setTranscript((previous) => ({
        ...page,
        items: [...(restart ? [] : (previous?.items ?? [])), ...page.items],
        eventsTrimmed:
          (!restart && !!previous?.eventsTrimmed) || page.eventsTrimmed,
      }));
    } catch (error) {
      setTranscriptError(
        error instanceof Error ? error.message : "Could not load transcript",
      );
    } finally {
      setTranscriptBusy(false);
    }
  }
  return (
    <div className="mt-4 space-y-3 border-t pt-3 text-xs">
      <Button
        kind="ghost"
        disabled={transcriptBusy}
        onClick={() => void loadTranscript(true)}
      >
        {transcriptBusy
          ? "Loading transcript…"
          : transcript
            ? "Reload transcript"
            : "View transcript"}
      </Button>
      {transcriptError && <p role="alert">{transcriptError}</p>}
      {transcript && (
        <section aria-label="Review transcript" className="space-y-2">
          {transcript.eventsTrimmed && (
            <p>Older events were trimmed by ordinary task retention.</p>
          )}
          {transcript.items.length === 0 && (
            <p>No retained transcript entries.</p>
          )}
          <div className="max-h-72 overflow-auto space-y-3">
            {transcript.items.map((item) => (
              <article key={`${item.kind}:${item.id}`}>
                <p className="font-semibold">
                  {item.kind} · {item.entryType} · {item.createdAt}
                </p>
                <pre className="whitespace-pre-wrap break-words">
                  {item.content}
                </pre>
                {item.contentTruncated && (
                  <p>Entry shortened to fit the transcript page.</p>
                )}
              </article>
            ))}
          </div>
          {transcript.nextCursor && (
            <Button
              kind="ghost"
              disabled={transcriptBusy}
              onClick={() => void loadTranscript()}
            >
              Load more
            </Button>
          )}
        </section>
      )}
    </div>
  );
}
