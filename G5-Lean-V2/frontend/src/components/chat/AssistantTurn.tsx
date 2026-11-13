import { OntologyChangesCard } from "./OntologyChangesCard";
import { messageOntologyCard } from "../../lib/ontologyCard";
import { ErrorBoundary } from "../shared/ErrorBoundary";
import React, {
  createContext,
  useContext,
  useEffect,
  useRef,
  useState,
  useSyncExternalStore,
} from "react";
import { createPortal } from "react-dom";
import {
  ChevronDown,
  ChevronRight,
  FileText,
  History,
  MessageSquare,
  Terminal,
  Wrench,
  XCircle,
} from "lucide-react";
import type {
  AgentApprovalResolutionInput,
  Artifact,
  ArtifactAction,
  ChatMessageDTO,
  Citation,
  TimelineEntry,
} from "../../lib/types";
import type { ChatPanelProps } from "./ChatPanel";
import type { TurnActivityCoverage } from "../../lib/historyCoverage";
import { ApiError, getApiToken } from "../../lib/api";
import { ASK_USER_QUESTION_TOOL_NAME } from "../../lib/agentApprovals";
import { isLiveTimelineRow } from "../../lib/liveTimeline";
import { sameEntries, sameRecord } from "./unifiedTimeline";
import type {
  AgentApprovalClassification,
  TimedOutQuestionSubState,
} from "../../lib/agentApprovals";
import {
  activityLabel,
  activitySummary,
  backgroundTaskKindLabel,
  formatElapsedWait,
} from "../../lib/chatActivity";
import {
  latestBackgroundTaskSet,
  nativeProviderRequestIdentity,
} from "../../lib/taskTimeline";
import type { BackgroundTaskSet } from "../../lib/taskTimeline";
import {
  compareTimelineEntries,
  isProvidedAgentOutputEntry,
  timelineEntryContentText,
  projectAssistantOutputEntries,
  isInternalAgentText,
} from "../../lib/providerOutput";
import { readChatViewState, writeChatViewState } from "../../lib/chatViewState";
import { parseMessageContent } from "../../lib/messageContent";
import { useWorkspaceNames } from "../../hooks/useWorkspaceNames";
import {
  liveGateReason,
  GatedControl,
  useWorkspaceLiveGate,
} from "../../lib/workspaceLiveGate";
import { classNames, useDialogFocus } from "../ui/UI";
import { MarkdownRenderer } from "../shared/MarkdownRenderer";
import { ArtifactRenderer } from "./ArtifactRenderer";
import { ChatErrorNotice } from "./ChatErrorNotice";
import { CitationChips, formatMessageTimestamp } from "./MessageMetadata";

const FILE_TOOLS = new Set([
  "read_file",
  "write_file",
  "edit_file",
  "Edit",
  "MultiEdit",
  "Write",
  "create_file",
  "delete_file",
  "list_dir",
  "search_files",
  "grep",
  "find_references",
  "list_specs",
  "read_spec",
]);
const SHELL_TOOLS = new Set([
  "run_in_terminal",
  "run_command",
  "shell",
  "Bash",
  "bash",
]);

/** Extract a short summary for a tool call based on the tool name and args */
function toolIcon(toolName: string) {
  if (FILE_TOOLS.has(toolName)) return FileText;
  if (SHELL_TOOLS.has(toolName)) return Terminal;
  if (toolName === "respond_to_user" || toolName === "wait_for_user_input")
    return MessageSquare;
  return Wrench;
}

/** Format a tool name for display: snake_case → Title Case */
export function formatToolName(name: string): string {
  return name.replace(/_/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());
}

function providerStatusLabel(entry: TimelineEntry): string | null {
  const providerEvent = entry.metadata?.providerEvent as string | undefined;
  switch (providerEvent) {
    case "provided_agent.error":
      return (
        (entry.metadata?.message as string | undefined) ||
        timelineEntryContentText(entry) ||
        "Error generating response"
      );
    case "provided_agent.warning": {
      const text =
        (entry.metadata?.message as string | undefined) ||
        timelineEntryContentText(entry) ||
        "Agent warning";
      if (text.includes("No pending Claude approval was found")) return null;
      return text;
    }
    case "provided_agent.info":
      return (
        (entry.metadata?.message as string | undefined) ||
        timelineEntryContentText(entry) ||
        null
      );
    case "provided_agent.context_usage.updated":
      return "Agent context usage";
    case "provided_agent.control.result":
      return "Agent control result";
    case "provided_agent.interrupted":
      return "Stopped";
    default:
      return null;
  }
}

