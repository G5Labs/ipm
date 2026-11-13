// Role dispatch; all assistant content is rendered by the shared turn owner.
import React from "react";
import { ChevronDown } from "lucide-react";
import type {
  ChatMessageDTO,
  Artifact,
  ArtifactAction,
  Citation,
} from "../../lib/types";
import { labelWorkspaceFolderInPath } from "../../lib/utils";
import { useWorkspaceNames } from "../../hooks/useWorkspaceNames";
import { classNames } from "../ui/UI";
import { ChatErrorNotice } from "./ChatErrorNotice";
import { AssistantTurn } from "./AssistantTurn";
import { CitationChips, formatMessageTimestamp } from "./MessageMetadata";
export { CitationChips, formatMessageTimestamp } from "./MessageMetadata";

export interface ChatMessageProps {
  message: ChatMessageDTO;
  /** Whether this message is currently being streamed */
  isStreaming: boolean;
  /** Display this user message as queued behind the active assistant turn. */
  isQueued?: boolean;
  /** The transcript already explains this exact attempt's failure. Retain
   * error styling/delivery state without a redundant response-error label. */
  errorExplained?: boolean;
  /** Artifact rendering */
  onArtifactClick?: (artifact: Artifact) => void;
  /** Artifact actions */
  onArtifactAction?: (artifact: Artifact, action: ArtifactAction) => void;
  /** Citation links */
  onCitationClick?: (citation: Citation) => void;
  /** Fork session at this message */
  onFork?: (messageIndex: number) => void;
}

function renderInlineMarkdown(
  text: string,
  citations: Citation[],
  onCitationClick?: (citation: Citation) => void,
  workspaceNamesById?: Record<string, string>,
): React.ReactNode {
  const parts: React.ReactNode[] = [];
  let key = 0;

  // Process citation references [N] — clickable superscript numbers
  const citationRegex = /\[(\d+)\]/g;
  let lastIdx = 0;
  let citMatch: RegExpExecArray | null;

  while ((citMatch = citationRegex.exec(text)) !== null) {
    if (citMatch.index > lastIdx) {
      parts.push(
        <span key={key++}>{text.slice(lastIdx, citMatch.index)}</span>,
      );
    }

    const citIndex = parseInt(citMatch[1], 10) - 1; // Citations are 1-indexed in text
    const citation = citations[citIndex];

    parts.push(
      <sup
        key={key++}
        className="text-[var(--g5-info)] cursor-pointer hover:underline text-[10px]"
        onClick={
          citation && onCitationClick
            ? () => onCitationClick(citation)
            : undefined
        }
        title={
          citation
            ? `${citation.label}${citation.filePath ? ` — ${labelWorkspaceFolderInPath(citation.filePath, workspaceNamesById)}` : ""}`
            : undefined
        }
        role={citation ? "button" : undefined}
        tabIndex={citation ? 0 : undefined}
      >
        [{citMatch[1]}]
      </sup>,
    );
    lastIdx = citMatch.index + citMatch[0].length;
  }

  if (lastIdx === 0) {
    // No citations found, render plain text
    return <span>{text}</span>;
  }

  if (lastIdx < text.length) {
    parts.push(<span key={key++}>{text.slice(lastIdx)}</span>);
  }

  return <>{parts}</>;
}

function QueuedMessageIcon() {
  return (
    <svg
      data-testid="queued-message-icon"
      className="mt-0.5 h-4 w-4 shrink-0 text-[var(--g5-text2)]"
      viewBox="0 0 24 24"
      role="img"
      aria-label="Queued"
    >
      <path
        d="M9.92 2.22 A10 10 0 0 1 14.25 2.26 L13.44 5.76 A6.4 6.4 0 0 0 10.67 5.74 Z"
        fill="currentColor"
        opacity="0.95"
      />
      <path
        d="M16.07 2.86 A10 10 0 0 1 19.55 5.44 L16.83 7.80 A6.4 6.4 0 0 0 14.60 6.15 Z"
        fill="currentColor"
        opacity="0.82"
      />
      <path
        d="M20.66 7.00 A10 10 0 0 1 21.96 11.13 L18.38 11.44 A6.4 6.4 0 0 0 17.54 8.80 Z"
        fill="currentColor"
        opacity="0.7"
      />
      <path
        d="M21.95 13.05 A10 10 0 0 1 20.57 17.15 L17.49 15.30 A6.4 6.4 0 0 0 18.36 12.67 Z"
        fill="currentColor"
        opacity="0.58"
      />
      <path
        d="M19.43 18.69 A10 10 0 0 1 15.91 21.21 L14.50 17.89 A6.4 6.4 0 0 0 16.76 16.28 Z"
        fill="currentColor"
        opacity="0.48"
      />
      <path
        d="M14.08 21.78 A10 10 0 0 1 9.75 21.74 L10.56 18.24 A6.4 6.4 0 0 0 13.33 18.26 Z"
        fill="currentColor"
        opacity="0.4"
      />
      <path
        d="M7.93 21.14 A10 10 0 0 1 4.45 18.56 L7.17 16.20 A6.4 6.4 0 0 0 9.40 17.85 Z"
        fill="currentColor"
        opacity="0.32"
      />
      <path
        d="M3.34 17.00 A10 10 0 0 1 2.04 12.87 L5.62 12.56 A6.4 6.4 0 0 0 6.46 15.20 Z"
        fill="currentColor"
        opacity="0.25"
      />
      <path
        d="M2.05 10.95 A10 10 0 0 1 3.43 6.85 L6.51 8.70 A6.4 6.4 0 0 0 5.64 11.33 Z"
        fill="currentColor"
        opacity="0.18"
      />
      <path
        d="M4.57 5.31 A10 10 0 0 1 8.09 2.79 L9.50 6.11 A6.4 6.4 0 0 0 7.24 7.72 Z"
        fill="currentColor"
        opacity="0.1"
      />
    </svg>
  );
}

