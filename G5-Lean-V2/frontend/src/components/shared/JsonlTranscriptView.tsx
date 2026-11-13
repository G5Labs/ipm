import React, { useEffect, useMemo, useState } from "react";
import {
  AlertTriangle,
  Bot,
  Braces,
  Brain,
  ChevronDown,
  ChevronRight,
  Info,
  User,
  Wrench,
} from "lucide-react";
import { classNames } from "../ui/UI";
import { MarkdownRenderer } from "./MarkdownRenderer";
import {
  MAX_PARSED_LINES,
  asArray,
  asObject,
  asString,
  formatCharCount,
  isClaudeTranscript,
  parseJsonlDocument,
  prettyJson,
  type JsonObject,
  type JsonlLineRecord,
} from "./jsonlTranscript";

// =============================================================================
// JsonlTranscriptView — read-only rich preview for `.jsonl` project docs
// (center-panel spec D61). Biased toward the Claude Code session-transcript
// format; every other JSONL document falls back to a generic record list.
// Rendering is plain React DOM over parsed JSON: content is never treated as
// HTML (no iframe, no dangerouslySetInnerHTML). Markdown-bearing message text
// renders through the shared MarkdownRenderer, exactly as chat messages do.
// Pure parsing/detection helpers live in ./jsonlTranscript.ts.
// =============================================================================

/** Entries rendered before the explicit "Show more" control (D61 resilience). */
const RENDER_CHUNK = 200;
/** Character budget before long free text collapses behind an expand control. */
const TEXT_CLAMP_CHARS = 4_000;
/** Character budget for pretty-printed JSON payloads (tool input/results). */
const JSON_CLAMP_CHARS = 2_000;

// -----------------------------------------------------------------------------
// Small building blocks
// -----------------------------------------------------------------------------

interface ClampedPreProps {
  text: string;
  clampChars?: number;
}

/** Monospace payload with an explicit expand affordance past the clamp. */
function ClampedPre({ text, clampChars = JSON_CLAMP_CHARS }: ClampedPreProps) {
  const [expanded, setExpanded] = useState(false);
  const clipped = !expanded && text.length > clampChars;
  return (
    <div>
      <pre className="overflow-x-auto whitespace-pre-wrap break-words rounded-[var(--g5-radius-sm)] bg-[var(--g5-bg0)] p-2 font-[family-name:var(--g5-font-mono)] text-xs text-[var(--g5-text1)]">
        {clipped ? `${text.slice(0, clampChars)}…` : text}
      </pre>
      {text.length > clampChars && (
        <button
          type="button"
          data-testid="jsonl-transcript-toggle-json-clamp"
          className="mt-1 text-xs text-[var(--g5-info)] hover:underline"
          onClick={() => setExpanded((prev) => !prev)}
        >
          {expanded
            ? "Show less"
            : `Show all (${formatCharCount(text.length)})`}
        </button>
      )}
    </div>
  );
}

interface ClampedMarkdownProps {
  text: string;
}

/** Markdown text with an explicit expand affordance past the clamp. */
function ClampedMarkdown({ text }: ClampedMarkdownProps) {
  const [expanded, setExpanded] = useState(false);
  const clipped = !expanded && text.length > TEXT_CLAMP_CHARS;
  return (
    <div>
      <MarkdownRenderer
        content={clipped ? `${text.slice(0, TEXT_CLAMP_CHARS)}…` : text}
      />
      {text.length > TEXT_CLAMP_CHARS && (
        <button
          type="button"
          data-testid="jsonl-transcript-toggle-text-clamp"
          className="mt-1 text-xs text-[var(--g5-info)] hover:underline"
          onClick={() => setExpanded((prev) => !prev)}
        >
          {expanded
            ? "Show less"
            : `Show full text (${formatCharCount(text.length)})`}
        </button>
      )}
    </div>
  );
}

interface CollapsibleRowProps {
  icon: React.ReactNode;
  label: string;
  detail?: string;
  defaultOpen?: boolean;
  children: React.ReactNode;
}