export function shouldRenderProviderStatus(entry: TimelineEntry): boolean {
  const visibility = entry.metadata?.visibility as string | undefined;
  if (visibility === "internal" || visibility === "debug") return false;
  return providerStatusLabel(entry) != null;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function stringField(
  value: Record<string, unknown> | null | undefined,
  keys: string[],
): string | undefined {
  if (!value) return undefined;
  for (const key of keys) {
    const candidate = value[key];
    if (typeof candidate === "string" && candidate.trim().length > 0) {
      return candidate;
    }
  }
  return undefined;
}

function nestedRecord(
  value: unknown,
  key: string,
): Record<string, unknown> | null {
  return isRecord(value) && isRecord(value[key])
    ? (value[key] as Record<string, unknown>)
    : null;
}

function arrayField(value: unknown, key: string): unknown[] | undefined {
  return isRecord(value) && Array.isArray(value[key])
    ? (value[key] as unknown[])
    : undefined;
}

function prettyToolJson(value: unknown): string {
  if (value == null) return "";
  if (typeof value === "string") return value;
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

function previewText(value: string, max = 4000): string {
  return value.length > max ? `${value.slice(0, max)}\n…` : value;
}

function toolInputRecords(value: unknown): Record<string, unknown>[] {
  const records: Record<string, unknown>[] = [];
  if (isRecord(value)) {
    records.push(value);
    for (const key of ["toolInput", "input", "args", "params", "arguments"]) {
      if (isRecord(value[key]))
        records.push(value[key] as Record<string, unknown>);
    }
  }
  return records;
}

function firstToolStringField(
  value: unknown,
  keys: string[],
): string | undefined {
  for (const record of toolInputRecords(value)) {
    const candidate = stringField(record, keys);
    if (candidate !== undefined) return candidate;
  }
  return undefined;
}

function likelyEditLikeTool(toolName?: string): boolean {
  const normalized = toolName?.toLowerCase() ?? "";
  return (
    normalized === "edit" ||
    normalized === "write" ||
    normalized === "multiedit" ||
    normalized.includes("edit") ||
    normalized.includes("write")
  );
}

function likelyBashTool(toolName?: string): boolean {
  const normalized = toolName?.toLowerCase() ?? "";
  return (
    normalized === "bash" ||
    normalized.includes("bash") ||
    normalized.includes("shell") ||
    normalized.includes("terminal") ||
    normalized.includes("command")
  );
}

type ToolDetail = { label: string; value: string };

function formatToolEditDetails(
  value: unknown,
  toolName?: string,
): ToolDetail[] {
  const records = toolInputRecords(value);
  const details: ToolDetail[] = [];

  for (const record of records) {
    const oldString = stringField(record, [
      "old_string",
      "oldString",
      "old",
      "before",
      "original",
    ]);
    const newString = stringField(record, [
      "new_string",
      "newString",
      "new",
      "after",
      "replacement",
      "content",
    ]);
    if (oldString !== undefined || newString !== undefined) {
      if (oldString !== undefined) {
        details.push({ label: "Before", value: previewText(oldString) });
      }
      if (newString !== undefined) {
        details.push({ label: "After", value: previewText(newString) });
      }
      return details;
    }

    const edits = record.edits;
    if (Array.isArray(edits)) {
      edits.forEach((edit, index) => {
        if (!isRecord(edit)) return;
        const editOld = stringField(edit, [
          "old_string",
          "oldString",
          "old",
          "before",
        ]);
        const editNew = stringField(edit, [
          "new_string",
          "newString",
          "new",
          "after",
          "replacement",
        ]);
        const parts: string[] = [];
        if (editOld !== undefined)
          parts.push(`Before:\n${previewText(editOld)}`);
        if (editNew !== undefined)
          parts.push(`After:\n${previewText(editNew)}`);
        if (parts.length > 0) {
          details.push({
            label: `Edit ${index + 1}`,
            value: parts.join("\n\n"),
          });
        }
      });
      if (details.length > 0) return details;
    }
  }

  if (details.length === 0 && likelyEditLikeTool(toolName) && value != null) {
    details.push({ label: "Input", value: prettyToolJson(value) });
  }
  return details;
}

interface ToolDisplayDetails {
  description?: string;
  path?: string;
  paths?: string[];
  command?: string;
  details: ToolDetail[];
}

function toolDisplay(input: unknown, toolName?: string): ToolDisplayDetails {
  const records = toolInputRecords(input);
  const description =
    records
      .map((r) => stringField(r, ["description", "summary"]))
      .find(Boolean) ||
    toolName ||
    "Tool";
  const directPath = records
    .map((r) => stringField(r, ["file_path", "filePath", "path", "file"]))
    .find(Boolean);
  const changedPaths = records.flatMap((record) =>
    Array.isArray(record.changes)
      ? record.changes
          .filter(isRecord)
          .map((change) => stringField(change, ["path"]))
          .filter((path): path is string => Boolean(path))
      : [],
  );
  const path =
    directPath ?? (changedPaths.length === 1 ? changedPaths[0] : undefined);
  const command = records
    .map((r) => stringField(r, ["command", "cmd", "script"]))
    .find(Boolean);
  // Skill tool card is the "loaded skill <name>" affordance (issue #1491):
  // surface the skill name on the summary line like a file path.
  const skill = records.map((r) => stringField(r, ["skill"])).find(Boolean);

  const details = likelyEditLikeTool(toolName)
    ? formatToolEditDetails(input, toolName)
    : [];
  if (
    !command &&
    details.length === 0 &&
    input != null &&
    !likelyEditLikeTool(toolName) &&
    !likelyBashTool(toolName)
  ) {
    const json = prettyToolJson(input);
    if (json && json !== "{}") details.push({ label: "Input", value: json });
  }
  // An input the journal could only carry as a bounded rendering (a value
  // with too many string leaves to cut individually) arrives as a string;
  // show it for every tool kind rather than an empty body.
  if (!command && details.length === 0 && typeof input === "string") {
    if (input.trim()) details.push({ label: "Input", value: input });
  }

  return {
    description,
    path: path ?? skill,
    paths: directPath ? [directPath] : changedPaths,
    command,
    details,
  };
}

function trimmedApprovalLabel(label: string): string {
  return label
    .replace(
      /\s*\((this session|session|this run|local settings?|project settings?|user settings?)\)\s*$/i,
      "",
    )
    .replace(/\s+with session scope\.?$/i, "")
    .trim();
}

interface AgentApprovalOption {
  id: string;
  label: string;
  decision: "approved" | "rejected";
  interrupt?: boolean;
  reason?: string;
  updatedInput?: unknown;
  updatedPermissions?: unknown;
  description?: string;
}

function permissionSuggestionValues(
  input: unknown,
  options?: AgentApprovalOption[],
): unknown[] {
  const suggestions: unknown[] = [];
  const pushArray = (items: unknown[] | undefined) => {
    if (items) suggestions.push(...items);
  };

  if (isRecord(input)) {
    if (Array.isArray(input.rules)) suggestions.push(input);
    pushArray(arrayField(input, "suggestions"));
    pushArray(arrayField(input, "updatedPermissions"));
    pushArray(arrayField(input, "permissions"));
    const context = nestedRecord(input, "context");
    pushArray(arrayField(context, "suggestions"));
  }

  for (const option of options ?? []) {
    if (Array.isArray(option.updatedPermissions)) {
      suggestions.push(...option.updatedPermissions);
    } else if (isRecord(option.updatedPermissions)) {
      suggestions.push(option.updatedPermissions);
    }
  }

  return suggestions;
}

function permissionSuggestionRuleDetails(
  input: unknown,
  options?: AgentApprovalOption[],
): ToolDetail[] {
  const details: ToolDetail[] = [];
  const seen = new Set<string>();

  for (const suggestion of permissionSuggestionValues(input, options)) {
    if (!isRecord(suggestion)) continue;
    const rules = Array.isArray(suggestion.rules) ? suggestion.rules : [];
    for (const rule of rules) {
      let ruleContent: string | undefined;
      let toolName: string | undefined;

      if (typeof rule === "string") {
        ruleContent = rule;
      } else if (isRecord(rule)) {
        ruleContent = stringField(rule, ["ruleContent", "rule", "content"]);
        toolName = stringField(rule, ["toolName", "tool"]);
      }

      if (!ruleContent?.trim()) continue;
      const value = toolName ? `${toolName}: ${ruleContent}` : ruleContent;
      if (seen.has(value)) continue;
      seen.add(value);
      details.push({
        label:
          details.length === 0
            ? "Suggested Rule"
            : `Suggested Rule ${details.length + 1}`,
        value,
      });
    }
  }

  return details;
}

function shortApprovalLabel(value: string): string {
  const singleLine = value.replace(/\s+/g, " ").trim();
  return singleLine.length > 96 ? `${singleLine.slice(0, 93)}…` : singleLine;
}

function permissionDestinationLabel(
  suggestion: Record<string, unknown>,
): string | null {
  const destination = stringField(suggestion, ["destination", "scope"]);
  if (!destination) return null;
  return destination === "session" ? "this session" : destination;
}

/** Account field value naming the provider's Runtime Config variables. */

function permissionModeLabel(mode: string): string {
  switch (mode) {
    case "acceptEdits":
      return "Accept edits";
    case "bypassPermissions":
      return "Bypass permissions";
    case "dontAsk":
      return "Don’t ask again";
    case "plan":
      return "Plan mode";
    case "default":
      return "Default permissions";
    default:
      return mode;
  }
}

function permissionSuggestionDirectories(
  suggestion: Record<string, unknown>,
): string[] {
  const directories = suggestion.directories;
  if (Array.isArray(directories)) {
    return directories.filter(
      (item): item is string =>
        typeof item === "string" && item.trim().length > 0,
    );
  }
  const directory = stringField(suggestion, ["directory", "path", "root"]);
  return directory ? [directory] : [];
}

function structuredPermissionSuggestionLabel(
  suggestion: Record<string, unknown>,
): string | null {
  const type = stringField(suggestion, ["type", "kind"]);

  if (type === "setMode") {
    const mode = stringField(suggestion, ["mode", "permissionMode"]);
    return mode ? permissionModeLabel(mode) : null;
  }

  if (type === "addDirectories") {
    const directories = permissionSuggestionDirectories(suggestion);
    return directories.length > 1 ? "Allow dirs" : "Allow dir";
  }

  return null;
}

function structuredPermissionSuggestionDescription(
  suggestion: Record<string, unknown>,
): string | undefined {
  const type = stringField(suggestion, ["type", "kind"]);
  const destination = permissionDestinationLabel(suggestion);
  const suffix = destination ? ` (${destination})` : "";

  if (type === "setMode") {
    const mode = stringField(suggestion, ["mode", "permissionMode"]);
    return mode ? `${permissionModeLabel(mode)}${suffix}` : undefined;
  }

  if (type === "addDirectories") {
    const directories = permissionSuggestionDirectories(suggestion);
    if (directories.length === 0)
      return `Allow additional directories${suffix}`;
    return `${directories.length > 1 ? "Allow directories" : "Allow directory"}: ${directories.join(", ")}${suffix}`;
  }

  return undefined;
}

function concreteSuggestionLabel(
  suggestion: Record<string, unknown>,
  fallbackToolName?: string,
): string | null {
  const details = permissionSuggestionRuleDetails(suggestion);
  if (details.length > 0) {
    const value = details[0].value;
    if (/^[^:]{1,32}:\s*.+/.test(value)) {
      return `Allow ${shortApprovalLabel(value)}`;
    }
    const toolName =
      stringField(suggestion, ["toolName", "tool"]) ?? fallbackToolName;
    return toolName
      ? `Allow ${formatToolName(toolName)}: ${shortApprovalLabel(value)}`
      : `Allow rule: ${shortApprovalLabel(value)}`;
  }

  const structuredLabel = structuredPermissionSuggestionLabel(suggestion);
  if (structuredLabel) return structuredLabel;

  const explicit = stringField(suggestion, [
    "title",
    "label",
    "displayName",
    "description",
  ]);
  if (explicit) {
    const trimmed = trimmedApprovalLabel(explicit);
    if (
      /^(allow\s+)?suggest/i.test(trimmed) ||
      /^allow suggested rule$/i.test(trimmed)
    ) {
      return null;
    }
    return /^allow\b/i.test(trimmed)
      ? shortApprovalLabel(trimmed)
      : `Allow ${shortApprovalLabel(trimmed)}`;
  }

  return null;
}

function suggestionUpdatedPermissions(
  suggestion: Record<string, unknown>,
): unknown {
  if (suggestion.updatedPermissions != null)
    return suggestion.updatedPermissions;
  if (suggestion.permissions != null) return suggestion.permissions;
  if (Array.isArray(suggestion.rules)) return [suggestion];
  if (structuredPermissionSuggestionLabel(suggestion)) return [suggestion];
  return undefined;
}

function synthesizedPermissionRuleContent(
  toolName: string | undefined,
  input: unknown,
  context?: Record<string, unknown> | null,
): string {
  const tool = toolName?.trim() || "Tool";
  const command = firstToolStringField(input, ["command", "cmd", "script"]);
  const path =
    firstToolStringField(input, [
      "file_path",
      "filePath",
      "path",
      "file",
      "absolute_path",
      "absolutePath",
    ]) ?? stringField(context, ["blockedPath"]);
  const target = command ?? path;
  return target ? `${tool}(${target})` : tool;
}

function synthesizedApprovalSuggestionFromEntry(
  entry: TimelineEntry,
): Record<string, unknown> | null {
  const toolName = entry.metadata?.toolName as string | undefined;
  if (!toolName) return null;
  const input = entryToolInput(entry);
  const context = isRecord(entry.metadata?.context)
    ? entry.metadata.context
    : null;
  return {
    label: "Allow suggested rule",
    rules: [
      {
        toolName,
        ruleContent: synthesizedPermissionRuleContent(toolName, input, context),
      },
    ],
  };
}

function approvalSuggestionsFromEntry(
  entry: TimelineEntry,
): Record<string, unknown>[] {
  const suggestions: Record<string, unknown>[] = [];
  const push = (items: unknown) => {
    if (!Array.isArray(items)) return;
    for (const item of items) {
      if (isRecord(item)) suggestions.push(item);
    }
  };

  push(entry.metadata?.suggestions);
  const context = isRecord(entry.metadata?.context)
    ? entry.metadata.context
    : null;
  push(context?.suggestions);
  const input = isRecord(entry.metadata?.input) ? entry.metadata.input : null;
  push(input?.suggestions);
  const inputContext = input ? nestedRecord(input, "context") : null;
  push(inputContext?.suggestions);

  if (suggestions.length === 0) {
    const synthesized = synthesizedApprovalSuggestionFromEntry(entry);
    if (synthesized) suggestions.push(synthesized);
  }

  return suggestions;
}

function approvalOptionsFromEntry(entry: TimelineEntry): AgentApprovalOption[] {
  const suggestions = approvalSuggestionsFromEntry(entry);
  const options: AgentApprovalOption[] = [
    {
      id: "allow",
      label: "Allow",
      decision: "approved",
    },
  ];

  const seenSuggestionLabels = new Set<string>();
  suggestions.forEach((suggestion, index) => {
    const label = concreteSuggestionLabel(
      suggestion,
      entry.metadata?.toolName as string | undefined,
    );
    if (!label) return;
    const key = label.toLowerCase();
    if (seenSuggestionLabels.has(key)) return;
    seenSuggestionLabels.add(key);
    options.push({
      id: `suggestion-${index}`,
      label,
      decision: "approved",
      reason:
        typeof suggestion.message === "string"
          ? suggestion.message
          : "Accepted agent suggestion.",
      updatedInput: suggestion.updatedInput ?? suggestion.input,
      updatedPermissions: suggestionUpdatedPermissions(suggestion),
      description:
        typeof suggestion.description === "string"
          ? suggestion.description
          : structuredPermissionSuggestionDescription(suggestion),
    });
  });

  options.push(
    { id: "deny", label: "Deny", decision: "rejected" },
    {
      id: "deny-stop",
      label: "Deny & Stop",
      decision: "rejected",
      interrupt: true,
      reason: "Denied and stopped by user.",
    },
  );

  // The current native reply route supports allow/deny and updated input;
  // it cannot persist permission rules or combine denial with a Stop.
  return nativeProviderRequestIdentity(entry)
    ? options.filter(
        (option) =>
          !option.interrupt && option.updatedPermissions === undefined,
      )
    : options;
}

function sortedApprovalOptions(
  options: AgentApprovalOption[],
): AgentApprovalOption[] {
  return [...options].sort((a, b) => {
    const rank = (option: AgentApprovalOption) => {
      if (option.id === "allow") return 0;
      if (option.decision === "approved") return 1;
      if (option.interrupt) return 3;
      return 2;
    };
    return rank(a) - rank(b);
  });
}

function entryToolCallId(entry: TimelineEntry): string | undefined {
  return (
    (entry.metadata?.toolCallId as string | undefined) ??
    (entry.metadata?.toolUseId as string | undefined) ??
    (entry.metadata?.tool_use_id as string | undefined)
  );
}

function entryApprovalId(entry: TimelineEntry): string | undefined {
  return entry.metadata?.approvalId as string | undefined;
}

function entryToolName(entry: TimelineEntry): string {
  return (entry.metadata?.toolName as string | undefined) ?? "Tool";
}

function entryToolInput(entry: TimelineEntry): unknown {
  return (
    entry.metadata?.args ??
    entry.metadata?.input ??
    entry.metadata?.toolInput ??
    entry.metadata?.context ??
    null
  );
}

type AssistantToolStatus =
  | "pending"
  | "running"
  | "pending_approval"
  | "done"
  | "error"
  | "denied"
  | "stopped"
  | "ended";

function durableNativeQuestion(entry: TimelineEntry): boolean {
  return (
    entry.metadata?.requestKind === "question" &&
    typeof entry.metadata?.nativeResumeSessionId === "string"
  );
}

interface AgentApprovalBlock {
  approvalId: string;
  toolCallId?: string;
  toolName: string;
  input: unknown;
  entry: TimelineEntry;
  /// `timed_out` is the server's pending-question idle bound (issue #1217,
  /// `task_lifecycle_provider` D4F item 4): the question was never answered and
  /// its runtime was reclaimed. It is NOT a resolution — nobody answered and
  /// nothing was applied — so it must never render as answered/resolved.
  /// For AskUserQuestion frames the timeout splits into the three D25
  /// sub-states (`timed_out_answerable` / `late_answered` /
  /// `timed_out_superseded`); `timed_out` remains the fallback for
  /// non-question or unclassifiable timeouts.
  status:
    | "pending"
    | "reply_pending"
    | "resolved"
    | "stopped"
    | "failed"
    | "timed_out"
    /** An accepted answer whose request ended before delivery: the saved
     * answer is shown and may be sent as a new message
     * (`g5.chat.native-question-delivery-state`). */
    | "undelivered"
    /** A deferred question the saved session no longer offers; nobody
     * answered it (`g5.chat.native-question-recovery`). */
    | "lost"
    | TimedOutQuestionSubState;
  /** The accepted answer body (`g5AnswersByIndex`) when one was saved. */
  savedAnswer?: unknown;
  decision?: string;
  interrupt?: boolean;
  errorCode?: string;
  options: AgentApprovalOption[];
  /**
   * The request row reached the browser live, ahead of the history
   * transaction that validates and stores it (chat g5.chat.live-delivery):
   * shown at once, actionable only once its stored row replaces it. A
   * request the validation rejects arrives as an error row instead.
   */
  provisional: boolean;
}

type AssistantTurnBlock =
  | { type: "text"; id: string; text: string }
  | { type: "thought"; id: string; text: string }
  | { type: "artifact"; id: string; artifact: Artifact }
  | {
      type: "tool";
      id: string;
      toolCallId?: string;
      toolName: string;
      input: unknown;
      output?: string;
      inputTruncated?: boolean;
      outputTruncated?: boolean;
      durationMs?: number;
      isError?: boolean;
      status: AssistantToolStatus;
      approval?: AgentApprovalBlock;
      /** Persisted browser screenshot references (provided_agent_runtime
       *  D98) from the paired tool_result entry. */
      imageArtifacts?: ToolResultImageArtifact[];
    }
  | {
      type: "status";
      id: string;
      kind: "stopped" | "info" | "warning" | "error";
      text: string;
    };

export function buildAssistantTurnBlocks(
  message: ChatMessageDTO,
  entries: TimelineEntry[],
  timedOutQuestionSubStates?: ReadonlyMap<string, TimedOutQuestionSubState>,
  approvalClassification?: AgentApprovalClassification,
  activityCoverage: TurnActivityCoverage = "complete",
): AssistantTurnBlock[] {
  const sorted = projectAssistantOutputEntries(message, entries).sort(
    compareTimelineEntries,
  );
  const blocks: AssistantTurnBlock[] = [];
  const toolsByKey = new Map<
    string,
    Extract<AssistantTurnBlock, { type: "tool" }>
  >();
  const approvalToToolKey = new Map<string, string>();
  const authoritativeIdleTimeoutEntry = [...sorted]
    .reverse()
    .find(
      (entry) =>
        entry.metadata?.providerEvent === "provided_agent.interrupted" &&
        entry.metadata?.reason === "idle_timeout",
    );
  let sawOutput = false;
  const textRuns = new Map<string, number>();

  const ensureTool = (
    key: string,
    entry: TimelineEntry,
    status: AssistantToolStatus,
  ) => {
    let block = toolsByKey.get(key);
    if (!block) {
      block = {
        type: "tool",
        id: `tool-${key}`,
        toolCallId: entryToolCallId(entry),
        toolName: entryToolName(entry),
        input: entryToolInput(entry),
        inputTruncated: entry.metadata?.inputTruncated === true,
        status,
      };
      toolsByKey.set(key, block);
      blocks.push(block);
    } else {
      if (block.toolName === "Tool") block.toolName = entryToolName(entry);
      block.toolCallId = block.toolCallId ?? entryToolCallId(entry);
      block.input = block.input ?? entryToolInput(entry);
      if (block.status === "running" || status === "pending_approval") {
        block.status = status;
      }
    }
    return block;
  };

  const findTool = (entry: TimelineEntry) => {
    const approvalId = entryApprovalId(entry);
    const mapped = approvalId ? approvalToToolKey.get(approvalId) : undefined;
    const unkeyedTool =
      !entryToolCallId(entry) && !approvalId
        ? [...toolsByKey.values()].find(
            (tool) =>
              tool.toolName === entryToolName(entry) &&
              tool.status === "running",
          )
        : undefined;
    if (unkeyedTool) return unkeyedTool;
    const key =
      mapped ??
      entryToolCallId(entry) ??
      approvalId ??
      `${entryToolName(entry)}-${entry.sequence ?? entry.id}`;
    return ensureTool(key, entry, "running");
  };

  for (const entry of sorted) {
    const providerEvent = entry.metadata?.providerEvent as string | undefined;
    const phase =
      (entry.metadata?.phase as string | undefined) ?? entry.entryType;

    if (isProvidedAgentOutputEntry(entry)) {
      // The provider may emit a generic user-interrupt result before the
      // server's authoritative idle-timeout control finishes draining the
      // turn. Once the durable timeout entry exists, none of that provider
      // output is the user-facing terminal result (chat_panel_runtime D25).
      if (authoritativeIdleTimeoutEntry) {
        continue;
      }
      const text = timelineEntryContentText(entry);
      if (text.trim() && !isInternalAgentText(text)) {
        sawOutput = true;
        const key = String(entry.metadata?.blockId ?? entry.id);
        const run = textRuns.get(key) ?? 0;
        textRuns.set(key, run + 1);
        blocks.push({ type: "text", id: `text-${key}-${run}`, text });
      }
      continue;
    }

    // Claude thinking summaries (chat_panel_runtime D26, worker
    // provided_agent_runtime D65): reasoning content, shown in-turn ahead of
    // the answer text and never merged into it.
    if (providerEvent === "provided_agent.thought" || phase === "thought") {
      const text = timelineEntryContentText(entry);
      if (text.trim() && !isInternalAgentText(text)) {
        blocks.push({ type: "thought", id: `thought-${entry.id}`, text });
      }
      continue;
    }

    if (
      providerEvent === "provided_agent.tool.started" ||
      phase === "tool_call"
    ) {
      if (
        entryToolName(entry) === "respond_to_user" ||
        entryToolName(entry) === "wait_for_user_input"
      ) {
        continue;
      }
      const key =
        entryToolCallId(entry) ??
        entryApprovalId(entry) ??
        `${entryToolName(entry)}-${entry.sequence ?? entry.id}`;
      ensureTool(key, entry, "running");
      continue;
    }

    if (providerEvent === "provided_agent.approval.requested") {
      const approvalId = entryApprovalId(entry);
      if (!approvalId) continue;
      const key =
        entryToolCallId(entry) ??
        approvalId ??
        `${entryToolName(entry)}-${entry.sequence ?? entry.id}`;
      approvalToToolKey.set(approvalId, key);
      const tool = ensureTool(key, entry, "pending_approval");
      tool.approval = {
        approvalId,
        toolCallId: entryToolCallId(entry),
        toolName: entryToolName(entry),
        input: entryToolInput(entry),
        entry,
        provisional: isLiveTimelineRow(entry),
        status:
          entry.metadata?.nativeRequestStatus === "responded"
            ? "resolved"
            : entry.metadata?.nativeRequestStatus === "invalidated"
              ? // An accepted answer the request ended without delivering
                // stays shown with the request, never as a plain stop
                // (`g5.chat.native-question-delivery-state`).
                entryToolName(entry) === ASK_USER_QUESTION_TOOL &&
                entry.metadata?.nativeRequestAnswer != null
                ? "undelivered"
                : entryToolName(entry) === ASK_USER_QUESTION_TOOL &&
                    isLostQuestionReason(entry.metadata?.nativeRequestEndReason)
                  ? "lost"
                  : "stopped"
              : entry.metadata?.nativeRequestStatus === "reply_pending"
                ? "reply_pending"
                : "pending",
        savedAnswer: entry.metadata?.nativeRequestAnswer ?? undefined,
        errorCode:
          typeof entry.metadata?.nativeRequestEndReason === "string"
            ? entry.metadata.nativeRequestEndReason
            : undefined,
        options: approvalOptionsFromEntry(entry),
      };
      tool.status =
        tool.approval.status === "resolved"
          ? "done"
          : tool.approval.status === "stopped" ||
              tool.approval.status === "undelivered" ||
              tool.approval.status === "lost"
            ? "stopped"
            : "pending_approval";
      continue;
    }

    if (providerEvent === "provided_agent.approval.resolved") {
      const approvalId = entryApprovalId(entry);
      const key =
        (approvalId && approvalToToolKey.get(approvalId)) ??
        entryToolCallId(entry) ??
        approvalId;
      const tool = key ? toolsByKey.get(key) : findTool(entry);
      const decision = entry.metadata?.decision as string | undefined;
      const interrupt = entry.metadata?.interrupt as boolean | undefined;
      // The server's pending-question idle bound reclaims an unanswered
      // question's runtime and writes a synthetic settlement carrying
      // `resolutionSource: "idle_timeout"` (issue #1217). Without reading it,
      // this entry looks like an ordinary settlement: `interrupt` is unset and
      // `decision` is not "rejected", so the card would fall through to
      // "Answered" and the tool would flip to "running" — telling the user they
      // answered a question they never saw resolved, and that the agent is
      // proceeding on it. It is neither answered nor applied.
      const timedOut =
        (entry.metadata?.resolutionSource as string | undefined) ===
        "idle_timeout";
      // Late-answerable split (chat_panel_runtime D25): an idle_timeout
      // settlement of a renderable AskUserQuestion yields one of three derived
      // sub-states judged from durable timeline evidence by the caller
      // (`classifyTimedOutQuestions`). A question whose input cannot render as
      // a form falls back to the plain timed-out collapse — the same fallback
      // the pending path uses — because an unanswerable form must not claim to
      // be actionable.
      let timedOutStatus: AgentApprovalBlock["status"] = "timed_out";
      if (timedOut && approvalId) {
        const subState = timedOutQuestionSubStates?.get(approvalId);
        if (subState === "timed_out_answerable") {
          // Tag AND parse (chat_panel_runtime D25): the tag is the
          // authoritative tool identity; the parse only guards against an
          // unanswerable form. Never key answerability on input shape alone.
          const renderableQuestion =
            tool?.approval?.toolName === ASK_USER_QUESTION_TOOL &&
            parseAskUserQuestions(tool.approval.input) !== null;
          if (renderableQuestion) timedOutStatus = subState;
        } else if (subState) {
          timedOutStatus = subState;
        }
      }
      if (tool?.approval) {
        tool.approval.status = timedOut
          ? timedOutStatus
          : interrupt
            ? "stopped"
            : "resolved";
        tool.approval.decision = decision;
        tool.approval.interrupt = interrupt;
      }
      if (tool) {
        // D25: no timed-out sub-state ever advances the tool to `running`.
        // The answerable card stays visually identical to pending, so its
        // tool keeps `pending_approval`; every other timeout collapses the
        // tool to stopped.
        if (timedOut) {
          tool.status =
            timedOutStatus === "timed_out_answerable"
              ? "pending_approval"
              : "stopped";
        } else if (interrupt) tool.status = "stopped";
        else if (decision === "rejected") tool.status = "denied";
        else if (tool.status === "pending_approval") tool.status = "running";
      }
      continue;
    }

    if (providerEvent === "provided_agent.approval.failed") {
      const approvalId = entryApprovalId(entry);
      const key =
        (approvalId && approvalToToolKey.get(approvalId)) ??
        entryToolCallId(entry) ??
        approvalId;
      const tool = key ? toolsByKey.get(key) : findTool(entry);
      const reason = entry.metadata?.errorCode as string | undefined;
      if (tool?.approval) {
        // The adapter withdrew a deferred question its saved session no
        // longer offers; a saved answer keeps the undelivered card
        // (`g5.chat.native-question-recovery`).
        if (tool.approval.status !== "undelivered") {
          tool.approval.status =
            questionApproval(tool.approval) && isLostQuestionReason(reason)
              ? "lost"
              : "failed";
        }
        tool.approval.errorCode = reason;
      }
      if (tool) {
        tool.status =
          tool.approval?.status === "lost" ||
          tool.approval?.status === "undelivered"
            ? "stopped"
            : "error";
      }
      continue;
    }

    if (
      providerEvent === "provided_agent.tool.completed" ||
      phase === "tool_result"
    ) {
      if (
        entryToolName(entry) === "respond_to_user" ||
        isInternalChatDeliveryAck(entry)
      ) {
        continue;
      }
      const tool = findTool(entry);
      // A task-journal row's content is its projected output when the event
      // carried one (lib/taskTimeline.ts sets contentSnippet); a digest-only
      // row has no output to show, and its content is the literal event kind.
      const journaled = entry.metadata?.schema_version === "g5.task-event.v1";
      const output =
        (entry.metadata?.contentSnippet as string | undefined) ??
        (journaled ? undefined : timelineEntryContentText(entry));
      tool.output =
        output !== undefined && isInternalAgentText(output)
          ? undefined
          : output;
      tool.isError = entry.metadata?.isError === true;
      tool.outputTruncated = entry.metadata?.outputTruncated === true;
      tool.durationMs =
        typeof entry.metadata?.durationMs === "number"
          ? entry.metadata.durationMs
          : undefined;
      tool.status = tool.isError ? "error" : "done";
      const artifacts = toolResultImageArtifacts(entry);
      if (artifacts.length > 0) {
        tool.imageArtifacts = artifacts;
      }
      continue;
    }

    if (providerEvent === "provided_agent.interrupted") {
      const isIdleTimeout = entry.metadata?.reason === "idle_timeout";
      if (isIdleTimeout && entry.id !== authoritativeIdleTimeoutEntry?.id) {
        continue;
      }
      for (const block of toolsByKey.values()) {
        // D25 timed-out-answerable questions already carry their settlement
        // evidence: the idle-timeout interrupt that follows the settlement
        // must not collapse the still-actionable form to stopped history.
        if (
          block.status === "pending_approval" &&
          block.approval &&
          block.approval.status !== "timed_out_answerable" &&
          !durableNativeQuestion(block.approval.entry)
        ) {
          block.status = "stopped";
          block.approval.status = "stopped";
          block.approval.interrupt = true;
        }
      }
      if (isIdleTimeout) {
        const timeoutGuidance = timelineEntryContentText(entry).trim();
        sawOutput = true;
        if (timeoutGuidance) {
          blocks.push({
            type: "text",
            id: `text-${entry.id}`,
            text: timeoutGuidance,
          });
        }
      }
      blocks.push({
        type: "status",
        id: `status-${entry.id}`,
        kind: "stopped",
        text: "Stopped",
      });
      continue;
    }

    // Claude's background set and sub-task steps feed only the waiting panel
    // and the activity label; they never become transcript blocks.
    if (shouldRenderProviderStatus(entry)) {
      if (
        entry.metadata?.outcomeFallback &&
        (message.isErrorNotice || message.errorMessage?.trim())
      )
        continue;
      const text =
        providerStatusLabel(entry) ?? timelineEntryContentText(entry);
      if (
        entry.metadata?.providerEvent === "provided_agent.error" &&
        blocks.some(
          (block) =>
            block.type === "status" &&
            block.kind === "error" &&
            block.text.trim() === text.trim(),
        )
      )
        continue;
      blocks.push({
        type: "status",
        id: `status-${entry.id}`,
        kind:
          entry.metadata?.providerEvent === "provided_agent.error"
            ? "error"
            : entry.metadata?.providerEvent === "provided_agent.info"
              ? "info"
              : "warning",
        text,
      });
    }
  }

  // A saved aggregate may outlive the loaded event window. If every loaded
  // prose block occurs in it in order, recover its missing prose without
  // replacing the known text/tool positions or replaying the whole aggregate.
  // Text ahead of the first loaded block of an incomplete turn belongs to its
  // unloaded activity, so it waits for that activity (g5.chat.history-window).
  if (
    sawOutput &&
    !authoritativeIdleTimeoutEntry &&
    !message.isErrorNotice &&
    message.status !== "streaming" &&
    message.status !== "pending"
  ) {
    const prose = blocks.filter((block) => block.type === "text");
    let offset = 0;
    const gaps: { before: string | null; text: string }[] = [];
    const aligns =
      prose.length > 0 &&
      prose.every((block) => {
        const position = message.content.indexOf(block.text, offset);
        if (position < 0) return false;
        const missing = message.content.slice(offset, position).trim();
        if (missing) gaps.push({ before: block.id, text: missing });
        offset = position + block.text.length;
        return true;
      });
    if (aligns) {
      const tail = message.content.slice(offset).trim();
      if (tail) gaps.push({ before: null, text: tail });
      gaps.forEach((gap, index) => {
        if (activityCoverage !== "complete" && gap.before === prose[0].id)
          return;
        const position =
          gap.before === prose[0].id
            ? 0
            : gap.before === null
              ? blocks.findIndex((block) => block.type === "status")
              : blocks.findIndex((block) => block.id === gap.before);
        blocks.splice(position < 0 ? blocks.length : position, 0, {
          type: "text",
          id: `saved-${message.id}-${index}`,
          text: gap.text,
        });
      });
    }
  }

  // A persisted diagnostic and its owned timeline status are two
  // representations of one notice, not assistant prose plus an error.
  const contentIsErrorNotice = blocks.some(
    (block) =>
      block.type === "status" &&
      block.kind === "error" &&
      block.text.trim() === message.content.trim(),
  );
  if (
    (message.isErrorNotice ||
      (!sawOutput && activityCoverage === "complete")) &&
    message.content.trim() &&
    !contentIsErrorNotice
  ) {
    blocks.push({
      ...(message.status === "error" && message.isErrorNotice
        ? { type: "status" as const, kind: "error" as const }
        : { type: "text" as const }),
      id: `message-${message.id}`,
      text: message.content,
    });
  }

  if (
    message.status === "error" &&
    !blocks.some((block) => block.type === "status" && block.kind === "error")
  ) {
    blocks.push({
      type: "status",
      id: `error-${message.id}`,
      kind: "error",
      text: message.errorMessage?.trim() || "Error generating response",
    });
  }

  // Deferred questions end an attempt, not the conversation. Older and
  // current attempts are persisted as stopped; the request's authoritative
  // state keeps that bookkeeping status out of pending and answered history.
  // Explicit interrupt blocks above and invalidated requests still show Stop.
  const hasDeferredQuestion = [...toolsByKey.values()].some(
    ({ approval }) =>
      approval &&
      durableNativeQuestion(approval.entry) &&
      !approval.interrupt &&
      approval.decision !== "rejected" &&
      (approval.status === "pending" ||
        approval.status === "reply_pending" ||
        approval.status === "resolved"),
  );
  if (
    message.status === "stopped" &&
    !hasDeferredQuestion &&
    !blocks.some((b) => b.type === "status" && b.kind === "stopped")
  ) {
    blocks.push({
      type: "status",
      id: `stopped-${message.id}`,
      kind: "stopped",
      text: "Stopped",
    });
  }

  const dtoCalls = (message as unknown as { toolCalls?: unknown }).toolCalls;
  if (Array.isArray(dtoCalls)) {
    const matched = new Set<string>();
    dtoCalls.filter(isRecord).forEach((call, index) => {
      const name = typeof call.name === "string" ? call.name : "Tool";
      const callId = stringField(call, ["id", "toolCallId"]);
      const existing = [...toolsByKey.values()].find(
        (tool) =>
          !matched.has(tool.id) &&
          (callId ? tool.toolCallId === callId : tool.toolName === name),
      );
      if (existing) {
        if (!callId) existing.id = `tool-dto-${message.id}-${index}`;
        matched.add(existing.id);
        return;
      }
      let input = call.arguments ?? null;
      if (typeof input === "string") {
        try {
          input = JSON.parse(input);
        } catch {
          /* Legacy plain-text arguments. */
        }
      }
      const key = callId ?? `dto-${message.id}-${index}`;
      const tool: Extract<AssistantTurnBlock, { type: "tool" }> = {
        type: "tool",
        id: `tool-${key}`,
        toolCallId: callId,
        toolName: name,
        input,
        output: call.result == null ? undefined : String(call.result),
        durationMs:
          typeof call.duration === "number" ? call.duration : undefined,
        status:
          call.status === "error"
            ? "error"
            : call.status === "pending"
              ? "pending"
              : call.status === "running"
                ? "running"
                : "done",
      };
      toolsByKey.set(key, tool);
      blocks.push(tool);
    });
  }

  // #603: once the assistant turn is terminal, a tool chip still left
  // "running" has no completion entry in the loaded timeline window (e.g. a
  // worker that exited mid-tool). Reconcile it to a neutral terminal "ended"
  // state so a completed turn never renders a perpetually running tool. The
  // newest-first persisted window already delivers real completions for the
  // active turn, so this only affects genuinely orphaned tools. In-flight
  // turns (pending/streaming) are untouched.
  if (
    message.status === "complete" ||
    message.status === "error" ||
    message.status === "stopped"
  ) {
    for (const block of blocks) {
      if (block.type === "tool" && block.status === "running") {
        block.status = "ended";
      }
      // Stale-question unblocking (chat_panel_runtime D25): a terminal turn
      // without a resolution has no one left to receive an answer — collapse
      // the pending form to the stopped summary, mirroring the
      // interrupted-entry path above. The timed-out-answerable card is the
      // explicit D25 exception: it IS settled (idle_timeout) and the server
      // admits a late answer as a new turn, so it stays actionable.
      if (
        block.type === "tool" &&
        block.status === "pending_approval" &&
        block.approval?.status !== "timed_out_answerable" &&
        !(block.approval && durableNativeQuestion(block.approval.entry))
      ) {
        block.status = "stopped";
        if (block.approval) {
          block.approval.status = "stopped";
          block.approval.interrupt = true;
        }
      }
    }
  }

  if (approvalClassification) {
    for (const tool of toolsByKey.values()) {
      const approval = tool.approval;
      if (
        !approval ||
        durableNativeQuestion(approval.entry) ||
        timedOutQuestionSubStates?.has(approval.approvalId) ||
        approval.status === "reply_pending" ||
        approval.status === "resolved"
      )
        continue;
      if (approvalClassification.failed.has(approval.approvalId)) {
        approval.status = "failed";
        tool.status = "error";
      } else if (approvalClassification.dead.has(approval.approvalId)) {
        approval.status = "stopped";
        tool.status = "stopped";
      } else if (approvalClassification.pending.includes(approval.approvalId)) {
        approval.status = "pending";
        approval.interrupt = false;
        tool.status = "pending_approval";
      }
    }
  }

  // Inline artifacts stay at their prose position. DTO artifacts are appended
  // once, even when the same artifact occurs in a loaded output block.
  const projected: AssistantTurnBlock[] = [];
  const artifactContents = new Set<Artifact["content"]>();
  for (const block of blocks) {
    if (block.type !== "text") {
      projected.push(block);
      continue;
    }
    parseMessageContent(block.text, []).forEach((part, index) => {
      if (part.type === "text")
        projected.push({
          type: "text",
          id: index === 0 ? block.id : `${block.id}-${index}`,
          text: part.content,
        });
      else {
        artifactContents.add(part.artifact.content);
        projected.push({
          type: "artifact",
          id: `${block.id}-artifact-${index}`,
          artifact: part.artifact,
        });
      }
    });
  }
  const artifacts = Array.isArray(message.artifacts) ? message.artifacts : [];
  for (const [index, artifact] of artifacts.entries()) {
    if (artifactContents.has(artifact.content)) continue;
    artifactContents.add(artifact.content);
    projected.push({
      type: "artifact",
      id: `artifact-${message.id}-${index}`,
      artifact,
    });
  }
  // A turn with no loaded activity shows its marker and terminal state only;
  // its saved reply appears with its activity (g5.chat.history-window).
  if (activityCoverage === "unloaded")
    return projected.filter((block) => block.type === "status");
  return projected;
}

// ---------------------------------------------------------------------------
// AskUserQuestion picker (chat_panel_runtime D25, worker D4F, issue #1057)
// ---------------------------------------------------------------------------

// Single source of truth lives in lib/agentApprovals (shared with the
// timed-out sub-state classifier); aliased for the existing call sites. The
// tag is the authoritative tool IDENTITY (D25): every question-detection site
// gates on it — input shape alone must never turn a tool approval into a
// question card, and the server's late-answer admission is tag-gated too.
const ASK_USER_QUESTION_TOOL = ASK_USER_QUESTION_TOOL_NAME;
// Ordinary question forms may be dismissed in every conversation. Native
// requests and late-answer forms retain their own protocol-specific controls.
const QUESTION_DISMISS_REASON = "User declined to answer the questions.";

// Question delivery state (`g5.chat.native-question-delivery-state`): an
// accepted answer the saved session could not take stays visible with its
// question, and the owner may send both as an ordinary new message. The card
// shows the ordinary stopped label and explains no cause.
export const RESEND_QUESTION_ANSWER_LABEL = "Send as a new message";

/**
 * Puts text into the chat composer for the owner to send. Provided by the
 * panel that owns the composer; absent in read-only views.
 */
export const QuestionAnswerResendContext = createContext<
  ((text: string) => void) | null
>(null);

/** Whether an end reason means a deferred question was withdrawn or could
 * not be restored (`g5.chat.native-question-recovery`); the card does not
 * explain which. */
function isLostQuestionReason(reason: unknown): boolean {
  return (
    reason === "native_session_advanced" ||
    reason === "session_not_saved" ||
    reason === "native_question_restore_failed"
  );
}

interface SavedAnswerEntry {
  selectedOptionIndexes?: unknown;
  otherText?: unknown;
}

/** The saved answer to each question as readable text, in question order. */
export function savedAnswerLines(
  questions: ParsedQuestion[],
  savedAnswer: unknown,
): { question: string; answer: string }[] {
  const byIndex =
    typeof savedAnswer === "object" && savedAnswer !== null
      ? ((savedAnswer as { g5AnswersByIndex?: unknown }).g5AnswersByIndex as
          | Record<string, SavedAnswerEntry>
          | undefined)
      : undefined;
  return questions.map((question, index) => {
    const entry = byIndex?.[String(index)];
    const selected = Array.isArray(entry?.selectedOptionIndexes)
      ? entry.selectedOptionIndexes
          .map((optionIndex) =>
            typeof optionIndex === "number"
              ? question.options[optionIndex]?.label
              : undefined,
          )
          .filter((label): label is string => typeof label === "string")
      : [];
    const other =
      typeof entry?.otherText === "string" && entry.otherText.trim()
        ? [entry.otherText.trim()]
        : [];
    const parts = [...selected, ...other];
    return {
      question: question.question,
      answer: parts.length ? parts.join(", ") : "(no selection)",
    };
  });
}

/** The ordinary message that carries a saved answer to the agent. */
export function formatSavedQuestionAnswer(
  questions: ParsedQuestion[],
  savedAnswer: unknown,
): string {
  const lines = savedAnswerLines(questions, savedAnswer).flatMap(
    ({ question, answer }) => [`Q: ${question}`, `A: ${answer}`],
  );
  return ["Answering your earlier question:", ...lines].join("\n");
}

interface ParsedQuestionOption {
  label: string;
  description?: string;
}

interface ParsedQuestion {
  question: string;
  header?: string;
  multiSelect: boolean;
  options: ParsedQuestionOption[];
}

// Parse the (worker-redacted) AskUserQuestion input for rendering. Returns
// null when the shape is not renderable — the caller falls back to the
// generic approval UI, whose plain Allow the sidecar converts to a deny.
function parseAskUserQuestions(input: unknown): ParsedQuestion[] | null {
  if (typeof input !== "object" || input === null) return null;
  const questions = (input as { questions?: unknown }).questions;
  if (!Array.isArray(questions) || questions.length === 0) return null;
  const parsed: ParsedQuestion[] = [];
  for (const raw of questions) {
    if (typeof raw !== "object" || raw === null) return null;
    const question = raw as {
      question?: unknown;
      header?: unknown;
      multiSelect?: unknown;
      options?: unknown;
    };
    if (typeof question.question !== "string" || !question.question.trim())
      return null;
    if (!Array.isArray(question.options) || question.options.length === 0)
      return null;
    const options: ParsedQuestionOption[] = [];
    for (const rawOption of question.options) {
      if (typeof rawOption !== "object" || rawOption === null) return null;
      const option = rawOption as { label?: unknown; description?: unknown };
      if (typeof option.label !== "string" || !option.label.trim()) return null;
      options.push({
        label: option.label,
        description:
          typeof option.description === "string"
            ? option.description
            : undefined,
      });
    }
    parsed.push({
      question: question.question,
      header: typeof question.header === "string" ? question.header : undefined,
      multiSelect: question.multiSelect === true,
      options,
    });
  }
  return parsed;
}

function requestStoreKey(
  sessionId: string | null,
  approvalId: string,
  nativeRequest?: ReturnType<typeof nativeProviderRequestIdentity>,
): string {
  return nativeRequest
    ? JSON.stringify([
        sessionId,
        approvalId,
        nativeRequest.taskId,
        nativeRequest.attempt,
        nativeRequest.providerRequestId,
        nativeRequest.requestKind,
      ])
    : `${sessionId ?? ""}:${approvalId}`;
}

interface QuestionDraftEntry {
  selected: number[];
  otherText: string;
}

type QuestionDraft = Record<number, QuestionDraftEntry>;

// Draft answers keyed by session+approval id at module scope so a hydration
// remount of either approval render path (message block vs trajectory row)
// cannot drop the user's selections (chat_panel_runtime D25). Bounded: a
// question resolved from another surface (or never scrolled back to) would
// otherwise retain its draft for the page lifetime, so on insert past the cap
// the oldest draft is evicted (Map preserves insertion order).
const QUESTION_DRAFT_CAP = 50;
const questionDrafts = new Map<string, QuestionDraft>();

function storeQuestionDraft(storeKey: string, draft: QuestionDraft) {
  // Re-inserting moves the key to the newest position so active forms are
  // never the eviction victim.
  questionDrafts.delete(storeKey);
  questionDrafts.set(storeKey, draft);
  writeChatViewState("questions", storeKey, draft);
  if (questionDrafts.size > QUESTION_DRAFT_CAP) {
    // Never evict an in-flight draft: clearing its guard would re-enable the
    // controls mid-request and allow a duplicate submission. If every entry
    // is somehow in flight, defer eviction until one settles.
    for (const key of questionDrafts.keys()) {
      if (requestDeliveriesInFlight.has(key)) continue;
      questionDrafts.delete(key);
      writeChatViewState("questions", key, undefined);
      break;
    }
  }
}

// In-flight resolve guards, module-scoped for the same reason: a remount of
// either render path must not re-enable submission while the first request is
// still outstanding. Components subscribe via useSyncExternalStore.
const requestDeliveriesInFlight = new Set<string>();
const requestDeliverySubscribers = new Map<string, Set<() => void>>();

function subscribeRequestDelivery(storeKey: string, listener: () => void) {
  let listeners = requestDeliverySubscribers.get(storeKey);
  if (!listeners) {
    listeners = new Set();
    requestDeliverySubscribers.set(storeKey, listeners);
  }
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
    if (listeners.size === 0) requestDeliverySubscribers.delete(storeKey);
  };
}

function setRequestDeliveryInFlight(storeKey: string, inFlight: boolean) {
  if (inFlight) requestDeliveriesInFlight.add(storeKey);
  else requestDeliveriesInFlight.delete(storeKey);
  requestDeliverySubscribers.get(storeKey)?.forEach((listener) => listener());
}

// Drop a question's module-scoped draft/in-flight state once its approval is
// no longer pending (resolved from any surface, interrupted, or terminal) so
// abandoned questions cannot retain free text indefinitely.
function clearQuestionFormState(storeKey: string) {
  questionDrafts.delete(storeKey);
  writeChatViewState("questions", storeKey, undefined);
  setRequestDeliveryInFlight(storeKey, false);
}

// Rendered by the resolved/stale question branches (an effect, not a render
// side effect, so StrictMode double-renders stay pure).
function QuestionFormStateCleanup({ storeKey }: { storeKey: string }) {
  useEffect(() => {
    clearQuestionFormState(storeKey);
  }, [storeKey]);
  return null;
}

function questionDraftEntry(
  draft: QuestionDraft,
  index: number,
): QuestionDraftEntry {
  return draft[index] ?? { selected: [], otherText: "" };
}

function questionAnswered(
  question: ParsedQuestion,
  entry: QuestionDraftEntry,
): boolean {
  const hasOther = entry.otherText.trim().length > 0;
  if (question.multiSelect) return entry.selected.length > 0 || hasOther;
  return hasOther ? entry.selected.length === 0 : entry.selected.length === 1;
}

// Encode the draft as the shared by-index wire protocol: answers travel
// by question/option INDEX because persisted question text is redacted and
// the live adapter reconstructs original-text answers from its retained input.
function encodeQuestionAnswers(
  questions: ParsedQuestion[],
  draft: QuestionDraft,
): Record<string, { selectedOptionIndexes: number[]; otherText?: string }> {
  const byIndex: Record<
    string,
    { selectedOptionIndexes: number[]; otherText?: string }
  > = {};
  questions.forEach((question, index) => {
    const entry = questionDraftEntry(draft, index);
    const otherText = entry.otherText.trim();
    byIndex[String(index)] = {
      selectedOptionIndexes: [...entry.selected].sort((a, b) => a - b),
      ...(otherText ? { otherText } : {}),
    };
  });
  return byIndex;
}

// D25 timed-out-answerable copy: exactly one hint line distinguishes the
// still-answerable timed-out form from a live pending form.
const TIMED_OUT_ANSWERABLE_HINT =
  "Agent is paused — answering will restart it.";

// A provisional request (chat g5.chat.live-delivery): the row is shown live
// while its history transaction validates and stores it; its controls open
// once the stored row arrives, and a rejected request shows its error instead.
const CONFIRMING_LABEL = "Confirming…";
const CONFIRMING_TITLE =
  "The request is being saved. It can be answered once it is confirmed.";

function ConfirmingNotice({ className }: { className?: string }) {
  return (
    <p
      role="status"
      title={CONFIRMING_TITLE}
      data-testid="agent-request-confirming"
      className={classNames("text-[11px] text-[var(--g5-text2)]", className)}
    >
      {CONFIRMING_LABEL}
    </p>
  );
}

// Truthful inline notices for the two late-answer 409 conflicts
// (routes/chat D25). Gated on the structured ApiError envelope first: the
// wire code for both variants is the shared `CONFLICT` (errors catalog), so
// the machine-readable subtype travels as the server's code-prefixed reason —
// the prefix match runs only on a typed CONFLICT ApiError, never on arbitrary
// error text. The parent refetches the timeline on these conflicts so the
// card collapses to its durable state.
function lateAnswerConflictNotice(error: unknown): string | null {
  if (!(error instanceof ApiError) || !error.is("CONFLICT")) {
    return null;
  }
  if (error.message.startsWith("LATE_ANSWER_SUPERSEDED")) {
    return "A newer message or question superseded this answer — the question is closed.";
  }
  if (error.message.startsWith("LATE_ALREADY_ANSWERED")) {
    return "This question was already answered from another tab or surface.";
  }
  return null;
}

export interface QuestionCardFocusContextValue {
  approvalId: string | null;
  claim: (approvalId: string) => boolean;
}
export const QuestionCardFocusContext =
  React.createContext<QuestionCardFocusContextValue>({
    approvalId: null,
    claim: () => false,
  });

function QuestionCardFocusTarget({
  approvalId,
  enabled,
  className,
  children,
}: {
  approvalId: string;
  enabled: boolean;
  className: string;
  children: React.ReactNode;
}) {
  const focus = React.useContext(QuestionCardFocusContext);
  const cardRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!enabled || focus.approvalId !== approvalId || !cardRef.current) return;
    if (!focus.claim(approvalId)) return;
    const reduceMotion = window.matchMedia?.(
      "(prefers-reduced-motion: reduce)",
    ).matches;
    cardRef.current.scrollIntoView?.({
      behavior: reduceMotion ? "auto" : "smooth",
      block: "center",
    });
    cardRef.current.focus({ preventScroll: true });
  }, [approvalId, enabled, focus]);

  return (
    <div
      ref={cardRef}
      tabIndex={-1}
      data-ask-user-question-card={approvalId}
      className={classNames(
        className,
        "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]",
      )}
    >
      {children}
    </div>
  );
}