/** A row re-renders only when its props changed; the transcript keeps an
 * unchanged message's object and callbacks by identity. */
export const ChatMessage = React.memo(function ChatMessage(
  props: ChatMessageProps,
) {
  if (props.message.role === "assistant") {
    return (
      <AssistantTurn
        key={props.message.id}
        message={props.message}
        isStreaming={props.isStreaming}
        sessionId={props.message.sessionId}
        onCitationClick={props.onCitationClick}
        onArtifactAction={props.onArtifactAction}
      />
    );
  }
  return <RoleMessage {...props} />;
});

function RoleMessage({
  message,
  isQueued = false,
  errorExplained = false,
  onCitationClick,
}: ChatMessageProps) {
  const { role, content, status, createdAt, errorMessage } = message;
  const workspaceNamesById = useWorkspaceNames();
  const citations = Array.isArray(message.citations) ? message.citations : [];
  const text = renderInlineMarkdown(
    content,
    citations,
    onCitationClick,
    workspaceNamesById,
  );
  if (role === "system")
    return (
      <div className="flex justify-center py-2 px-4">
        <div className="min-w-0 w-full text-xs text-[var(--g5-text2)] text-center">
          {text}
        </div>
      </div>
    );
  if (role === "tool")
    return (
      <div className="px-4 py-1">
        <details className="group">
          <summary
            data-testid="chat-message-tool-output-toggle"
            className="flex min-w-0 w-full items-center gap-2 overflow-hidden text-xs text-[var(--g5-text2)] cursor-pointer hover:text-[var(--g5-text1)] transition-colors list-none"
          >
            <ChevronDown
              size={12}
              className="shrink-0 group-open:rotate-180 transition-transform"
            />
            <span className="shrink-0 font-medium">Tool output</span>
            <span className="min-w-0 flex-1 truncate opacity-60">
              {content.slice(0, 80)}
            </span>
          </summary>
          <pre className="mt-1 ml-5 p-2 text-xs text-[var(--g5-text1)] font-[var(--g5-font-mono)] bg-[var(--g5-bg0)] rounded-[var(--g5-radius-sm)] overflow-auto max-h-64">
            {content}
          </pre>
        </details>
      </div>
    );
  return (
    <div
      data-testid="user-message"
      data-message-id={message.id}
      className="flex flex-col items-end gap-1 px-3 py-1.5"
    >
      <div
        title={
          status === "error" && message.codexAuthFailure
            ? "Message not delivered"
            : undefined
        }
        className={classNames(
          "min-w-0 max-w-[min(100%,8in)] rounded-xl px-3 py-2 text-[13px] leading-relaxed bg-[var(--g5-info)]/20 text-[var(--g5-text0)]",
          isQueued && "opacity-70",
          status === "pending" && "opacity-60",
          status === "error" &&
            "border border-[var(--g5-blocked)] bg-[var(--g5-blocked)]/5",
          status === "stopped" && "border border-[var(--g5-warn)]/50",
        )}
      >
        {isQueued ? (
          <div className="flex min-w-0 items-start gap-2">
            <QueuedMessageIcon />
            <div className="min-w-0 whitespace-pre-wrap break-words">
              {text}
            </div>
          </div>
        ) : (
          <div className="whitespace-pre-wrap break-words">{text}</div>
        )}
        {status === "error" && !errorExplained && (
          <ChatErrorNotice className="mt-2">
            {errorMessage?.trim() || "Error generating response"}
          </ChatErrorNotice>
        )}
        {status === "stopped" && (
          <div className="flex items-center gap-2 mt-2 text-xs text-[var(--g5-warn)]">
            Stopped
          </div>
        )}
      </div>
      <CitationChips
        citations={citations}
        onCitationClick={onCitationClick}
        workspaceNamesById={workspaceNamesById}
      />
      {status === "complete" && createdAt && (
        <div className="text-[10px] text-[var(--g5-text2)] px-1">
          {formatMessageTimestamp(createdAt)}
        </div>
      )}
    </div>
  );
}