/** Chevron-collapsed row used for thinking, tool, meta, and generic records. */
function CollapsibleRow({
  icon,
  label,
  detail,
  defaultOpen = false,
  children,
}: CollapsibleRowProps) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <div className="rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg1)]">
      <button
        type="button"
        data-testid="jsonl-transcript-collapsible-row-toggle"
        aria-expanded={open}
        onClick={() => setOpen((prev) => !prev)}
        className="flex w-full items-center gap-2 px-2 py-1.5 text-left text-xs text-[var(--g5-text1)] hover:bg-[var(--g5-bg2)]"
      >
        {open ? (
          <ChevronDown size={14} className="shrink-0 text-[var(--g5-text2)]" />
        ) : (
          <ChevronRight size={14} className="shrink-0 text-[var(--g5-text2)]" />
        )}
        <span className="shrink-0 text-[var(--g5-text2)]">{icon}</span>
        <span className="min-w-0 truncate font-medium">{label}</span>
        {detail && (
          <span className="min-w-0 truncate text-[var(--g5-text2)]">
            {detail}
          </span>
        )}
      </button>
      {open && <div className="px-2 pb-2">{children}</div>}
    </div>
  );
}

// -----------------------------------------------------------------------------
// Claude transcript rendering
// -----------------------------------------------------------------------------

/** Best-effort plain text extraction from a tool_result `content` value. */
function toolResultText(content: unknown): string {
  const text = asString(content);
  if (text !== null) return text;
  const blocks = asArray(content);
  if (blocks) {
    const parts = blocks
      .map((block) => {
        const obj = asObject(block);
        if (obj && obj.type === "text") return asString(obj.text) ?? "";
        return prettyJson(block);
      })
      .filter((part) => part.length > 0);
    if (parts.length > 0) return parts.join("\n");
  }
  return prettyJson(content);
}

function MessageBubble({
  role,
  children,
}: {
  role: "user" | "assistant";
  children: React.ReactNode;
}) {
  return (
    <div
      className={classNames(
        "rounded-[var(--g5-radius-md)] border px-3 py-2",
        role === "user"
          ? "border-[var(--g5-stroke)] bg-[var(--g5-bg2)]"
          : "border-[var(--g5-stroke)] bg-[var(--g5-bg1)]",
      )}
    >
      <div className="mb-1 flex items-center gap-1.5 text-[11px] font-semibold uppercase tracking-wide text-[var(--g5-text2)]">
        {role === "user" ? <User size={12} /> : <Bot size={12} />}
        {role === "user" ? "User" : "Assistant"}
      </div>
      <div className="space-y-2 text-sm text-[var(--g5-text0)]">{children}</div>
    </div>
  );
}

/** Render one Claude message content block (user or assistant side). */
function ClaudeContentBlock({ block }: { block: unknown }) {
  const text = asString(block);
  if (text !== null) {
    return <ClampedMarkdown text={text} />;
  }
  const obj = asObject(block);
  if (!obj) {
    return <ClampedPre text={prettyJson(block)} />;
  }
  switch (obj.type) {
    case "text":
      return <ClampedMarkdown text={asString(obj.text) ?? ""} />;
    case "thinking":
      return (
        <CollapsibleRow
          icon={<Brain size={13} />}
          label="Thinking"
          detail={(asString(obj.thinking) ?? "").slice(0, 120)}
        >
          <div className="italic text-[var(--g5-text1)]">
            <ClampedMarkdown text={asString(obj.thinking) ?? ""} />
          </div>
        </CollapsibleRow>
      );
    case "redacted_thinking":
      return (
        <CollapsibleRow icon={<Brain size={13} />} label="Redacted thinking">
          <p className="text-xs text-[var(--g5-text2)]">
            This thought was redacted at the source.
          </p>
        </CollapsibleRow>
      );
    case "tool_use":
      return (
        <CollapsibleRow
          icon={<Wrench size={13} />}
          label={`Tool: ${asString(obj.name) ?? "unknown"}`}
        >
          <ClampedPre text={prettyJson(obj.input)} />
        </CollapsibleRow>
      );
    case "tool_result": {
      const isError = obj.is_error === true;
      return (
        <CollapsibleRow
          icon={
            isError ? (
              <AlertTriangle size={13} className="text-[var(--g5-blocked)]" />
            ) : (
              <Wrench size={13} />
            )
          }
          label={isError ? "Tool result (error)" : "Tool result"}
        >
          <ClampedPre text={toolResultText(obj.content)} />
        </CollapsibleRow>
      );
    }
    case "image":
      return (
        <CollapsibleRow icon={<Info size={13} />} label="Image attachment">
          <ClampedPre text={prettyJson(obj)} />
        </CollapsibleRow>
      );
    default:
      return (
        <CollapsibleRow
          icon={<Braces size={13} />}
          label={asString(obj.type) ?? "content block"}
        >
          <ClampedPre text={prettyJson(obj)} />
        </CollapsibleRow>
      );
  }
}