type RequestDeliveryOptions = {
  sessionId: string | null;
  approvalId: string;
  nativeRequest?: ReturnType<typeof nativeProviderRequestIdentity>;
  onResolveAgentApproval?: ChatPanelProps["onResolveAgentApproval"];
  kind: "answer" | "approval";
};
const requestDeliveryErrors = new Map<string, string>();

function useRequestDelivery({
  sessionId,
  approvalId,
  nativeRequest,
  onResolveAgentApproval,
  kind,
}: RequestDeliveryOptions) {
  const storeKey = requestStoreKey(sessionId, approvalId, nativeRequest);
  const subscribe = (listener: () => void) =>
    subscribeRequestDelivery(storeKey, listener);
  const busy = useSyncExternalStore(subscribe, () =>
    requestDeliveriesInFlight.has(storeKey),
  );
  const error = useSyncExternalStore(
    subscribe,
    () => requestDeliveryErrors.get(storeKey) ?? null,
  );
  const setError = (message: string | null) => {
    requestDeliveryErrors.delete(storeKey);
    if (message) requestDeliveryErrors.set(storeKey, message);
    if (requestDeliveryErrors.size > QUESTION_DRAFT_CAP) {
      const oldest = requestDeliveryErrors.keys().next().value;
      if (oldest !== undefined) requestDeliveryErrors.delete(oldest);
    }
    requestDeliverySubscribers.get(storeKey)?.forEach((listener) => listener());
  };
  const resolve = async (
    input: Omit<AgentApprovalResolutionInput, "sessionId" | "approvalId">,
    preResolve?: () => Promise<void>,
    onDelivered?: () => void,
  ) => {
    if (!sessionId || requestDeliveriesInFlight.has(storeKey)) return;
    const label = kind === "answer" ? "Answer" : "Approval";
    if (!onResolveAgentApproval) {
      setError(`${label} delivery is unavailable in this view.`);
      return;
    }
    setRequestDeliveryInFlight(storeKey, true);
    setError(null);
    try {
      // Saving recommended answers must finish inside the same repeat guard
      // before the runtime can receive the decision.
      if (preResolve) {
        try {
          await preResolve();
        } catch (applyError) {
          const detail =
            applyError instanceof Error && applyError.message
              ? applyError.message
              : "the save failed";
          setError(
            `Recommended answers were NOT saved (${detail}). Nothing was submitted — retry, or answer in the doc.`,
          );
          return;
        }
      }
      const result = await onResolveAgentApproval({
        sessionId,
        approvalId,
        ...input,
        ...(nativeRequest ?? {}),
      });
      const pending =
        typeof result === "object" &&
        result !== null &&
        (("status" in result && result.status === "resolving") ||
          ("delivered" in result && result.delivered === false));
      if (pending) {
        setError(
          kind === "answer"
            ? "Answer delivery is still resolving. Your draft is preserved."
            : "Approval delivery is still resolving. Retry shortly.",
        );
        return;
      }
      onDelivered?.();
    } catch (failure) {
      setError(
        (kind === "answer" ? lateAnswerConflictNotice(failure) : null) ??
          (failure instanceof Error
            ? failure.message
            : `The ${kind} could not be delivered. Retry.`),
      );
    } finally {
      setRequestDeliveryInFlight(storeKey, false);
    }
  };
  return { busy, error, resolve };
}

