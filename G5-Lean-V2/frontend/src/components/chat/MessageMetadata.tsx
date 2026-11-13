import { FileText } from "lucide-react";
import type { Citation } from "../../lib/types";
import { labelWorkspaceFolderInPath } from "../../lib/utils";
import { classNames } from "../ui/UI";

export function formatMessageTimestamp(createdAt: string): string {
  return new Date(createdAt).toLocaleString([], {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

export function CitationChips({
  citations,
  onCitationClick,
  workspaceNamesById,
}: {
  citations: Citation[] | null | undefined;
  onCitationClick?: (citation: Citation) => void;
  workspaceNamesById?: Record<string, string>;
}) {
  if (!Array.isArray(citations) || citations.length === 0) return null;
  return (
    <div className="flex flex-wrap gap-1 mt-1 px-1 max-w-[90%]">
      {citations.map((citation, i) => {
        const fileLabel = citation.filePath
          ? workspaceNamesById
            ? labelWorkspaceFolderInPath(citation.filePath, workspaceNamesById)
            : citation.filePath
          : "";
        const interactive = Boolean(citation.filePath && onCitationClick);
        const props = {
          className: classNames(
            "inline-flex items-center gap-1 px-1.5 py-0.5 text-[10px]",
            "rounded-[var(--g5-radius-sm)] bg-[var(--g5-bg2)]",
            interactive
              ? "text-[var(--g5-info)] hover:bg-[var(--g5-info)]/10 transition-colors cursor-pointer"
              : "text-[var(--g5-text2)]",
          ),
          title: `${citation.label}${citation.filePath ? ` — ${fileLabel}` : ""}${citation.lineRange ? ` (L${citation.lineRange[0]}–${citation.lineRange[1]})` : ""}`,
        };
        const content = (
          <>
            {citation.filePath && <FileText size={10} />}
            <span>{citation.label}</span>
          </>
        );
        return interactive ? (
          <button
            key={i}
            {...props}
            type="button"
            data-testid="chat-message-citation"
            onClick={() => onCitationClick?.(citation)}
          >
            {content}
          </button>
        ) : (
          <span key={i} {...props} data-testid="chat-message-citation">
            {content}
          </span>
        );
      })}
    </div>
  );
}