/** Compact one-line summary for meta/session records rendered as chips. */
function metaSummary(json: JsonObject): string {
  const type = asString(json.type) ?? "record";
  switch (type) {
    case "mode":
      return `mode: ${asString(json.mode) ?? "?"}`;
    case "permission-mode":
      return `permission mode: ${asString(json.permissionMode) ?? "?"}`;
    case "file-history-snapshot":
      return "file history snapshot";
    case "attachment": {
      const attachment = asObject(json.attachment);
      const kind = attachment ? asString(attachment.type) : null;
      return kind ? `attachment: ${kind}` : "attachment";
    }
    case "summary": {
      const summary = asString(json.summary);
      return summary ? `summary: ${summary}` : "summary";
    }
    case "system":
      return "system";
    default:
      return type;
  }
}

function SidechainMarker() {
  return (
    <span className="inline-flex items-center gap-1 rounded-full border border-[var(--g5-stroke)] bg-[var(--g5-bg2)] px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wide text-[var(--g5-text2)]">
      sidechain
    </span>
  );
}

function MalformedLineRow({ record }: { record: JsonlLineRecord }) {
  return (
    <div className="rounded-[var(--g5-radius-sm)] border border-[var(--g5-blocked)] bg-[var(--g5-bg1)] px-2 py-1.5">
      <div className="mb-1 flex items-center gap-1.5 text-xs font-medium text-[var(--g5-blocked)]">
        <AlertTriangle size={13} />
        Line {record.lineNumber} is not valid JSON
        <span className="font-normal text-[var(--g5-text2)]">
          {record.parseError}
        </span>
      </div>
      <ClampedPre text={record.raw} clampChars={500} />
    </div>
  );
}

/** One transcript entry (Claude layout): message, meta chip, or error row. */
function TranscriptEntryRow({ record }: { record: JsonlLineRecord }) {
  if (record.parseError !== undefined) {
    return <MalformedLineRow record={record} />;
  }
  const json = asObject(record.json);
  if (!json) {
    return (
      <CollapsibleRow
        icon={<Braces size={13} />}
        label={`Line ${record.lineNumber}`}
      >
        <ClampedPre text={prettyJson(record.json)} />
      </CollapsibleRow>
    );
  }

  const message = asObject(json.message);
  const sidechain = json.isSidechain === true;
  if ((json.type === "user" || json.type === "assistant") && message) {
    const role =
      json.type === "user" ? ("user" as const) : ("assistant" as const);
    const content = message.content;
    const blocks = asArray(content) ?? [content];
    return (
      <div
        className={classNames(
          sidechain && "border-l-2 border-[var(--g5-info)] pl-2",
        )}
      >
        {sidechain && (
          <div className="mb-1">
            <SidechainMarker />
          </div>
        )}
        <MessageBubble role={role}>
          {blocks.map((block, index) => (
            <ClaudeContentBlock key={index} block={block} />
          ))}
        </MessageBubble>
      </div>
    );
  }

  return (
    <CollapsibleRow icon={<Info size={13} />} label={metaSummary(json)}>
      <ClampedPre text={prettyJson(json)} />
    </CollapsibleRow>
  );
}

// -----------------------------------------------------------------------------
// Generic JSONL fallback rendering
// -----------------------------------------------------------------------------

function genericSummary(record: JsonlLineRecord): string {
  const json = asObject(record.json);
  if (json) {
    const type = asString(json.type);
    if (type) return type;
    const keys = Object.keys(json);
    return keys.length > 0
      ? `{ ${keys.slice(0, 4).join(", ")}${keys.length > 4 ? ", …" : ""} }`
      : "{}";
  }
  if (Array.isArray(record.json)) return `array[${record.json.length}]`;
  return String(record.json);
}

function GenericRecordRow({ record }: { record: JsonlLineRecord }) {
  if (record.parseError !== undefined) {
    return <MalformedLineRow record={record} />;
  }
  return (
    <CollapsibleRow
      icon={<Braces size={13} />}
      label={`#${record.lineNumber}`}
      detail={genericSummary(record)}
    >
      <ClampedPre text={prettyJson(record.json)} />
    </CollapsibleRow>
  );
}

// -----------------------------------------------------------------------------
// Main view
// -----------------------------------------------------------------------------

export interface JsonlTranscriptViewProps {
  /** Repo-relative display path shown in the preview-shell header. */
  displayPath: string;
  /** Full JSONL document text (dirty-gated live workspace file snapshot per D42/D61). */
  content: string;
  /** Parse bound override (D61 resilience); defaults to MAX_PARSED_LINES. */
  maxParsedLines?: number;
}