function QuestionAnswerForm({
  sessionId,
  approvalId,
  questions,
  nativeRequest,
  lateAnswerable = false,
  provisional = false,
  onResolveAgentApproval,
}: {
  sessionId: string | null;
  approvalId: string;
  questions: ParsedQuestion[];
  nativeRequest?: ReturnType<typeof nativeProviderRequestIdentity>;
  /**
   * D25 `timed_out_answerable`: render the form identically to pending plus
   * the single hint line, and hide Dismiss — the server never late-admits a
   * rejection (routes/chat D25).
   */
  lateAnswerable?: boolean;
  /** The request is not stored yet: the form is shown but not actionable. */
  provisional?: boolean;
  onResolveAgentApproval?: ChatPanelProps["onResolveAgentApproval"];
}) {
  const storeKey = requestStoreKey(sessionId, approvalId, nativeRequest);
  const { reason: answerBlocked, startAndAnswer } = useAnswerBlockedReason(
    onResolveAgentApproval,
  );
  const [draft, setDraft] = useState<QuestionDraft>(
    () =>
      questionDrafts.get(storeKey) ??
      readChatViewState<QuestionDraft>("questions", storeKey) ??
      {},
  );
  const {
    busy,
    error,
    resolve: deliver,
  } = useRequestDelivery({
    sessionId,
    approvalId,
    nativeRequest,
    onResolveAgentApproval,
    kind: "answer",
  });

  const updateEntry = (
    index: number,
    updater: (entry: QuestionDraftEntry) => QuestionDraftEntry,
  ) => {
    setDraft((previous) => {
      const next = {
        ...previous,
        [index]: updater(questionDraftEntry(previous, index)),
      };
      storeQuestionDraft(storeKey, next);
      return next;
    });
  };

  const toggleOption = (
    index: number,
    optionIndex: number,
    multiSelect: boolean,
  ) => {
    updateEntry(index, (entry) => {
      if (multiSelect) {
        const selected = entry.selected.includes(optionIndex)
          ? entry.selected.filter((value) => value !== optionIndex)
          : [...entry.selected, optionIndex];
        return { ...entry, selected };
      }
      // Single-select: an option is exclusive with free text.
      const selected = entry.selected.includes(optionIndex)
        ? []
        : [optionIndex];
      return { selected, otherText: "" };
    });
  };

  const setOtherText = (
    index: number,
    multiSelect: boolean,
    otherText: string,
  ) => {
    updateEntry(index, (entry) => ({
      // Single-select: free text is exclusive with an option selection.
      selected: multiSelect || !otherText.trim() ? entry.selected : [],
      otherText,
    }));
  };

  const complete = questions.every((question, index) =>
    questionAnswered(question, questionDraftEntry(draft, index)),
  );

  const resolve = (
    input: Omit<AgentApprovalResolutionInput, "sessionId" | "approvalId">,
    preResolve?: () => Promise<void>,
  ) =>
    deliver(input, preResolve, () => {
      questionDrafts.delete(storeKey);
      writeChatViewState("questions", storeKey, undefined);
    });

  return (
    <div className="mt-2 space-y-3" data-testid="agent-question-form">
      {lateAnswerable && (
        <p
          className="text-[11px] text-[var(--g5-text2)]"
          data-testid="agent-question-timeout-hint"
        >
          {TIMED_OUT_ANSWERABLE_HINT}
        </p>
      )}
      {provisional && <ConfirmingNotice />}
      {questions.map((question, index) => {
        const entry = questionDraftEntry(draft, index);
        return (
          <div key={index}>
            <p className="text-xs font-medium text-[var(--g5-text0)]">
              {question.header && (
                <span className="mr-1.5 rounded bg-[var(--g5-bg2)] px-1.5 py-0.5 text-[10px] text-[var(--g5-text2)]">
                  {question.header}
                </span>
              )}
              {question.question}
            </p>
            <div className="mt-1.5 flex flex-wrap gap-1.5">
              {question.options.map((option, optionIndex) => {
                const selected = entry.selected.includes(optionIndex);
                return (
                  <button
                    key={optionIndex}
                    type="button"
                    disabled={busy || provisional}
                    title={option.description}
                    aria-pressed={selected}
                    data-testid="agent-question-option"
                    onClick={() =>
                      toggleOption(index, optionIndex, question.multiSelect)
                    }
                    className={classNames(
                      "rounded border px-2.5 py-1 text-xs",
                      selected
                        ? "border-[var(--g5-active)]/60 bg-[var(--g5-active)]/15 font-medium text-[var(--g5-active)]"
                        : "border-[var(--g5-stroke)] bg-[var(--g5-bg1)] text-[var(--g5-text1)] hover:bg-[var(--g5-bg2)]",
                    )}
                  >
                    {option.label}
                  </button>
                );
              })}
            </div>
            <input
              type="text"
              value={entry.otherText}
              disabled={busy || provisional}
              placeholder="Other…"
              aria-label={`Other answer for: ${question.question}`}
              data-testid="agent-question-other"
              onChange={(event) =>
                setOtherText(index, question.multiSelect, event.target.value)
              }
              className="mt-1.5 w-full rounded border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-2 py-1 text-xs text-[var(--g5-text0)] placeholder:text-[var(--g5-text2)]"
            />
          </div>
        );
      })}
      {error && (
        <p role="alert" className="text-[11px] text-[var(--g5-blocked)]">
          {error}
        </p>
      )}
      <div className="flex flex-wrap gap-2">
        <GatedControl
          verb="answer"
          reason={answerBlocked}
          testId="agent-question-blocked"
        >
          <button
            type="button"
            disabled={
              busy || provisional || !complete || !sessionId || !!answerBlocked
            }
            title={
              answerBlocked ??
              (provisional
                ? CONFIRMING_TITLE
                : startAndAnswer
                  ? START_AND_ANSWER_TITLE
                  : undefined)
            }
            data-testid="agent-question-submit"
            data-start-and-answer={startAndAnswer ? "true" : undefined}
            onClick={() => {
              void resolve({
                decision: "approved",
                ...(startAndAnswer ? { startCompute: true } : {}),
                updatedInput: {
                  g5AnswersByIndex: encodeQuestionAnswers(questions, draft),
                },
                ...(nativeRequest
                  ? {
                      validatedResponse: {
                        g5AnswersByIndex: encodeQuestionAnswers(
                          questions,
                          draft,
                        ),
                      },
                    }
                  : {}),
              });
            }}
            className="rounded border border-[var(--g5-ok)]/50 bg-[var(--g5-ok)]/15 px-2.5 py-1 text-xs font-medium text-[var(--g5-ok)] hover:bg-[var(--g5-ok)]/25 disabled:cursor-not-allowed disabled:opacity-50"
          >
            {startAndAnswer ? "Start and answer" : "Answer"}
          </button>
        </GatedControl>
        {/* Native and late-answer requests use their own response controls. */}
        {!nativeRequest && !lateAnswerable && (
          <button
            type="button"
            disabled={busy || provisional || !sessionId}
            data-testid="agent-question-dismiss"
            onClick={() =>
              void resolve({
                decision: "rejected",
                reason: QUESTION_DISMISS_REASON,
              })
            }
            className="rounded border border-[var(--g5-blocked)]/45 bg-[var(--g5-blocked)]/12 px-2.5 py-1 text-xs font-medium text-[var(--g5-blocked)] hover:bg-[var(--g5-blocked)]/20 disabled:cursor-not-allowed disabled:opacity-50"
          >
            Dismiss
          </button>
        )}
      </div>
    </div>
  );
}

