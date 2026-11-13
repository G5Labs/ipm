// =============================================================================
// G5 Client — Intent Traceability Panel (Table Layout)
// =============================================================================
// Inline panel that displays parent or child intent records for a given I/D
// block. The standalone surface keeps its table, while semantic record frames
// use responsive result cards that fit the current editor width. Each result
// has navigate, expand-subtree, and hide actions; intent labels remain links to
// their source files.
// =============================================================================

import React, { useCallback, useState } from "react";
import { Link, Minus, Plus, X } from "lucide-react";
import { useTraceParents, useTraceChildren } from "../../hooks/useGrounding";
import type { TraceRequest, TraceResult } from "../../hooks/useGrounding";
import { classNames } from "../ui/UI";
import { MarkdownRenderer } from "../shared/MarkdownRenderer";
import { AnchoredPopover } from "./AnchoredPopover";
import { IdentityAvatar } from "../shared/IdentityAvatar";

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface NavigateTarget {
  type: "spec" | "file" | "doc";
  id: string;
  workspaceId?: string;
  repoId?: string;
  intentRecordNumber?: string;
  /** Source file the navigation originated from (for the back button) */
  sourceFileName?: string;
  /** Source intent record label the navigation originated from */
  sourceIntentRecordNumber?: string;
  /** Which editor tab family the source file belongs to. */
  sourceFileType?: "code" | "spec" | "doc";
}