/**
 * Read-only `.jsonl` project-doc rich preview (center-panel D61): a
 * Claude-transcript layout when the document matches the Claude Code session
 * format, a generic per-line record list otherwise. FileView owns Source/Preview
 * navigation and source editing.
 */
export function JsonlTranscriptView({
  displayPath,
  content,
  maxParsedLines = MAX_PARSED_LINES,
}: JsonlTranscriptViewProps) {
  const parsed = useMemo(
    () => parseJsonlDocument(content, maxParsedLines),
    [content, maxParsedLines],
  );
  const records = parsed.records;
  const claudeFormat = useMemo(() => isClaudeTranscript(records), [records]);
  const [visibleCount, setVisibleCount] = useState(RENDER_CHUNK);
  useEffect(() => {
    setVisibleCount(RENDER_CHUNK);
  }, [content]);

  const stats = useMemo(() => {
    let userMessages = 0;
    let assistantMessages = 0;
    let toolUses = 0;
    let malformed = 0;
    let sessionId: string | null = null;
    for (const record of records) {
      if (record.parseError !== undefined) {
        malformed += 1;
        continue;
      }
      const json = asObject(record.json);
      if (!json) continue;
      sessionId ??= asString(json.sessionId);
      const message = asObject(json.message);
      if (json.type === "user" && message) userMessages += 1;
      if (json.type === "assistant" && message) {
        assistantMessages += 1;
        for (const block of asArray(message.content) ?? []) {
          const obj = asObject(block);
          if (obj?.type === "tool_use") toolUses += 1;
        }
      }
    }
    return { userMessages, assistantMessages, toolUses, malformed, sessionId };
  }, [records]);

  const visibleRecords = records.slice(0, visibleCount);
  const hiddenCount = records.length - visibleRecords.length;

  return (
    <div
      className="flex h-full min-h-0 flex-col bg-[var(--g5-bg0)]"
      data-testid="jsonl-transcript-view"
    >
      <div className="flex shrink-0 items-center justify-between gap-3 border-b border-[var(--g5-stroke)] px-3 py-2">
        <div className="min-w-0 truncate text-xs text-[var(--g5-text2)]">
          {displayPath}
        </div>
      </div>
      <div className="flex shrink-0 flex-wrap items-center gap-x-3 gap-y-1 border-b border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-3 py-1.5 text-[11px] text-[var(--g5-text2)]">
        <span className="font-medium text-[var(--g5-text1)]">
          {claudeFormat ? "Claude Code session transcript" : "JSONL records"}
        </span>
        <span>{parsed.totalNonEmptyLines} lines</span>
        {parsed.truncated && <span>parsed first {records.length}</span>}
        {claudeFormat && (
          <>
            <span>{stats.userMessages} user</span>
            <span>{stats.assistantMessages} assistant</span>
            <span>{stats.toolUses} tool calls</span>
          </>
        )}
        {stats.malformed > 0 && (
          <span className="text-[var(--g5-blocked)]">
            {stats.malformed} malformed
          </span>
        )}
        {stats.sessionId && (
          <span className="truncate">session {stats.sessionId}</span>
        )}
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-3 py-3">
        {records.length === 0 ? (
          <p className="text-sm text-[var(--g5-text2)]">
            This document has no JSONL records yet. Switch to source mode to
            edit it.
          </p>
        ) : (
          <div className="flex min-w-0 w-full flex-col gap-2">
            {visibleRecords.map((record) =>
              claudeFormat ? (
                <TranscriptEntryRow key={record.lineNumber} record={record} />
              ) : (
                <GenericRecordRow key={record.lineNumber} record={record} />
              ),
            )}
            {hiddenCount > 0 && (
              <button
                type="button"
                data-testid="jsonl-transcript-show-more-entries"
                className="mt-1 rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-3 py-2 text-xs font-medium text-[var(--g5-text1)] hover:bg-[var(--g5-bg2)]"
                onClick={() => setVisibleCount((prev) => prev + RENDER_CHUNK)}
              >
                Show {Math.min(hiddenCount, RENDER_CHUNK)} more entries (
                {hiddenCount} hidden)
              </button>
            )}
            {parsed.truncated && (
              <p className="mt-1 rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-3 py-2 text-xs text-[var(--g5-text2)]">
                Parsing stopped after {records.length} records —{" "}
                {parsed.totalNonEmptyLines - records.length} more lines are not
                shown. Open source mode to view the full document.
              </p>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