/**
 * The composer's disabled reason, with the header's Start beside it when
 * that is the valid action (`g5.conversation.composer.stopped`). The draft
 * stays in the textarea either way.
 */
/**
 * Why an agent's request cannot be answered now: the reply runs in the
 * owner's VM, so the page's gate decides; without a gate, only a missing
 * resolver blocks. `reason` is `null` when answering is possible. While the
 * owner's VM is stopped or failed, answering is possible as Start and
 * answer (`startAndAnswer`, chat g5.chat.start-and-send): the same request
 * records Start consent, and the held turn resumes with the answer once the
 * VM runs.
 */
function useAnswerBlockedReason(
  onResolveAgentApproval: ChatPanelProps["onResolveAgentApproval"] | undefined,
): { reason: string | null; startAndAnswer: boolean } {
  const gate = useWorkspaceLiveGate();
  const unavailable = onResolveAgentApproval
    ? null
    : "Approval delivery is unavailable in this view.";
  if (gate) {
    if (gate.canStart) return { reason: unavailable, startAndAnswer: true };
    const reason = liveGateReason(gate.kind, "answer", {
      workspaceNoun: gate.workspaceNoun,
    });
    if (reason) return { reason, startAndAnswer: false };
  }
  return { reason: unavailable, startAndAnswer: false };
}

/** The tooltip of an answer control that also starts the workspace. */
const START_AND_ANSWER_TITLE =
  "Starts the workspace and delivers this answer once it is running";