export interface IntentTraceabilityPanelProps {
  /** Record cards use a compact, width-contained result layout. */
  variant?: "table" | "record";
  direction: "parents" | "children";
  workspaceId: string;
  repoId: string;
  fileType: string;
  fileName: string;
  intentRecordNumber: string;
  onDismiss: () => void;
  onNavigate: (target: NavigateTarget) => void;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

type SourceFileType = "code" | "spec" | "doc";

function isProjectDocFile(fileName: string | null): boolean {
  return !!fileName && fileName.startsWith("docs/");
}

function isNavigable(r: TraceResult): boolean {
  if (!r.fileName || r.fileType === "chat") return false;
  if (r.fileType === "code") return !!r.sourceFilePath;
  if (r.fileType === "external_doc") return isProjectDocFile(r.fileName);
  return ["ontology", "view"].includes(r.fileType);
}

function sourceFileTypeFromTraceFileType(fileType: string): SourceFileType {
  if (fileType === "code") return "code";
  if (fileType === "ontology") return "spec";
  return "doc";
}

/** Scope local UI state to the record owner as well as its record id. */
function traceResultStateKey(result: TraceResult): string {
  return JSON.stringify([
    result.workspaceId ?? "",
    result.repoId ?? "",
    result.intentRecordId,
  ]);
}

function navigateTarget(
  r: TraceResult,
  sourceFileName?: string,
  sourceIntentRecordNumber?: string,
  sourceFileType?: SourceFileType,
): NavigateTarget | null {
  if (!isNavigable(r) || !r.fileName) return null;
  const type =
    r.fileType === "code" ? "file" : r.fileType === "ontology" ? "spec" : "doc";
  return {
    type,
    // The canonical fileName remains the trace/grounding identity. Code file
    // navigation uses only the API-resolved physical path and never guesses a
    // checkout prefix in the client.
    id: r.fileType === "code" ? r.sourceFilePath! : r.fileName,
    workspaceId: r.workspaceId,
    repoId: r.repoId,
    intentRecordNumber: r.intentRecordNumber ?? r.codeBlockName ?? undefined,
    sourceFileName,
    sourceIntentRecordNumber,
    sourceFileType,
  };
}

// ---------------------------------------------------------------------------
// File name formatting — type tag + shortened path
// ---------------------------------------------------------------------------

const FILE_TYPE_TAG_STYLES: Record<string, { label: string; cls: string }> = {
  ontology: { label: "ontology", cls: "bg-blue-500/20 text-blue-400" },
  view: { label: "view", cls: "bg-green-500/20 text-green-400" },
  code: { label: "code", cls: "bg-amber-500/20 text-amber-400" },
  chat: { label: "chat", cls: "bg-purple-500/20 text-purple-400" },
  external_doc: { label: "external", cls: "bg-cyan-500/20 text-cyan-400" },
};

/** Shorten a file path to just the basename (last path segment) after stripping known prefixes/suffixes. */
function shortenFileName(
  fileType: string,
  fileName: string | null,
  intentRecordId?: string,
): string {
  if (fileType === "chat") {
    // Chat intents have no navigable file — show abbreviated session id
    const sessionId = intentRecordId ? intentRecordId.slice(0, 8) : "—";
    return sessionId;
  }
  if (!fileName) return "—";
  let s = fileName;
  if (fileType === "ontology") {
    s = s
      .replace(/^\.?semlift\/specs\//, "")
      .replace(/^\.semlift\/specs\//, "");
    s = s.replace(/\.semlift\.md$/, "");
  } else if (fileType === "view") {
    s = s.replace(/^views\//, "");
    s = s.replace(/\.pm_view\.md$/, "").replace(/\.test_view\.md$/, "");
  } else if (fileType === "external_doc") {
    s = s.replace(/^docs\//, "");
  }
  // Extract only the last path segment (basename)
  const lastSlash = s.lastIndexOf("/");
  if (lastSlash >= 0) s = s.substring(lastSlash + 1);
  return s;
}

// Titles and labels come from server canonical-extraction metadata
// (`title`/`blockLabel` on each trace result — grounding_identity_and_routing
// D10): the panel performs no label-line parsing of `content`, which arrives
// label-line-free for non-chat records.

interface TraceContentPreviewProps {
  content: string;
  isCode: boolean;
  variant: "table" | "record";
}

const TraceContentPreview: React.FC<TraceContentPreviewProps> = ({
  content,
  isCode,
  variant,
}) => {
  const [expanded, setExpanded] = useState(false);
  const toggleExpanded = () => setExpanded((current) => !current);
  const expansionLabel = expanded
    ? "Collapse intent content"
    : "Expand intent content";
  const collapsedMaxHeight = isCode
    ? variant === "record"
      ? "52px"
      : "40px"
    : variant === "record"
      ? "40px"
      : "36px";

  return (
    <div
      className={classNames(
        "block w-full rounded text-left",
        variant === "record" && "mt-2",
      )}
    >
      <div
        data-testid="trace-content-preview"
        role="button"
        tabIndex={0}
        aria-expanded={expanded}
        aria-label={expansionLabel}
        title={expanded ? "Collapse content" : "Expand content"}
        onClick={(event) => {
          if ((event.target as HTMLElement).closest("a,button")) return;
          toggleExpanded();
        }}
        onKeyDown={(event) => {
          if (event.key !== "Enter" && event.key !== " ") return;
          event.preventDefault();
          toggleExpanded();
        }}
        style={
          expanded
            ? undefined
            : {
                maxHeight: collapsedMaxHeight,
                overflow: "hidden",
              }
        }
        className={classNames(
          "block cursor-pointer break-words text-[var(--g5-text1)] transition-colors hover:text-[var(--g5-text0)] focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-[var(--g5-info)]",
          variant === "record"
            ? "text-[11px] leading-[18px]"
            : "text-[10px] leading-[16px]",
        )}
      >
        {isCode ? (
          <pre
            data-testid="trace-code-preview"
            className={classNames(
              "m-0 whitespace-pre-wrap break-words rounded bg-[var(--g5-bg0)] font-mono text-inherit leading-inherit",
              variant === "record" ? "px-2 py-1.5" : "px-1 py-0.5",
            )}
          >
            <code>{content}</code>
          </pre>
        ) : (
          <MarkdownRenderer
            content={content}
            className={classNames(
              "!text-inherit",
              "[&_p]:!m-0 [&_p]:!text-inherit [&_p]:!leading-inherit [&_p+*]:!mt-1.5",
              "[&_ul]:!my-1 [&_ol]:!my-1 [&_li]:!my-0 [&_li]:!text-inherit [&_li]:!leading-inherit",
              "[&_blockquote]:!my-1 [&_blockquote]:!py-0 [&_blockquote]:!text-inherit",
              "[&_table]:!my-1 [&_table]:!text-[10px] [&_th]:!px-1.5 [&_th]:!py-1 [&_td]:!px-1.5 [&_td]:!py-1",
              "[&_pre]:!my-1 [&_pre]:!text-[10px]",
              "[&_h1]:!my-1 [&_h1]:!text-sm [&_h2]:!my-1 [&_h2]:!text-sm [&_h3]:!my-1 [&_h3]:!text-xs",
            )}
          />
        )}
      </div>
      <button
        type="button"
        data-testid="trace-content-toggle"
        aria-expanded={expanded}
        aria-label={expansionLabel}
        title={expanded ? "Collapse content" : "Expand content"}
        onClick={toggleExpanded}
        className="mt-1 rounded px-1 py-0.5 text-[9px] font-medium text-[var(--g5-info)] hover:bg-[var(--g5-info)]/10 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-[var(--g5-info)]"
      >
        {expanded ? "Show less" : "Show more"}
      </button>
    </div>
  );
};

// ---------------------------------------------------------------------------
// Table row for a single trace result (reused at all nesting levels)
// ---------------------------------------------------------------------------

interface TraceTableRowProps {
  r: TraceResult;
  depth: number;
  isLast: boolean;
  isExpanded: boolean;
  direction: "parents" | "children";
  onNavigate: (target: NavigateTarget) => void;
  onToggleExpand: (id: string) => void;
  onHide: (id: string) => void;
  sourceFileName: string;
  sourceIntentRecordNumber: string;
  sourceFileType: SourceFileType;
}

interface ChatDetailPopoverProps {
  result: TraceResult;
  anchor: HTMLElement;
  onDismiss: () => void;
}

const ChatDetailPopover: React.FC<ChatDetailPopoverProps> = ({
  result,
  anchor,
  onDismiss,
}) => {
  const hasStructuredText =
    result.chatContext != null || result.userContent != null;

  return (
    <AnchoredPopover
      anchor={anchor}
      ariaLabel="Chat intent details"
      onDismiss={onDismiss}
      testId="chat-intent-details"
    >
      <div className="mb-2 flex items-start justify-between gap-2">
        <div className="min-w-0">
          <p className="text-xs font-medium text-[var(--g5-text0)]">
            Chat intent
          </p>
          <p className="truncate font-mono text-[11px] text-[var(--g5-info)]">
            {result.chatSessionId ?? result.intentRecordId}
          </p>
        </div>
        <button
          type="button"
          data-testid="intent-traceability-close-chat-details-button"
          aria-label="Close chat details"
          className="shrink-0 rounded p-1 text-[var(--g5-text2)] hover:bg-[var(--g5-overlay-hover)]"
          onClick={onDismiss}
        >
          <X size={14} />
        </button>
      </div>

      <div className="mb-2 flex flex-wrap gap-1 text-[10px] text-[var(--g5-text2)]">
        {result.isHuman && (
          <span className="rounded bg-[var(--g5-ok)]/15 px-1.5 py-0.5 text-[var(--g5-ok)]">
            Human-authored
          </span>
        )}
        {result.priority != null && (
          <span className="rounded bg-[var(--g5-bg0)] px-1.5 py-0.5">
            Priority {result.priority}
          </span>
        )}
        {result.certainty != null && (
          <span className="rounded bg-[var(--g5-bg0)] px-1.5 py-0.5">
            Certainty {result.certainty}%
          </span>
        )}
      </div>

      {(result.createdBy || result.createdAt) && (
        <dl className="mb-2 grid grid-cols-[auto_1fr] gap-x-2 text-[10px] text-[var(--g5-text2)]">
          {result.createdBy && (
            <>
              <dt>Created by</dt>
              <dd className="flex items-center gap-1.5 break-all text-[var(--g5-text1)]">
                <IdentityAvatar identity={result.createdBy} size={14} />
                <span className="break-all">{result.createdBy}</span>
              </dd>
            </>
          )}
          {result.createdAt && (
            <>
              <dt>Created at</dt>
              <dd className="break-all text-[var(--g5-text1)]">
                {result.createdAt}
              </dd>
            </>
          )}
        </dl>
      )}

      <div className="max-h-72 space-y-2 overflow-auto">
        {result.chatContext != null && (
          <div>
            <p className="mb-0.5 text-[10px] font-medium uppercase tracking-wide text-[var(--g5-text2)]">
              Context
            </p>
            <pre className="whitespace-pre-wrap rounded border border-[var(--g5-stroke)]/50 bg-[var(--g5-bg0)] p-2 text-[11px] leading-relaxed text-[var(--g5-text0)]">
              {result.chatContext}
            </pre>
          </div>
        )}
        {result.userContent != null && (
          <div>
            <p className="mb-0.5 text-[10px] font-medium uppercase tracking-wide text-[var(--g5-text2)]">
              User content
            </p>
            <pre className="whitespace-pre-wrap rounded border border-[var(--g5-stroke)]/50 bg-[var(--g5-bg0)] p-2 text-[11px] leading-relaxed text-[var(--g5-text0)]">
              {result.userContent}
            </pre>
          </div>
        )}
        {!hasStructuredText && result.content != null && (
          <div>
            <p className="mb-0.5 text-[10px] font-medium uppercase tracking-wide text-[var(--g5-text2)]">
              Content
            </p>
            <pre className="whitespace-pre-wrap rounded border border-[var(--g5-stroke)]/50 bg-[var(--g5-bg0)] p-2 text-[11px] leading-relaxed text-[var(--g5-text0)]">
              {result.content}
            </pre>
          </div>
        )}
      </div>
    </AnchoredPopover>
  );
};

const TraceTableRow: React.FC<TraceTableRowProps> = ({
  r,
  depth,
  isLast,
  isExpanded,
  direction,
  onNavigate,
  onToggleExpand,
  onHide,
  sourceFileName,
  sourceIntentRecordNumber,
  sourceFileType,
}) => {
  const [chatAnchor, setChatAnchor] = useState<HTMLElement | null>(null);
  const hasId = !!(r.intentRecordNumber ?? r.codeBlockName);
  const navTarget = navigateTarget(
    r,
    sourceFileName,
    sourceIntentRecordNumber,
    sourceFileType,
  );
  const tagStyle = FILE_TYPE_TAG_STYLES[r.fileType] ?? {
    label: r.fileType,
    cls: "bg-gray-500/20 text-gray-400",
  };
  const shortName = shortenFileName(r.fileType, r.fileName, r.intentRecordId);
  const treeGlyph = depth > 0 ? (isLast ? "└── " : "├── ") : "";
  const content = r.content;
  const intentLabel =
    r.blockLabel ?? r.intentRecordNumber ?? r.codeBlockName ?? "—";
  const intentTitle = r.title ?? null;
  const fullIntentLabel = intentTitle
    ? `${intentLabel}. ${intentTitle}`
    : intentLabel;
  const chatLabel = r.chatSessionId ?? r.intentRecordId.slice(0, 8);

  return (
    <>
      <tr className="border-b border-[var(--g5-stroke)]/20 hover:bg-[var(--g5-bg0)]/30">
        {/* File */}
        <td
          className="py-1 pr-2 text-xs align-top"
          style={{ paddingLeft: `${depth * 24}px` }}
        >
          <div className="flex items-center gap-1">
            {treeGlyph && (
              <span className="font-mono text-[var(--g5-text2)]/30 whitespace-pre select-none">
                {treeGlyph}
              </span>
            )}
            <span
              className={classNames(
                "px-1 py-0.5 rounded text-[10px] font-medium shrink-0",
                tagStyle.cls,
              )}
            >
              {tagStyle.label}
            </span>
            <span className="text-[var(--g5-text1)] break-all text-xs">
              {shortName}
            </span>
          </div>
        </td>
        {/* Intent */}
        <td className="py-1 pr-2 text-xs align-top overflow-hidden">
          <div className="truncate">
            {navTarget ? (
              <button
                type="button"
                data-testid="intent-traceability-open-intent"
                data-g5-traceability-identifier
                className="font-mono text-[var(--g5-info)] hover:underline cursor-pointer bg-transparent border-none p-0 text-left text-xs truncate max-w-full"
                title={`${fullIntentLabel} — Open ${r.fileName}`}
                onClick={(event) => {
                  event.stopPropagation();
                  onNavigate(navTarget);
                }}
              >
                {fullIntentLabel}
              </button>
            ) : r.fileType === "chat" ? (
              <button
                type="button"
                data-testid="intent-traceability-open-chat"
                data-g5-traceability-identifier
                data-g5-anchored-popover-trigger
                aria-label={`Show chat details for ${chatLabel}`}
                className="max-w-full cursor-pointer truncate border-none bg-transparent p-0 text-left font-mono text-xs text-[var(--g5-info)] hover:underline"
                onClick={(event) => {
                  event.stopPropagation();
                  setChatAnchor(event.currentTarget);
                }}
              >
                {fullIntentLabel}
              </button>
            ) : (
              <span
                data-g5-traceability-identifier
                className="font-mono text-[var(--g5-info)] truncate"
              >
                {fullIntentLabel}
              </span>
            )}
          </div>
          {r.isHuman && (
            <span
              className="inline-block w-1.5 h-1.5 ml-1 rounded-full bg-[var(--g5-ok)] align-middle"
              title="Human-authored"
            />
          )}
        </td>
        {/* Content */}
        <td className="py-1 pr-2 align-top">
          {content ? (
            <TraceContentPreview
              content={content}
              isCode={r.fileType === "code"}
              variant="table"
            />
          ) : (
            <span className="text-[10px] text-[var(--g5-text2)]/50">—</span>
          )}
        </td>
        {/* Actions */}
        <td className="py-1 text-xs align-top">
          <span className="flex items-center gap-0.5 justify-end">
            <button
              type="button"
              className="p-0.5 rounded hover:bg-[var(--g5-bg0)] text-[var(--g5-text2)] hover:text-[var(--g5-text0)] transition-colors"
              title={
                navTarget
                  ? `Open ${r.fileName}`
                  : r.fileType === "code"
                    ? "Code source unavailable — no file to open"
                    : "External intent — no file to open"
              }
              data-testid="trace-navigate"
              onClick={(event) => {
                event.stopPropagation();
                if (navTarget) onNavigate(navTarget);
              }}
              disabled={!navTarget}
            >
              <Link size={12} className={!navTarget ? "opacity-30" : ""} />
            </button>
            {hasId && (
              <button
                type="button"
                className="p-0.5 rounded hover:bg-[var(--g5-bg0)] text-[var(--g5-text2)] hover:text-[var(--g5-text0)] transition-colors"
                title={isExpanded ? "Collapse sub-tree" : "Expand sub-tree"}
                data-testid="trace-expand"
                onClick={(event) => {
                  event.stopPropagation();
                  onToggleExpand(traceResultStateKey(r));
                }}
              >
                <Plus size={12} className={isExpanded ? "rotate-45" : ""} />
              </button>
            )}
            <button
              type="button"
              className="p-0.5 rounded hover:bg-[var(--g5-bg0)] text-[var(--g5-text2)] hover:text-[var(--g5-text0)] transition-colors"
              title="Hide this intent"
              data-testid="trace-hide"
              onClick={(event) => {
                event.stopPropagation();
                onHide(traceResultStateKey(r));
              }}
            >
              <Minus size={12} />
            </button>
          </span>
        </td>
      </tr>
      {/* Expanded sub-table */}
      {isExpanded && (
        <tr>
          <td colSpan={4} className="p-0">
            <ExpandedSubTable
              parentResult={r}
              direction={direction}
              depth={depth + 1}
              onNavigate={onNavigate}
              sourceFileName={sourceFileName}
              sourceIntentRecordNumber={sourceIntentRecordNumber}
              sourceFileType={sourceFileType}
            />
          </td>
        </tr>
      )}
      {chatAnchor && (
        <ChatDetailPopover
          result={r}
          anchor={chatAnchor}
          onDismiss={() => setChatAnchor(null)}
        />
      )}
    </>
  );
};

interface TraceRecordCardProps {
  r: TraceResult;
  isExpanded: boolean;
  direction: "parents" | "children";
  onNavigate: (target: NavigateTarget) => void;
  onToggleExpand: (id: string) => void;
  onHide: (id: string) => void;
  sourceFileName: string;
  sourceIntentRecordNumber: string;
  sourceFileType: SourceFileType;
}

const TraceRecordCard: React.FC<TraceRecordCardProps> = ({
  r,
  isExpanded,
  direction,
  onNavigate,
  onToggleExpand,
  onHide,
  sourceFileName,
  sourceIntentRecordNumber,
  sourceFileType,
}) => {
  const [chatAnchor, setChatAnchor] = useState<HTMLElement | null>(null);
  const hasId = Boolean(r.intentRecordNumber ?? r.codeBlockName);
  const navTarget = navigateTarget(
    r,
    sourceFileName,
    sourceIntentRecordNumber,
    sourceFileType,
  );
  const tagStyle = FILE_TYPE_TAG_STYLES[r.fileType] ?? {
    label: r.fileType,
    cls: "bg-gray-500/20 text-gray-400",
  };
  const shortName = shortenFileName(r.fileType, r.fileName, r.intentRecordId);
  const intentLabel =
    r.blockLabel ?? r.intentRecordNumber ?? r.codeBlockName ?? "—";
  const intentTitle = r.title ?? null;
  const fullIntentLabel = intentTitle
    ? `${intentLabel}. ${intentTitle}`
    : intentLabel;
  const chatLabel = r.chatSessionId ?? r.intentRecordId.slice(0, 8);
  const content = r.content;

  return (
    <article
      data-testid="traceability-record-card"
      className="min-w-0 rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)]/45 bg-[var(--g5-bg1)]/80 p-3 shadow-sm"
    >
      <div className="flex min-w-0 items-start justify-between gap-3">
        <div className="flex min-w-0 flex-1 flex-wrap items-center gap-1.5 text-xs">
          <span
            className={classNames(
              "shrink-0 rounded px-1.5 py-0.5 text-[9px] font-semibold uppercase tracking-wide",
              tagStyle.cls,
            )}
          >
            {tagStyle.label}
          </span>
          <span className="min-w-0 break-all text-[var(--g5-text2)]">
            {shortName}
          </span>
          <span aria-hidden="true" className="text-[var(--g5-text2)]/40">
            /
          </span>
          {navTarget ? (
            <button
              type="button"
              data-testid="intent-traceability-open-intent"
              data-g5-traceability-identifier
              className="min-w-0 break-words bg-transparent p-0 text-left font-mono font-semibold text-[var(--g5-info)] hover:underline"
              title={`${fullIntentLabel} — Open ${r.fileName}`}
              onClick={() => onNavigate(navTarget)}
            >
              {fullIntentLabel}
            </button>
          ) : r.fileType === "chat" ? (
            <button
              type="button"
              data-testid="intent-traceability-open-chat"
              data-g5-traceability-identifier
              data-g5-anchored-popover-trigger
              aria-label={`Show chat details for ${chatLabel}`}
              className="min-w-0 break-words bg-transparent p-0 text-left font-mono font-semibold text-[var(--g5-info)] hover:underline"
              onClick={(event) => setChatAnchor(event.currentTarget)}
            >
              {fullIntentLabel}
            </button>
          ) : (
            <span
              data-g5-traceability-identifier
              className="min-w-0 break-words font-mono font-semibold text-[var(--g5-info)]"
            >
              {fullIntentLabel}
            </span>
          )}
          {r.isHuman && (
            <span
              className="h-1.5 w-1.5 shrink-0 rounded-full bg-[var(--g5-ok)]"
              title="Human-authored"
            />
          )}
        </div>
        <div className="flex shrink-0 items-center gap-1 text-[var(--g5-text2)]">
          <button
            type="button"
            data-testid="trace-navigate"
            title={
              navTarget
                ? `Open ${r.fileName}`
                : r.fileType === "code"
                  ? "Code source unavailable — no file to open"
                  : "External intent — no file to open"
            }
            disabled={!navTarget}
            className="rounded p-1 hover:bg-[var(--g5-overlay-hover)] hover:text-[var(--g5-text0)] disabled:cursor-not-allowed disabled:opacity-30"
            onClick={() => navTarget && onNavigate(navTarget)}
          >
            <Link size={12} />
          </button>
          {hasId && (
            <button
              type="button"
              data-testid="trace-expand"
              title={isExpanded ? "Collapse sub-tree" : "Expand sub-tree"}
              className="rounded p-1 hover:bg-[var(--g5-overlay-hover)] hover:text-[var(--g5-text0)]"
              onClick={() => onToggleExpand(traceResultStateKey(r))}
            >
              <Plus size={12} className={isExpanded ? "rotate-45" : ""} />
            </button>
          )}
          <button
            type="button"
            data-testid="trace-hide"
            title="Hide this intent"
            className="rounded p-1 hover:bg-[var(--g5-overlay-hover)] hover:text-[var(--g5-text0)]"
            onClick={() => onHide(traceResultStateKey(r))}
          >
            <Minus size={12} />
          </button>
        </div>
      </div>
      {content ? (
        <TraceContentPreview
          content={content}
          isCode={r.fileType === "code"}
          variant="record"
        />
      ) : (
        <span
          data-testid="trace-no-content"
          className="mt-2 block text-[11px] text-[var(--g5-text2)]/50"
        >
          —
        </span>
      )}
      {isExpanded && (
        <div className="mt-2 min-w-0 overflow-x-auto border-t border-[var(--g5-stroke)]/30 pt-2">
          <ExpandedSubTable
            parentResult={r}
            direction={direction}
            depth={1}
            onNavigate={onNavigate}
            sourceFileName={sourceFileName}
            sourceIntentRecordNumber={sourceIntentRecordNumber}
            sourceFileType={sourceFileType}
          />
        </div>
      )}
      {chatAnchor && (
        <ChatDetailPopover
          result={r}
          anchor={chatAnchor}
          onDismiss={() => setChatAnchor(null)}
        />
      )}
    </article>
  );
};

// ---------------------------------------------------------------------------
// Sub-table expansion (recursive)
// ---------------------------------------------------------------------------

interface ExpandedSubTableProps {
  parentResult: TraceResult;
  direction: "parents" | "children";
  depth: number;
  onNavigate: (target: NavigateTarget) => void;
  sourceFileName: string;
  sourceIntentRecordNumber: string;
  sourceFileType: SourceFileType;
}

const ExpandedSubTable: React.FC<ExpandedSubTableProps> = ({
  parentResult,
  direction,
  depth,
  onNavigate,
  sourceFileName,
  sourceIntentRecordNumber,
  sourceFileType,
}) => {
  const [expandedNodes, setExpandedNodes] = useState<Set<string>>(new Set());
  const [hiddenNodes, setHiddenNodes] = useState<Set<string>>(new Set());

  const subRequest: TraceRequest | null = parentResult.fileName
    ? {
        workspaceId: parentResult.workspaceId,
        repoId: parentResult.repoId,
        fileType: parentResult.fileType,
        fileName: parentResult.fileName,
        intentRecordNumber:
          parentResult.intentRecordNumber ?? parentResult.codeBlockName ?? "",
      }
    : null;

  const parentsResult = useTraceParents(
    direction === "parents" ? subRequest : null,
  );
  const childrenResult = useTraceChildren(
    direction === "children" ? subRequest : null,
  );
  const result = direction === "parents" ? parentsResult : childrenResult;

  const toggleExpand = useCallback((id: string) => {
    setExpandedNodes((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  const hideNode = useCallback((id: string) => {
    setHiddenNodes((prev) => new Set(prev).add(id));
    setExpandedNodes((prev) => {
      const next = new Set(prev);
      next.delete(id);
      return next;
    });
  }, []);

  const indentPx = depth * 24;

  if (result.status === "loading") {
    return (
      <div
        className="flex items-center gap-1.5 py-1 text-xs text-[var(--g5-text2)]"
        style={{ paddingLeft: `${indentPx}px` }}
      >
        <div className="w-3 h-3 border border-current border-t-transparent rounded-full animate-spin" />
        Loading…
      </div>
    );
  }

  if (result.status === "error") {
    return (
      <p
        className="py-1 text-xs text-[var(--g5-text2)] italic"
        style={{ paddingLeft: `${indentPx}px` }}
      >
        Failed to load
      </p>
    );
  }

  if (result.status !== "ok" || result.data.results.length === 0) {
    return (
      <p
        className="py-1 text-xs text-[var(--g5-text2)]/50 italic"
        style={{ paddingLeft: `${indentPx}px` }}
      >
        {result.status === "ok" && result.data.availability === "not_recorded"
          ? "No explicit grounding record is available."
          : "None"}
      </p>
    );
  }

  const visibleResults = result.data.results.filter(
    (r) => !hiddenNodes.has(traceResultStateKey(r)),
  );

  return (
    <div
      className="border-l-2 border-[var(--g5-info)]/30"
      style={{ marginLeft: `${indentPx}px` }}
    >
      <table className="w-full table-fixed">
        <tbody>
          {visibleResults.map((r, idx) => (
            <TraceTableRow
              key={traceResultStateKey(r)}
              r={r}
              depth={0}
              isLast={idx === visibleResults.length - 1}
              isExpanded={expandedNodes.has(traceResultStateKey(r))}
              direction={direction}
              onNavigate={onNavigate}
              onToggleExpand={toggleExpand}
              onHide={hideNode}
              sourceFileName={sourceFileName}
              sourceIntentRecordNumber={sourceIntentRecordNumber}
              sourceFileType={sourceFileType}
            />
          ))}
        </tbody>
      </table>
    </div>
  );
};

// ---------------------------------------------------------------------------
// Main Panel Component (Table Layout)
// ---------------------------------------------------------------------------

export const IntentTraceabilityPanel: React.FC<
  IntentTraceabilityPanelProps
> = ({
  variant = "table",
  direction,
  workspaceId,
  repoId,
  fileType,
  fileName,
  intentRecordNumber,
  onDismiss,
  onNavigate,
}) => {
  const [expandedNodes, setExpandedNodes] = useState<Set<string>>(new Set());
  const [hiddenNodes, setHiddenNodes] = useState<Set<string>>(new Set());

  const request: TraceRequest = {
    workspaceId,
    repoId,
    fileType,
    fileName,
    intentRecordNumber,
  };

  // Both hooks must be called unconditionally (rules of hooks).
  // Pass null to disable the unused direction.
  const parentsResult = useTraceParents(
    direction === "parents" ? request : null,
  );
  const childrenResult = useTraceChildren(
    direction === "children" ? request : null,
  );
  const result = direction === "parents" ? parentsResult : childrenResult;

  const dirLabel = direction === "parents" ? "Source" : "Drives";
  const emptyLabel =
    result.status === "ok" && result.data.availability === "not_recorded"
      ? "No explicit grounding record is available for this statement."
      : direction === "parents"
        ? "No sources found"
        : "No driven intents found";

  // Parents use darker bg; children use lighter bg
  const bgClass =
    direction === "parents" ? "bg-[var(--g5-bg1)]" : "bg-[var(--g5-bg2)]";

  const toggleExpand = useCallback((id: string) => {
    setExpandedNodes((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  const hideNode = useCallback((id: string) => {
    setHiddenNodes((prev) => new Set(prev).add(id));
    setExpandedNodes((prev) => {
      const next = new Set(prev);
      next.delete(id);
      return next;
    });
  }, []);

  return (
    <div
      data-testid="intent-traceability-panel"
      data-direction={direction}
      className={classNames(
        variant === "record"
          ? "min-w-0 rounded-[var(--g5-radius-sm)] bg-transparent"
          : "ml-4 mb-2 rounded-r-[var(--g5-radius-sm)] border-l-2 border-[var(--g5-info)] py-2 pr-2 pl-3",
        variant === "table" && bgClass,
      )}
    >
      {/* Header */}
      <div className="mb-2 flex items-center justify-between gap-2">
        <span
          className={classNames(
            "text-xs font-medium text-[var(--g5-text2)]",
            variant === "record" &&
              "uppercase tracking-[0.14em] text-[var(--g5-text1)]",
          )}
        >
          {dirLabel}
          {variant === "record" ? ` · ${intentRecordNumber}` : ""}
        </span>
        <button
          type="button"
          className="p-0.5 rounded hover:bg-[var(--g5-bg0)] text-[var(--g5-text2)] hover:text-[var(--g5-text0)] transition-colors"
          onClick={onDismiss}
          title="Dismiss"
          data-testid="traceability-dismiss"
        >
          <X size={12} />
        </button>
      </div>

      {/* Content */}
      {result.status === "loading" && (
        <div className="flex items-center gap-1.5 text-xs text-[var(--g5-text2)]">
          <div className="w-3 h-3 border border-current border-t-transparent rounded-full animate-spin" />
          Loading…
        </div>
      )}

      {result.status === "error" && (
        <p className="text-xs text-[var(--g5-text2)] italic">
          Failed to load intents
        </p>
      )}

      {result.status === "ok" && result.data.results.length === 0 && (
        <p className="text-xs text-[var(--g5-text2)] italic">{emptyLabel}</p>
      )}

      {result.status === "ok" &&
        result.data.results.length > 0 &&
        (variant === "record" ? (
          <div
            data-testid="traceability-record-results"
            className="grid min-w-0 gap-2"
          >
            {result.data.results
              .filter((r) => !hiddenNodes.has(traceResultStateKey(r)))
              .map((r) => (
                <TraceRecordCard
                  key={traceResultStateKey(r)}
                  r={r}
                  isExpanded={expandedNodes.has(traceResultStateKey(r))}
                  direction={direction}
                  onNavigate={onNavigate}
                  onToggleExpand={toggleExpand}
                  onHide={hideNode}
                  sourceFileName={fileName}
                  sourceIntentRecordNumber={intentRecordNumber}
                  sourceFileType={sourceFileTypeFromTraceFileType(fileType)}
                />
              ))}
          </div>
        ) : (
          <table className="w-full table-fixed">
            <colgroup>
              <col style={{ width: "20%" }} />
              <col style={{ width: "26%" }} />
              <col style={{ width: "46%" }} />
              <col style={{ width: "8%" }} />
            </colgroup>
            <thead>
              <tr className="border-b border-[var(--g5-stroke)]/30">
                <th className="pb-0.5 text-left text-[10px] uppercase tracking-wider text-[var(--g5-text2)]/50 font-normal">
                  File
                </th>
                <th className="pb-0.5 text-left text-[10px] uppercase tracking-wider text-[var(--g5-text2)]/50 font-normal">
                  Intent
                </th>
                <th className="pb-0.5 text-left text-[10px] uppercase tracking-wider text-[var(--g5-text2)]/50 font-normal">
                  Content
                </th>
                <th />
              </tr>
            </thead>
            <tbody>
              {result.data.results
                .filter((r) => !hiddenNodes.has(traceResultStateKey(r)))
                .map((r, idx, arr) => (
                  <TraceTableRow
                    key={traceResultStateKey(r)}
                    r={r}
                    depth={0}
                    isLast={idx === arr.length - 1}
                    isExpanded={expandedNodes.has(traceResultStateKey(r))}
                    direction={direction}
                    onNavigate={onNavigate}
                    onToggleExpand={toggleExpand}
                    onHide={hideNode}
                    sourceFileName={fileName}
                    sourceIntentRecordNumber={intentRecordNumber}
                    sourceFileType={sourceFileTypeFromTraceFileType(fileType)}
                  />
                ))}
            </tbody>
          </table>
        ))}
    </div>
  );
};