function ApprovalPanel({
  approval,
  sessionId,
  pending,
  onResolveAgentApproval,
}: {
  approval: AgentApprovalBlock;
  sessionId: string | null;
  pending: boolean;
  onResolveAgentApproval?: ChatPanelProps["onResolveAgentApproval"];
}) {
  const {
    busy: resolving,
    error: resolveError,
    resolve: deliver,
  } = useRequestDelivery({
    sessionId,
    approvalId: approval.approvalId,
    nativeRequest: nativeProviderRequestIdentity(approval.entry),
    onResolveAgentApproval,
    kind: "approval",
  });
  const { reason: answerBlocked, startAndAnswer } = useAnswerBlockedReason(
    onResolveAgentApproval,
  );
  const nativeRequest = nativeProviderRequestIdentity(approval.entry);
  const requestedAction = stringField(approval.entry.metadata ?? {}, [
    "requestedAction",
    "action",
    "description",
  ]);
  const display = toolDisplay(approval.input, approval.toolName);
  const description = requestedAction ?? display.description;
  // Tool IDENTITY gates the question card (chat_panel_runtime D25): a
  // non-question tool with a questions-shaped argument is a permission
  // approval, and rendering it as a survey would disguise the authorization.
  const questions = questionApproval(approval)
    ? parseAskUserQuestions(approval.input)
    : null;
  const suggestionDetails = permissionSuggestionRuleDetails(
    approval.input,
    approval.options,
  );
  const details = [...suggestionDetails, ...display.details];
  const resolve = (option: AgentApprovalOption) =>
    deliver({
      decision: option.decision,
      reason: option.reason,
      updatedInput: option.updatedInput,
      updatedPermissions: option.updatedPermissions,
      interrupt: option.interrupt,
      ...(startAndAnswer ? { startCompute: true } : {}),
    });

  if (approval.status === "reply_pending") {
    return (
      <p className="mt-2 text-xs text-[var(--g5-text1)]">
        Answer submitted. Waiting for the agent to resume…
      </p>
    );
  }

  // D25 timed-out-answerable: the question form stays fully actionable,
  // visually identical to pending plus the single hint line.
  const lateAnswerable = approval.status === "timed_out_answerable";
  if (questions && (pending || lateAnswerable)) {
    return (
      <QuestionCardFocusTarget
        approvalId={approval.approvalId}
        enabled
        className="mt-2 rounded border border-[var(--g5-active)]/40 bg-[var(--g5-active)]/10 p-2"
      >
        <p className="text-xs font-medium text-[var(--g5-text0)]">
          The agent has a question for you
        </p>
        <QuestionAnswerForm
          sessionId={sessionId}
          approvalId={approval.approvalId}
          questions={questions}
          nativeRequest={nativeRequest}
          lateAnswerable={lateAnswerable}
          provisional={approval.provisional}
          onResolveAgentApproval={onResolveAgentApproval}
        />
      </QuestionCardFocusTarget>
    );
  }

  if (!pending) {
    const denied = approval.decision === "rejected";
    // A late-answered question collapses to the Answered summary (the answer
    // text lives in the late-answer user message); superseded timeouts — and
    // an answerable sub-state whose questions cannot render — keep today's
    // non-actionable timed-out collapse.
    const lateAnswered = approval.status === "late_answered";
    const timedOut =
      approval.status === "timed_out" ||
      approval.status === "timed_out_superseded" ||
      approval.status === "timed_out_answerable";
    // A withdrawn question or an undelivered saved answer reads as stopped,
    // like any other ended request; the resend action below is the only
    // addition (`g5.chat.native-question-delivery-state`).
    const undelivered = approval.status === "undelivered";
    const stopped =
      approval.interrupt ||
      approval.status === "stopped" ||
      approval.status === "lost" ||
      undelivered;
    const failed = approval.status === "failed";
    return (
      <details className="mt-2 rounded border border-[var(--g5-stroke)]/40 bg-[var(--g5-bg0)]/70">
        {questions && (
          <QuestionFormStateCleanup
            storeKey={requestStoreKey(
              sessionId,
              approval.approvalId,
              nativeRequest,
            )}
          />
        )}
        <summary
          data-testid="chat-approval-resolved-toggle"
          className="cursor-pointer list-none px-2 py-1.5 text-[11px] text-[var(--g5-text2)] [&::-webkit-details-marker]:hidden"
        >
          {questions
            ? failed
              ? "Answer delivery failed"
              : lateAnswered
                ? "Answered"
                : timedOut
                  ? "Timed out — ask again"
                  : stopped
                    ? "Stopped"
                    : denied
                      ? "Dismissed"
                      : "Answered"
            : failed
              ? "Approval delivery failed"
              : lateAnswered
                ? "Answered"
                : timedOut
                  ? "Timed out — ask again"
                  : stopped
                    ? "Stopped"
                    : denied
                      ? "Denied"
                      : "Resolved: allow"}
        </summary>
        <div className="border-t border-[var(--g5-stroke)]/30 p-2 text-[11px]">
          {failed && (
            <div className="mb-2 text-[var(--g5-warn)]">
              The provider rejected this {questions ? "answer" : "approval"}{" "}
              delivery
              {approval.errorCode ? ` (${approval.errorCode})` : ""}.
            </div>
          )}
          {undelivered && questions && (
            <UndeliveredAnswer
              questions={questions}
              savedAnswer={approval.savedAnswer}
            />
          )}
          {display.command && (
            <pre className="overflow-x-auto whitespace-pre-wrap break-words rounded bg-[var(--g5-bg1)] p-2 font-mono">
              {display.command}
            </pre>
          )}
          {details.map((detail, index) => (
            <div key={`${detail.label}-${index}`} className="mt-2">
              <p className="mb-1 font-medium text-[var(--g5-text2)]">
                {detail.label}
              </p>
              <pre className="overflow-x-auto whitespace-pre-wrap break-words rounded bg-[var(--g5-bg1)] p-2 font-mono">
                {detail.value}
              </pre>
            </div>
          ))}
        </div>
      </details>
    );
  }

  return (
    <div className="mt-2 rounded border border-[var(--g5-warn)]/40 bg-[var(--g5-warn)]/10 p-2">
      <p className="text-xs font-medium text-[var(--g5-text0)]">
        Approval requested: {formatToolName(approval.toolName)}
      </p>
      {description && (
        <p className="mt-1 text-[11px] text-[var(--g5-text1)]">{description}</p>
      )}
      {display.path && (
        <p className="mt-1 truncate text-[11px] text-[var(--g5-text2)]">
          {display.path}
        </p>
      )}
      {display.command && (
        <pre className="mt-2 overflow-x-auto whitespace-pre-wrap break-words rounded bg-[var(--g5-bg0)] p-2 font-mono text-[11px] text-[var(--g5-text1)]">
          {display.command}
        </pre>
      )}
      {details.map((detail, index) => (
        <div key={`${detail.label}-${index}`} className="mt-2">
          <p className="mb-1 text-[11px] font-medium text-[var(--g5-text2)]">
            {detail.label}
          </p>
          <pre className="overflow-x-auto whitespace-pre-wrap break-words rounded bg-[var(--g5-bg0)] p-2 font-mono text-[11px] text-[var(--g5-text1)]">
            {detail.value}
          </pre>
        </div>
      ))}
      {approval.provisional && <ConfirmingNotice className="mt-2" />}
      {resolving && (
        <p role="status" className="mt-2 text-xs text-[var(--g5-text2)]">
          Delivering approval…
        </p>
      )}
      {resolveError && (
        <p role="alert" className="mt-2 text-[11px] text-[var(--g5-blocked)]">
          {resolveError}
        </p>
      )}
      <GatedControl
        verb="answer"
        reason={answerBlocked}
        testId="agent-approval-blocked"
        className="mt-3 flex-wrap gap-2"
      >
        {sortedApprovalOptions(approval.options).map((option) => (
          <button
            key={option.id}
            type="button"
            title={
              answerBlocked ??
              (approval.provisional
                ? CONFIRMING_TITLE
                : startAndAnswer
                  ? `${START_AND_ANSWER_TITLE}: ${option.description ?? option.label}`
                  : option.description)
            }
            data-start-and-answer={startAndAnswer ? "true" : undefined}
            disabled={
              resolving || approval.provisional || !sessionId || !!answerBlocked
            }
            data-testid={
              option.id === "allow"
                ? "agent-approval-approve"
                : option.id === "deny"
                  ? "agent-approval-reject"
                  : option.interrupt
                    ? "agent-approval-deny-stop"
                    : "agent-approval-suggestion"
            }
            onClick={() => void resolve(option)}
            className={classNames(
              "rounded border px-2.5 py-1 text-xs font-medium",
              option.decision === "approved"
                ? "border-[var(--g5-ok)]/50 bg-[var(--g5-ok)]/15 text-[var(--g5-ok)] hover:bg-[var(--g5-ok)]/25"
                : "border-[var(--g5-blocked)]/45 bg-[var(--g5-blocked)]/12 text-[var(--g5-blocked)] hover:bg-[var(--g5-blocked)]/20",
            )}
          >
            {startAndAnswer
              ? `Start and ${trimmedApprovalLabel(option.label)}`
              : trimmedApprovalLabel(option.label)}
          </button>
        ))}
      </GatedControl>
    </div>
  );
}

function questionApproval(approval: AgentApprovalBlock): boolean {
  return (
    approval.toolName === ASK_USER_QUESTION_TOOL &&
    approval.entry.metadata?.requestKind !== "permission"
  );
}

/**
 * An accepted answer whose question the saved session could not take: the
 * saved selection stays visible, and the owner may send question and answer
 * as an ordinary new message (`g5.chat.native-question-delivery-state`).
 */
function UndeliveredAnswer({
  questions,
  savedAnswer,
}: {
  questions: ParsedQuestion[];
  savedAnswer: unknown;
}) {
  const resend = useContext(QuestionAnswerResendContext);
  return (
    <div className="mb-2" data-testid="agent-question-undelivered">
      <ul className="mt-1 space-y-0.5 text-[var(--g5-text1)]">
        {savedAnswerLines(questions, savedAnswer).map((line) => (
          <li key={line.question}>
            <span className="text-[var(--g5-text2)]">{line.question}:</span>{" "}
            {line.answer}
          </li>
        ))}
      </ul>
      {resend && (
        <button
          type="button"
          data-testid="agent-question-resend"
          onClick={() =>
            resend(formatSavedQuestionAnswer(questions, savedAnswer))
          }
          className="mt-1.5 rounded border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-2 py-0.5 text-[11px] text-[var(--g5-text1)] hover:bg-[var(--g5-bg2)]"
        >
          {RESEND_QUESTION_ANSWER_LABEL}
        </button>
      )}
    </div>
  );
}

function toolStatusLabel(
  block: Extract<AssistantTurnBlock, { type: "tool" }>,
): string {
  const approval = block.approval;
  if (approval?.provisional && approval.status === "pending")
    return CONFIRMING_LABEL;
  if (approval && questionApproval(approval)) {
    if (
      approval.interrupt ||
      approval.status === "stopped" ||
      approval.status === "undelivered" ||
      approval.status === "lost"
    )
      return "Stopped";
    switch (approval.status) {
      case "pending":
      case "timed_out_answerable":
        return "Waiting for your answer";
      case "reply_pending":
        return "Answer submitted";
      case "resolved":
        return approval.decision === "rejected" ? "Dismissed" : "Answered";
      case "late_answered":
        return "Answered";
      case "failed":
        return "Answer delivery failed";
      case "timed_out":
      case "timed_out_superseded":
        return "Timed out — ask again";
    }
  }
  return block.status.replace("_", " ");
}

function AssistantToolBlock({
  block,
  sessionId,
  onResolveAgentApproval,
  expanded,
  onExpandedChange,
  grouped = false,
}: {
  block: Extract<AssistantTurnBlock, { type: "tool" }>;
  sessionId: string | null;
  expanded?: boolean;
  onExpandedChange?: (expanded: boolean) => void;
  grouped?: boolean;
  onResolveAgentApproval?: ChatPanelProps["onResolveAgentApproval"];
}) {
  const [localExpanded, setLocalExpanded] = React.useState<boolean>();
  const display = toolDisplay(block.input, block.toolName);
  const label = activityLabel(block.toolName, display);
  const pending = block.status === "pending_approval";
  const statusLabel = toolStatusLabel(block);
  const showStatus =
    block.status !== "done" ||
    Boolean(block.approval && questionApproval(block.approval));
  const details = pending && block.approval ? [] : display.details;
  const isError = block.status === "error" || block.status === "denied";
  const defaultOpen = pending || block.status === "running";
  const Icon = toolIcon(block.toolName);
  const open = pending || (expanded ?? localExpanded ?? defaultOpen);

  return (
    <div className="w-full">
      <details
        className={classNames(
          "group/tool w-full text-xs",
          !grouped &&
            "rounded-[var(--g5-radius-md)] border border-[var(--g5-stroke)]",
        )}
        open={open}
        data-testid="assistant-tool-block"
      >
        <summary
          className="flex min-w-0 w-full cursor-pointer list-none items-center gap-2 overflow-hidden px-3 py-2.5 text-[var(--g5-text2)] hover:bg-[var(--g5-bg1)] [&::-webkit-details-marker]:hidden"
          title={label}
          onClick={(event) => {
            event.preventDefault();
            if (!pending) (onExpandedChange ?? setLocalExpanded)(!open);
          }}
        >
          <Icon
            size={13}
            className={classNames(
              "shrink-0",
              isError && "text-[var(--g5-blocked)]",
            )}
          />
          <span
            className={classNames(
              "min-w-0 flex-1 truncate",
              label === display.command?.trim() && "font-mono",
            )}
          >
            {label}
          </span>
          {showStatus && (
            <span
              className={classNames(
                "shrink-0 text-[11px]",
                isError && "text-[var(--g5-blocked)]",
              )}
            >
              {statusLabel}
            </span>
          )}
          <ChevronRight
            size={13}
            className="shrink-0 group-open/tool:rotate-90"
          />
        </summary>
        <div className="space-y-2 border-t border-[var(--g5-stroke)] p-3 text-[11px] text-[var(--g5-text1)]">
          <p className="text-[var(--g5-text2)]">
            <span>{formatToolName(block.toolName)}</span> ·{" "}
            <span>{statusLabel}</span>
            {block.durationMs != null && (
              <span>
                {" "}
                ·{" "}
                {block.durationMs < 1000
                  ? `${block.durationMs}ms`
                  : `${(block.durationMs / 1000).toFixed(1)}s`}
              </span>
            )}
          </p>
          {display.path && (
            <p className="break-all font-mono">{display.path}</p>
          )}
          {block.inputTruncated && <p>Input truncated</p>}
          {block.outputTruncated && <p>Output truncated</p>}
          {block.input == null && !block.output && !pending && (
            <p>Details unavailable for this activity.</p>
          )}
          {!pending &&
            display.description &&
            display.description !== block.toolName && (
              <p>{display.description}</p>
            )}
          {!pending && display.command && (
            <pre className="overflow-x-auto whitespace-pre-wrap break-words rounded bg-[var(--g5-bg1)] p-2 font-mono">
              {display.command}
            </pre>
          )}
          {details.map((detail, index) => (
            <div key={`${detail.label}-${index}`}>
              <p className="mb-1 font-medium text-[var(--g5-text2)]">
                {detail.label}
              </p>
              <pre className="overflow-x-auto whitespace-pre-wrap break-words rounded bg-[var(--g5-bg1)] p-2 font-mono">
                {detail.value}
              </pre>
            </div>
          ))}
          {block.approval && (
            <ApprovalPanel
              approval={block.approval}
              sessionId={sessionId}
              pending={block.approval.status === "pending"}
              onResolveAgentApproval={onResolveAgentApproval}
            />
          )}
          {block.output && (
            <pre
              className={classNames(
                "overflow-x-auto whitespace-pre-wrap break-words rounded bg-[var(--g5-bg1)] p-2 font-mono",
                block.isError
                  ? "text-[var(--g5-blocked)]"
                  : "text-[var(--g5-text1)]",
              )}
            >
              {block.output}
            </pre>
          )}
        </div>
      </details>
      {/* Screenshot evidence stays visible without expanding the details
          (provided_agent_runtime D98) — the collapsed summary would
          otherwise hide the one output users want to SEE. */}
      {block.imageArtifacts && block.imageArtifacts.length > 0 && sessionId && (
        <ToolResultImageGallery
          artifacts={block.imageArtifacts}
          sessionId={sessionId}
          className="mt-1.5"
        />
      )}
    </div>
  );
}

// Reasoning stays available without a permanent preview in the conversation.
function ThinkingBlock({
  text,
  expanded = false,
  onExpandedChange,
}: {
  text: string;
  expanded?: boolean;
  onExpandedChange: (next: boolean) => void;
}) {
  const Chevron = expanded ? ChevronDown : ChevronRight;
  return (
    <div className="py-0.5" data-testid="thinking-block">
      <button
        type="button"
        data-testid="chat-thinking-toggle"
        onClick={() => onExpandedChange(!expanded)}
        aria-expanded={expanded}
        aria-label={expanded ? "Collapse thinking" : "Expand thinking"}
        className="flex cursor-pointer items-center gap-1.5 py-1 text-left text-xs text-[var(--g5-text2)]"
      >
        Thinking <Chevron size={13} />
      </button>
      {expanded && (
        <div className="mt-1 text-[13px] leading-relaxed text-[var(--g5-text2)] [overflow-wrap:anywhere]">
          <MarkdownRenderer content={text} className="text-[13px]" />
        </div>
      )}
    </div>
  );
}

type ActivityBlock = Extract<AssistantTurnBlock, { type: "tool" | "thought" }>;
type ActivityDisclosure = {
  group?: boolean;
  tool?: boolean;
  thought?: boolean;
};
type ActivityDisclosures = Record<string, ActivityDisclosure>;
type ChangeActivityDisclosure = (
  toolIds: string[],
  choice: ActivityDisclosure,
) => void;

function groupConversationActivity(blocks: AssistantTurnBlock[]) {
  const grouped: (
    | AssistantTurnBlock
    | { type: "activity"; id: string; blocks: ActivityBlock[] }
  )[] = [];
  let pending: ActivityBlock[] = [];
  const flush = () => {
    const firstTool = pending.find((block) => block.type === "tool");
    if (firstTool)
      grouped.push({ type: "activity", id: firstTool.id, blocks: pending });
    else grouped.push(...pending);
    pending = [];
  };
  for (const block of blocks) {
    if (block.type === "thought" && pending.length === 0) {
      grouped.push(block);
      continue;
    }
    // Questions, permissions, and visual evidence must remain outside collapsed activity.
    if (
      block.type === "text" ||
      block.type === "status" ||
      block.type === "artifact" ||
      (block.type === "tool" &&
        (block.approval || block.imageArtifacts?.length))
    ) {
      flush();
      grouped.push(block);
    } else pending.push(block);
  }
  flush();
  return grouped;
}

type TextBlock = Extract<AssistantTurnBlock, { type: "text" }>;
type TextRunBlock = { type: "textRun"; id: string; blocks: TextBlock[] };
type GroupedBlock = ReturnType<typeof groupConversationActivity>[number];
type RenderBlock = Exclude<GroupedBlock, TextBlock> | TextRunBlock;

/** Text blocks per memoized run: a streaming turn's deltas are one block
 * each, so a long turn holds thousands; grouped in runs, a commit reconciles
 * the runs (one per sixty-four blocks) and re-renders only the open one. */
const TEXT_RUN_LENGTH = 64;

/** Consecutive text blocks become runs of at most `TEXT_RUN_LENGTH`, chunked
 * from the start so a run that is full keeps its content as later blocks
 * arrive (chat g5.chat.live-tail-recovery). */
function chunkTextRuns(blocks: GroupedBlock[]): RenderBlock[] {
  const chunked: RenderBlock[] = [];
  let run: TextBlock[] = [];
  const flush = () => {
    if (run.length)
      chunked.push({ type: "textRun", id: `run-${run[0].id}`, blocks: run });
    run = [];
  };
  for (const block of blocks) {
    if (block.type === "text") {
      run.push(block);
      if (run.length === TEXT_RUN_LENGTH) flush();
      continue;
    }
    flush();
    chunked.push(block);
  }
  flush();
  return chunked;
}

const TextRun = React.memo(
  function TextRun({ blocks }: { blocks: TextBlock[] }) {
    return (
      <div className="space-y-4" data-testid="assistant-text-run">
        {blocks.map((block) => (
          <div
            key={block.id}
            className="break-words [&_pre]:overflow-x-auto [&_pre]:whitespace-pre-wrap [&_pre]:break-words"
          >
            <MarkdownRenderer content={block.text} className="text-[14px]" />
          </div>
        ))}
      </div>
    );
  },
  (previous, next) =>
    previous.blocks.length === next.blocks.length &&
    previous.blocks.every(
      (block, index) =>
        block.id === next.blocks[index].id &&
        block.text === next.blocks[index].text,
    ),
);

function ConversationActivity({
  blocks,
  sessionId,
  onResolveAgentApproval,
  disclosures,
  onDisclosureChange,
}: {
  blocks: ActivityBlock[];
  sessionId: string | null;
  disclosures: ActivityDisclosures;
  onDisclosureChange: ChangeActivityDisclosure;
  onResolveAgentApproval?: ChatPanelProps["onResolveAgentApproval"];
}) {
  const tools = blocks.filter((block) => block.type === "tool");
  const toolIds = tools.map((tool) => tool.id);
  const expanded = toolIds
    .map((id) => disclosures[id]?.group)
    .find((choice) => choice !== undefined);
  const active = [...tools].reverse().find((tool) => tool.status === "running");
  const unsettled = tools.filter(
    (tool) => tool.status === "ended" || tool.status === "stopped",
  ).length;
  const renderBlock = (block: ActivityBlock, grouped: boolean) =>
    block.type === "thought" ? (
      <ThinkingBlock
        key={block.id}
        text={block.text}
        expanded={disclosures[block.id]?.thought}
        onExpandedChange={(next) =>
          onDisclosureChange([block.id], { thought: next })
        }
      />
    ) : (
      <AssistantToolBlock
        key={block.id}
        block={block}
        sessionId={sessionId}
        grouped={grouped}
        expanded={disclosures[block.id]?.tool}
        onExpandedChange={(next) => {
          onDisclosureChange([block.id], { tool: next });
          if (next) onDisclosureChange(toolIds, { group: true });
        }}
        onResolveAgentApproval={onResolveAgentApproval}
      />
    );
  if (blocks.length === 1) return renderBlock(blocks[0], false);
  const open = expanded ?? Boolean(active);
  return (
    <div data-testid="chat-activity-group">
      <details className="group/activity" open={open}>
        <summary
          className="flex cursor-pointer list-none flex-wrap items-center gap-1.5 py-1 text-[13px] text-[var(--g5-text2)] [&::-webkit-details-marker]:hidden"
          onClick={(event) => {
            event.preventDefault();
            onDisclosureChange(toolIds, { group: !open });
          }}
        >
          <span>
            {activitySummary(
              tools.map((tool) => ({
                ...tool,
                display: toolDisplay(tool.input, tool.toolName),
              })),
            )}
          </span>
          <ChevronRight
            size={13}
            className="shrink-0 group-open/activity:rotate-90"
          />
          {unsettled > 0 && <span>· {unsettled} without a result</span>}
        </summary>
        <div className="mt-2 overflow-hidden rounded-[var(--g5-radius-md)] border border-[var(--g5-stroke)] [&>*+*]:border-t [&>*+*]:border-[var(--g5-stroke)]">
          {blocks.map((block) => renderBlock(block, true))}
        </div>
      </details>
      {active && !open && (
        <p className="mt-1 text-xs text-[var(--g5-text2)] [overflow-wrap:anywhere]">
          Running:{" "}
          {activityLabel(
            active.toolName,
            toolDisplay(active.input, active.toolName),
          )}
        </p>
      )}
    </div>
  );
}

function isLiveAssistantStatus(status: string): boolean {
  return status === "pending" || status === "streaming";
}

/**
 * Claude's reported background set while `message` is still a live reply
 * (`g5.conversation.providers.claude-background.visibility`), or null when
 * the reply settled or no background work remains.
 */
export function liveBackgroundWait(
  message: Pick<ChatMessageDTO, "id" | "status">,
  entries: readonly TimelineEntry[],
  liveTaskWait: BackgroundTaskSet | null = null,
): BackgroundTaskSet | null {
  if (!isLiveAssistantStatus(message.status)) return null;
  // The newest report in the loaded timeline wins. The set is journaled only
  // when it changes, so during a long wait its row can leave the loaded page;
  // the session's live task then still carries it.
  const wait =
    latestBackgroundTaskSet(
      entries.filter((entry) => entry.metadata?.messageId === message.id),
    ) ?? liveTaskWait;
  return wait && wait.count > 0 ? wait : null;
}

/**
 * One displayed wait. When the set empties and fills again, Claude reports a
 * new wait start, so the panel and its pending End start over.
 */
function backgroundWaitKey(wait: BackgroundTaskSet): string {
  return [
    wait.taskId ?? "",
    wait.attempt ?? "",
    wait.waitStartedAtUnixMs ??
      wait.tasks.map((task) => task.subtaskId).join(","),
  ].join(":");
}

/** The running task's live background set, from the session's task view. */

function useTickingNow(intervalMs: number): number {
  const [nowMs, setNowMs] = React.useState(() => Date.now());
  React.useEffect(() => {
    const id = window.setInterval(() => setNowMs(Date.now()), intervalMs);
    return () => window.clearInterval(id);
  }, [intervalMs]);
  return nowMs;
}

/**
 * The live reply's wait on Claude's background work: the reported set, its
 * elapsed wait and, for the owner only, End.
 */
function BackgroundWaitPanel({
  wait,
  onEnd,
}: {
  wait: BackgroundTaskSet;
  onEnd?: () => Promise<unknown> | void;
}) {
  const nowMs = useTickingNow(1_000);
  // A delivered End stops this wait, which removes the panel; until then the
  // control stays disabled so a repeat cannot race settlement. A later wait
  // gets a fresh panel (`backgroundWaitKey`), so End never stays pending for
  // work it was not sent for.
  const [ending, setEnding] = React.useState(false);
  const endingRef = React.useRef(false);
  const startTimes = wait.tasks
    .map((task) => task.startedAtUnixMs)
    .filter((startedAt): startedAt is number => startedAt !== null);
  const waitStartedAt =
    wait.waitStartedAtUnixMs ??
    (startTimes.length > 0 ? Math.min(...startTimes) : null);
  const unlistedCount = wait.count - wait.tasks.length;
  const endWait = async () => {
    if (!onEnd || endingRef.current) return;
    endingRef.current = true;
    setEnding(true);
    try {
      await onEnd();
    } catch {
      endingRef.current = false;
      setEnding(false);
    }
  };

  return (
    <div
      data-testid="background-wait-panel"
      role="group"
      aria-label="Background work"
      className="w-full rounded-[var(--g5-radius-md)] border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-3 py-2 text-[13px] text-[var(--g5-text1)]"
    >
      <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
        <span className="relative flex h-2 w-2 shrink-0" aria-hidden="true">
          <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-[var(--g5-active)] opacity-75" />
          <span className="relative inline-flex h-2 w-2 rounded-full bg-[var(--g5-active)]" />
        </span>
        <span className="font-medium text-[var(--g5-text0)]">
          Waiting for background work
        </span>
        {waitStartedAt !== null && (
          <span
            data-testid="background-wait-elapsed"
            className="tabular-nums text-[var(--g5-text2)]"
          >
            {formatElapsedWait(nowMs - waitStartedAt)}
          </span>
        )}
        {onEnd && (
          <button
            type="button"
            data-testid="end-background-wait-button"
            onClick={() => void endWait()}
            disabled={ending}
            className={classNames(
              "ml-auto shrink-0 rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] px-2 py-0.5 text-xs text-[var(--g5-text1)] transition-colors",
              ending
                ? "cursor-default opacity-60"
                : "cursor-pointer hover:bg-[var(--g5-bg2)] hover:text-[var(--g5-text0)]",
            )}
          >
            {ending ? "Ending…" : "End wait"}
          </button>
        )}
      </div>
      <ul className="mt-2 space-y-1">
        {wait.tasks.map((task, index) => {
          const label = task.description || task.subtaskId;
          return (
            <li
              key={`${task.subtaskId}-${index}`}
              data-testid="background-wait-task"
              className="flex min-w-0 items-center gap-2 text-xs"
            >
              <span className="shrink-0 rounded bg-[var(--g5-bg2)] px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wide text-[var(--g5-text2)]">
                {backgroundTaskKindLabel(task.kind)}
              </span>
              <span
                data-testid="background-wait-task-description"
                className={classNames(
                  "min-w-0 flex-1 truncate text-[var(--g5-text1)]",
                  task.kind === "shell" && "font-mono",
                )}
                title={label}
              >
                {label}
              </span>
              {task.startedAtUnixMs !== null && (
                <span
                  data-testid="background-wait-task-elapsed"
                  className="shrink-0 tabular-nums text-[var(--g5-text2)]"
                >
                  {formatElapsedWait(nowMs - task.startedAtUnixMs)}
                </span>
              )}
            </li>
          );
        })}
      </ul>
      {unlistedCount > 0 && (
        <p className="mt-1 text-xs text-[var(--g5-text2)]">
          {`+${unlistedCount} more`}
        </p>
      )}
      <p className="mt-2 text-xs text-[var(--g5-text2)]">
        New messages are queued and sent when this finishes.
      </p>
    </div>
  );
}

/** Loading state of the session's older activity, as a turn's marker shows it. */
export interface EarlierActivityControl {
  state: "idle" | "loading" | "error";
  onLoad?: () => void;
}

/**
 * Marks where a turn's loaded activity begins (g5.chat.history-window). It
 * loads older activity when activated and, while within one viewport of the
 * chat's scroll area, on its own; after a failure it waits for a retry.
 */
function EarlierActivityMarker({
  control,
}: {
  control: EarlierActivityControl;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [inRange, setInRange] = useState(false);
  useEffect(() => {
    const node = ref.current;
    if (!node || typeof IntersectionObserver === "undefined") return;
    const observer = new IntersectionObserver(
      (records) => setInRange(records.some((record) => record.isIntersecting)),
      {
        root: node.closest("[data-chat-scroll-root]"),
        rootMargin: "100% 0px 100% 0px",
      },
    );
    observer.observe(node);
    return () => observer.disconnect();
  }, []);
  const { state, onLoad } = control;
  useEffect(() => {
    if (inRange && state === "idle") onLoad?.();
  }, [inRange, state, onLoad]);
  return (
    <div
      ref={ref}
      data-testid="chat-earlier-activity"
      data-state={state}
      className="flex items-center gap-2 text-xs text-[var(--g5-text2)]"
    >
      <History
        size={12}
        className={state === "loading" ? "animate-pulse" : undefined}
      />
      <span>
        {state === "loading"
          ? "Loading earlier activity in this reply…"
          : state === "error"
            ? "Couldn't load earlier activity in this reply."
            : "Earlier activity in this reply isn't loaded."}
      </span>
      {state !== "loading" && onLoad && (
        <button
          type="button"
          data-testid="chat-earlier-activity-button"
          onClick={onLoad}
          className="rounded px-1.5 py-0.5 font-medium text-[var(--g5-text1)] hover:bg-white/5 hover:text-[var(--g5-text0)] cursor-pointer"
        >
          {state === "error" ? "Retry" : "Load"}
        </button>
      )}
    </div>
  );
}

function AssistantTurnComponent({
  message,
  entries = [],
  activityCoverage = "complete",
  earlierActivity,
  approvalClassification,
  onArtifactAction,
  isStreaming,
  sessionId,
  timedOutQuestionSubStates,
  onResolveAgentApproval,
  onEndBackgroundWait,
  liveTaskWait = null,
  onCitationClick,
  ontologyCard,
}: {
  message: ChatMessageDTO;
  entries?: TimelineEntry[];
  /** How much of this turn's activity the held timeline covers. */
  activityCoverage?: TurnActivityCoverage;
  /** Older-activity loading, shown on an incomplete turn's marker. */
  earlierActivity?: EarlierActivityControl;
  approvalClassification?: AgentApprovalClassification;
  onArtifactAction?: (artifact: Artifact, action: ArtifactAction) => void;
  isStreaming: boolean;
  sessionId: string | null;
  /** The running task's live background set, used when the loaded timeline
   * no longer holds the latest report. */
  liveTaskWait?: BackgroundTaskSet | null;
  /** D25 timed-out question sub-states derived from the full session timeline. */
  timedOutQuestionSubStates?: ReadonlyMap<string, TimedOutQuestionSubState>;
  onResolveAgentApproval?: ChatPanelProps["onResolveAgentApproval"];
  onEndBackgroundWait?: ChatPanelProps["onEndBackgroundWait"];
  onCitationClick?: (citation: Citation) => void;
  /** How this reply's ontology changes card shows (chat g5.chat.ontology-card). */
  ontologyCard?: {
    newest: boolean;
    turnRunning: boolean;
    onOpenAllChanges?: () => void;
    onOpenFile?: (
      filePath: string,
      repoId?: string,
      line?: number,
      statementId?: string,
    ) => void;
  };
}) {
  // Choices belong to tool IDs in this turn, so a late approval/image can
  // split an activity group without resetting the remaining disclosures.
  const disclosureKey = `${sessionId ?? message.sessionId}:${message.id}`;
  const [disclosures, setDisclosures] = React.useState<ActivityDisclosures>(
    () =>
      readChatViewState<ActivityDisclosures>("disclosures", disclosureKey) ??
      {},
  );
  useEffect(() => {
    if (Object.keys(disclosures).length > 0)
      writeChatViewState("disclosures", disclosureKey, disclosures);
  }, [disclosureKey, disclosures]);
  const onDisclosureChange: ChangeActivityDisclosure = (toolIds, choice) => {
    setDisclosures((current) => {
      const next = { ...current };
      for (const id of toolIds) next[id] = { ...current[id], ...choice };
      return next;
    });
  };
  const blocks = React.useMemo(
    () =>
      chunkTextRuns(
        groupConversationActivity(
          buildAssistantTurnBlocks(
            message,
            entries,
            timedOutQuestionSubStates,
            approvalClassification,
            activityCoverage,
          ),
        ),
      ),
    [
      message,
      entries,
      timedOutQuestionSubStates,
      approvalClassification,
      activityCoverage,
    ],
  );
  // Friendly workspace labels are shared with user citations.
  const workspaceNamesById = useWorkspaceNames();
  const backgroundWait = React.useMemo(
    () => liveBackgroundWait(message, entries, liveTaskWait),
    [message, entries, liveTaskWait],
  );
  const endSessionId = sessionId || message.sessionId || null;
  const endBackgroundWait =
    onEndBackgroundWait &&
    endSessionId &&
    backgroundWait?.taskId &&
    backgroundWait.attempt !== null
      ? () =>
          onEndBackgroundWait(endSessionId, {
            taskId: backgroundWait.taskId as string,
            attempt: backgroundWait.attempt as number,
          })
      : undefined;

  const showEarlierActivity = activityCoverage !== "complete";
  const card = messageOntologyCard(message);
  if (
    !blocks.length &&
    !message.citations?.length &&
    !backgroundWait &&
    !showEarlierActivity &&
    !card
  )
    return null;
  return (
    <div
      data-testid="assistant-message"
      data-message-id={message.id}
      className="flex flex-col items-start gap-1 px-3 py-1.5"
    >
      <div
        data-testid="assistant-turn-content"
        className="g5-conversation-prose min-w-0 max-w-[min(100%,8in)] rounded-xl bg-[var(--g5-bg2)] px-3 py-2 text-[14px] leading-relaxed text-[var(--g5-text0)]"
      >
        <div className="space-y-4">
          {showEarlierActivity && (
            <EarlierActivityMarker
              control={earlierActivity ?? { state: "loading" }}
            />
          )}
          {blocks.map((block) => {
            if (block.type === "activity") {
              return (
                <ConversationActivity
                  key={block.id}
                  blocks={block.blocks}
                  sessionId={sessionId}
                  disclosures={disclosures}
                  onDisclosureChange={onDisclosureChange}
                  onResolveAgentApproval={onResolveAgentApproval}
                />
              );
            }
            if (block.type === "textRun") {
              return <TextRun key={block.id} blocks={block.blocks} />;
            }
            if (block.type === "artifact") {
              return (
                <ArtifactRenderer
                  key={block.id}
                  artifact={block.artifact}
                  onAction={(action) =>
                    onArtifactAction?.(block.artifact, action)
                  }
                />
              );
            }
            if (block.type === "thought") {
              return (
                <ThinkingBlock
                  key={block.id}
                  text={block.text}
                  expanded={disclosures[block.id]?.thought}
                  onExpandedChange={(next) =>
                    onDisclosureChange([block.id], { thought: next })
                  }
                />
              );
            }
            if (block.type === "tool") {
              return (
                <AssistantToolBlock
                  key={block.id}
                  block={block}
                  sessionId={sessionId}
                  expanded={disclosures[block.id]?.tool}
                  onExpandedChange={(next) =>
                    onDisclosureChange([block.id], {
                      tool: next,
                      ...(next ? { group: true } : {}),
                    })
                  }
                  onResolveAgentApproval={onResolveAgentApproval}
                />
              );
            }
            if (block.kind === "error") {
              return (
                <ChatErrorNotice
                  key={block.id}
                  data-testid="assistant-status-block"
                  data-status-kind="error"
                >
                  {block.text}
                </ChatErrorNotice>
              );
            }
            return (
              <div
                key={block.id}
                data-testid="assistant-status-block"
                data-status-kind={block.kind}
                className={classNames(
                  "inline-flex items-center gap-1 rounded px-2 py-1 text-xs",
                  block.kind === "stopped"
                    ? "bg-[var(--g5-warn)]/10 text-[var(--g5-warn)]"
                    : block.kind === "info"
                      ? "border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] text-[var(--g5-text2)]"
                      : "bg-[var(--g5-warn)]/10 text-[var(--g5-text2)]",
                )}
              >
                {block.text}
              </div>
            );
          })}
          {isStreaming && message.status === "streaming" && (
            <span
              role="status"
              aria-label="Generating response"
              className="inline-block h-4 w-2 animate-pulse bg-[var(--g5-text0)]"
            />
          )}
          {backgroundWait && (
            <BackgroundWaitPanel
              key={backgroundWaitKey(backgroundWait)}
              wait={backgroundWait}
              onEnd={endBackgroundWait}
            />
          )}
          {card && (
            // A card that cannot render must not take the chat down with it.
            <ErrorBoundary
              fallback={
                <p className="text-xs text-[var(--g5-text2)]">
                  This reply's ontology changes card can't be shown.
                </p>
              }
            >
              <OntologyChangesCard
                card={card}
                sessionId={sessionId || message.sessionId}
                messageId={message.id}
                newest={ontologyCard?.newest ?? false}
                turnRunning={ontologyCard?.turnRunning ?? true}
                onOpenAllChanges={ontologyCard?.onOpenAllChanges}
                onOpenFile={ontologyCard?.onOpenFile}
              />
            </ErrorBoundary>
          )}
        </div>
      </div>

      {/* Server-derived citations render on the provided-agent turn path too
          (issue #722): clicking routes through onCitationClick →
          onOpenFile. */}
      <CitationChips
        citations={
          activityCoverage === "unloaded" ? undefined : message.citations
        }
        onCitationClick={onCitationClick}
        workspaceNamesById={workspaceNamesById}
      />

      {/* Timestamp — agent turns carry the same full send timestamp as user
          bubbles, shown only once the turn completes
          (chat_panel_runtime#D28, message_rendering#D3). */}
      {message.status === "complete" && message.createdAt && (
        <div className="text-[10px] text-[var(--g5-text2)] px-1">
          {formatMessageTimestamp(message.createdAt)}
        </div>
      )}
    </div>
  );
}

interface ToolResultImageArtifact {
  attachmentId: string;
  contentType?: string;
  sizeBytes?: number;
}

type AssistantTurnProps = Parameters<typeof AssistantTurnComponent>[0];

/**
 * A turn re-renders only when something it renders changed: its message's
 * fields, its entries' identity (the transcript keeps an unchanged turn's
 * array), the per-turn approval slices, its flags and its callbacks
 * (chat g5.chat.live-tail-recovery). Exported for tests.
 */
export function assistantTurnPropsEqual(
  previous: AssistantTurnProps,
  next: AssistantTurnProps,
): boolean {
  return (
    sameRecord(previous.message, next.message) &&
    sameEntries(previous.entries, next.entries) &&
    previous.activityCoverage === next.activityCoverage &&
    previous.earlierActivity === next.earlierActivity &&
    previous.approvalClassification === next.approvalClassification &&
    previous.onArtifactAction === next.onArtifactAction &&
    previous.isStreaming === next.isStreaming &&
    previous.sessionId === next.sessionId &&
    previous.timedOutQuestionSubStates === next.timedOutQuestionSubStates &&
    previous.onResolveAgentApproval === next.onResolveAgentApproval &&
    previous.onEndBackgroundWait === next.onEndBackgroundWait &&
    previous.liveTaskWait === next.liveTaskWait &&
    previous.onCitationClick === next.onCitationClick &&
    sameOntologyCard(previous.ontologyCard, next.ontologyCard)
  );
}

function sameOntologyCard(
  previous: AssistantTurnProps["ontologyCard"],
  next: AssistantTurnProps["ontologyCard"],
): boolean {
  if (previous === next) return true;
  if (!previous || !next) return false;
  return (
    previous.newest === next.newest &&
    previous.turnRunning === next.turnRunning &&
    previous.onOpenAllChanges === next.onOpenAllChanges &&
    previous.onOpenFile === next.onOpenFile
  );
}

export const AssistantTurn = React.memo(
  AssistantTurnComponent,
  assistantTurnPropsEqual,
);

function toolResultImageArtifacts(
  entry: TimelineEntry,
): ToolResultImageArtifact[] {
  const raw = entry.metadata?.imageArtifacts;
  if (!Array.isArray(raw)) return [];
  return raw.filter(
    (item): item is ToolResultImageArtifact =>
      typeof item === "object" &&
      item !== null &&
      typeof (item as { attachmentId?: unknown }).attachmentId === "string",
  );
}

/// Authenticated image bytes for one screenshot attachment
/// (provided_agent_runtime D98): a plain `<img src>` is a browser-native load
/// that cannot carry the bearer Authorization header the content route
/// requires, so it 401s and the error path hid every thumbnail (observed
/// live). Fetch with the API token and feed the `<img>` an object URL.
function useAuthedImageUrl(url: string): {
  objectUrl: string | null;
  failed: boolean;
} {
  const [state, setState] = React.useState<{
    objectUrl: string | null;
    failed: boolean;
  }>({ objectUrl: null, failed: false });
  React.useEffect(() => {
    let cancelled = false;
    let created: string | null = null;
    const token = getApiToken();
    fetch(url, {
      headers: token ? { Authorization: `Bearer ${token}` } : undefined,
    })
      .then(async (response) => {
        if (!response.ok) throw new Error(String(response.status));
        const blob = await response.blob();
        if (cancelled) return;
        created = URL.createObjectURL(blob);
        setState({ objectUrl: created, failed: false });
      })
      .catch(() => {
        if (!cancelled) setState({ objectUrl: null, failed: true });
      });
    return () => {
      cancelled = true;
      if (created) URL.revokeObjectURL(created);
      setState({ objectUrl: null, failed: false });
    };
  }, [url]);
  return state;
}

function ScreenshotThumb({
  url,
  onZoom,
}: {
  url: string;
  onZoom: (objectUrl: string) => void;
}) {
  const { objectUrl, failed } = useAuthedImageUrl(url);
  // Aged-out or unauthorized references (retention sweep, chat_service D63)
  // hide instead of rendering broken.
  if (failed) return null;
  if (!objectUrl) {
    return (
      <div className="h-[100px] w-[160px] animate-pulse rounded-lg border border-[var(--g5-stroke)]/40 bg-[var(--g5-bg1)]" />
    );
  }
  return (
    <button
      type="button"
      data-testid="chat-screenshot-thumb"
      onClick={() => onZoom(objectUrl)}
      className={classNames(
        "block max-w-[300px] overflow-hidden rounded-lg",
        "border border-[var(--g5-stroke)]/40 bg-[var(--g5-bg0)]",
        "hover:border-[var(--g5-info)]/60 cursor-zoom-in",
      )}
      title="Click to zoom"
    >
      <img
        src={objectUrl}
        alt="Browser screenshot captured by the agent"
        className="block max-h-[200px] w-auto"
      />
    </button>
  );
}

/// Thumbnail gallery + lightbox for persisted browser screenshots
/// (provided_agent_runtime D98), used by every normalized tool result.
function ToolResultImageGallery({
  artifacts,
  sessionId,
  className,
}: {
  artifacts: ToolResultImageArtifact[];
  sessionId: string;
  className?: string;
}) {
  const [lightboxUrl, setLightboxUrl] = React.useState<string | null>(null);
  if (artifacts.length === 0) return null;
  return (
    <>
      <div
        className={classNames("flex flex-wrap gap-2", className)}
        data-testid="tool-result-image-artifacts"
      >
        {artifacts.map((artifact) => (
          <ScreenshotThumb
            key={artifact.attachmentId}
            url={`/api/v1/sessions/${sessionId}/attachments/${artifact.attachmentId}/content`}
            onZoom={setLightboxUrl}
          />
        ))}
      </div>
      {lightboxUrl && (
        <ScreenshotLightbox
          url={lightboxUrl}
          onClose={() => setLightboxUrl(null)}
        />
      )}
    </>
  );
}

/**
 * Full-screen screenshot preview. Registered with the shared dialog stack at
 * the modal tier so Escape, the focus trap and the chrome-covering signal
 * (platform_reports g5.quality.platform-reports.trigger) see it.
 */
function ScreenshotLightbox({
  url,
  onClose,
}: {
  url: string;
  onClose: () => void;
}) {
  const panelRef = React.useRef<HTMLDivElement>(null);
  useDialogFocus(panelRef, { onClose, priority: 50 });
  // Portaled to <body>, as the artifact image viewer is: a detached-chat
  // FloatingPanel is a fixed-position stacking context no z-index escapes
  // from inside, and any containment an ancestor adopts would clip an
  // in-tree overlay to the turn.
  return createPortal(
    <div
      ref={panelRef}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/80 p-8"
      role="dialog"
      aria-label="Screenshot preview"
      onClick={onClose}
    >
      <img
        src={url}
        alt="Browser screenshot captured by the agent"
        className="max-h-full max-w-full rounded-lg"
      />
      <button
        type="button"
        data-testid="chat-screenshot-lightbox-close"
        aria-label="Close screenshot preview"
        onClick={onClose}
        className="absolute right-4 top-4 rounded p-2 text-white/80 hover:text-white"
      >
        <XCircle size={20} />
      </button>
    </div>,
    document.body,
  );
}

function isInternalChatDeliveryAck(entry: TimelineEntry): boolean {
  const text = timelineEntryContentText(entry).trim();
  return /^Response sent \(\d+ chars\)(?:\. Waiting for user input\.)?$/.test(
    text,
  );
}
