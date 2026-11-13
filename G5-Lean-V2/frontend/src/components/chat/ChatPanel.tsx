// =============================================================================
// ChatPanel — Container component managing session state, message list,
// input, streaming indicator, and panel chrome
// =============================================================================

import {
  ArrowRight,
  ChevronDown,
  ExternalLink,
  FileText,
  History,
  ImagePlus,
  MessageSquare,
  Paperclip,
  PenLine,
  Plus,
  Search,
  Settings,
  Send,
  Square,
  X,
  XCircle,
} from "lucide-react";
import React, {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { createPortal } from "react-dom";
import type {
  WorkspacePhase,
  AgentAccountSelectionV1,
  AgentApprovalResolutionInput,
  AgentAuthModeV1,
  ChatAgentProvider,
  AgentLaunchPreferencesV1,
  AgentLaunchSelectionV1,
  ChatMessageDTO,
  ChatProjectReference,
  ChatSessionDTO,
  ChatSessionResultDTO,
  ChatUploadAttachment,
  Citation,
  HarnessChoice,
  PickerHarness,
  ProviderAccountDTO,
  ProviderControlSetSettingsPayload,
  ResolvedAgentExecutionV1,
  SessionInputDTO,
  SubscriptionUsage,
  SubscriptionUsageWindowSetting,
  SyncAgentConfig,
  TaskSummary,
  TimelineEntry,
} from "../../lib/types";
import { WORKBENCH_PHASES } from "../../lib/workbenchDefaults";
import { visibleChatQueue } from "../../lib/chatQueue";
import {
  type ComposerSeedHeader as ComposerSeedHeaderValue,
  withReviewIterations,
} from "../../lib/reviewPrompts";
import { ComposerSeedHeader } from "./ComposerSeedHeader";
import {
  backgroundTaskSetFromPayload,
  latestBackgroundTaskSet,
} from "../../lib/taskTimeline";
import type { BackgroundTaskSet } from "../../lib/taskTimeline";
import { CodexDeviceAuth } from "./CodexDeviceAuth";
import {
  codexAuthFailureFromEntry,
  codexAuthFailureKey,
  reconcileCodexAuthFailures,
} from "../../lib/codexAuthFailure";
import {
  AgentLaunchFieldControls,
  AgentLaunchResolutionStatus,
  agentLaunchPreferencesForConfig,
  effortOptionsForModel,
  launchEffortLabel,
} from "../agent/AgentLaunchConfigControls";
import type { ResolveAgentLaunchPreferences } from "../agent/AgentLaunchConfigControls";
import {
  SubscriptionUsageChip,
  shouldRenderSubscriptionUsageChip,
  type SubscriptionUsageWindow,
} from "./SubscriptionUsageChip";
import {
  CLAUDE_CHAT_MODEL_OPTIONS,
  CLAUDE_CODE_DEFAULT_MODEL_LABEL,
  CLAUDE_OPENROUTER_CHAT_MODEL_OPTIONS,
  claudeChatModelForOpenRouter,
  claudeModelLabelFor,
  normalizeClaudeChatModel,
} from "../../lib/claudeModels";
import {
  CODEX_CHAT_DEFAULT_MODEL,
  CODEX_CHAT_DEFAULT_MODEL_LABEL,
} from "../../lib/codexModels";
import {
  catalogDefaultChoiceFor,
  chatModelOptionsForHarness,
  clampEffortForModel,
  launchModeForHarness,
} from "../../lib/harnessPicker";
import {
  classifyAgentApprovals,
  classifyTimedOutQuestions,
} from "../../lib/agentApprovals";
import { ApiError } from "../../lib/api";
import {
  providerAccountKindLabel,
  providerAccountScopeLabel,
} from "../../lib/agentLaunchPresentation";
import {
  RUNTIME_CONFIG_ACCOUNT_VALUE,
  accountMatchesHarness,
  accountSelectionFromValue,
  accountSelectionValue,
  unassignedAccountOption,
} from "../../lib/providerAccountSelection";
import {
  compareTimelineEntries,
  projectProvidedAgentOutput,
  sortedTimelineEntries,
  timelineEntryContentText,
} from "../../lib/providerOutput";
import {
  buildUnifiedTimeline,
  createUnifiedTimelineCache,
  sliceApprovalsPerTurn,
  type TimelineItem,
} from "./unifiedTimeline";
import { Chip, classNames } from "../ui/UI";
import { ChatMessage } from "./ChatMessage";
import {
  AssistantTurn,
  QuestionCardFocusContext,
  QuestionAnswerResendContext,
  liveBackgroundWait,
  formatToolName,
  shouldRenderProviderStatus,
} from "./AssistantTurn";
import type {
  EarlierActivityControl,
  QuestionCardFocusContextValue,
} from "./AssistantTurn";
import {
  turnActivityCoverage,
  type TimelineCoverage,
} from "../../lib/historyCoverage";
import {
  GatedControl,
  liveGateReason,
  useWorkspaceLiveGate,
} from "../../lib/workspaceLiveGate";
import {
  useEditSessionInput,
  useRedeliverSessionInput,
  useSessionInputs,
  useWithdrawSessionInput,
  useCancelUnknownSessionInput,
} from "../../hooks/useChat";
import {
  chatStopActivityLabel,
  type ChatStopState,
} from "../../hooks/useChatStopController";
import type { LiveTailState } from "../../hooks/useLiveTail";
import {
  chatPromptHistoryKey,
  readChatPromptHistory,
  recordChatPromptHistory,
  stepPromptHistory,
  withRecordedPrompt,
  type ChatPromptHistoryScope,
  type PromptHistoryCursor,
} from "../../lib/chatPromptHistory";
import {
  ChatActivityIndicator,
  ChatTitleRenameEditor,
} from "./ChatSessionActivity";
import {
  chatPickerTimeLine,
  chatRecentTimeLabel,
  formatAgo,
} from "../../lib/chatSessionActivity";
import { newestOntologyCardMessageId } from "../../lib/ontologyCard";
import { useChatResultAcknowledgement } from "../../hooks/useChatResultAcknowledgement";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/**
 * A send's outcome: `false` when refused; otherwise accepted, optionally
 * naming the chat that accepted it (the chat a first send created). Prompt
 * history records against that chat, or the chat shown when Enter was
 * pressed (chat g5.chat.ui.prompt-history.store).
 */
export type ChatSendResult = boolean | void | { sessionId: string };

export interface ChatPanelSendOptions {
  agentLaunch?: AgentLaunchSelectionV1;
  launchSource?: "inherit_configured" | "canonical_default" | "explicit";
  model?: string;
  permissionMode?: string;
  effort?: string;
  /**
   * An Account the user picked in an active session (routes/chat D33);
   * omitted while the field is untouched, so the chat's saved account
   * applies. A new chat carries its account inside `agentLaunch` instead.
   */
  account?: AgentAccountSelectionV1;
  projectReferences?: ChatProjectReference[];
  /** Uploaded files (e.g. screenshots) sent inline in the send/start request. */
  attachments?: ChatUploadAttachment[];
  /**
   * Start and send (personal_workspace g5.personal.start-and-send): the
   * request also records Start consent, and the input waits for the VM.
   */
  startCompute?: boolean;
}

/**
 * Workbench scope pill (workbench g5.workbench.phases): the workspace's phase
 * in its hue beside the "Chat" label, only under the `alignment` layout.
 */
/**
 * Persona pill (workbench g5.workbench.persona): the session's persona in its
 * hue, or the workspace phase before a session exists. When the workbench
 * phase differs from the session persona, "follow view" offers to fork the
 * chat under the current phase's persona — never re-pointing the open chat.
 */
export interface ChatPersonaScope {
  phase: WorkspacePhase;
  label: string;
  /** What the pill names (`persona` for a session, `phase` otherwise). */
  source?: "persona" | "phase";
  followView?: {
    targetPhase: WorkspacePhase;
    onFollow: () => void;
    pending?: boolean;
  };
}

const PhaseScopePill: React.FC<{
  scope: ChatPersonaScope | undefined;
}> = ({ scope }) => {
  if (!scope) return null;
  const info = WORKBENCH_PHASES[scope.phase];
  const target = scope.followView
    ? WORKBENCH_PHASES[scope.followView.targetPhase]
    : null;
  return (
    <>
      <span
        data-testid="chat-phase-pill"
        data-phase={scope.phase}
        data-source={scope.source ?? "phase"}
        title={
          scope.source === "persona"
            ? `Chat persona: ${info.name}`
            : `Workspace phase: ${info.name}`
        }
        className="inline-flex h-5 shrink-0 items-center rounded-full border px-2 text-[10px] font-semibold uppercase tracking-wide"
        style={{
          color: `var(${info.token})`,
          backgroundColor: `var(${info.softToken})`,
          borderColor: `var(${info.token})`,
        }}
      >
        {scope.label}
      </span>
      {scope.followView && target ? (
        <button
          type="button"
          data-testid="chat-follow-view"
          disabled={scope.followView.pending}
          onClick={scope.followView.onFollow}
          title={`Continue this chat under the ${target.name} persona (forks the conversation)`}
          className="inline-flex h-5 shrink-0 items-center gap-1 rounded-full border border-dashed px-2 text-[10px] font-medium disabled:opacity-60"
          style={{
            color: `var(${target.token})`,
            borderColor: `var(${target.token})`,
          }}
        >
          follow view
          <span className="font-semibold uppercase">{target.short}</span>
        </button>
      ) : null}
    </>
  );
};

/**
 * Unsent composer state an owner keeps across remounts of its ChatPanel
 * (workspaces g5.workspace.ui.chat-corner-button). Mutable by design: the
 * panel writes it as the person types; the owner never needs to re-render.
 */
export interface ChatComposerDraft {
  text: string;
  /** The last composer seed token applied while this draft was mounted. */
  appliedSeedToken: number | null;
  /**
   * Send pulses for a seeded draft until it is sent or emptied (workbench
   * g5.workbench.chat-actions); on the draft so a remount keeps it.
   */
  seedPulse: boolean;
  /**
   * The applied seed's header (ui g5.workbench.review-prompts): shown at
   * the top of the chat for exactly as long as Send pulses.
   */
  seedHeader: ComposerSeedHeaderValue | null;
}

export function createChatComposerDraft(): ChatComposerDraft {
  return {
    text: "",
    appliedSeedToken: null,
    seedPulse: false,
    seedHeader: null,
  };
}

/** One-shot composer seed: the text and, optionally, its chat header. */
export interface ChatComposerSeed {
  token: number;
  text: string;
  header?: ComposerSeedHeaderValue;
}

export interface ChatPanelProps {
  /** Use the wide center-workspace treatment instead of the narrow panel layout. */
  layout?: "panel" | "center";
  /**
   * Workbench scope pill beside the header label (workbench
   * g5.workbench.phases): the workspace's current phase under the `alignment`
   * layout. Absent in every other layout — nothing else in chat changes.
   */
  phaseScope?: ChatPersonaScope;
  /** Current session (null = no session yet, show empty + create prompt) */
  session: ChatSessionDTO | null;
  messages: ChatMessageDTO[];
  /** Whether the assistant is currently streaming a response */
  isStreaming: boolean;
  /** Hide Stop during an accepted turn handoff without changing streaming UI. */
  stopControlSuppressed?: boolean;
  /** The Stop in progress for a turn of this session (chat g5.chat.controls):
   * `Stopping…` while it is requested and signalled, `Finishing…` when the
   * execution had already ended, Retry when the request did not commit. */
  stopState?: ChatStopState | null;
  /** Whether the running turn's unsaved tail is held, still catching up, or
   * absent (chat g5.chat.live-tail-recovery). */
  liveTailState?: LiveTailState;
  /** All timeline entries — API-fetched history + live WS entries, sorted chronologically */
  timelineEntries?: TimelineEntry[];
  /** Verified one-shot Home deep-link target for an exact question card. */
  focusApprovalId?: string | null;
  /** Called only after the matching actionable card is focused. */
  onFocusApprovalConsumed?: (approvalId: string) => void;
  /**
   * Backward timeline pagination (chat_panel_runtime D35, issue #2333). Older
   * timeline history remains beyond the loaded newest-window; when true the
   * panel shows a top-of-stream "Load earlier history" affordance.
   */
  hasEarlierHistory?: boolean;
  /** An older-history page load is in flight (drives the loading affordance). */
  isLoadingEarlierHistory?: boolean;
  /** Load the next older page of timeline history (newest-first backward cursor). */
  onLoadEarlierHistory?: () => void;
  /**
   * What the held timeline covers (g5.chat.history-window). Turns whose
   * activity begins below it show a not-loaded marker instead of saved text;
   * without it every turn renders as complete.
   */
  timelineCoverage?: TimelineCoverage;
  /** The latest older-timeline load failed; markers offer a retry. */
  earlierHistoryError?: Error | null;
  /**
   * Messages-source scrollback (job_chat_draft_hooks D39; chat_panel_runtime
   * D38): older persisted message rows remain below the loaded tail window.
   * The top-of-stream affordance and near-top sentinel fire this source's
   * single-flight loader independently of the timeline source.
   */
  hasOlderMessages?: boolean;
  /** An older-messages page load is in flight. */
  isLoadingOlderMessages?: boolean;
  /** Load the page of messages strictly older than the loaded window. */
  onLoadOlderMessages?: () => void;
  /** The active session's task summary used for the live activity label. */
  activeTaskSummary?: TaskSummary | null;
  /** Scoping — determines what the chat "knows about" */
  scopeType: "workspace" | "node" | "task";
  scopeRef: string;
  scopeLabel: string;
  /** Session list for the picker */
  sessions: ChatSessionDTO[];
  /** Provider selection */
  selectedAgentProvider?: ChatAgentProvider;
  availableAgentProviders?: ChatAgentProvider[];
  /**
   * Provider-accounts kill-switch state (right-panel D29/D30), threaded from
   * WorkspacePage's instance-info read. The
   * Account field renders only while `true`; otherwise no `account`
   * preference is emitted and the resolver applies the effective assignment.
   */
  providerAccountsEnabled?: boolean;
  /** Visible provider accounts (org, workspace, own) for the Account field. */
  providerAccounts?: ProviderAccountDTO[];
  /**
   * Effective per-harness assignment id (`""` = Runtime variables) that seeds
   * the pre-session Account field (agent_interaction_ux D7).
   */
  providerAccountAssignments?: Record<PickerHarness, string>;
  /**
   * Raw credential keys exist for the harness's family (chat
   * g5.chat.account-defaults): only then is "Runtime variables" offered;
   * otherwise an unassigned harness reads "No account". Hosts that load the
   * scoped configs pass it; omitted, the choice stays offered.
   */
  runtimeVariablesAvailable?: Record<PickerHarness, boolean>;
  onSelectAgentProvider?: (provider: ChatAgentProvider) => void;
  /** Preview the exact backend resolution for the unconfirmed chat draft. */
  resolveAgentLaunch?: ResolveAgentLaunchPreferences;
  defaultClaudeModel?: string;
  /**
   * Per-harness remembered picker choices (#1342 plan Story 3): pre-fills
   * model/mode/effort for a NEW chat per harness; active-session hydration
   * (right-panel D30) still wins for existing sessions.
   */
  defaultAgentProviderChoices?: Record<PickerHarness, HarnessChoice>;
  /**
   * Per-user composer usage-chip window setting (agent_interaction_ux D8):
   * `off` hides the read-only subscription usage chip everywhere (active and
   * pre-session); `7d`/`5h` selects which rate-limit window the chip headlines.
   * Defaults to `7d`.
   */
  subscriptionUsageWindow?: SubscriptionUsageWindowSetting;
  /**
   * Pre-session workspace provider-quota snapshot (agent_interaction_ux D8;
   * provider quota snapshot D13), fetched by the parent for the composer's
   * new-chat state. Rendered only when NO session is bound, the confirmed
   * resolve preview's auth mode is Anthropic OAuth, and the window setting is
   * not `off`. Active sessions keep sourcing their snapshot from the session
   * detail payload.
   */
  workspaceSubscriptionUsage?: SubscriptionUsage | null;
  setupError?: string | null;
  composerDisabledReason?: string | null;
  /**
   * The composer offers Start and send instead of Send: the VM is stopped
   * and this workspace starts on a send (g5.personal.start-and-send). The
   * caller then passes no `composerDisabledReason` for the stopped VM.
   */
  startAndSend?: boolean;
  /** Shown above the composer, e.g. the Start and send note and progress. */
  composerNotice?: React.ReactNode;
  /** Current owner authority for durable queued-input decisions. This does
   * not require a running VM because uncertain delivery must be resolved
   * before a continuation can be admitted. */
  canManageSessionInputs?: boolean;
  projectReferences?: ChatProjectReference[];
  activeProjectReference?: ChatProjectReference | null;
  /** One-shot composer seed (chat_panel_runtime D30): when `token` changes to
   * an unapplied value, the composer replaces its draft with `text` and
   * focuses the input. Never sends and never creates a session. */
  composerSeed?: ChatComposerSeed | null;
  /**
   * The person emptied a seeded draft: the seed with this applied token is
   * abandoned (ui g5.workbench.chat-actions), so its owner drops whatever it
   * attached to the seed. A send consumes the seed instead and is not
   * reported here.
   */
  onComposerSeedEmptied?: (token: number) => void;
  /**
   * The empty composer's placeholder, replacing the default; a disabled
   * composer still shows its reason instead.
   */
  composerPlaceholder?: string;
  /** Pending sends belong to this visit; replace it on a reset or layout switch. */
  composerScope?: object;
  /**
   * Owner-held unsent composer text (workspaces
   * g5.workspace.ui.chat-corner-button): Dock and Pop out move the panel
   * between the center and the floating panel, which remounts it, so the
   * draft and the last applied seed live in this object across mounts. The
   * panel reads it on mount and writes it on every change. Omit it for a
   * composer whose draft should start empty on every mount.
   */
  draft?: ChatComposerDraft;
  /**
   * Whose browser-local prompt history this is (chat
   * g5.chat.ui.prompt-history): the panel keeps one history per chat under
   * this person and workspace, records accepted prompts there and recalls
   * them with Up/Down. Omit it to keep per-chat history only for this mount.
   */
  promptHistoryScope?: ChatPromptHistoryScope;
  /**
   * Whether this panel is actually on screen (chat
   * g5.chat.inspection.acknowledge): a hidden center tab keeps the panel
   * mounted, and mounting proves nothing was seen. Defaults to shown.
   */
  surfaceVisible?: boolean;
  /** Record that this person saw the chat's latest finished result. */
  onAcknowledgeResult?: (
    sessionId: string,
    result: ChatSessionResultDTO,
  ) => Promise<unknown> | void;
  /** One-shot focus request (workspaces g5.workspace.ui.chat-corner-button):
   * when it changes to an unapplied value and the composer is enabled, the
   * input takes focus. Never changes the draft. */
  focusRequest?: number;
  /** Actions */
  onSendMessage: (
    content: string,
    options?: ChatPanelSendOptions,
  ) => ChatSendResult | Promise<ChatSendResult>;
  onResolveAgentApproval?: (
    input: AgentApprovalResolutionInput,
  ) => void | Promise<unknown>;
  onSendProviderControl?: (input: {
    sessionId: string;
    action: string;
    payload?: Record<string, unknown>;
  }) => void | Promise<unknown>;
  onStopStreaming?: () => void;
  /**
   * Owner-only End for a live reply's Claude background wait
   * (`g5.conversation.providers.claude-background.end`). Omitted for readers,
   * who see the same waiting panel without the control. A rejected promise
   * re-enables the control; the caller reports the failure.
   */
  onEndBackgroundWait?: (
    sessionId: string,
    wait: { taskId: string; attempt: number },
  ) => Promise<unknown> | void;
  /** Open the workspace's All changes view from an ontology changes card's
   * View all changes in workspace link (chat g5.chat.ontology-card). */
  onOpenOntologyChanges?: () => void;
  /** Open an ontology file at a statement a changes card's statement links
   * to (chat g5.chat.ontology-card). */
  onOpenOntologyStatement?: (
    filePath: string,
    repoId: string | undefined,
    statementId: string,
  ) => void;
  onForkSession?: (atMessageIndex: number) => void;
  onSummarize: () => void;
  onCreateSession?: () => void;
  onSelectSession: (sessionId: string) => void;
  onRenameSession?: (
    sessionId: string,
    title: string | null,
  ) => void | Promise<unknown>;
  onOpenFile?: (
    filePath: string,
    lineRange?: [number, number],
    repoId?: string,
  ) => void;
  /** Detach/popout the chat panel as a floating window */
  onDetach?: () => void;
  /** Close the chat panel */
  onClose?: () => void;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function sessionDisplayTitle(
  session: ChatSessionDTO | null | undefined,
): string {
  const title = session?.title?.trim();
  return title ? title : "New Session";
}

const CENTER_CHAT_STARTERS = [
  {
    label: "Explore the codebase",
    prompt: "Give me a tour of the architecture and where things live.",
  },
  {
    label: "Plan a feature",
    prompt: "Design and scope a new capability before we build it.",
  },
  {
    label: "Debug an issue",
    prompt: "A test is failing — trace it to intent and fix the root cause.",
  },
  {
    label: "Edit the ontology",
    prompt: "Change a spec node and propagate it down to code and tests.",
  },
] as const;

function projectReferenceKindLabel(kind: ChatProjectReference["kind"]): string {
  switch (kind) {
    case "project_doc":
      return "Doc";
    case "view":
      return "View";
    case "source_file":
      return "Source";
  }
}

function projectReferenceSubtitle(reference: ChatProjectReference): string {
  return reference.path ?? reference.docId ?? reference.id;
}

function referenceFallbackContent(references: ChatProjectReference[]): string {
  if (references.length === 1) {
    return `Referenced ${references[0]?.label || "workspace item"}`;
  }
  const names = references.map((reference) => reference.label).join(", ");
  return `Referenced ${names}`;
}

// Uploads are external attachments (chat_panel_runtime I3): the picker accepts
// arbitrary regular files, while clipboard paste remains limited to the safe
// raster types in D37. Both surfaces share validation, encoding, and de-dup.
const SAFE_IMAGE_PREVIEW_TYPES = [
  "image/png",
  "image/jpeg",
  "image/gif",
  "image/webp",
];
const MAX_ATTACHMENT_BYTES = 5 * 1024 * 1024;
// Per-turn guards (chat_panel_runtime I3): a 5 MiB image expands to ~6.7 MiB of
// base64, and the send/start route body limit is 16 MiB. Cap the combined RAW
// bytes with headroom so a batch of large images surfaces a clear inline error
// here instead of an opaque oversized-request failure at the API.
const MAX_TOTAL_ATTACHMENT_BYTES = 10 * 1024 * 1024;
const MAX_ATTACHMENT_COUNT = 8;

/** Exact decoded byte length of a padded raw base64 string. */
function base64ByteLength(base64: string): number {
  const padding = base64.endsWith("==") ? 2 : base64.endsWith("=") ? 1 : 0;
  return Math.floor((base64.length * 3) / 4) - padding;
}

function isSafeImagePreview(contentType?: string): boolean {
  return contentType
    ? SAFE_IMAGE_PREVIEW_TYPES.includes(contentType.toLowerCase())
    : false;
}

function attachmentFallbackContent(
  attachments: ChatUploadAttachment[],
): string {
  if (attachments.length === 1) {
    return `Uploaded ${attachments[0]?.fileName || "a file"}`;
  }
  const names = attachments.map((attachment) => attachment.fileName).join(", ");
  return `Uploaded ${names}`;
}

/**
 * Read a File as RAW base64 (no `data:` URL prefix), matching the locked
 * send/start `attachments[].contentBase64` wire contract. Mirrors the
 * FileReader pattern in CreateProjectDocModal but uses `readAsDataURL` and
 * strips the `data:<type>;base64,` prefix.
 */
async function readFileAsBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const result = String(reader.result ?? "");
      const commaIndex = result.indexOf(",");
      resolve(commaIndex >= 0 ? result.slice(commaIndex + 1) : result);
    };
    reader.onerror = () =>
      reject(reader.error ?? new Error("Failed to read file"));
    reader.readAsDataURL(file);
  });
}

/**
 * Synthetic file name for a pasted image (chat_panel_runtime D37). Clipboard
 * image blobs frequently have no meaningful `name`, so derive a stable one from
 * the (already validated) image content type for the chip and wire entry.
 */
function pastedImageName(contentType: string): string {
  const ext =
    contentType === "image/jpeg"
      ? "jpg"
      : contentType === "image/gif"
        ? "gif"
        : contentType === "image/webp"
          ? "webp"
          : "png";
  return `pasted-image.${ext}`;
}

/**
 * Shared attachment ingestion for the composer (chat_panel_runtime I3, D37).
 * The picker accepts arbitrary regular files; paste callers request the safe
 * raster gate. Both surfaces use the same caps, encoding, and byte de-dup.
 */
type PreparedAttachment =
  | { ok: true; attachment: ChatUploadAttachment }
  | { ok: false; error: string };

async function prepareAttachment(
  file: File,
  existing: ChatUploadAttachment[],
  requireSafeImage: boolean,
): Promise<PreparedAttachment> {
  if (requireSafeImage && !isSafeImagePreview(file.type)) {
    return {
      ok: false,
      error: "Only PNG, JPEG, GIF, or WebP images can be uploaded.",
    };
  }
  const noun = requireSafeImage ? "image" : "file";
  const plural = requireSafeImage ? "images" : "files";
  if (file.size === 0) {
    return { ok: false, error: `That ${noun} is empty.` };
  }
  if (file.size > MAX_ATTACHMENT_BYTES) {
    return {
      ok: false,
      error: `${requireSafeImage ? "Images" : "Files"} must be 5 MiB or smaller.`,
    };
  }
  if (existing.length >= MAX_ATTACHMENT_COUNT) {
    return {
      ok: false,
      error: `You can attach at most ${MAX_ATTACHMENT_COUNT} ${plural} per message.`,
    };
  }
  const currentTotalBytes = existing.reduce(
    (sum, attachment) => sum + base64ByteLength(attachment.contentBase64),
    0,
  );
  if (currentTotalBytes + file.size > MAX_TOTAL_ATTACHMENT_BYTES) {
    return {
      ok: false,
      error: `These ${plural} are too large to send together — remove one and try again.`,
    };
  }
  let contentBase64: string;
  try {
    contentBase64 = await readFileAsBase64(file);
  } catch {
    return { ok: false, error: `Could not read the selected ${noun}.` };
  }
  // De-dupe identical images by content so the agent does not receive the same
  // file twice in one turn (dedupe is by bytes, not name).
  if (
    existing.some((attachment) => attachment.contentBase64 === contentBase64)
  ) {
    return { ok: false, error: `That ${noun} is already attached.` };
  }
  return {
    ok: true,
    attachment: {
      fileName:
        file.name ||
        (requireSafeImage ? pastedImageName(file.type) : "attachment"),
      contentType: file.type || undefined,
      contentBase64,
    },
  };
}

function scopeChipState(
  scopeType: ChatPanelProps["scopeType"],
): "info" | "active" | "warn" {
  switch (scopeType) {
    case "workspace":
      return "info";
    case "node":
      return "active";
    case "task":
      return "warn";
  }
}

export function chatPanelChromeLabel(
  scopeType: ChatPanelProps["scopeType"],
  scopeLabel: string,
  sessionTitle: string | null | undefined,
): string {
  if (scopeType !== "task") return "Chat";
  const taskLabel = `${scopeLabel} ${sessionTitle ?? ""}`.toLowerCase();
  return /\bmerg(?:e|ing)\b/.test(taskLabel) ? "Merge" : "Sync";
}

// #1370 (core D7/D9): last-resort effort tails resolve per model via the
// catalog default choice instead of a flat legacy value.
const DEFAULT_REASONING_EFFORT = catalogDefaultChoiceFor("claude_code").effort;
const MAX_CHAT_REFERENCE_RESULTS = 60;

const CLAUDE_PERMISSION_OPTIONS = [
  {
    value: "default",
    label: "Ask",
    shortLabel: "ask",
    description: "Ask before potentially dangerous tool use.",
  },
  {
    value: "acceptEdits",
    label: "Accept edits",
    shortLabel: "accept edits",
    description: "Auto-accept file edits and common filesystem operations.",
  },
  {
    value: "plan",
    label: "Plan",
    shortLabel: "plan",
    description: "Read-only planning mode.",
  },
  {
    value: "dontAsk",
    label: "Don't ask",
    shortLabel: "don't ask",
    description: "Deny actions unless pre-approved by rules.",
  },
  // The legacy `auto` option is removed (#1370, types/core D15): it was
  // behaviorally identical to bypassPermissions (the sidecar auto-allowed
  // every tool). A legacy persisted `auto` session mode displays through
  // claudePermissionOption's fallback.
  {
    value: "bypassPermissions",
    label: "Bypass",
    shortLabel: "bypass",
    description: "Dangerously skip permissions when Claude Code permits it.",
    risky: true,
  },
];

const CLAUDE_EFFORT_OPTIONS = [
  {
    value: "low",
    label: "Low",
    description: "Faster, lower reasoning budget.",
  },
  {
    value: "medium",
    label: "Medium",
    description: "Balanced reasoning budget.",
  },
  { value: "high", label: "High", description: "Strong reasoning budget." },
  {
    value: "xhigh",
    label: "XHigh",
    description: "Default reasoning effort from Claude Code settings.",
  },
  {
    value: "max",
    label: "Max",
    description: "Maximum available reasoning effort.",
  },
];

// ---------------------------------------------------------------------------
// Codex session controls display native values without changing their policy.
// Retain historical permission labels for saved sessions; the shared catalog
// owns which modes and model-specific efforts are offered for new launches.
// ---------------------------------------------------------------------------

const CODEX_PERMISSION_OPTIONS = [
  {
    value: "never/read-only",
    label: "Read Only",
    shortLabel: "read only",
    description: "Read-only sandbox, no approval prompts.",
  },
  {
    value: "never/workspace-write",
    label: "Workspace Write",
    shortLabel: "workspace write",
    description: "Writes inside the workspace sandbox, no approval prompts.",
  },
  {
    value: "on-request/read-only",
    label: "Read Only (ask)",
    shortLabel: "read only (ask)",
    description: "Reads freely; must ask to edit or run commands.",
  },
  {
    value: "on-failure/workspace-write",
    label: "Auto",
    shortLabel: "auto",
    description:
      "Edits and runs inside the workspace without prompting; asks before escaping the workspace or on failure.",
  },
  {
    value: "never/danger-full-access",
    label: "Full Access",
    shortLabel: "full access",
    description: "No sandbox, no approval prompts.",
    risky: true,
  },
];

const CODEX_EFFORT_OPTIONS = [
  {
    value: "minimal",
    label: "Minimal",
    description: "Lowest reasoning budget.",
  },
  {
    value: "low",
    label: "Low",
    description: "Faster, lower reasoning budget.",
  },
  {
    value: "medium",
    label: "Medium",
    description: "Balanced reasoning budget.",
  },
  { value: "high", label: "High", description: "Strong reasoning budget." },
  {
    value: "xhigh",
    label: launchEffortLabel("xhigh"),
    description: "Maximum native Codex reasoning effort.",
  },
  {
    value: "max",
    label: "Max",
    description: "Extended reasoning on models that offer it.",
  },
  {
    value: "ultra",
    label: "Ultra",
    description: "Deepest reasoning on models that offer it.",
  },
];

const DEFAULT_CODEX_EFFORT = catalogDefaultChoiceFor("codex").effort;

const CODEX_DEFAULT_MODEL = CODEX_CHAT_DEFAULT_MODEL;

const CODEX_MODEL_OPTIONS = [
  { value: CODEX_CHAT_DEFAULT_MODEL, label: CODEX_CHAT_DEFAULT_MODEL_LABEL },
];

function codexPickerModelOptions() {
  return chatModelOptionsForHarness("codex");
}

function codexPermissionOption(value: string) {
  if (!value) return { value, label: "Not set" };
  return (
    CODEX_PERMISSION_OPTIONS.find((option) => option.value === value) ?? {
      value,
      label: value,
    }
  );
}

function codexEffortOption(value: string) {
  return (
    CODEX_EFFORT_OPTIONS.find((option) => option.value === value) ??
    CODEX_EFFORT_OPTIONS.find(
      (option) => option.value === DEFAULT_CODEX_EFFORT,
    ) ??
    CODEX_EFFORT_OPTIONS[0]
  );
}

function codexModelOption(value: string) {
  return (
    codexPickerModelOptions().find((option) => option.value === value) ??
    CODEX_MODEL_OPTIONS.find((option) => option.value === value) ?? {
      value,
      label: value || CODEX_CHAT_DEFAULT_MODEL_LABEL,
    }
  );
}

function agentProviderLabel(provider: ChatAgentProvider): string {
  switch (provider) {
    case "claude_code":
      return "Claude Code";
    case "codex":
      return "Codex";
  }
}

// ---------------------------------------------------------------------------
// Agent Trajectory — Claude Code / Codex–style linear conversation display
// Tool calls, results, and agent text render inline in the message stream.
// ---------------------------------------------------------------------------

/** Known tool categories for icon/display mapping */
function claudePermissionOption(value: string) {
  if (!value) return { value, label: "Not set" };
  // Legacy `auto` (removed, #1370) always behaved as bypassPermissions; a
  // persisted value displays as Bypass rather than the generic fallback.
  const normalized = value === "auto" ? "bypassPermissions" : value;
  return (
    CLAUDE_PERMISSION_OPTIONS.find((option) => option.value === normalized) ?? {
      value,
      label: value,
    }
  );
}

function claudeModelOption(value: string) {
  return (
    CLAUDE_CHAT_MODEL_OPTIONS.find((option) => option.value === value) ?? {
      value,
      // A saved but no longer offered variant (a `[1m]` pick) keeps its
      // catalog label; unknown pins show their raw id.
      label: value
        ? claudeModelLabelFor(value)
        : CLAUDE_CODE_DEFAULT_MODEL_LABEL,
      description: value || "Claude Code configured default.",
    }
  );
}

function claudeEffortOption(value: string) {
  if (value === "none")
    return {
      value,
      label: "Not supported",
      description: "This model has no effort control.",
    };
  return (
    CLAUDE_EFFORT_OPTIONS.find((option) => option.value === value) ??
    CLAUDE_EFFORT_OPTIONS.find((option) => option.value === "xhigh") ??
    CLAUDE_EFFORT_OPTIONS[0]
  );
}

/**
 * Session-detail account hydration precedence (session_provider_controls D30):
 * `nextAccount` is seeded at session start and is authoritative; the frozen
 * `account` and the Runtime-variables sentinel cover legacy sessions only.
 */
function accountValueFromSession(session: ChatSessionDTO): string {
  return (
    accountSelectionValue(session.nextAccount) ??
    session.account?.id ??
    RUNTIME_CONFIG_ACCOUNT_VALUE
  );
}

const PROVIDER_SETTINGS_ACCOUNT_ERRORS: Record<string, string> = {
  account_not_visible:
    "That account is not visible from this chat or no longer exists.",
  account_kind_mismatch: "That account's type does not match this agent.",
};

function providerSettingsErrorMessage(error: unknown): string {
  if (error instanceof ApiError) {
    if (error.status === 422 && error.code === "VALIDATION_ERROR") {
      const token = error.details.code;
      if (
        typeof token === "string" &&
        PROVIDER_SETTINGS_ACCOUNT_ERRORS[token]
      ) {
        return PROVIDER_SETTINGS_ACCOUNT_ERRORS[token];
      }
    }
    return error.message || "Failed to update provider settings.";
  }
  return error instanceof Error && error.message
    ? error.message
    : "Failed to update provider settings.";
}

function claudeSettingsFromSession(session: ChatSessionDTO | null | undefined) {
  const providerState = session?.providerState;
  if (!providerState || typeof providerState !== "object") {
    return {};
  }
  const claudeCode = (providerState as Record<string, unknown>).claudeCode;
  if (!claudeCode || typeof claudeCode !== "object") {
    return {};
  }
  const settings = (claudeCode as Record<string, unknown>).settings;
  if (!settings || typeof settings !== "object") {
    return {};
  }
  const record = settings as Record<string, unknown>;
  const stringSetting = (key: string) => {
    const value = record[key];
    return typeof value === "string" && value.trim() ? value : undefined;
  };
  return {
    model: stringSetting("model"),
    permissionMode:
      stringSetting("permissionMode") === "auto"
        ? "bypassPermissions"
        : stringSetting("permissionMode"),
    effort: stringSetting("effort"),
  };
}

function codexSettingsFromSession(session: ChatSessionDTO | null | undefined) {
  const providerState = session?.providerState;
  if (!providerState || typeof providerState !== "object") {
    return {};
  }
  const codex = (providerState as Record<string, unknown>).codex;
  if (!codex || typeof codex !== "object") {
    return {};
  }
  const settings = (codex as Record<string, unknown>).settings;
  if (!settings || typeof settings !== "object") {
    return {};
  }
  const record = settings as Record<string, unknown>;
  const stringSetting = (key: string) => {
    const value = record[key];
    return typeof value === "string" && value.trim() ? value : undefined;
  };
  return {
    model: stringSetting("model"),
    permissionMode: stringSetting("permissionMode"),
    effort: stringSetting("effort"),
  };
}

function liveTaskBackgroundWait(
  task: TaskSummary | null | undefined,
): BackgroundTaskSet | null {
  if (!task || task.status !== "running") return null;
  return backgroundTaskSetFromPayload(task.backgroundWait, {
    taskId: task.id,
    attempt: task.latestAttempt?.attemptNumber ?? null,
  });
}

/** Wall-clock milliseconds, refreshed every `intervalMs` while mounted. */
/** Rows that only occur while the provider is working inside a turn. */
function isInTurnProviderActivity(entry: TimelineEntry): boolean {
  const phase =
    (entry.metadata?.phase as string | undefined) ?? entry.entryType;
  if (
    phase === "tool_call" ||
    phase === "tool_result" ||
    phase === "thought" ||
    phase === "approval"
  ) {
    return true;
  }
  return (
    phase === "output" &&
    entry.metadata?.providerEvent === "provided_agent.output.delta"
  );
}

function initializingAgentLabel(provider?: ChatAgentProvider | null): string {
  if (provider === "codex") return "Initializing Codex…";
  if (provider === "claude_code") return "Initializing Claude…";
  return "Initializing agent…";
}

const ATTEMPT_FENCED_INFRASTRUCTURE_PHASES = new Set([
  "host_microvm_started",
  "host_microvm_exited",
  "host_bootstrap_exited",
  "host_microvm_ready",
  "preview_attach_started",
  "preview_attach_exited",
]);

type DurableInstant = {
  milliseconds: number;
  submillisecondMicros: number;
};

/** An entry's durable instant, parsed once per entry object: the activity
 * label walks every held entry on each commit. */
const entryInstants = new WeakMap<TimelineEntry, DurableInstant | null>();

function entryDurableInstant(entry: TimelineEntry): DurableInstant | null {
  const cached = entryInstants.get(entry);
  if (cached !== undefined) return cached;
  const instant = parseDurableInstant(entry.createdAt);
  entryInstants.set(entry, instant);
  return instant;
}

function parseDurableInstant(value: string): DurableInstant | null {
  const milliseconds = Date.parse(value);
  if (!Number.isFinite(milliseconds)) return null;
  const fractionalDigits =
    value.match(/\.(\d+)(?:[zZ]|[+-]\d{2}:?\d{2})$/)?.[1] ?? "";
  const micros = Number(fractionalDigits.padEnd(6, "0").slice(0, 6));
  return {
    milliseconds,
    submillisecondMicros: micros % 1_000,
  };
}

function compareDurableInstants(
  left: DurableInstant,
  right: DurableInstant,
): number {
  if (left.milliseconds !== right.milliseconds) {
    return left.milliseconds - right.milliseconds;
  }
  return left.submillisecondMicros - right.submillisecondMicros;
}

/** Derive a live status line from the most recent trace entries.
 *
 * Boot-phase mapping (chat_panel_runtime D24, issues #833/#1050): before the
 * guest agent is live, the authoritative task handoff distinguishes queueing,
 * assignment, recovery, and provider initialization. `host_microvm_started`
 * is provider-neutral telemetry and must use the session's displayed provider
 * rather than hardcoding Claude.
 * Exported for unit tests.
 */
export function currentActivityLabel(
  entries: TimelineEntry[] | undefined,
  provider?: ChatAgentProvider | null,
  activeTask?: Pick<TaskSummary, "status" | "latestAttempt"> | null,
  activeTurn?: Pick<ChatMessageDTO, "id" | "createdAt"> | null,
): string {
  const turnCreatedAt = activeTurn
    ? parseDurableInstant(activeTurn.createdAt)
    : null;
  const currentTurnEntries = activeTurn
    ? (entries ?? []).filter((entry) => {
        const entryCreatedAt = entryDurableInstant(entry);
        const isInsideActiveTurn =
          entryCreatedAt !== null &&
          turnCreatedAt !== null &&
          compareDurableInstants(entryCreatedAt, turnCreatedAt) >= 0;
        const messageId = entry.metadata?.messageId;
        if (typeof messageId === "string" && messageId !== activeTurn.id) {
          return false;
        }
        const phase =
          typeof entry.metadata?.phase === "string"
            ? entry.metadata.phase
            : entry.entryType;
        if (ATTEMPT_FENCED_INFRASTRUCTURE_PHASES.has(phase)) {
          const executionAttemptId = entry.metadata?.executionAttemptId;
          const latestAttempt = activeTask?.latestAttempt;
          if (typeof executionAttemptId === "string") {
            if (!latestAttempt) return isInsideActiveTurn;
            if (latestAttempt.endedAt == null) {
              return executionAttemptId === latestAttempt.id;
            }
            if (executionAttemptId === latestAttempt.id) return false;
            const latestAttemptEndedAt = parseDurableInstant(
              latestAttempt.endedAt,
            );
            return (
              isInsideActiveTurn &&
              entryCreatedAt !== null &&
              latestAttemptEndedAt !== null &&
              compareDurableInstants(entryCreatedAt, latestAttemptEndedAt) >= 0
            );
          }
          // Once the task endpoint exposes attempt authority, timestamp alone
          // cannot distinguish a late host row from the preceding queued turn.
          if (latestAttempt) return false;
        }

        if (typeof messageId === "string") return true;
        return isInsideActiveTurn;
      })
    : (entries ?? []);

  if (currentTurnEntries.length > 0) {
    // REST rows and live WebSocket rows can be merged in arrival order. Seed
    // from the positional tail so all-invalid/equal timestamps preserve the
    // prior stable behavior, but otherwise select the newest durable instant.
    let last = currentTurnEntries[currentTurnEntries.length - 1];
    let lastCreatedAt = entryDurableInstant(last);
    for (const candidate of currentTurnEntries) {
      const candidateCreatedAt = entryDurableInstant(candidate);
      if (!candidateCreatedAt) continue;
      if (
        !lastCreatedAt ||
        compareDurableInstants(candidateCreatedAt, lastCreatedAt) > 0
      ) {
        last = candidate;
        lastCreatedAt = candidateCreatedAt;
      }
    }
    const phase = (last.metadata?.phase as string) ?? last.entryType;
    const toolName = last.metadata?.toolName as string | undefined;
    // Claude reports background work it still owns and nothing newer shows
    // it working in a turn: a background report or a turn end (`usage`,
    // final text) is the newest row, so the live reply is waiting on that
    // work (`g5.conversation.providers.claude-background.visibility`). An
    // empty newest report means no background work remains.
    if (
      !isInTurnProviderActivity(last) &&
      (latestBackgroundTaskSet(currentTurnEntries)?.count ?? 0) > 0
    ) {
      return "Waiting for background work…";
    }
    if (phase === "host_microvm_started")
      return initializingAgentLabel(provider);
    if (phase === "checkpoint") {
      const content = timelineEntryContentText(last);
      if (content.startsWith("Codex started")) return "Initializing Codex…";
      if (content.startsWith("Claude Code started"))
        return "Initializing Claude…";
      return "Thinking…";
    }
    if (phase === "tool_call" && toolName)
      return `Running ${formatToolName(toolName)}…`;
    if (phase === "tool_result") return "Awaiting model response…";
    if (phase === "thought") return "Reasoning…";
    if (phase === "output") return "Generating response…";
    return "Working…";
  }

  if (activeTask?.status === "queued") return "Queued for workspace…";
  if (activeTask?.status === "running") return initializingAgentLabel(provider);
  if (activeTask?.status === "waiting") return "Waiting for input…";
  return activeTask ? "Working…" : "Creating task…";
}

// ---------------------------------------------------------------------------
// Unified Timeline — single append-only chronological stream
// Merges messages and timeline entries by timestamp.
// ---------------------------------------------------------------------------

function isSyncLifecycleReminderTimelineEntry(entry: TimelineEntry): boolean {
  const messageId = entry.metadata?.messageId;
  if (
    typeof messageId === "string" &&
    messageId.endsWith("-sync-lifecycle-reminder")
  ) {
    return true;
  }

  const providerEvent = entry.metadata?.providerEvent;
  return (
    typeof providerEvent === "string" &&
    providerEvent.startsWith("provided_agent.sync_lifecycle_reminder.")
  );
}

function chatTranscriptTimelineEntries(
  entries: TimelineEntry[] | undefined,
): TimelineEntry[] {
  const visible = (entries ?? []).filter(
    (entry) => !isSyncLifecycleReminderTimelineEntry(entry),
  );
  return visible.filter(
    (entry) =>
      !entry.metadata?.outcomeFallback ||
      !visible.some((candidate) => {
        if (
          candidate.metadata?.providerEvent !== "provided_agent.error" ||
          candidate.metadata?.outcomeFallback ||
          !shouldRenderProviderStatus(candidate)
        )
          return false;
        const owner = entry.metadata ?? {};
        const other = candidate.metadata ?? {};
        return (
          (typeof owner.messageId === "string" &&
            owner.messageId === other.messageId) ||
          (typeof owner.taskId === "string" &&
            owner.taskId === other.taskId &&
            typeof owner.attempt === "number" &&
            owner.attempt === other.attempt)
        );
      }),
  );
}

function isProviderThinking(entries: TimelineEntry[] | undefined): boolean {
  if (!entries) return false;

  const thinkingByMessageId = new Map<string, boolean>();
  const sorted = sortedTimelineEntries(entries);
  for (const entry of sorted) {
    const messageId = entry.metadata?.messageId;
    if (typeof messageId !== "string" || messageId.length === 0) continue;
    const providerEvent = entry.metadata?.providerEvent as string | undefined;
    if (providerEvent === "provided_agent.thinking.started") {
      thinkingByMessageId.set(messageId, true);
    } else if (
      providerEvent === "provided_agent.thinking.completed" ||
      providerEvent === "provided_agent.output.final" ||
      providerEvent === "provided_agent.error" ||
      providerEvent === "provided_agent.interrupted"
    ) {
      thinkingByMessageId.set(messageId, false);
    }
  }

  return [...thinkingByMessageId.values()].some(Boolean);
}

function hasProvidedAgentStreamingTurn(
  entries: TimelineEntry[] | undefined,
  activeAssistantMessageId: string | null,
): boolean {
  if (!entries || !activeAssistantMessageId) return false;
  return entries.some((entry) => {
    if (entry.metadata?.messageId !== activeAssistantMessageId) return false;
    const providerEvent = entry.metadata?.providerEvent as string | undefined;
    return providerEvent?.startsWith("provided_agent.") ?? false;
  });
}

function timelineEntryBelongsToSession(
  entry: TimelineEntry,
  sessionId: string | null,
): boolean {
  if (!sessionId) return true;
  const entrySessionId = entry.metadata?.sessionId;
  return typeof entrySessionId !== "string" || entrySessionId === sessionId;
}

interface UnifiedTimelineProps {
  messages: ChatMessageDTO[];
  timelineEntries?: TimelineEntry[];
  agentProvider: ChatAgentProvider | null;
  activeTaskSummary?: TaskSummary | null;
  isStreaming: boolean;
  onCitationClick?: (citation: Citation) => void;
  onForkSession?: (atMessageIndex: number) => void;
  sessionId: string | null;
  onResolveAgentApproval?: ChatPanelProps["onResolveAgentApproval"];
  onEndBackgroundWait?: ChatPanelProps["onEndBackgroundWait"];
  timelineCoverage?: TimelineCoverage;
  earlierActivity?: EarlierActivityControl;
  /** Replaces the derived activity label while a Stop is in progress or the
   * live tail is catching up. */
  activityOverride?: string | null;
  onOpenOntologyChanges?: () => void;
  /** Opens a file an ontology changes card's statement links to. */
  onOpenOntologyFile?: (
    filePath: string,
    repoId?: string,
    line?: number,
    statementId?: string,
  ) => void;
}

const EMPTY_TURN_ENTRIES: TimelineEntry[] = [];
const NOOP_ARTIFACT_ACTION = () => {};

function UnifiedTimeline({
  messages,
  timelineEntries,
  agentProvider,
  activeTaskSummary,
  isStreaming,
  onCitationClick,
  onForkSession,
  sessionId,
  onResolveAgentApproval,
  onEndBackgroundWait,
  timelineCoverage,
  earlierActivity,
  activityOverride = null,
  onOpenOntologyChanges,
  onOpenOntologyFile,
}: UnifiedTimelineProps) {
  // Project provided-agent output to one logical answer per turn BEFORE the
  // Chat builders consume the entries (core_workflow/chat_agent_session#D18).
  // This suppresses the aggregate `provided_agent.output.final` when a same-turn
  // semantic delta exists — grouped by turnId, so it also covers Codex, whose
  // older output rows carried messageId/turnId but no blockId (buildProvidedAgentOutput-
  // Projections keys on blockId and would otherwise leave both rows to render).
  const scopedTimelineEntries = React.useMemo(
    () =>
      projectProvidedAgentOutput(
        chatTranscriptTimelineEntries(timelineEntries).filter((entry) =>
          timelineEntryBelongsToSession(entry, sessionId),
        ),
      ),
    [timelineEntries, sessionId],
  );
  // A turn whose entries did not change keeps its item, entries and
  // approval slice by identity across commits, so the memoized turn below
  // skips it (chat g5.chat.live-tail-recovery).
  const [timelineCache] = React.useState(createUnifiedTimelineCache);
  // Only the newest card is expanded and editable, and only between turns
  // (chat g5.chat.ontology-card.edit).
  const newestCardId = React.useMemo(
    () => newestOntologyCardMessageId(messages),
    [messages],
  );
  // Stable openers keep memoized turns from re-rendering when the parent
  // passes fresh callbacks.
  const ontologyOpenersRef = React.useRef({
    onOpenOntologyChanges,
    onOpenOntologyFile,
  });
  React.useLayoutEffect(() => {
    ontologyOpenersRef.current = { onOpenOntologyChanges, onOpenOntologyFile };
  });
  const openOntologyChanges = React.useCallback(
    () => ontologyOpenersRef.current.onOpenOntologyChanges?.(),
    [],
  );
  const openOntologyFile = React.useCallback(
    (
      ...args: Parameters<
        NonNullable<UnifiedTimelineProps["onOpenOntologyFile"]>
      >
    ) => ontologyOpenersRef.current.onOpenOntologyFile?.(...args),
    [],
  );
  const ontologyTurnRunning =
    isStreaming ||
    ["queued", "running", "waiting"].includes(activeTaskSummary?.status ?? "");
  const items = React.useMemo(
    () => buildUnifiedTimeline(messages, scopedTimelineEntries, timelineCache),
    [messages, scopedTimelineEntries, timelineCache],
  );
  const explainedAuthFailures = React.useMemo(
    () =>
      new Set(
        scopedTimelineEntries.flatMap((entry) => {
          const failure = codexAuthFailureFromEntry(entry);
          return failure ? [codexAuthFailureKey(failure)] : [];
        }),
      ),
    [scopedTimelineEntries],
  );
  // Stale-approval gating is shared with the WorkspacePage composer gate.
  // Legacy requests follow their owning assistant message; native requests
  // follow the exact active task/attempt because that path has no persisted
  // assistant placeholder until the final message.
  const approvalClassification = React.useMemo(
    () =>
      classifyAgentApprovals(
        scopedTimelineEntries,
        messages,
        activeTaskSummary,
      ),
    [scopedTimelineEntries, messages, activeTaskSummary],
  );
  const liveTaskWait = React.useMemo(
    () => liveTaskBackgroundWait(activeTaskSummary),
    [activeTaskSummary],
  );
  // Timed-out AskUserQuestion sub-states (chat_panel_runtime D25): derived
  // from durable timeline evidence only (idle_timeout settlement ± the
  // late_answered marker ± a newer user message/question after the
  // settlement), so the classification survives reload and is shared by both
  // approval render paths. It never feeds composer gating — an idle_timeout
  // settlement already settles its approval above.
  const timedOutQuestionSubStates = React.useMemo(
    () => classifyTimedOutQuestions(scopedTimelineEntries, messages),
    [scopedTimelineEntries, messages],
  );
  const turnApprovals = React.useMemo(
    () =>
      sliceApprovalsPerTurn(
        items,
        timelineCache,
        approvalClassification,
        timedOutQuestionSubStates,
      ),
    [items, timelineCache, approvalClassification, timedOutQuestionSubStates],
  );

  const activeAssistantMessage = messages.find(
    (message) =>
      message.role === "assistant" &&
      (message.status === "pending" || message.status === "streaming"),
  );
  const activityLabel =
    activityOverride ??
    currentActivityLabel(
      scopedTimelineEntries,
      agentProvider,
      activeTaskSummary,
      activeAssistantMessage,
    );
  // Durable transcript anchor for queued detection: the first assistant
  // placeholder still awaiting/producing a turn (status pending/streaming).
  // This is derived from persisted message order, NOT the transient
  // `isStreaming` flag, so queued styling stays stable across the frame where
  // a newly sent follow-up briefly flips `isStreaming` false.
  const firstPendingAssistantIndex = items.findIndex(
    (item) =>
      item.kind === "message" &&
      item.msg.role === "assistant" &&
      (item.msg.status === "pending" || item.msg.status === "streaming"),
  );

  const rendered: React.ReactNode[] = [];
  let trajectoryBatch: TimelineEntry[] = [];
  // First entry id of the accumulating batch — the batch's stable key
  // (chat_panel_runtime D38): position-derived keys remounted every later
  // batch when an earlier one was evicted from the loaded window, destroying
  // the browser's native scroll anchoring.
  let trajectoryBatchFirstEntryId: string | null = null;
  let streamingIndicatorInserted = false;

  function streamingIndicator() {
    return (
      <div className="py-2" data-testid="streaming-indicator">
        <div className="ml-7 flex items-center gap-2 text-[12px] text-[var(--g5-text2)]">
          <span className="relative flex h-2 w-2 shrink-0">
            <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-[var(--g5-active)] opacity-75" />
            <span className="relative inline-flex h-2 w-2 rounded-full bg-[var(--g5-active)]" />
          </span>
          <span className="truncate">{activityLabel}</span>
        </div>
      </div>
    );
  }

  function framedStreamingIndicator(key: string) {
    return (
      <div key={key} className="px-3 py-1">
        {streamingIndicator()}
      </div>
    );
  }

  function flushTrajectory() {
    if (trajectoryBatch.length > 0) {
      rendered.push(
        <div
          key={`traj-${trajectoryBatchFirstEntryId ?? rendered.length}`}
          className="my-1"
        >
          <AssistantTurn
            message={{
              id: `trace-${trajectoryBatchFirstEntryId}`,
              sessionId: sessionId ?? "",
              role: "assistant",
              content: "",
              status: isStreaming ? "streaming" : "complete",
              artifacts: null,
              citations: null,
              inputTokens: null,
              outputTokens: null,
              messageIndex: 0,
              createdAt: "",
            }}
            entries={trajectoryBatch}
            isStreaming={false}
            sessionId={sessionId}
            approvalClassification={approvalClassification}
            timedOutQuestionSubStates={timedOutQuestionSubStates}
            onResolveAgentApproval={onResolveAgentApproval}
          />
        </div>,
      );
      trajectoryBatch = [];
      trajectoryBatchFirstEntryId = null;
    }
  }

  for (let i = 0; i < items.length; i++) {
    const item = items[i];

    if (item.kind === "message") {
      // Flush any accumulated trajectory entries before rendering a message
      flushTrajectory();

      const msg = item.msg;
      // Queued styling comes from durable chat message order (message_rendering
      // I: "queue ordering still comes from the durable chat message order"): a
      // user message that lands after the first still-pending/streaming
      // assistant placeholder is queued behind that turn. It must NOT depend on
      // the transient `isStreaming` flag — gating it there made already-queued
      // follow-ups flash back to the normal (active) user format for the frame
      // where `isStreaming` is momentarily false, then snap back to queued.
      const isQueuedUserMessage =
        firstPendingAssistantIndex >= 0 &&
        i > firstPendingAssistantIndex &&
        msg.role === "user";

      // The live activity indicator is only meaningful while a turn is actively
      // streaming (chat_panel_runtime I: "When `isStreaming` is true, show an
      // animated activity indicator … place this indicator immediately before
      // the first queued user message"), so its insertion stays gated on it.
      if (isQueuedUserMessage && isStreaming && !streamingIndicatorInserted) {
        rendered.push(framedStreamingIndicator("streaming-indicator-queued"));
        streamingIndicatorInserted = true;
      }

      const isLiveStreaming =
        isStreaming &&
        msg.role === "assistant" &&
        (msg.status === "streaming" || msg.status === "pending");

      // Sparse saved messages and every event format share one turn owner.
      const displayMsg = msg;
      if (msg.role === "assistant") {
        const approvals = turnApprovals.get(msg.id);
        rendered.push(
          <AssistantTurn
            key={msg.id}
            message={displayMsg}
            entries={item.entries ?? EMPTY_TURN_ENTRIES}
            activityCoverage={turnActivityCoverage(
              msg.createdAt,
              item.entries?.length ?? 0,
              timelineCoverage,
            )}
            earlierActivity={earlierActivity}
            approvalClassification={approvals?.classification}
            isStreaming={isLiveStreaming}
            sessionId={sessionId}
            timedOutQuestionSubStates={approvals?.timedOut}
            onResolveAgentApproval={onResolveAgentApproval}
            onEndBackgroundWait={onEndBackgroundWait}
            liveTaskWait={liveTaskWait}
            onCitationClick={onCitationClick}
            ontologyCard={{
              newest: msg.id === newestCardId,
              turnRunning: ontologyTurnRunning,
              onOpenAllChanges: onOpenOntologyChanges
                ? openOntologyChanges
                : undefined,
              onOpenFile: onOpenOntologyFile ? openOntologyFile : undefined,
            }}
          />,
        );
      } else {
        rendered.push(
          <ChatMessage
            key={msg.id}
            message={displayMsg}
            isStreaming={isLiveStreaming}
            isQueued={isQueuedUserMessage}
            errorExplained={
              !!msg.codexAuthFailure &&
              explainedAuthFailures.has(
                codexAuthFailureKey(msg.codexAuthFailure),
              )
            }
            onArtifactAction={NOOP_ARTIFACT_ACTION}
            onCitationClick={onCitationClick}
            onFork={onForkSession}
          />,
        );
      }
    } else {
      if (trajectoryBatch.length === 0)
        trajectoryBatchFirstEntryId = item.entry.id;
      trajectoryBatch.push(item.entry);
    }
  }

  flushTrajectory();
  if (isStreaming && !streamingIndicatorInserted) {
    rendered.push(framedStreamingIndicator("streaming-indicator"));
  }

  return <div className="py-2 space-y-0">{rendered}</div>;
}

/**
 * Distance (px) from the top of the message stream within which a near-top
 * scroll auto-loads the next older page from each scrollback source
 * (chat_panel_runtime D35/D38: about 200px, one single-flight load per
 * source per top visit).
 */
const LOAD_EARLIER_SCROLL_THRESHOLD = 200;

function rowBottom(container: HTMLElement, row: HTMLElement): number {
  return (
    row.getBoundingClientRect().bottom - container.getBoundingClientRect().top
  );
}

function transcriptRow(container: HTMLElement, id: string): HTMLElement | null {
  for (const row of container.querySelectorAll<HTMLElement>(
    "[data-message-id]",
  )) {
    if (row.dataset.messageId === id) return row;
  }
  return null;
}

/** The first transcript row still visible at the top of the viewport. */
function readingAnchor(
  container: HTMLElement,
): { id: string; bottom: number } | null {
  const rows = container.querySelectorAll<HTMLElement>("[data-message-id]");
  let low = 0;
  let high = rows.length;
  while (low < high) {
    const middle = (low + high) >> 1;
    if (rowBottom(container, rows[middle]) > 0) high = middle;
    else low = middle + 1;
  }
  const row = rows[low];
  const id = row?.dataset.messageId;
  return row && id ? { id, bottom: rowBottom(container, row) } : null;
}

export const ChatPanel = React.memo(function ChatPanel({
  layout = "panel",
  phaseScope,
  session,
  messages,
  isStreaming,
  stopControlSuppressed = false,
  stopState = null,
  liveTailState = "idle",
  timelineEntries,
  focusApprovalId = null,
  onFocusApprovalConsumed,
  hasEarlierHistory = false,
  isLoadingEarlierHistory = false,
  onLoadEarlierHistory,
  timelineCoverage,
  earlierHistoryError = null,
  hasOlderMessages = false,
  isLoadingOlderMessages = false,
  onLoadOlderMessages,
  activeTaskSummary = null,
  scopeType,
  scopeRef,
  scopeLabel,
  sessions,
  selectedAgentProvider = "claude_code",
  defaultClaudeModel,
  defaultAgentProviderChoices,
  subscriptionUsageWindow = "7d",
  workspaceSubscriptionUsage = null,
  availableAgentProviders = ["claude_code"],
  providerAccountsEnabled = false,
  providerAccounts,
  providerAccountAssignments,
  runtimeVariablesAvailable,
  onSelectAgentProvider,
  resolveAgentLaunch,
  setupError,
  composerDisabledReason = null,
  startAndSend = false,
  composerNotice = null,
  canManageSessionInputs,
  projectReferences = [],
  activeProjectReference = null,
  composerSeed = null,
  onComposerSeedEmptied,
  composerPlaceholder,
  composerScope,
  draft,
  promptHistoryScope,
  surfaceVisible = true,
  onAcknowledgeResult,
  focusRequest = 0,
  onSendMessage,
  onResolveAgentApproval,
  onSendProviderControl,
  onStopStreaming,
  onEndBackgroundWait,
  onOpenOntologyChanges,
  onOpenOntologyStatement,
  onForkSession,
  onSummarize,
  onCreateSession,
  onSelectSession,
  onRenameSession,
  onOpenFile,
  onDetach,
  onClose,
}: ChatPanelProps) {
  const consumedFocusApprovalRef = useRef<string | null>(null);
  useEffect(() => {
    if (focusApprovalId !== consumedFocusApprovalRef.current) {
      consumedFocusApprovalRef.current = null;
    }
  }, [focusApprovalId]);
  const questionCardFocus = useMemo<QuestionCardFocusContextValue>(
    () => ({
      approvalId: focusApprovalId,
      claim: (approvalId) => {
        if (
          focusApprovalId !== approvalId ||
          consumedFocusApprovalRef.current === approvalId
        ) {
          return false;
        }
        consumedFocusApprovalRef.current = approvalId;
        onFocusApprovalConsumed?.(approvalId);
        return true;
      },
    }),
    [focusApprovalId, onFocusApprovalConsumed],
  );
  // -------------------------------------------------------------------------
  // State
  // -------------------------------------------------------------------------
  // `defaultClaudeModel` wins when supplied — WorkspacePage derives it from the
  // same `claude_code` entry through the D-11 interim launcher read — but fall
  // back to the choices map so a caller passing only the map still pre-fills
  // Claude's model, matching how the Codex model seeds below.
  const resolvedDefaultClaudeModel = normalizeClaudeChatModel(
    defaultClaudeModel ?? defaultAgentProviderChoices?.claude_code.model,
  );
  // The draft outlives a remount of this panel (Dock, Pop out) through the
  // owner's `draft` object, or stays private to this mount without one;
  // references and attachments do not, as the session-change effect below
  // clears them on mount.
  const draftRef = useRef(draft ?? createChatComposerDraft());
  const [inputValue, setInputValue] = useState(() => draftRef.current.text);
  useEffect(() => {
    draftRef.current.text = inputValue;
  }, [inputValue]);
  const [claudeModel, setClaudeModel] = useState(resolvedDefaultClaudeModel);
  // New-chat pre-fill (#1342 plan Story 3, Task 3.3): each harness's
  // effort (and the Codex model) seed from its remembered
  // defaultAgentProviderChoices entry, falling back to the legacy defaults
  // when the map is absent. Active-session hydration (right-panel D30)
  // overwrites these below.
  const [claudeEffort, setClaudeEffort] = useState(
    defaultAgentProviderChoices?.claude_code.effort ?? DEFAULT_REASONING_EFFORT,
  );
  // #1370: ONE shared settings-popup open state so a harness switch inside
  // the popup re-renders it for the new harness instead of unmounting it with
  // the per-provider block (outside-click and Escape dismissal unchanged).
  const [providerSettingsOpen, setProviderSettingsOpen] = useState(false);
  const [providerSettingsSaving, setProviderSettingsSaving] = useState(false);
  const [providerSettingsError, setProviderSettingsError] = useState<
    string | null
  >(null);
  const [launchSource, setLaunchSource] = useState<
    "inherit_configured" | "canonical_default" | "explicit"
  >("explicit");
  const [draftAgentProvider, setDraftAgentProvider] = useState(
    selectedAgentProvider,
  );
  // Account field value (session_provider_controls D30): an account id or the
  // Runtime-variables sentinel. Pre-session it follows the effective
  // per-harness assignment; active sessions hydrate from session detail.
  const [accountValue, setAccountValue] = useState<string>(
    providerAccountAssignments?.[
      selectedAgentProvider === "codex" ? "codex" : "claude_code"
    ] || RUNTIME_CONFIG_ACCOUNT_VALUE,
  );
  const [launchResolutionPending, setLaunchResolutionPending] = useState(
    Boolean(resolveAgentLaunch),
  );
  const [launchResolutionError, setLaunchResolutionError] = useState<
    string | null
  >(null);
  const [claudeModelAdjustment, setClaudeModelAdjustment] = useState<{
    from: string;
    to: string;
  } | null>(null);
  const [confirmedAgentLaunch, setConfirmedAgentLaunch] =
    useState<AgentLaunchSelectionV1 | null>(null);
  // Auth mode of the latest confirmed pre-session launch resolution
  // (agent_interaction_ux D8): gates the pre-session usage chip on the exact
  // auth mode the launch-resolution state already holds — no new resolution
  // logic. Cleared while a fresh draft is resolving and on any failure.
  const [confirmedResolvedAuthMode, setConfirmedResolvedAuthMode] =
    useState<AgentAuthModeV1 | null>(null);
  const [codexModel, setCodexModel] = useState(
    defaultAgentProviderChoices?.codex.model ?? CODEX_DEFAULT_MODEL,
  );
  const [codexEffort, setCodexEffort] = useState(
    defaultAgentProviderChoices?.codex.effort ?? DEFAULT_CODEX_EFFORT,
  );
  const [isAtBottom, setIsAtBottom] = useState(true);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [historySearch, setHistorySearch] = useState("");
  const [nowMs, setNowMs] = useState(() => Date.now());
  const [headerRenameOpen, setHeaderRenameOpen] = useState(false);
  const [pendingReferences, setPendingReferences] = useState<
    ChatProjectReference[]
  >([]);
  const [pendingAttachments, setPendingAttachments] = useState<
    ChatUploadAttachment[]
  >([]);
  const pendingAttachmentsRef = useRef<ChatUploadAttachment[]>([]);
  const attachmentSessionGenerationRef = useRef(0);
  const composerSendGenerationRef = useRef(0);
  const [attachmentError, setAttachmentError] = useState<string | null>(null);
  // True while a submit's onSendMessage promise is in flight. Folds into
  // canSend so a second Enter cannot re-submit the identical turn before the
  // first request resolves (chat_panel_runtime D32). Scoped to the single send
  // request only — NOT the streaming gate — so follow-up queuing still works.
  const [isSending, setIsSending] = useState(false);
  const [referencePickerOpen, setReferencePickerOpen] = useState(false);
  const [referenceSearch, setReferenceSearch] = useState("");
  const messageListRef = useRef<HTMLDivElement>(null);
  const shouldStickToBottomRef = useRef(true);
  const scrollRafRef = useRef<number | null>(null);
  // Scrollback loading (chat_panel_runtime D35/D38, issues #2333 and chat
  // d45027de). Mirror both sources' pagination props into refs so the stable
  // `handleScroll` callback can read the latest values without being
  // re-created (and re-attached) each render. `topEdgeAnchorRef` is the
  // continuous top-edge baseline (previous scrollHeight + first rendered item
  // identities) the viewport-retention layout effect compares against, and
  // `timelineLoadArmedRef`/`messagesLoadArmedRef` fire each source's near-top
  // auto-load once per top visit — the arm is PER SOURCE (PR #2435 CodeRabbit
  // round 2), so a source skipped while ineligible keeps its arm.
  const hasEarlierHistoryRef = useRef(hasEarlierHistory);
  hasEarlierHistoryRef.current = hasEarlierHistory;
  const isLoadingEarlierHistoryRef = useRef(isLoadingEarlierHistory);
  isLoadingEarlierHistoryRef.current = isLoadingEarlierHistory;
  const onLoadEarlierHistoryRef = useRef(onLoadEarlierHistory);
  onLoadEarlierHistoryRef.current = onLoadEarlierHistory;
  const hasOlderMessagesRef = useRef(hasOlderMessages);
  hasOlderMessagesRef.current = hasOlderMessages;
  const isLoadingOlderMessagesRef = useRef(isLoadingOlderMessages);
  isLoadingOlderMessagesRef.current = isLoadingOlderMessages;
  const onLoadOlderMessagesRef = useRef(onLoadOlderMessages);
  onLoadOlderMessagesRef.current = onLoadOlderMessages;
  const topEdgeAnchorRef = useRef<{
    scrollHeight: number;
    firstMessageId: string | null;
    firstEntryId: string | null;
  } | null>(null);
  // The transcript row at the top of the viewport and its bottom edge's
  // offset from the viewport top, refreshed on every scroll and commit.
  // Older history grows a turn only at its head (g5.chat.history-window), so
  // holding that edge still keeps the visible rows in place wherever the
  // growth lands: above the viewport, inside the top row, or below it.
  const readingAnchorRef = useRef<{ id: string; bottom: number } | null>(null);
  const timelineLoadArmedRef = useRef(true);
  const messagesLoadArmedRef = useRef(true);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  // Prompt history (chat g5.chat.ui.prompt-history). A cursor means Up/Down
  // browsing is active. Editing or other caret navigation leaves this mode;
  // only an empty composer can enter it again.
  const promptHistoryCursorRef = useRef<PromptHistoryCursor | null>(null);
  // Per-chat history for a panel without a stored scope, for this mount.
  const mountPromptHistoryRef = useRef(new Map<string, string[]>());
  const pendingHistoryCaretRef = useRef(false);
  const [historyCaretSerial, setHistoryCaretSerial] = useState(0);
  // History belongs to one chat; a new chat has none until its first send
  // creates it.
  const promptHistoryChatId = session?.id ?? null;
  const promptHistoryKey =
    promptHistoryScope && promptHistoryChatId
      ? chatPromptHistoryKey(
          promptHistoryScope.userId,
          promptHistoryScope.workspaceId,
          promptHistoryChatId,
        )
      : null;
  const promptHistoryIdentity = `${promptHistoryScope?.userId ?? ""}:${
    promptHistoryScope?.workspaceId ?? ""
  }:${promptHistoryChatId ?? ""}`;
  // The header editor renames the chat it opened on.
  useEffect(() => {
    setHeaderRenameOpen(false);
  }, [session?.id]);
  // Changing chat (or person or workspace) ends browsing, and an unedited
  // recalled prompt does not carry into the other chat: the composer returns
  // to the draft browsing began from. Edited text is an ordinary draft.
  useEffect(() => {
    const cursor = promptHistoryCursorRef.current;
    promptHistoryCursorRef.current = null;
    pendingHistoryCaretRef.current = false;
    if (cursor && draftRef.current.text === cursor.recalled) {
      setInputValue(cursor.draft);
    }
  }, [promptHistoryIdentity]);
  // One-shot composer seed (chat_panel_runtime D30): apply each seed token
  // exactly once so re-renders never clobber user edits made afterwards.
  // The applied token lives on the draft, so a remount (Dock, Pop out)
  // never re-applies a seed over what was typed after it.
  // A seeded composer pulses Send until the draft is sent or emptied
  // (workbench g5.workbench.chat-actions); edits keep the pulse. The flag
  // lives on the draft, so a remount (the fresh composer, Dock, Pop out)
  // keeps pulsing until the send.
  const [seedPulse, setSeedPulseState] = useState(
    () => draftRef.current.seedPulse ?? false,
  );
  const [seedHeader, setSeedHeaderState] = useState(
    () => draftRef.current.seedHeader ?? null,
  );
  const setSeedPulse = useCallback((pulse: boolean) => {
    draftRef.current.seedPulse = pulse;
    setSeedPulseState(pulse);
    // The header lasts exactly as long as the pulse: a send or an emptied
    // draft drops both.
    if (!pulse) {
      draftRef.current.seedHeader = null;
      setSeedHeaderState(null);
    }
  }, []);
  const setSeedHeader = useCallback(
    (header: ComposerSeedHeaderValue | null) => {
      draftRef.current.seedHeader = header;
      setSeedHeaderState(header);
    },
    [],
  );
  // Put a saved-but-undelivered question answer into the composer so the owner
  // sends it as an ordinary message (`g5.chat.native-question-delivery-state`).
  // Nothing is sent automatically and no history is replayed. Only a repeat of
  // the same resend is a no-op: the payload and the exact composer value it
  // produced are remembered, so a different question's answer is still
  // appended, and nothing is matched by substring. Owner edits do not clear
  // the memory; retyping that exact value no-ops, and the composer already
  // holds the answer.
  const lastResendSeedRef = useRef<{ text: string; value: string } | null>(
    null,
  );
  const resendQuestionAnswer = useCallback(
    (text: string) => {
      setInputValue((current) => {
        const last = lastResendSeedRef.current;
        if (last && last.text === text && current === last.value)
          return current;
        // A draft already typed stays first; the saved answer follows it.
        const next = current.trim() ? `${current.trimEnd()}\n\n${text}` : text;
        lastResendSeedRef.current = { text, value: next };
        return next;
      });
      setSeedPulse(true);
      requestAnimationFrame(() => textareaRef.current?.focus());
    },
    [setSeedPulse],
  );
  useEffect(() => {
    if (!composerSeed) return;
    if (draftRef.current.appliedSeedToken === composerSeed.token) return;
    draftRef.current.appliedSeedToken = composerSeed.token;
    setInputValue(composerSeed.text);
    const pulse = composerSeed.text.trim().length > 0;
    setSeedPulse(pulse);
    setSeedHeader(pulse ? (composerSeed.header ?? null) : null);
    requestAnimationFrame(() => textareaRef.current?.focus());
  }, [composerSeed, setSeedHeader, setSeedPulse]);
  // The loop option rewrites only the count in the draft, keeping the
  // person's other edits (g5.workbench.review-prompts).
  const handleSeedIterationsChange = useCallback(
    (iterations: number) => {
      setInputValue((current) => withReviewIterations(current, iterations));
      setSeedHeader({ kind: "review-loop", iterations });
    },
    [setSeedHeader],
  );
  const seedHeaderBlock = seedHeader ? (
    <ComposerSeedHeader
      header={seedHeader}
      onIterationsChange={handleSeedIterationsChange}
    />
  ) : null;
  const attachmentInputRef = useRef<HTMLInputElement>(null);
  const referencePickerRef = useRef<HTMLDivElement>(null);
  const historyRef = useRef<HTMLDivElement>(null);
  const claudeSettingsRef = useRef<HTMLDivElement>(null);
  const codexSettingsRef = useRef<HTMLDivElement>(null);
  const claudeSettingsTriggerRef = useRef<HTMLButtonElement>(null);
  const codexSettingsTriggerRef = useRef<HTMLButtonElement>(null);
  const providerSettingsPopoverRef = useRef<HTMLDivElement>(null);
  const providerSettingsMutationGenerationRef = useRef(0);
  const [providerSettingsPosition, setProviderSettingsPosition] = useState<{
    bottom: number;
    right: number;
    width: number;
  } | null>(null);
  const confirmedAgentLaunchRef = useRef<AgentLaunchSelectionV1 | null>(null);

  function replacePendingAttachments(attachments: ChatUploadAttachment[]) {
    pendingAttachmentsRef.current = attachments;
    setPendingAttachments(attachments);
  }
  const activeSessionId = session?.id ?? null;
  // Acknowledge the chat's latest finished result only once it is actually
  // displayed (chat g5.chat.inspection.acknowledge).
  useChatResultAcknowledgement({
    sessionId: activeSessionId,
    activity:
      session?.activity ??
      sessions.find((listed) => listed.id === activeSessionId)?.activity,
    messages,
    messagesReady: messages.some(
      (message) => message.sessionId === activeSessionId,
    ),
    surfaceVisible,
    atBottom: isAtBottom,
    onAcknowledge: onAcknowledgeResult,
  });
  const [codexAuthLayout, setCodexAuthLayout] = useState({
    sessionId: null as string | null,
    visible: false,
    revision: 0,
  });
  const handleCodexAuthLayoutChange = useCallback(
    (visible: boolean) => {
      setCodexAuthLayout((previous) => ({
        sessionId: activeSessionId,
        visible,
        revision: previous.revision + 1,
      }));
    },
    [activeSessionId],
  );
  const sessionInputsState = useSessionInputs(activeSessionId ?? "", messages);
  const editSessionInput = useEditSessionInput();
  const withdrawSessionInput = useWithdrawSessionInput();
  const cancelUnknownSessionInput = useCancelUnknownSessionInput();
  const redeliverSessionInput = useRedeliverSessionInput();
  const [editingInputId, setEditingInputId] = useState<string | null>(null);
  const [editingInputValue, setEditingInputValue] = useState("");
  const [inputMutationError, setInputMutationError] = useState<string | null>(
    null,
  );
  const visibleSessionInputs = useMemo(
    () =>
      sessionInputsState.status === "ok"
        ? visibleChatQueue(sessionInputsState.data, messages)
        : [],
    [sessionInputsState, messages],
  );
  useEffect(() => {
    setEditingInputId(null);
    setEditingInputValue("");
    setInputMutationError(null);
  }, [activeSessionId]);
  const activeProviderSettingsSessionIdRef = useRef(activeSessionId);
  // Keep this UNPROJECTED. `panelTimelineEntries` feeds `isProviderThinking`
  // (which clears the "Thinking…" indicator on `provided_agent.output.final`)
  // and the auto-scroll effect — not the answer transcript. The transcript is
  // rendered by `UnifiedTimeline`, which applies `projectProvidedAgentOutput`
  // to its own `scopedTimelineEntries`, so the one-logical-answer projection
  // (core_workflow/chat_agent_session#D18) still holds there. Projecting here
  // would drop the `output.final` that a delta-bearing turn relies on to clear
  // thinking and leave the indicator stuck on (issue #1313 review).
  const panelTimelineEntries = React.useMemo(
    () =>
      chatTranscriptTimelineEntries(
        reconcileCodexAuthFailures(
          messages.filter((message) => message.sessionId === activeSessionId),
          (timelineEntries ?? []).filter((entry) =>
            timelineEntryBelongsToSession(entry, activeSessionId),
          ),
        ),
      ),
    [messages, timelineEntries, activeSessionId],
  );
  // Stop and live-tail presentation for the session's active turn
  // (chat g5.chat.controls, g5.chat.live-tail-recovery).
  const sessionStopState =
    stopState && stopState.sessionId === activeSessionId ? stopState : null;
  const stopRequesting = sessionStopState?.phase === "requesting";
  const turnActivityOverride =
    chatStopActivityLabel(sessionStopState) ??
    (liveTailState === "catching_up" ? "Catching up…" : null);
  const displayedAgentProvider = session?.agentProvider ?? draftAgentProvider;
  useEffect(() => {
    if (!session) setDraftAgentProvider(selectedAgentProvider);
  }, [selectedAgentProvider, session]);
  const openProviderSettings = useCallback(() => {
    setProviderSettingsOpen(true);
  }, []);
  const closeProviderSettings = useCallback(() => {
    setProviderSettingsOpen(false);
  }, []);
  const acceptResolvedAgentLaunch = useCallback(
    (selection: AgentLaunchSelectionV1 | null) => {
      if (!selection) return;
      confirmedAgentLaunchRef.current = selection;
      setConfirmedAgentLaunch(selection);
    },
    [],
  );
  // Track the resolved auth mode of the pre-session draft (agent_interaction_ux
  // D8). The resolution monitor clears it to null while resolving and on
  // failure, and reports the resolved execution on success; we keep only its
  // auth mode to gate the pre-session usage chip.
  const handleResolvedExecutionChange = useCallback(
    (resolution: ResolvedAgentExecutionV1 | null) => {
      setConfirmedResolvedAuthMode(resolution?.authMode ?? null);
    },
    [],
  );
  const restoreConfirmedAgentLaunch = useCallback(
    (preferences: AgentLaunchPreferencesV1) => {
      if (
        preferences.provider.source !== "explicit" ||
        preferences.model.source !== "explicit" ||
        preferences.permissionMode.source !== "explicit" ||
        preferences.effort.source !== "explicit"
      ) {
        setLaunchSource(preferences.model.source);
        return;
      }
      const provider = preferences.provider.value;
      setLaunchSource("explicit");
      setDraftAgentProvider(provider);
      if (preferences.account?.source === "explicit") {
        setAccountValue(
          accountSelectionValue(preferences.account.value) ??
            RUNTIME_CONFIG_ACCOUNT_VALUE,
        );
      }
      if (provider === "codex") {
        setCodexModel(preferences.model.value);
        setCodexEffort(preferences.effort.value);
      } else {
        setClaudeModel(preferences.model.value);
        setClaudeEffort(preferences.effort.value);
      }
      onSelectAgentProvider?.(provider);
    },
    [onSelectAgentProvider],
  );
  const handleLaunchResolutionError = useCallback(
    (message: string) => {
      setLaunchResolutionError(message);
      setClaudeModelAdjustment(null);
      const confirmed = confirmedAgentLaunchRef.current;
      if (confirmed) restoreConfirmedAgentLaunch(confirmed.preferences);
    },
    [restoreConfirmedAgentLaunch],
  );
  const persistActiveProviderSettings = useCallback(
    async (
      payload: ProviderControlSetSettingsPayload,
      restorePreviousValues: () => void,
    ) => {
      if (!session || !onSendProviderControl) {
        restorePreviousValues();
        setProviderSettingsError("Provider settings are unavailable.");
        return;
      }
      const initiatingSessionId = session.id;
      const mutationGeneration =
        ++providerSettingsMutationGenerationRef.current;
      const mutationIsCurrent = () =>
        activeProviderSettingsSessionIdRef.current === initiatingSessionId &&
        providerSettingsMutationGenerationRef.current === mutationGeneration;
      setProviderSettingsSaving(true);
      setProviderSettingsError(null);
      try {
        await onSendProviderControl({
          sessionId: session.id,
          action: "set_settings",
          payload,
        });
      } catch (error) {
        if (mutationIsCurrent()) {
          restorePreviousValues();
          setProviderSettingsError(providerSettingsErrorMessage(error));
        }
      } finally {
        if (mutationIsCurrent()) setProviderSettingsSaving(false);
      }
    },
    [onSendProviderControl, session],
  );
  const dismissProviderSettings = closeProviderSettings;
  // Latest user message send time (chat_panel_runtime D31): the
  // background-agents card scopes its Agent-spawn rows to the current
  // fan-out — spawns older than this belong to a previous turn.
  const latestUserMessageCreatedAt = React.useMemo(() => {
    for (let i = messages.length - 1; i >= 0; i--) {
      if (messages[i].role === "user") return messages[i].createdAt ?? null;
    }
    return null;
  }, [messages]);
  const activeAssistantMessageId =
    messages
      .slice()
      .reverse()
      .find(
        (message) =>
          message.role === "assistant" &&
          (message.status === "pending" || message.status === "streaming"),
      )?.id ?? null;
  const providerThinking = React.useMemo(
    () => isStreaming && isProviderThinking(panelTimelineEntries),
    [isStreaming, panelTimelineEntries],
  );
  // The server advertises native steering only for an eligible Claude attempt.
  const providedAgentTurnLive = React.useMemo(
    () =>
      isStreaming &&
      hasProvidedAgentStreamingTurn(
        panelTimelineEntries,
        activeAssistantMessageId,
      ),
    [isStreaming, panelTimelineEntries, activeAssistantMessageId],
  );
  const backgroundWaitShowing = React.useMemo(() => {
    if (!providedAgentTurnLive || !activeAssistantMessageId) return false;
    const activeAssistantMessage = messages.find(
      (message) => message.id === activeAssistantMessageId,
    );
    return (
      activeAssistantMessage !== undefined &&
      liveBackgroundWait(
        activeAssistantMessage,
        panelTimelineEntries,
        liveTaskBackgroundWait(activeTaskSummary),
      ) !== null
    );
  }, [
    activeAssistantMessageId,
    activeTaskSummary,
    messages,
    panelTimelineEntries,
    providedAgentTurnLive,
  ]);
  const effectiveComposerDisabledReason = composerDisabledReason;
  const activePlaceholder =
    effectiveComposerDisabledReason && !providedAgentTurnLive
      ? effectiveComposerDisabledReason
      : providedAgentTurnLive
        ? backgroundWaitShowing
          ? "Your message will be sent after the background work finishes."
          : activeTaskSummary?.nativeSteeringAvailable &&
              displayedAgentProvider === "claude_code" &&
              pendingAttachments.length === 0 &&
              !visibleSessionInputs.some((input) =>
                ["queued", "reserved", "delivery_unknown"].includes(
                  input.delivery,
                ),
              )
            ? "Claude reads this after its current step."
            : "Messages you send now are queued until this reply finishes."
        : isStreaming
          ? "Send a follow-up…"
          : "Ask about this codebase…";
  const claudeSessionActive = displayedAgentProvider === "claude_code";
  const claudeOpenRouterAccount =
    providerAccountsEnabled &&
    providerAccounts?.some(
      (account) =>
        account.id === accountValue && account.kind === "openrouter_api_key",
    );
  const requestedClaudeModel = normalizeClaudeChatModel(claudeModel);
  // Also cover remembered defaults and account hydration: never preview or
  // send a direct-only variant for a known OpenRouter account.
  const effectiveClaudeModel = claudeOpenRouterAccount
    ? claudeChatModelForOpenRouter(requestedClaudeModel)
    : requestedClaudeModel;
  const displayedClaudeModelAdjustment =
    effectiveClaudeModel !== requestedClaudeModel
      ? { from: requestedClaudeModel, to: effectiveClaudeModel }
      : claudeModelAdjustment?.to === effectiveClaudeModel
        ? claudeModelAdjustment
        : null;
  const claudeLaunchMode = launchModeForHarness("claude_code");
  // Existing chat modes come from the server's first-launch policy. Do not
  // invent a mode from remembered defaults while its settings are missing;
  // omit it on send so the server uses the frozen policy.
  const effectiveClaudePermissionMode = session
    ? (claudeSettingsFromSession(session).permissionMode ?? "")
    : claudeLaunchMode.mode;
  // Older Haiku sessions can carry the former, ineffective High setting.
  // New inputs express the absence of this control without rewriting history.
  const effectiveClaudeEffort =
    clampEffortForModel("claude_code", effectiveClaudeModel, null) === "none"
      ? "none"
      : session
        ? claudeEffort
        : clampEffortForModel(
            "claude_code",
            effectiveClaudeModel,
            claudeEffort,
          );
  const claudeTurnOptions = claudeSessionActive
    ? {
        model: effectiveClaudeModel,
        permissionMode: effectiveClaudePermissionMode || undefined,
        effort: effectiveClaudeEffort || undefined,
      }
    : undefined;
  const selectedClaudePermission = claudePermissionOption(
    effectiveClaudePermissionMode,
  );
  const selectedClaudeModel = claudeModelOption(effectiveClaudeModel);
  const selectedClaudeEffort = claudeEffortOption(effectiveClaudeEffort);
  // Codex-native turn options (codex_support D8/I12/I13/D5). Exactly one
  // provider's controls render per session because the provider is start-locked.
  const codexSessionActive = displayedAgentProvider === "codex";
  const codexLaunchMode = launchModeForHarness("codex");
  const effectiveCodexPermissionMode = session
    ? (codexSettingsFromSession(session).permissionMode ?? "")
    : codexLaunchMode.mode;
  const effectiveCodexEffort = session
    ? codexEffort
    : clampEffortForModel("codex", codexModel, codexEffort);
  const codexTurnOptions = codexSessionActive
    ? {
        model: codexModel.trim() || undefined,
        permissionMode: effectiveCodexPermissionMode || undefined,
        effort: effectiveCodexEffort || undefined,
      }
    : undefined;
  const selectedCodexPermission = codexPermissionOption(
    effectiveCodexPermissionMode,
  );
  const selectedCodexModel = codexModelOption(codexModel);
  const selectedCodexEffort = codexEffortOption(effectiveCodexEffort);
  // Account field (session_provider_controls D30): shown only while the
  // provider-accounts switch is on; once shown its value is always submitted
  // explicitly so the resolver validates the exact visible combination.
  const accountFieldVisible = providerAccountsEnabled;
  const displayedHarness: PickerHarness =
    displayedAgentProvider === "codex" ? "codex" : "claude_code";
  const assignedAccountValue =
    providerAccountAssignments?.[displayedHarness] ||
    RUNTIME_CONFIG_ACCOUNT_VALUE;
  const harnessAccounts = useMemo(
    () =>
      (providerAccounts ?? []).filter((account) =>
        accountMatchesHarness(account, displayedAgentProvider),
      ),
    [providerAccounts, displayedAgentProvider],
  );
  const accountOptions = useMemo(() => {
    if (!accountFieldVisible) return undefined;
    const unassigned = unassignedAccountOption(
      runtimeVariablesAvailable?.[displayedHarness] ?? true,
      accountValue,
    );
    const options = [
      ...(unassigned ? [unassigned] : []),
      ...harnessAccounts.map((account) => ({
        value: account.id,
        label: account.name,
        assistiveDescription: `${providerAccountKindLabel(account.kind)} · ${providerAccountScopeLabel(account.scope)}`,
      })),
    ];
    if (
      accountValue !== RUNTIME_CONFIG_ACCOUNT_VALUE &&
      !harnessAccounts.some((account) => account.id === accountValue)
    ) {
      options.push({
        value: accountValue,
        label: `Unavailable (${accountValue.slice(0, 8)})`,
      });
    }
    return options;
  }, [
    accountFieldVisible,
    harnessAccounts,
    accountValue,
    runtimeVariablesAvailable,
    displayedHarness,
  ]);
  const explicitAccountSelection = accountFieldVisible
    ? accountSelectionFromValue(accountValue)
    : undefined;
  // Composer pill harness segment (session_provider_controls D30, chat
  // intent 252e7313): a visible selected account replaces the harness
  // label so the pill names the account the next turn will use; Runtime
  // variables, an unavailable id, or the switch off keep "Claude Code" /
  // "Codex".
  const selectedComposerAccount = accountFieldVisible
    ? harnessAccounts.find((candidate) => candidate.id === accountValue)
    : undefined;
  const composerHarnessLabel =
    selectedComposerAccount?.name ?? agentProviderLabel(displayedAgentProvider);
  const launchResolutionPreferences = useMemo(
    () =>
      agentLaunchPreferencesForConfig(
        {
          agentProvider: displayedAgentProvider,
          launchSource,
          model: claudeSessionActive ? effectiveClaudeModel : codexModel,
          permissionMode: claudeSessionActive
            ? effectiveClaudePermissionMode
            : effectiveCodexPermissionMode,
          effort: claudeSessionActive
            ? effectiveClaudeEffort
            : effectiveCodexEffort,
        } satisfies SyncAgentConfig,
        { account: explicitAccountSelection },
      ),
    [
      displayedAgentProvider,
      launchSource,
      claudeSessionActive,
      effectiveClaudeModel,
      codexModel,
      effectiveClaudePermissionMode,
      effectiveCodexPermissionMode,
      effectiveClaudeEffort,
      effectiveCodexEffort,
      explicitAccountSelection,
    ],
  );
  // Composite `set_settings` payload (contract D12): Model, Effort, and — while
  // the field is shown — Account travel together in one mutation.
  const activeSettingsPayload = useCallback(
    (
      model: string,
      effort: string,
      account: string = accountValue,
    ): ProviderControlSetSettingsPayload =>
      accountFieldVisible
        ? { model, effort, account: accountSelectionFromValue(account) }
        : { model, effort },
    [accountFieldVisible, accountValue],
  );
  // Pre-session: the field follows the effective assignment for the displayed
  // harness and resets on a harness switch. Active session: hydrate from the
  // committed next-turn default, then the frozen latest-turn account.
  useEffect(() => {
    if (session) return;
    setAccountValue(assignedAccountValue);
  }, [session, displayedHarness, assignedAccountValue]);
  // Keyed on the DERIVED value, not the session object: a session refetch
  // that lands while a set_settings mutation is in flight (streaming WS
  // invalidations) must not revert the optimistic choice to the pre-write
  // value; only a server-side change of the committed account re-hydrates,
  // and only until the user picks an Account in this chat. A pick is saved
  // by the next turn that carries it (routes/chat D33), not when it is made,
  // so a later change of the saved account is an earlier turn's save or
  // another tab's, and must not replace it; like Model and Effort, the pick
  // is what the next turn sends.
  const sessionAccountValue = session ? accountValueFromSession(session) : null;
  const accountPickedRef = useRef(false);
  useEffect(() => {
    accountPickedRef.current = false;
  }, [activeSessionId]);
  useEffect(() => {
    if (sessionAccountValue === null || accountPickedRef.current) return;
    setAccountValue(sessionAccountValue);
  }, [activeSessionId, sessionAccountValue]);
  // Hydrate from the saved model and effort values, not the
  // provider-state object: a turn that names an account saves only that
  // account (routes/chat D33), and must not reset the composer's other
  // choices.
  const savedClaudeSettings = claudeSettingsFromSession(session);
  const savedClaudeModel = savedClaudeSettings.model;
  const savedClaudeEffort = savedClaudeSettings.effort;
  useEffect(() => {
    if (!claudeSessionActive) return;
    // right-panel D30: fall back to the session-persisted model before the
    // global default so the composer does not flash the workspace default
    // between send and the worker's provider-state write (issue #819).
    setClaudeModel(
      normalizeClaudeChatModel(
        savedClaudeModel ?? session?.model ?? resolvedDefaultClaudeModel,
      ),
    );
    setClaudeEffort(
      savedClaudeEffort ??
        defaultAgentProviderChoices?.claude_code.effort ??
        DEFAULT_REASONING_EFFORT,
    );
  }, [
    defaultAgentProviderChoices,
    activeSessionId,
    claudeSessionActive,
    resolvedDefaultClaudeModel,
    session?.model,
    savedClaudeModel,
    savedClaudeEffort,
  ]);

  const savedCodexSettings = codexSettingsFromSession(session);
  const savedCodexModel = savedCodexSettings.model;
  const savedCodexEffort = savedCodexSettings.effort;
  useEffect(() => {
    if (!codexSessionActive) return;
    setCodexModel(
      savedCodexModel ??
        defaultAgentProviderChoices?.codex.model ??
        CODEX_DEFAULT_MODEL,
    );
    setCodexEffort(
      savedCodexEffort ??
        defaultAgentProviderChoices?.codex.effort ??
        DEFAULT_CODEX_EFFORT,
    );
  }, [
    defaultAgentProviderChoices,
    activeSessionId,
    codexSessionActive,
    savedCodexModel,
    savedCodexEffort,
  ]);

  useEffect(() => {
    const id = window.setInterval(() => setNowMs(Date.now()), 60_000);
    return () => window.clearInterval(id);
  }, []);

  useLayoutEffect(() => {
    composerSendGenerationRef.current += 1;
    setIsSending(false);
    return () => {
      composerSendGenerationRef.current += 1;
    };
  }, [activeSessionId, composerScope]);

  useLayoutEffect(() => {
    setPendingReferences([]);
    attachmentSessionGenerationRef.current += 1;
    pendingAttachmentsRef.current = [];
    setPendingAttachments([]);
    setAttachmentError(null);
    setReferencePickerOpen(false);
    setReferenceSearch("");
  }, [activeSessionId]);

  useEffect(() => {
    activeProviderSettingsSessionIdRef.current = activeSessionId;
    providerSettingsMutationGenerationRef.current += 1;
    setProviderSettingsSaving(false);
    setProviderSettingsError(null);
    setClaudeModelAdjustment(null);
  }, [activeSessionId]);

  useEffect(() => {
    if (activeSessionId) return;
    // New-chat controls start from saved per-provider values, and those
    // visible values are the exact explicit draft sent to the resolver.
    setLaunchSource("explicit");
    setLaunchResolutionPending(Boolean(resolveAgentLaunch));
    setLaunchResolutionError(null);
    confirmedAgentLaunchRef.current = null;
    setConfirmedAgentLaunch(null);
    // The pre-session usage chip gate must not survive the launch state it
    // was derived from (agent_interaction_ux D8) — clear it with the rest of
    // the confirmed-launch state so a fresh draft never shows a stale chip.
    setConfirmedResolvedAuthMode(null);
  }, [activeSessionId, resolveAgentLaunch]);

  // Close history dropdown on outside click
  useEffect(() => {
    if (!historyOpen) return;
    const handleClickOutside = (e: MouseEvent) => {
      if (
        historyRef.current &&
        !historyRef.current.contains(e.target as Node)
      ) {
        setHistoryOpen(false);
      }
    };
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, [historyOpen]);

  useEffect(() => {
    if (!referencePickerOpen) return;

    const handlePointerDown = (event: PointerEvent) => {
      const root = referencePickerRef.current;
      if (
        root &&
        event.target instanceof Node &&
        !root.contains(event.target)
      ) {
        setReferencePickerOpen(false);
      }
    };

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setReferencePickerOpen(false);
      }
    };

    document.addEventListener("pointerdown", handlePointerDown, true);
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("pointerdown", handlePointerDown, true);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [referencePickerOpen]);

  // ONE dismissal effect for the shared provider-settings popover (#1370):
  // exactly one provider block renders at a time, so "inside" is judged
  // against whichever ref is mounted. Capture-phase listeners so page/editor
  // controls that stop propagation cannot trap the popover open; a harness
  // switch inside the popover keeps it open (the shared state survives the
  // block swap), while outside click and Escape still dismiss.
  useEffect(() => {
    if (!providerSettingsOpen) return;

    const updatePosition = () => {
      const trigger =
        displayedAgentProvider === "codex"
          ? codexSettingsTriggerRef.current
          : claudeSettingsTriggerRef.current;
      const controls =
        displayedAgentProvider === "codex"
          ? codexSettingsRef.current
          : claudeSettingsRef.current;
      if (!trigger || !controls) return;
      const triggerRect = trigger.getBoundingClientRect();
      setProviderSettingsPosition({
        bottom: Math.max(8, window.innerHeight - triggerRect.top + 8),
        right: Math.max(16, window.innerWidth - triggerRect.right),
        width: controls.getBoundingClientRect().width,
      });
    };

    updatePosition();
    window.addEventListener("resize", updatePosition);
    window.addEventListener("scroll", updatePosition, true);

    const isInsideSettings = (event: Event) => {
      const root = claudeSettingsRef.current ?? codexSettingsRef.current;
      const popover = providerSettingsPopoverRef.current;
      if (!root && !popover) return false;
      const path =
        typeof event.composedPath === "function" ? event.composedPath() : [];
      if (
        (root && path.includes(root)) ||
        (popover && path.includes(popover))
      ) {
        return true;
      }
      return (
        event.target instanceof Node &&
        Boolean(root?.contains(event.target) || popover?.contains(event.target))
      );
    };

    const handlePointerDown = (event: PointerEvent) => {
      if (!isInsideSettings(event)) {
        dismissProviderSettings();
      }
    };

    const handleMouseDown = (event: MouseEvent) => {
      if (!isInsideSettings(event)) {
        dismissProviderSettings();
      }
    };

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        dismissProviderSettings();
      }
    };

    document.addEventListener("pointerdown", handlePointerDown, true);
    document.addEventListener("mousedown", handleMouseDown, true);
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("resize", updatePosition);
      window.removeEventListener("scroll", updatePosition, true);
      document.removeEventListener("pointerdown", handlePointerDown, true);
      document.removeEventListener("mousedown", handleMouseDown, true);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [dismissProviderSettings, displayedAgentProvider, providerSettingsOpen]);

  // Filter sessions for history dropdown
  const filteredSessions = sessions.filter((s) =>
    (s.title ?? "").toLowerCase().includes(historySearch.toLowerCase()),
  );
  // The server orders chats by turn activity (chat g5.chat.activity.order);
  // a rename or settings change must not promote one here either.
  const recentSessions = sessions.slice(0, 3);

  // -------------------------------------------------------------------------
  // Auto-scroll behavior
  // -------------------------------------------------------------------------
  const scrollToBottom = useCallback(() => {
    const el = messageListRef.current;
    if (!el) return;
    el.scrollTop = el.scrollHeight;
  }, []);

  // Scrollback loading (chat_panel_runtime D35/D38): invoke each source's
  // single-flight loader. Older content renders ABOVE the viewport; the
  // continuous top-edge retention layout effect below shifts scrollTop by the
  // height delta afterward, keeping the same rows in view (no viewport jump).
  // A source is eligible when it has older content, no load of its own in
  // flight, and a loader wired (chat_panel_runtime D38).
  const canLoadEarlierTimeline = useCallback(
    () =>
      !!onLoadEarlierHistoryRef.current &&
      hasEarlierHistoryRef.current &&
      !isLoadingEarlierHistoryRef.current,
    [],
  );
  const canLoadOlderMessages = useCallback(
    () =>
      !!onLoadOlderMessagesRef.current &&
      hasOlderMessagesRef.current &&
      !isLoadingOlderMessagesRef.current,
    [],
  );

  const triggerLoadEarlierHistory = useCallback(() => {
    if (!canLoadEarlierTimeline()) return;
    onLoadEarlierHistoryRef.current?.();
  }, [canLoadEarlierTimeline]);

  const triggerLoadOlderMessages = useCallback(() => {
    if (!canLoadOlderMessages()) return;
    onLoadOlderMessagesRef.current?.();
  }, [canLoadOlderMessages]);

  // Incomplete turns' markers (g5.chat.history-window) drive the same
  // single-flight timeline loader; a marker's own load does not release
  // sticky-bottom, since the content it adds is above the reader.
  const loadEarlierActivity = useCallback(() => {
    onLoadEarlierHistoryRef.current?.();
  }, []);
  const hasEarlierActivityLoader = !!onLoadEarlierHistory;
  const earlierActivity = React.useMemo<
    EarlierActivityControl | undefined
  >(() => {
    if (!timelineCoverage) return undefined;
    const onLoad = hasEarlierActivityLoader ? loadEarlierActivity : undefined;
    if (timelineCoverage.status === "loading" || isLoadingEarlierHistory)
      return { state: "loading" };
    if (timelineCoverage.status === "error" || earlierHistoryError)
      return { state: "error", onLoad };
    return { state: "idle", onLoad };
  }, [
    timelineCoverage,
    isLoadingEarlierHistory,
    earlierHistoryError,
    hasEarlierActivityLoader,
    loadEarlierActivity,
  ]);

  // A scrollback load request releases sticky-bottom BEFORE any loader runs
  // (chat_panel_runtime D38, PR #2435 CodeRabbit round 2): in a transcript
  // shorter than the viewport (or within the 24px near-bottom tolerance of a
  // barely-overflowing one) scrollTop 0 is the top AND the bottom at once, so
  // sticky-bottom is still armed when the user asks for earlier history, and
  // the stick-to-bottom layout effect — which runs before retention — would
  // otherwise yank the freshly prepended history to the bottom.
  const releaseStickyBottom = useCallback(() => {
    shouldStickToBottomRef.current = false;
    setIsAtBottom(false);
  }, []);

  // One older-page load per SOURCE (chat_panel_runtime D38): the timeline and
  // messages loaders fire independently; a source already in flight is
  // skipped by its own eligibility guard. Sticky-bottom is released only when
  // at least one source actually fires.
  const triggerScrollbackLoads = useCallback(() => {
    const fireTimeline = canLoadEarlierTimeline();
    const fireMessages = canLoadOlderMessages();
    if (!fireTimeline && !fireMessages) return;
    releaseStickyBottom();
    if (fireTimeline) triggerLoadEarlierHistory();
    if (fireMessages) triggerLoadOlderMessages();
  }, [
    canLoadEarlierTimeline,
    canLoadOlderMessages,
    releaseStickyBottom,
    triggerLoadEarlierHistory,
    triggerLoadOlderMessages,
  ]);

  const handleScroll = useCallback(() => {
    const el = messageListRef.current;
    if (!el) return;
    readingAnchorRef.current = readingAnchor(el);
    const threshold = 24; // tolerate near-bottom jitter, but do not yank after a moderate manual scroll-up.
    const distanceFromBottom = el.scrollHeight - el.scrollTop - el.clientHeight;
    const nextIsAtBottom = distanceFromBottom <= threshold;
    shouldStickToBottomRef.current = nextIsAtBottom;
    setIsAtBottom(nextIsAtBottom);

    // Near-top sentinel for backward history (chat_panel_runtime D35/D38).
    // Re-arm once the user scrolls back down so a single top visit loads
    // exactly one older page per source rather than firing repeatedly while
    // pinned to top. The arm is PER SOURCE (PR #2435 CodeRabbit round 2): a
    // top visit consumes only the arms of the sources it fires, so a source
    // that was ineligible at that moment (in flight, exhausted, unwired)
    // keeps its arm and fires on the next near-top scroll once eligible.
    if (el.scrollTop > LOAD_EARLIER_SCROLL_THRESHOLD * 2) {
      timelineLoadArmedRef.current = true;
      messagesLoadArmedRef.current = true;
    }
    if (el.scrollTop <= LOAD_EARLIER_SCROLL_THRESHOLD) {
      const fireTimeline =
        timelineLoadArmedRef.current && canLoadEarlierTimeline();
      const fireMessages =
        messagesLoadArmedRef.current && canLoadOlderMessages();
      if (fireTimeline) timelineLoadArmedRef.current = false;
      if (fireMessages) messagesLoadArmedRef.current = false;
      if (fireTimeline || fireMessages) {
        releaseStickyBottom();
        if (fireTimeline) triggerLoadEarlierHistory();
        if (fireMessages) triggerLoadOlderMessages();
      }
    }
  }, [
    canLoadEarlierTimeline,
    canLoadOlderMessages,
    releaseStickyBottom,
    triggerLoadEarlierHistory,
    triggerLoadOlderMessages,
  ]);

  useEffect(() => {
    shouldStickToBottomRef.current = true;
    setIsAtBottom(true);
    // The top-edge baseline is NOT nulled here: a session switch replaces the
    // transcript lists, so the retention effect's first-item lookup simply
    // fails and it refreshes the baseline without compensating. Nulling it
    // would leave the FIRST data commit after a switch uncompensatable — in
    // an idle session that commit can be the older-page prepend itself.
    timelineLoadArmedRef.current = true;
    messagesLoadArmedRef.current = true;
    scrollToBottom();
    if (scrollRafRef.current != null)
      cancelAnimationFrame(scrollRafRef.current);
    scrollRafRef.current = requestAnimationFrame(() => {
      scrollToBottom();
      scrollRafRef.current = null;
    });
  }, [activeSessionId, scrollToBottom]);

  useLayoutEffect(() => {
    if (!shouldStickToBottomRef.current) return;
    scrollToBottom();
    if (scrollRafRef.current != null)
      cancelAnimationFrame(scrollRafRef.current);
    scrollRafRef.current = requestAnimationFrame(() => {
      scrollToBottom();
      scrollRafRef.current = null;
    });
  }, [
    messages,
    panelTimelineEntries,
    isStreaming,
    scrollToBottom,
    codexAuthLayout,
  ]);

  // Continuous top-edge viewport retention (chat_panel_runtime D38; supersedes
  // the per-load anchor of PR #2343). Declared AFTER the stick-to-bottom
  // effect so it runs last: top-edge growth happens while scrolled up (not
  // stuck to bottom), so that effect no-ops and this one shifts scrollTop by
  // the height the prepended content added, leaving the same rows under the
  // viewport instead of jumping (issues #2333 and chat d45027de).
  //
  // Every transcript commit records a baseline: the scroll container's
  // scrollHeight plus the first rendered item identities (first message id
  // and first timeline entry id). Compensation happens only when the top edge
  // ACTUALLY grew — a previous first item is found again at an index above
  // zero. A poll refresh or live WS entry appends at the bottom (previous
  // first items stay at index 0); that commit must not compensate, so it only
  // refreshes the baseline — absorbing bottom-append growth so the eventual
  // delta measures the prepended height alone (PR #2343 review). The restore
  // adds the delta to the LIVE scrollTop (not a captured value), so any user
  // scrolling during the wait is preserved (PR #2343 review). If the previous
  // first items vanish entirely (list replaced or session switched), the
  // baseline resets without compensating. While stuck to bottom the effect
  // only refreshes the baseline — the stick-to-bottom effect owns position.
  // A prepend never re-arms sticky-bottom (D38): compensation forces
  // shouldStickToBottomRef false so loaded history is not yanked back down.
  useLayoutEffect(() => {
    const el = messageListRef.current;
    if (!el) return;
    const prev = topEdgeAnchorRef.current;
    const nextAnchor = {
      scrollHeight: el.scrollHeight,
      firstMessageId: messages[0]?.id ?? null,
      firstEntryId: panelTimelineEntries[0]?.id ?? null,
    };
    if (prev && !shouldStickToBottomRef.current) {
      const messageTopGrew =
        prev.firstMessageId != null &&
        messages.findIndex((m) => m.id === prev.firstMessageId) > 0;
      const entryTopGrew =
        prev.firstEntryId != null &&
        panelTimelineEntries.findIndex((e) => e.id === prev.firstEntryId) > 0;
      if (messageTopGrew || entryTopGrew) {
        const delta = el.scrollHeight - prev.scrollHeight;
        if (delta > 0) {
          shouldStickToBottomRef.current = false;
          if (el.scrollTop <= LOAD_EARLIER_SCROLL_THRESHOLD) {
            // Near-top-triggered load: keep the user AT the top reading the
            // newly revealed history — pinning beats delta-shifting them a
            // page-height away from the top (chat 4484a12a: "keep at the top
            // instead of a big drop").
            el.scrollTop = 0;
          } else {
            // The user scrolled away during the load (or the prepend was not
            // top-triggered): keep the same rows under the viewport so the
            // mid-history reading position never teleports. Older activity
            // can also fill a turn below the viewport, so the shift follows
            // the anchored row rather than the total growth when it can.
            const anchor = readingAnchorRef.current;
            const anchorRow = anchor ? transcriptRow(el, anchor.id) : null;
            el.scrollTop +=
              anchor && anchorRow
                ? rowBottom(el, anchorRow) - anchor.bottom
                : delta;
          }
        }
      }
    }
    topEdgeAnchorRef.current = nextAnchor;
    readingAnchorRef.current = readingAnchor(el);
  }, [messages, panelTimelineEntries]);

  useEffect(() => {
    return () => {
      if (scrollRafRef.current != null)
        cancelAnimationFrame(scrollRafRef.current);
    };
  }, []);

  // -------------------------------------------------------------------------
  // Textarea auto-resize
  // -------------------------------------------------------------------------
  useEffect(() => {
    const textarea = textareaRef.current;
    if (!textarea) return;
    textarea.style.height = "auto";
    const maxHeight = 6 * 20; // ~6 lines at 20px line height
    textarea.style.height = `${Math.min(textarea.scrollHeight, maxHeight)}px`;
    // Every history step leaves the prompt ready to append to, including
    // after Up and when Down restores the unsent draft.
    if (pendingHistoryCaretRef.current) {
      pendingHistoryCaretRef.current = false;
      const caret = textarea.value.length;
      textarea.setSelectionRange(caret, caret);
      textarea.scrollTop = textarea.scrollHeight;
    }
  }, [inputValue, historyCaretSerial]);

  // -------------------------------------------------------------------------
  // Send message (auto-create session if needed)
  // -------------------------------------------------------------------------
  const inputDisabled = !!effectiveComposerDisabledReason;
  // A focus request is applied once; a disabled composer (Base, a stopped VM)
  // consumes it without focusing so the reason stays the visible state.
  const appliedFocusRequestRef = useRef(0);
  useEffect(() => {
    if (!focusRequest || appliedFocusRequestRef.current === focusRequest) {
      return;
    }
    appliedFocusRequestRef.current = focusRequest;
    if (inputDisabled) return;
    requestAnimationFrame(() => textareaRef.current?.focus());
  }, [focusRequest, inputDisabled]);
  const liveGate = useWorkspaceLiveGate();
  const sessionInputMutationAllowed = canManageSessionInputs ?? !inputDisabled;
  const showCodexAuth =
    !!activeSessionId &&
    displayedAgentProvider === "codex" &&
    sessionInputMutationAllowed;
  const hasVisibleCodexAuth =
    showCodexAuth &&
    codexAuthLayout.sessionId === activeSessionId &&
    codexAuthLayout.visible;
  const selectedReferenceIds = new Set(
    pendingReferences.map((reference) => reference.id),
  );
  const normalizedReferenceSearch = referenceSearch.trim().toLowerCase();
  const filteredProjectReferences = projectReferences
    .filter((reference) => !selectedReferenceIds.has(reference.id))
    .filter((reference) => reference.id !== activeProjectReference?.id)
    .filter((reference) => {
      if (!normalizedReferenceSearch) return true;
      return [
        reference.label,
        reference.path,
        reference.docId,
        projectReferenceKindLabel(reference.kind),
      ]
        .filter((value): value is string => !!value)
        .some((value) =>
          value.toLowerCase().includes(normalizedReferenceSearch),
        );
    })
    .slice(0, MAX_CHAT_REFERENCE_RESULTS);
  const activeReference =
    activeProjectReference !== null &&
    !selectedReferenceIds.has(activeProjectReference.id)
      ? activeProjectReference
      : null;
  const confirmedLaunchMatchesPreferences =
    confirmedAgentLaunch !== null &&
    JSON.stringify(confirmedAgentLaunch.preferences) ===
      JSON.stringify(launchResolutionPreferences);
  const launchResolutionReadyForSend =
    !!session ||
    !resolveAgentLaunch ||
    (!launchResolutionPending && confirmedLaunchMatchesPreferences);
  const canSend =
    !inputDisabled &&
    !isSending &&
    launchResolutionReadyForSend &&
    (inputValue.trim().length > 0 ||
      pendingReferences.length > 0 ||
      pendingAttachments.length > 0);

  async function sendContent(
    content: string,
    clearDraft: boolean,
    historyPrompt?: string,
  ) {
    const composerGeneration = composerSendGenerationRef.current;
    const references = pendingReferences;
    const attachments = pendingAttachments;
    // Snapshot the actual textarea draft (not the derived `content`, which for a
    // reference/attachment-only turn is a synthesized fallback string) so a
    // failed send restores exactly what the user had typed.
    const previousInput = inputValue;
    const previousSeedPulse = seedPulse;
    const previousSeedHeader = seedHeader;
    // A seed applied while this send is in flight owns the composer (a chat
    // action seeded into the open chat advances no send generation); the
    // failed send's draft must not restore over it.
    const seedTokenAtSend = draftRef.current.appliedSeedToken;
    // Optimistically clear the draft BEFORE awaiting the send so the composer
    // becomes un-sendable within the same tick — a second Enter pressed while
    // the request is in flight can no longer re-submit the identical turn
    // (chat_panel_runtime D32). Snapshot the draft so we can restore it if the
    // send is rejected/failed.
    if (clearDraft) {
      setInputValue("");
      setSeedPulse(false);
      setPendingReferences([]);
      replacePendingAttachments([]);
      setAttachmentError(null);
      setReferenceSearch("");
      setReferencePickerOpen(false);
    }
    setIsSending(true);
    // The chat this send belongs to, whatever is shown when it resolves.
    const sendHistoryScope = promptHistoryScope;
    const sendChatId = session?.id ?? null;
    try {
      const result = await onSendMessage(content, {
        agentLaunch: confirmedLaunchMatchesPreferences
          ? confirmedAgentLaunch
          : undefined,
        launchSource,
        ...(claudeTurnOptions ?? {}),
        ...(codexTurnOptions ?? {}),
        // A later turn carries an Account the user picked in this chat. An
        // untouched field sends none, and the server applies the chat's
        // saved account (routes/chat D33): the field can show a fallback
        // while the session detail loads.
        account:
          session && accountPickedRef.current
            ? explicitAccountSelection
            : undefined,
        projectReferences: references.length ? references : undefined,
        attachments: attachments.length ? attachments : undefined,
        ...(startAndSend ? { startCompute: true } : {}),
      });
      // Only an accepted send is history; a refused one keeps its draft. It
      // is recorded against the chat that accepted it: the one a first send
      // created, or the one shown when Enter was pressed.
      const acceptedChatId =
        result && typeof result === "object" ? result.sessionId : sendChatId;
      if (result !== false && historyPrompt && acceptedChatId) {
        recordPromptHistory(sendHistoryScope, acceptedChatId, historyPrompt);
      }
      if (
        result === false &&
        clearDraft &&
        composerGeneration === composerSendGenerationRef.current &&
        draftRef.current.appliedSeedToken === seedTokenAtSend
      ) {
        // Send was rejected/failed: restore the draft verbatim so the user can
        // retry without retyping (chat_panel_runtime D32).
        setInputValue(previousInput);
        setPendingReferences(references);
        replacePendingAttachments(attachments);
        // A refused seeded send was neither sent nor emptied: it keeps its
        // pulse (g5.workbench.chat-actions) and, when it had one, its header.
        if (previousSeedPulse) setSeedPulse(true);
        if (previousSeedHeader) setSeedHeader(previousSeedHeader);
      }
    } finally {
      if (composerGeneration === composerSendGenerationRef.current) {
        setIsSending(false);
        textareaRef.current?.focus();
      }
    }
  }

  function handleSend() {
    if (!canSend) return;
    // Attachments/references reach the agent only as files materialized into the
    // worktree, so the turn text is the ONLY channel that tells the agent they
    // exist. The label is therefore appended to a typed draft, not used as an
    // `||` fallback for an empty one — otherwise typing instructions alongside
    // an image silently hid the image (chat_panel_runtime D36, I3).
    const labelParts: string[] = [];
    if (pendingReferences.length) {
      labelParts.push(referenceFallbackContent(pendingReferences));
    }
    if (pendingAttachments.length) {
      labelParts.push(attachmentFallbackContent(pendingAttachments));
    }
    const label = labelParts.join(" ");
    const draft = inputValue.trim();
    // Typed instruction stays first and verbatim; the label trails as provenance.
    const content = draft && label ? `${draft}\n\n${label}` : draft || label;
    promptHistoryCursorRef.current = null;
    // History keeps only the typed prompt: references and attachments are
    // per-turn and not kept with it.
    void sendContent(content, true, draft || undefined);
  }

  function recordPromptHistory(
    scope: ChatPromptHistoryScope | undefined,
    chatId: string,
    prompt: string,
  ) {
    if (scope) {
      recordChatPromptHistory(
        chatPromptHistoryKey(scope.userId, scope.workspaceId, chatId),
        prompt,
      );
    } else {
      const history = mountPromptHistoryRef.current;
      history.set(
        chatId,
        withRecordedPrompt(history.get(chatId) ?? [], prompt),
      );
    }
  }

  function leavePromptHistory() {
    promptHistoryCursorRef.current = null;
    pendingHistoryCaretRef.current = false;
  }

  function handleComposerChange(e: React.ChangeEvent<HTMLTextAreaElement>) {
    leavePromptHistory();
    setInputValue(e.target.value);
    if (!e.target.value.trim()) {
      // Emptying a seeded draft abandons the seed (g5.workbench.chat-actions):
      // the owner drops the start and reference it attached to this token.
      const appliedToken = draftRef.current.appliedSeedToken;
      if (seedPulse && appliedToken !== null) {
        onComposerSeedEmptied?.(appliedToken);
      }
      setSeedPulse(false);
    }
  }

  // Up/Down browse from an empty composer or continue an active history
  // session, regardless of line count. Other actions return to normal editing
  // until the composer is empty again.
  function handlePromptHistoryKey(
    e: React.KeyboardEvent<HTMLTextAreaElement>,
  ): boolean {
    if (e.key !== "ArrowUp" && e.key !== "ArrowDown") {
      if (!["Shift", "Alt", "Control", "Meta"].includes(e.key)) {
        leavePromptHistory();
      }
      return false;
    }
    if (
      e.shiftKey ||
      e.altKey ||
      e.ctrlKey ||
      e.metaKey ||
      e.nativeEvent.isComposing ||
      referencePickerOpen
    ) {
      leavePromptHistory();
      return false;
    }
    const { selectionStart, selectionEnd, value } = e.currentTarget;
    if (selectionStart !== selectionEnd) {
      leavePromptHistory();
      return false;
    }
    const older = e.key === "ArrowUp";
    const browsing = promptHistoryCursorRef.current?.recalled === value;
    if (!browsing) {
      leavePromptHistory();
      if (value !== "") return false;
    }
    const entries =
      older && promptHistoryChatId
        ? promptHistoryKey
          ? readChatPromptHistory(promptHistoryKey)
          : (mountPromptHistoryRef.current.get(promptHistoryChatId) ?? [])
        : [];
    const step = stepPromptHistory(
      entries,
      promptHistoryCursorRef.current,
      value,
      older ? "older" : "newer",
    );
    if (!step) {
      // At the oldest entry, keep browsing and the end caret rather than
      // letting the browser move up into the recalled multiline prompt.
      if (!browsing) return false;
      e.preventDefault();
      return true;
    }
    e.preventDefault();
    promptHistoryCursorRef.current = step.cursor;
    pendingHistoryCaretRef.current = true;
    setInputValue(step.value);
    setHistoryCaretSerial((serial) => serial + 1);
    return true;
  }

  function handleReferenceClick() {
    if (inputDisabled) return;
    setReferencePickerOpen((open) => !open);
  }

  function handleAttachmentPick() {
    if (inputDisabled) return;
    // Click-to-browse: open the native file dialog. Pasting into the composer
    // (handlePasteAttachments) is the other ingestion path; drag-drop is not.
    setReferencePickerOpen(false);
    attachmentInputRef.current?.click();
  }

  // Add files through the shared ingestion path (chat_panel_runtime I3, D37).
  // Paste callers request the safe-image gate; picker callers accept arbitrary
  // regular files. Each item is evaluated against the accumulating list.
  async function addAttachments(files: File[], requireSafeImage: boolean) {
    if (inputDisabled || files.length === 0) return;
    const attachmentSessionGeneration = attachmentSessionGenerationRef.current;
    let firstError: string | null = null;
    for (const file of files) {
      // Sequential by design: each file's caps/de-dup are evaluated against the
      // accumulating attachment list, so a multi-file paste cannot bypass them.
      const result = await prepareAttachment(
        file,
        pendingAttachmentsRef.current,
        requireSafeImage,
      );
      if (
        attachmentSessionGeneration !== attachmentSessionGenerationRef.current
      ) {
        return;
      }
      if (!result.ok) {
        firstError = firstError ?? result.error;
        continue;
      }
      // FileReader completions can race when users make rapid picker selections.
      // Re-read the synchronously maintained ref after the await so each result
      // validates against attachments committed by earlier completions.
      const latestAttachments = pendingAttachmentsRef.current;
      const plural = requireSafeImage ? "images" : "files";
      const noun = requireSafeImage ? "image" : "file";
      if (latestAttachments.length >= MAX_ATTACHMENT_COUNT) {
        firstError =
          firstError ??
          `You can attach at most ${MAX_ATTACHMENT_COUNT} ${plural} per message.`;
        continue;
      }
      const latestTotalBytes = latestAttachments.reduce(
        (sum, attachment) => sum + base64ByteLength(attachment.contentBase64),
        0,
      );
      if (
        latestTotalBytes + base64ByteLength(result.attachment.contentBase64) >
        MAX_TOTAL_ATTACHMENT_BYTES
      ) {
        firstError =
          firstError ??
          `These ${plural} are too large to send together — remove one and try again.`;
        continue;
      }
      if (
        latestAttachments.some(
          (attachment) =>
            attachment.contentBase64 === result.attachment.contentBase64,
        )
      ) {
        firstError = firstError ?? `That ${noun} is already attached.`;
        continue;
      }
      replacePendingAttachments([...latestAttachments, result.attachment]);
    }
    if (
      attachmentSessionGeneration === attachmentSessionGenerationRef.current
    ) {
      setAttachmentError(firstError);
    }
  }

  async function handleAttachmentChange(
    event: React.ChangeEvent<HTMLInputElement>,
  ) {
    const input = event.target;
    const file = input.files?.[0];
    // Reset so re-selecting the same file still fires a change event.
    input.value = "";
    if (!file) return;
    await addAttachments([file], false);
  }

  // Composer paste (chat_panel_runtime D37): pull image items from the OS/browser
  // clipboard and route them through the same ingestion as the file picker. Only
  // consume the paste (preventDefault) when at least one image is present, so a
  // normal text paste still lands in the draft untouched.
  function handlePasteAttachments(
    event: React.ClipboardEvent<HTMLTextAreaElement>,
  ) {
    if (inputDisabled) return;
    const clipboard = event.clipboardData;
    if (!clipboard) return;
    const files: File[] = [];
    for (let i = 0; i < clipboard.items.length; i++) {
      const item = clipboard.items[i];
      if (item.kind === "file" && item.type.startsWith("image/")) {
        const file = item.getAsFile();
        if (file) files.push(file);
      }
    }
    if (files.length === 0) return;
    event.preventDefault();
    void addAttachments(files, true);
  }

  function removeAttachment(index: number) {
    replacePendingAttachments(
      pendingAttachmentsRef.current.filter(
        (_, itemIndex) => itemIndex !== index,
      ),
    );
  }

  function addProjectReference(reference: ChatProjectReference) {
    setPendingReferences((current) =>
      current.some((item) => item.id === reference.id)
        ? current
        : [...current, reference],
    );
    setReferencePickerOpen(false);
    setReferenceSearch("");
  }

  function removeProjectReference(referenceId: string) {
    setPendingReferences((current) =>
      current.filter((reference) => reference.id !== referenceId),
    );
  }

  const renderReferenceRow = (
    reference: ChatProjectReference,
    testId: string,
  ) => (
    <button
      key={reference.id}
      type="button"
      data-testid={testId}
      onClick={() => addProjectReference(reference)}
      className={classNames(
        "flex w-full items-start gap-2 rounded-md px-2 py-1.5 text-left",
        "text-[var(--g5-text1)] hover:bg-[var(--g5-bg2)] hover:text-[var(--g5-text0)]",
      )}
    >
      <FileText size={13} className="mt-0.5 shrink-0 text-[var(--g5-text2)]" />
      <span className="min-w-0 flex-1">
        <span className="flex min-w-0 items-center gap-1.5">
          <span className="truncate text-xs font-medium">
            {reference.label}
          </span>
          <span className="shrink-0 rounded bg-[var(--g5-bg2)] px-1 py-0.5 text-[9px] uppercase tracking-wide text-[var(--g5-text2)]">
            {projectReferenceKindLabel(reference.kind)}
          </span>
        </span>
        <span className="block truncate text-[11px] text-[var(--g5-text2)]">
          {projectReferenceSubtitle(reference)}
        </span>
      </span>
    </button>
  );

  const referenceButton = (
    <div
      className="relative flex shrink-0 items-center"
      ref={referencePickerRef}
    >
      <button
        type="button"
        onClick={handleReferenceClick}
        disabled={inputDisabled}
        className={classNames(
          "flex h-7 shrink-0 items-center justify-center rounded-md",
          layout === "center"
            ? "gap-1.5 border border-[var(--g5-stroke)] px-2.5 text-xs"
            : "w-7",
          inputDisabled
            ? "text-[var(--g5-text2)]/30 cursor-not-allowed"
            : "text-[var(--g5-text2)] hover:text-[var(--g5-text0)] hover:bg-[var(--g5-bg2)] cursor-pointer",
          "transition-colors",
        )}
        title="Add workspace context"
        aria-label="Add workspace context"
        data-testid="chat-add-reference-button"
      >
        {layout === "center" ? (
          <>
            <Paperclip size={13} />
            <span>Context</span>
          </>
        ) : (
          <Plus size={16} />
        )}
      </button>

      {/* Hidden click-to-browse input for arbitrary external file uploads
          (chat_panel_runtime I3). Clipboard paste (D37) is image-only; drag-drop
          is not supported. */}
      <input
        ref={attachmentInputRef}
        type="file"
        className="hidden"
        data-testid="chat-attachment-input"
        onChange={handleAttachmentChange}
      />

      {referencePickerOpen && (
        <div
          className={classNames(
            "absolute bottom-full left-0 z-30 mb-2 w-80 max-w-[calc(100vw-2rem)]",
            "rounded-lg border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] shadow-xl",
          )}
          data-testid="chat-reference-picker"
        >
          <div className="border-b border-[var(--g5-stroke)] p-1">
            <button
              type="button"
              onClick={handleAttachmentPick}
              className={classNames(
                "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left",
                "text-[var(--g5-text1)] hover:bg-[var(--g5-bg2)] hover:text-[var(--g5-text0)]",
              )}
              data-testid="chat-upload-file-action"
            >
              <ImagePlus
                size={13}
                className="shrink-0 text-[var(--g5-text2)]"
              />
              <span className="min-w-0 flex-1">
                <span className="block truncate text-xs font-medium">
                  Upload a file
                </span>
                <span className="block truncate text-[11px] text-[var(--g5-text2)]">
                  Attach any file up to 5 MiB
                </span>
              </span>
            </button>
          </div>
          <div className="border-b border-[var(--g5-stroke)] p-2">
            <div className="flex items-center gap-2 rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-2 py-1.5">
              <Search size={13} className="shrink-0 text-[var(--g5-text2)]" />
              <input
                value={referenceSearch}
                onChange={(event) => setReferenceSearch(event.target.value)}
                placeholder="Search workspace"
                autoFocus
                className="min-w-0 flex-1 bg-transparent text-xs text-[var(--g5-text1)] placeholder:text-[var(--g5-text2)]/60"
                data-testid="chat-reference-search"
              />
            </div>
          </div>
          <div className="max-h-72 overflow-y-auto p-1">
            {activeReference && (
              <div className="mb-1">
                <div className="px-2 py-1 text-[10px] font-semibold uppercase tracking-wide text-[var(--g5-text2)]">
                  Current tab
                </div>
                {renderReferenceRow(
                  activeReference,
                  "chat-current-reference-option",
                )}
              </div>
            )}
            {filteredProjectReferences.length > 0 ? (
              <div>
                <div className="px-2 py-1 text-[10px] font-semibold uppercase tracking-wide text-[var(--g5-text2)]">
                  Workspace
                </div>
                {filteredProjectReferences.map((reference) =>
                  renderReferenceRow(reference, "chat-reference-option"),
                )}
              </div>
            ) : (
              <div
                className="px-3 py-4 text-center text-xs text-[var(--g5-text2)]"
                data-testid="chat-reference-empty"
              >
                No references
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );

  const inputMutationPending =
    editSessionInput.isLoading ||
    withdrawSessionInput.isLoading ||
    cancelUnknownSessionInput.isLoading ||
    redeliverSessionInput.isLoading;

  async function saveQueuedInput(input: SessionInputDTO) {
    const content = editingInputValue.trim();
    if (
      !sessionInputMutationAllowed ||
      !activeSessionId ||
      content.length === 0 ||
      inputMutationPending
    )
      return;
    setInputMutationError(null);
    try {
      await editSessionInput.mutateAsync({
        sessionId: activeSessionId,
        inputId: input.id,
        expectedRevision: input.revision,
        content,
      });
      setEditingInputId(null);
      setEditingInputValue("");
    } catch (error) {
      setInputMutationError(
        error instanceof Error ? error.message : "Could not edit queued input.",
      );
    }
  }

  async function withdrawQueuedInput(input: SessionInputDTO) {
    if (
      !sessionInputMutationAllowed ||
      !activeSessionId ||
      inputMutationPending
    )
      return;
    setInputMutationError(null);
    try {
      await withdrawSessionInput.mutateAsync({
        sessionId: activeSessionId,
        inputId: input.id,
        expectedRevision: input.revision,
      });
      if (editingInputId === input.id) {
        setEditingInputId(null);
        setEditingInputValue("");
      }
    } catch (error) {
      setInputMutationError(
        error instanceof Error
          ? error.message
          : "Could not withdraw queued input.",
      );
    }
  }

  async function redeliverUnknownInput(input: SessionInputDTO) {
    if (
      !sessionInputMutationAllowed ||
      !activeSessionId ||
      inputMutationPending
    )
      return;
    setInputMutationError(null);
    try {
      await redeliverSessionInput.mutateAsync({
        sessionId: activeSessionId,
        inputId: input.id,
      });
    } catch (error) {
      setInputMutationError(
        error instanceof Error
          ? error.message
          : "Could not queue input for redelivery.",
      );
    }
  }

  async function cancelUnknownInput(input: SessionInputDTO) {
    if (
      !sessionInputMutationAllowed ||
      !activeSessionId ||
      inputMutationPending
    )
      return;
    setInputMutationError(null);
    try {
      await cancelUnknownSessionInput.mutateAsync({
        sessionId: activeSessionId,
        inputId: input.id,
      });
    } catch (error) {
      setInputMutationError(
        error instanceof Error
          ? error.message
          : "Could not cancel the uncertain delivery.",
      );
    }
  }

  const queuedInputsTray =
    visibleSessionInputs.length > 0 ? (
      <div
        className="mb-2 space-y-1 rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] p-2"
        data-testid="chat-queued-inputs"
      >
        <div className="text-[10px] font-semibold uppercase tracking-wide text-[var(--g5-text2)]">
          Queued messages
        </div>
        {visibleSessionInputs.map((input) => {
          // Prefer the server-evaluated permissions so no offered action is
          // one the API always rejects (a steering reservation, a frozen
          // possibly-delivered revision); the local fallback covers an older
          // API pod that does not send them yet.
          const queuedHuman =
            input.inputOrigin === "human" && input.delivery === "queued";
          const editable =
            sessionInputMutationAllowed && (input.editable ?? queuedHuman);
          const withdrawable =
            sessionInputMutationAllowed && (input.withdrawable ?? queuedHuman);
          const uncertain =
            !input.awaitingHistory && input.delivery === "delivery_unknown";
          const editing = editable && editingInputId === input.id;
          return (
            <div
              key={input.id}
              className="rounded border border-[var(--g5-stroke)] px-2 py-1.5"
              data-testid={`chat-queued-input-${input.id}`}
            >
              {editing && sessionInputMutationAllowed ? (
                <textarea
                  value={editingInputValue}
                  onChange={(event) => setEditingInputValue(event.target.value)}
                  rows={2}
                  disabled={inputMutationPending}
                  className="w-full resize-y rounded border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-2 py-1 text-xs text-[var(--g5-text1)]"
                  aria-label="Edit queued message"
                />
              ) : (
                <p className="line-clamp-2 whitespace-pre-wrap text-xs text-[var(--g5-text1)] [overflow-wrap:anywhere]">
                  {input.content}
                </p>
              )}
              <div className="mt-1 flex items-center gap-1.5">
                <span
                  className={classNames(
                    "text-[10px]",
                    uncertain
                      ? "text-[var(--g5-warn)]"
                      : "text-[var(--g5-text2)]",
                  )}
                >
                  {input.delivery === "queued"
                    ? "Waiting"
                    : input.delivery === "reserved" ||
                        input.delivery === "delivered"
                      ? "Sending"
                      : "Delivery uncertain — it may already have been received"}
                </span>
                <span className="flex-1" />
                {editing ? (
                  <>
                    <button
                      type="button"
                      data-testid={`chat-queued-input-save-${input.id}`}
                      onClick={() => void saveQueuedInput(input)}
                      disabled={
                        inputMutationPending ||
                        editingInputValue.trim().length === 0
                      }
                      className="rounded px-1.5 py-0.5 text-[10px] text-[var(--g5-info)] hover:bg-[var(--g5-bg2)] disabled:opacity-40"
                    >
                      Save
                    </button>
                    <button
                      type="button"
                      data-testid={`chat-queued-input-cancel-edit-${input.id}`}
                      onClick={() => {
                        setEditingInputId(null);
                        setEditingInputValue("");
                      }}
                      disabled={inputMutationPending}
                      className="rounded px-1.5 py-0.5 text-[10px] text-[var(--g5-text2)] hover:bg-[var(--g5-bg2)] disabled:opacity-40"
                    >
                      Cancel
                    </button>
                  </>
                ) : (
                  <>
                    {editable && (
                      <button
                        type="button"
                        data-testid={`chat-queued-input-edit-${input.id}`}
                        aria-label="Edit queued message"
                        onClick={() => {
                          setEditingInputId(input.id);
                          setEditingInputValue(input.content);
                        }}
                        disabled={inputMutationPending}
                        className="rounded p-1 text-[var(--g5-text2)] hover:bg-[var(--g5-bg2)] hover:text-[var(--g5-text0)] disabled:opacity-40"
                      >
                        <PenLine size={12} />
                      </button>
                    )}
                    {uncertain && sessionInputMutationAllowed && (
                      <button
                        type="button"
                        data-testid={`chat-queued-input-redeliver-${input.id}`}
                        onClick={() => void redeliverUnknownInput(input)}
                        disabled={inputMutationPending}
                        className="rounded px-1.5 py-0.5 text-[10px] text-[var(--g5-warn)] hover:bg-[var(--g5-bg2)] disabled:opacity-40"
                      >
                        Redeliver (possibly duplicate)
                      </button>
                    )}
                    {withdrawable && (
                      <button
                        type="button"
                        data-testid={`chat-queued-input-withdraw-${input.id}`}
                        aria-label="Withdraw queued message"
                        onClick={() => void withdrawQueuedInput(input)}
                        disabled={inputMutationPending}
                        className="rounded p-1 text-[var(--g5-text2)] hover:bg-[var(--g5-bg2)] hover:text-[var(--g5-blocked)] disabled:opacity-40"
                      >
                        <XCircle size={12} />
                      </button>
                    )}
                    {uncertain && sessionInputMutationAllowed && (
                      <button
                        type="button"
                        data-testid={`chat-queued-input-cancel-${input.id}`}
                        aria-label="Cancel uncertain message"
                        onClick={() => void cancelUnknownInput(input)}
                        disabled={inputMutationPending}
                        className="rounded p-1 text-[var(--g5-text2)] hover:bg-[var(--g5-bg2)] hover:text-[var(--g5-blocked)] disabled:opacity-40"
                      >
                        <XCircle size={12} />
                      </button>
                    )}
                  </>
                )}
              </div>
            </div>
          );
        })}
        {inputMutationError && (
          <p className="text-[10px] text-[var(--g5-blocked)]">
            {inputMutationError}
          </p>
        )}
      </div>
    ) : null;

  const referenceTray =
    pendingReferences.length > 0 ? (
      <div className="mb-2">
        <div className="flex flex-wrap gap-1" data-testid="chat-reference-tray">
          {pendingReferences.map((reference) => (
            <span
              key={reference.id}
              className={classNames(
                "inline-flex min-w-0 max-w-full items-center gap-1 rounded-md",
                "border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-1.5 py-1",
                "text-[11px] text-[var(--g5-text1)]",
              )}
            >
              <FileText size={12} className="shrink-0 text-[var(--g5-text2)]" />
              <span className="shrink-0 text-[var(--g5-text2)]">
                {projectReferenceKindLabel(reference.kind)}
              </span>
              <span className="max-w-[180px] truncate">{reference.label}</span>
              <button
                type="button"
                data-testid="chat-remove-project-reference"
                onClick={() => removeProjectReference(reference.id)}
                className="shrink-0 rounded p-0.5 text-[var(--g5-text2)] hover:bg-[var(--g5-bg2)] hover:text-[var(--g5-text0)]"
                aria-label={`Remove ${reference.label}`}
                title="Remove"
              >
                <X size={12} />
              </button>
            </span>
          ))}
        </div>
      </div>
    ) : null;

  // Upload chips are visually distinct from reference chips (chat_panel_runtime
  // I3): they carry the info accent, an image glyph/thumbnail, and an "Upload"
  // marker instead of the reference kind label + file glyph.
  const attachmentTray =
    pendingAttachments.length > 0 ? (
      <div className="mb-2">
        <div
          className="flex flex-wrap gap-1"
          data-testid="chat-attachment-tray"
        >
          {pendingAttachments.map((attachment, index) => (
            <span
              key={`${attachment.fileName}-${index}`}
              className={classNames(
                "inline-flex min-w-0 max-w-full items-center gap-1 rounded-md",
                "border border-[var(--g5-info)]/40 bg-[var(--g5-info)]/10 px-1.5 py-1",
                "text-[11px] text-[var(--g5-info)]",
              )}
              data-testid="chat-attachment-chip"
            >
              {isSafeImagePreview(attachment.contentType) ? (
                <img
                  src={`data:${attachment.contentType};base64,${attachment.contentBase64}`}
                  alt=""
                  className="h-4 w-4 shrink-0 rounded object-cover"
                />
              ) : (
                <FileText size={12} className="shrink-0" />
              )}
              <span className="shrink-0 text-[10px] uppercase tracking-wide">
                Upload
              </span>
              <span className="max-w-[180px] truncate">
                {attachment.fileName}
              </span>
              <button
                type="button"
                data-testid="chat-remove-attachment"
                onClick={() => removeAttachment(index)}
                className="shrink-0 rounded p-0.5 hover:bg-[var(--g5-info)]/20"
                aria-label={`Remove ${attachment.fileName}`}
                title="Remove"
              >
                <X size={12} />
              </button>
            </span>
          ))}
        </div>
      </div>
    ) : null;

  const attachmentErrorNotice = attachmentError ? (
    <div
      className="mb-2 rounded border border-[var(--g5-blocked)]/40 bg-[var(--g5-blocked)]/10 px-2 py-1.5 text-xs text-[var(--g5-blocked)]"
      data-testid="chat-attachment-error"
    >
      {attachmentError}
    </div>
  ) : null;

  // Model, Effort and Account choices are kept in the composer and travel
  // with the next input (g5.chat.start-and-send, g5.chat.account-defaults):
  // nothing reaches the VM at change time, so a stopped VM does not lock
  // them; only the authority to send does (a reader, Base, a read-only or
  // freezing policy). A new chat's choices are local until the first send.
  const providerSettingsBlocked =
    session && liveGate && !liveGate.canWrite
      ? liveGateReason(liveGate.kind, "change provider settings", {
          workspaceNoun: liveGate.workspaceNoun,
        })
      : null;
  const launchSettingsLocked = session
    ? providerSettingsSaving || providerSettingsBlocked !== null
    : !!resolveAgentLaunch && launchResolutionPending;
  const launchResolutionStatus = session ? (
    <div className="space-y-1">
      <p
        className="text-[10px] leading-snug text-[var(--g5-text2)]"
        data-testid="chat-provider-settings-scope"
      >
        Agent and Mode are fixed for this chat.{" "}
        {accountFieldVisible
          ? "Model, Effort, and Account"
          : "Model and Effort"}{" "}
        changes apply to the next message you send, whether the workspace is
        running or not; the current turn and already queued turns keep their
        snapshots.
      </p>
      {providerSettingsBlocked ? (
        <p
          data-testid="chat-provider-settings-blocked"
          className="text-[10px] text-[var(--g5-warn)]"
        >
          {providerSettingsBlocked}
        </p>
      ) : null}
      {providerSettingsSaving ? (
        <p
          data-testid="chat-provider-settings-saving"
          className="text-[10px] text-[var(--g5-text2)]"
        >
          Saving settings…
        </p>
      ) : null}
      {providerSettingsError ? (
        <p
          data-testid="chat-provider-settings-error"
          role="alert"
          className="text-[10px] text-[var(--g5-blocked)]"
        >
          {providerSettingsError}
        </p>
      ) : null}
    </div>
  ) : null;
  const compactLaunchModel =
    displayedAgentProvider === "codex" ? codexModel : effectiveClaudeModel;
  const compactLaunchEffort =
    displayedAgentProvider === "codex"
      ? effectiveCodexEffort
      : effectiveClaudeEffort;
  const availableCompactLaunchModels =
    displayedAgentProvider === "codex"
      ? codexPickerModelOptions()
      : claudeOpenRouterAccount
        ? CLAUDE_OPENROUTER_CHAT_MODEL_OPTIONS
        : CLAUDE_CHAT_MODEL_OPTIONS;
  const compactLaunchModelOption =
    displayedAgentProvider === "codex"
      ? codexModelOption(compactLaunchModel)
      : claudeModelOption(compactLaunchModel);
  const compactLaunchModels = availableCompactLaunchModels.some(
    (option) => option.value === compactLaunchModel,
  )
    ? availableCompactLaunchModels
    : [...availableCompactLaunchModels, compactLaunchModelOption];
  const compactLaunchModeLabel = session
    ? displayedAgentProvider === "codex"
      ? selectedCodexPermission.label
      : selectedClaudePermission.label
    : launchModeForHarness(displayedAgentProvider).label;
  const compactLaunchEfforts = effortOptionsForModel(
    displayedAgentProvider,
    compactLaunchModel,
  );
  const compactLaunchEditor = (
    <AgentLaunchFieldControls
      provider={displayedAgentProvider}
      providerOptions={availableAgentProviders.map((provider) => ({
        value: provider,
        label: agentProviderLabel(provider),
      }))}
      model={compactLaunchModel}
      modelOptions={compactLaunchModels.map((option) => ({
        value: option.value,
        label: option.label,
      }))}
      effort={compactLaunchEffort}
      effortOptions={compactLaunchEfforts.map((option) => ({
        value: option.value,
        label: option.label,
      }))}
      modeLabel={compactLaunchModeLabel}
      accountOptions={accountOptions}
      account={accountValue}
      onAccountChange={(nextValue) => {
        // Choosing the shown value is a pick too: it may be a fallback
        // shown while the session detail loads.
        accountPickedRef.current = true;
        if (nextValue === accountValue) return;
        setLaunchResolutionError(null);
        setLaunchSource("explicit");
        const previousAccount = accountValue;
        const previousClaudeModel = claudeModel;
        const nextAccount = harnessAccounts.find(
          (account) => account.id === nextValue,
        );
        const nextModel =
          claudeSessionActive && nextAccount?.kind === "openrouter_api_key"
            ? claudeChatModelForOpenRouter(effectiveClaudeModel)
            : compactLaunchModel;
        setClaudeModelAdjustment(
          claudeSessionActive && nextModel !== effectiveClaudeModel
            ? { from: effectiveClaudeModel, to: nextModel }
            : null,
        );
        // Model and Account are one choice, including when leaving OpenRouter:
        // keep the model the composer showed instead of reviving a plain pin.
        if (claudeSessionActive) setClaudeModel(nextModel);
        setAccountValue(nextValue);
        if (session) {
          void persistActiveProviderSettings(
            activeSettingsPayload(
              nextModel.trim(),
              compactLaunchEffort,
              nextValue,
            ),
            () => {
              setAccountValue(previousAccount);
              setClaudeModel(previousClaudeModel);
              setClaudeModelAdjustment(null);
            },
          );
        }
      }}
      onProviderChange={(nextValue) => {
        const provider = nextValue as ChatAgentProvider;
        if (provider === displayedAgentProvider) return;
        setLaunchResolutionError(null);
        setClaudeModelAdjustment(null);
        setDraftAgentProvider(provider);
        setLaunchSource("explicit");
        onSelectAgentProvider?.(provider);
      }}
      onModelChange={(model) => {
        if (model === compactLaunchModel) return;
        setLaunchResolutionError(null);
        setClaudeModelAdjustment(null);
        setLaunchSource("explicit");
        if (displayedAgentProvider === "codex") {
          const previousModel = codexModel;
          const previousEffort = codexEffort;
          const nextModel = model.trim();
          const nextEffort = clampEffortForModel(
            "codex",
            nextModel,
            codexEffort,
          );
          setCodexModel(nextModel);
          setCodexEffort(nextEffort);
          if (session) {
            void persistActiveProviderSettings(
              activeSettingsPayload(nextModel, nextEffort),
              () => {
                setCodexModel(previousModel);
                setCodexEffort(previousEffort);
              },
            );
          }
        } else {
          const previousModel = claudeModel;
          const previousEffort = claudeEffort;
          const nextModel = normalizeClaudeChatModel(model);
          const nextEffort = clampEffortForModel(
            "claude_code",
            nextModel,
            claudeEffort,
          );
          setClaudeModel(nextModel);
          setClaudeEffort(nextEffort);
          if (session) {
            void persistActiveProviderSettings(
              activeSettingsPayload(nextModel, nextEffort),
              () => {
                setClaudeModel(previousModel);
                setClaudeEffort(previousEffort);
              },
            );
          }
        }
      }}
      onEffortChange={(effort) => {
        if (effort === compactLaunchEffort) return;
        setLaunchResolutionError(null);
        setLaunchSource("explicit");
        if (displayedAgentProvider === "codex") {
          const previousEffort = codexEffort;
          setCodexEffort(effort);
          if (session) {
            void persistActiveProviderSettings(
              activeSettingsPayload(codexModel.trim(), effort),
              () => setCodexEffort(previousEffort),
            );
          }
        } else {
          const previousEffort = claudeEffort;
          setClaudeEffort(effort);
          if (session) {
            void persistActiveProviderSettings(
              activeSettingsPayload(effectiveClaudeModel, effort),
              () => setClaudeEffort(previousEffort),
            );
          }
        }
      }}
      disabled={launchSettingsLocked}
      agentDisabled={!!session || availableAgentProviders.length <= 1}
      testIdPrefix="chat"
      testId="chat-launch-controls"
      modeTestId="chat-mode-select"
      className="mt-3"
      selectVariant="themed"
      columns={2}
    />
  );
  const claudeControls = claudeSessionActive ? (
    <div
      ref={claudeSettingsRef}
      className="relative flex min-w-fit flex-1 items-center justify-end gap-2"
      data-testid="claude-code-controls"
    >
      <button
        ref={claudeSettingsTriggerRef}
        type="button"
        data-testid="claude-settings-control"
        onClick={() =>
          providerSettingsOpen
            ? dismissProviderSettings()
            : openProviderSettings()
        }
        className={classNames(
          "inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-xs",
          "border-[var(--g5-stroke)] bg-[var(--g5-bg0)] text-[var(--g5-text1)]",
          "hover:border-[var(--g5-info)]/60 hover:text-[var(--g5-text0)]",
        )}
      >
        <Settings size={12} />
        <span data-testid="claude-harness-control" className="font-medium">
          {composerHarnessLabel}
        </span>
        <span className="text-[var(--g5-text2)]">•</span>
        <span data-testid="claude-model-control" className="font-medium">
          {selectedClaudeModel.label}
        </span>
        <span className="text-[var(--g5-text2)]">•</span>
        <span data-testid="claude-permission-control" className="font-medium">
          {selectedClaudePermission.label}
        </span>
        <span className="text-[var(--g5-text2)]">•</span>
        <span data-testid="claude-effort-control" className="font-medium">
          {selectedClaudeEffort.label}
        </span>
      </button>

      {providerSettingsOpen && providerSettingsPosition
        ? createPortal(
            <div
              ref={providerSettingsPopoverRef}
              data-testid="claude-settings-modal"
              className="fixed z-[9999] flex max-h-[min(34rem,calc(100vh-8rem))] flex-col overflow-hidden rounded-xl border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] text-xs shadow-2xl"
              style={providerSettingsPosition}
            >
              <div
                data-testid="chat-provider-settings-body"
                className="min-h-0 overflow-y-auto p-3 scrollbar-thin scrollbar-thumb-[var(--g5-bg2)]"
              >
                {launchResolutionStatus}
                {compactLaunchEditor}
              </div>
            </div>,
            document.body,
          )
        : null}
    </div>
  ) : null;

  // Codex-native control surface (codex_support I12/I13/D5). Rendered ALONGSIDE
  // claudeControls; exactly one is active per session because the provider is
  // start-locked. Native compound permission and reasoning values are sent
  // verbatim through onSendProviderControl — no remap onto Claude Code modes.
  const codexControls = codexSessionActive ? (
    <div
      ref={codexSettingsRef}
      className="relative flex min-w-fit flex-1 items-center justify-end gap-2"
      data-testid="codex-controls"
    >
      <button
        ref={codexSettingsTriggerRef}
        type="button"
        data-testid="codex-settings-control"
        onClick={() =>
          providerSettingsOpen
            ? dismissProviderSettings()
            : openProviderSettings()
        }
        className={classNames(
          "inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-xs",
          "border-[var(--g5-stroke)] bg-[var(--g5-bg0)] text-[var(--g5-text1)]",
          "hover:border-[var(--g5-info)]/60 hover:text-[var(--g5-text0)]",
        )}
      >
        <Settings size={12} />
        <span data-testid="codex-harness-control" className="font-medium">
          {composerHarnessLabel}
        </span>
        <span className="text-[var(--g5-text2)]">•</span>
        <span data-testid="codex-model-control" className="font-medium">
          {selectedCodexModel.label}
        </span>
        <span className="text-[var(--g5-text2)]">•</span>
        <span data-testid="codex-permission-control" className="font-medium">
          {selectedCodexPermission.label}
        </span>
        <span className="text-[var(--g5-text2)]">•</span>
        <span data-testid="codex-effort-control" className="font-medium">
          {selectedCodexEffort.label}
        </span>
      </button>

      {providerSettingsOpen && providerSettingsPosition
        ? createPortal(
            <div
              ref={providerSettingsPopoverRef}
              data-testid="codex-settings-modal"
              className="fixed z-[9999] flex max-h-[min(34rem,calc(100vh-8rem))] flex-col overflow-hidden rounded-xl border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] text-xs shadow-2xl"
              style={providerSettingsPosition}
            >
              <div
                data-testid="chat-provider-settings-body"
                className="min-h-0 overflow-y-auto p-3 scrollbar-thin scrollbar-thumb-[var(--g5-bg2)]"
              >
                {launchResolutionStatus}
                {compactLaunchEditor}
              </div>
            </div>,
            document.body,
          )
        : null}
    </div>
  ) : null;

  // The former composer-row "AGENT:" provider cards are removed (#1370 UI
  // feedback, chat intent 2bca6571): the settings popover's Agent dropdown
  // (right-panel D25/D27) is the sole harness selector. The displayed
  // provider stays exposed for tests via data-selected-provider on the
  // composer's provider-settings controls below.
  const providerSettingsControls = claudeControls ?? codexControls;
  const launchResolutionMonitor =
    !session && resolveAgentLaunch ? (
      <AgentLaunchResolutionStatus
        preferences={launchResolutionPreferences}
        resolvePreferences={resolveAgentLaunch}
        testIdPrefix="chat"
        className="mb-2 text-right"
        onSelectionChange={acceptResolvedAgentLaunch}
        onResolutionChange={handleResolvedExecutionChange}
        onPendingChange={setLaunchResolutionPending}
        onError={handleLaunchResolutionError}
        showError={false}
      />
    ) : null;
  const launchResolutionErrorNotice =
    !session && launchResolutionError ? (
      <div
        data-testid="chat-resolution-error"
        role="alert"
        className="mb-2 flex items-center justify-between gap-2 rounded border border-[var(--g5-blocked)]/40 bg-[var(--g5-blocked)]/10 px-2 py-1.5 text-[10px] leading-snug text-[var(--g5-blocked)]"
      >
        <span>
          Could not use your selected settings: {launchResolutionError}
        </span>
        <button
          type="button"
          data-testid="chat-review-provider-settings"
          className="shrink-0 rounded border border-current px-2 py-1 font-medium"
          onClick={openProviderSettings}
        >
          Review settings
        </button>
      </div>
    ) : null;
  const claudeModelAdjustmentNotice =
    claudeSessionActive &&
    claudeOpenRouterAccount &&
    displayedClaudeModelAdjustment ? (
      <p
        data-testid="chat-model-adjustment-notice"
        role="status"
        className="mb-2 text-[10px] leading-snug text-[var(--g5-text2)]"
      >
        Switched from{" "}
        {claudeModelOption(displayedClaudeModelAdjustment.from).label} to{" "}
        {claudeModelOption(displayedClaudeModelAdjustment.to).label} because
        OpenRouter takes this family without the 1M suffix.
      </p>
    ) : null;
  // Live Claude/Codex subscription usage. The button expands both windows;
  // the display preference selects its headline or hides it entirely.
  // Retain the legacy pre-session prop for callers that still supply it.
  const usageChipHidden = subscriptionUsageWindow === "off";
  const usageChipWindow: SubscriptionUsageWindow =
    subscriptionUsageWindow === "5h" ? "5h" : "7d";
  const activeSubscriptionUsageChip =
    !usageChipHidden &&
    session &&
    shouldRenderSubscriptionUsageChip(session) &&
    session.subscriptionUsage ? (
      <SubscriptionUsageChip
        usage={session.subscriptionUsage}
        window={usageChipWindow}
        nowMs={nowMs}
      />
    ) : null;
  const preSessionSubscriptionUsageChip =
    !usageChipHidden &&
    !session &&
    confirmedResolvedAuthMode === "anthropic_oauth" &&
    workspaceSubscriptionUsage ? (
      <SubscriptionUsageChip
        usage={workspaceSubscriptionUsage}
        window={usageChipWindow}
        nowMs={nowMs}
      />
    ) : null;
  const subscriptionUsageChip =
    activeSubscriptionUsageChip ?? preSessionSubscriptionUsageChip;
  const composerControlRow = providerSettingsControls ? (
    <>
      <div
        className="mb-2 flex min-w-0 flex-wrap items-center justify-end gap-2"
        data-testid="chat-agent-options-row"
        data-selected-provider={displayedAgentProvider}
      >
        {subscriptionUsageChip && (
          <div className="mr-auto shrink-0">{subscriptionUsageChip}</div>
        )}
        {providerSettingsControls}
      </div>
      {launchResolutionMonitor}
      {launchResolutionErrorNotice}
      {claudeModelAdjustmentNotice}
    </>
  ) : null;
  const sessionHistoryPicker = (
    triggerLabel: string,
    options: {
      testId?: string;
      maxWidthClass?: string;
      subLabel?: string | null;
      align?: "left" | "right";
      /** Shown on hover: the full name a truncated label hides. */
      title?: string;
    } = {},
  ) => (
    <div ref={historyRef} className="relative min-w-0">
      <button
        type="button"
        data-testid={options.testId}
        title={options.title}
        onClick={() => setHistoryOpen(!historyOpen)}
        className={classNames(
          "flex items-center gap-1 px-2 py-0.5 rounded-md text-[11px] transition truncate cursor-pointer",
          options.maxWidthClass ?? "max-w-[160px]",
          historyOpen
            ? "bg-[var(--g5-info)]/20 text-[var(--g5-info)]"
            : "text-[var(--g5-text1)] hover:text-[var(--g5-text0)] hover:bg-white/5",
        )}
      >
        <History className="w-3 h-3 shrink-0" />
        <span className="min-w-0 truncate text-left leading-tight">
          <span className="block truncate">{triggerLabel}</span>
          {options.subLabel && (
            <span className="block truncate text-[10px] font-normal text-[var(--g5-text2)]">
              {options.subLabel}
            </span>
          )}
        </span>
        <ChevronDown className="w-3 h-3 shrink-0" />
      </button>

      {historyOpen && (
        <div
          className={classNames(
            "absolute top-full z-50 mt-1 w-72 overflow-hidden rounded-lg",
            "border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] shadow-xl",
            options.align === "right" ? "right-0" : "left-0",
          )}
        >
          <div className="px-2 py-1.5 border-b border-[var(--g5-stroke)]">
            <div className="flex items-center gap-1.5 px-2 py-1 rounded bg-[var(--g5-bg0)] border border-[var(--g5-stroke)]/30">
              <Search className="w-3 h-3 text-[var(--g5-text2)]" />
              <input
                type="text"
                value={historySearch}
                onChange={(e) => setHistorySearch(e.target.value)}
                placeholder="Search sessions…"
                className="flex-1 bg-transparent text-[11px] text-[var(--g5-text0)] placeholder:text-[var(--g5-text2)]/40"
              />
            </div>
          </div>
          <div className="max-h-52 overflow-y-auto">
            {filteredSessions.length > 0 ? (
              filteredSessions.map((s) => {
                const timeLine = chatPickerTimeLine(s, nowMs);
                return (
                  <button
                    key={s.id}
                    type="button"
                    data-testid="chat-session-select"
                    aria-current={s.id === activeSessionId ? "true" : undefined}
                    className={classNames(
                      "flex w-full items-start gap-2 px-3 py-1.5 text-left text-[11px] hover:bg-white/[0.04] transition cursor-pointer",
                      "border-b border-[var(--g5-stroke)]/10 last:border-b-0",
                      s.id === activeSessionId &&
                        "border-l-2 border-l-[var(--g5-info)] bg-[var(--g5-info)]/5",
                    )}
                    onClick={() => {
                      onSelectSession(s.id);
                      setHistoryOpen(false);
                      setHistorySearch("");
                    }}
                  >
                    <ChatActivityIndicator
                      activity={s.activity}
                      className="mt-0.5"
                    />
                    <span className="min-w-0 flex-1">
                      <span
                        className="block truncate font-medium text-[var(--g5-text0)]"
                        title={sessionDisplayTitle(s)}
                      >
                        {sessionDisplayTitle(s)}
                      </span>
                      {timeLine && (
                        <span className="mt-0.5 block truncate text-[10px] text-[var(--g5-text2)]/70">
                          {timeLine}
                        </span>
                      )}
                      {s.summary && (
                        <span className="mt-0.5 block truncate text-[10px] text-[var(--g5-text2)]/60">
                          {s.summary}
                        </span>
                      )}
                    </span>
                  </button>
                );
              })
            ) : (
              <div className="px-3 py-2 text-[11px] text-[var(--g5-text2)]">
                No matching sessions
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );

  const activeSessionTitle = sessionDisplayTitle(session);
  const activeSessionStartedAgo = session
    ? formatAgo(session.createdAt, nowMs)
    : null;
  const activeSessionStartedLabel = activeSessionStartedAgo
    ? `Started ${activeSessionStartedAgo}`
    : null;

  // The current chat's title takes the width the other header controls
  // leave, on one line, with a separate rename button beside it (chat
  // g5.chat.ui.header-title).
  const activeSessionTitleControl =
    headerRenameOpen && session && onRenameSession ? (
      <ChatTitleRenameEditor
        initialTitle={session.title?.trim() ?? ""}
        onSave={(title) => onRenameSession(session.id, title)}
        onClose={() => setHeaderRenameOpen(false)}
      />
    ) : (
      <div className="flex min-w-0 flex-1 items-center gap-0.5">
        {sessionHistoryPicker(activeSessionTitle, {
          testId: "chat-active-history",
          subLabel: activeSessionStartedLabel,
          maxWidthClass: "max-w-full",
          title: activeSessionTitle,
        })}
        {session && onRenameSession && (
          <button
            type="button"
            data-testid="chat-header-rename"
            onClick={() => {
              setHistoryOpen(false);
              setHeaderRenameOpen(true);
            }}
            className="shrink-0 rounded p-1 text-[var(--g5-text2)] hover:bg-white/5 hover:text-[var(--g5-text0)] cursor-pointer"
            title="Rename chat"
            aria-label="Rename chat"
          >
            <PenLine size={12} />
          </button>
        )}
      </div>
    );
  const panelChromeLabel = chatPanelChromeLabel(
    scopeType,
    scopeLabel,
    session?.title,
  );

  function handleKeyDown(e: React.KeyboardEvent<HTMLTextAreaElement>) {
    if (handlePromptHistoryKey(e)) return;
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    } else if (
      e.key === "Escape" &&
      isStreaming &&
      !stopControlSuppressed &&
      !stopRequesting &&
      onStopStreaming
    ) {
      e.preventDefault();
      onStopStreaming?.();
    }
  }

  // -------------------------------------------------------------------------
  // Citation click handler
  // -------------------------------------------------------------------------
  // Stable across commits: every memoized turn receives it.
  const handleCitationClick = React.useCallback(
    (citation: Citation) => {
      if (citation.filePath && onOpenFile) {
        // `repoId` routes the open to the owning repo's branch (chat_service
        // D55); undefined on legacy rows keeps the active-branch behavior.
        onOpenFile(citation.filePath, citation.lineRange, citation.repoId);
      }
    },
    [onOpenFile],
  );

  // Start and send replaces Send while the VM is stopped
  // (g5.personal.start-and-send): a labelled button, never an icon alone.
  const sendPulse = seedPulse && canSend;
  const startAndSendButton = (testId: string) => (
    <button
      type="button"
      data-testid={testId}
      data-start-and-send="true"
      data-pulse={sendPulse ? "true" : undefined}
      onClick={handleSend}
      disabled={!canSend}
      className={classNames(
        "flex h-8 shrink-0 items-center gap-1.5 rounded-md px-2.5 text-xs font-medium transition-colors",
        sendPulse && "g5-send-pulse",
        canSend
          ? "cursor-pointer bg-[var(--g5-info)] text-white hover:brightness-110"
          : "cursor-not-allowed bg-[var(--g5-bg2)] text-[var(--g5-text2)]/60",
      )}
      title="Starts the workspace and sends this message once it is running (Enter)"
    >
      <Send size={13} />
      <span>Start and send</span>
    </button>
  );
  const emptyComposerFeedback = (
    <>
      {composerNotice}
      {setupError && (
        <div className="mb-2 rounded border border-[var(--g5-blocked)]/40 bg-[var(--g5-blocked)]/10 px-2 py-1.5 text-xs text-[var(--g5-blocked)]">
          {setupError}
        </div>
      )}
      {queuedInputsTray}
      {referenceTray}
      {attachmentTray}
      {attachmentErrorNotice}
    </>
  );

  const emptyComposer =
    layout === "center" ? (
      <div
        data-testid="chat-center-composer"
        className={classNames(
          "mt-7 w-full overflow-visible rounded-[var(--g5-radius-xl)]",
          "border border-[var(--g5-stroke)] bg-[var(--g5-bg0)]",
          "shadow-[var(--shadow-pane)] transition-colors focus-within:border-[var(--g5-info)]/70",
        )}
      >
        <div className="px-4 pt-4">{emptyComposerFeedback}</div>
        <GatedControl
          verb="send messages"
          reason={effectiveComposerDisabledReason ?? null}
          testId="chat-composer-disabled-reason"
          className="w-full"
        >
          <textarea
            ref={textareaRef}
            data-testid="chat-input"
            value={inputValue}
            onChange={handleComposerChange}
            onPointerDown={leavePromptHistory}
            onKeyDown={handleKeyDown}
            onPaste={handlePasteAttachments}
            placeholder={
              effectiveComposerDisabledReason ??
              composerPlaceholder ??
              "Ask anything about this workspace, or describe a change…"
            }
            rows={4}
            disabled={inputDisabled}
            className={classNames(
              "block min-h-28 w-full resize-none bg-transparent px-5 pb-4 pt-1",
              "text-sm leading-6 text-[var(--g5-text0)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-[var(--g5-info)]",
              "placeholder:text-[var(--g5-text2)]/65",
              "scrollbar-thin scrollbar-thumb-[var(--g5-bg2)]",
            )}
          />
        </GatedControl>
        <div className="mx-4 flex min-w-0 items-center gap-2 border-t border-[var(--g5-stroke)] py-3">
          <button
            type="button"
            onClick={handleAttachmentPick}
            disabled={inputDisabled}
            className={classNames(
              "flex h-7 shrink-0 items-center gap-1.5 rounded-md border px-2.5 text-xs",
              "border-[var(--g5-stroke)] transition-colors",
              inputDisabled
                ? "cursor-not-allowed text-[var(--g5-text2)]/30"
                : "cursor-pointer text-[var(--g5-text1)] hover:border-[var(--g5-info)]/60 hover:bg-[var(--g5-bg1)] hover:text-[var(--g5-text0)]",
            )}
            data-testid="chat-attach-button"
          >
            <Plus size={13} />
            <span>Attach</span>
          </button>
          {referenceButton}
          {providerSettingsControls && (
            <div
              className="flex min-w-0 flex-1 items-center gap-2"
              data-testid="chat-agent-options-row"
              data-selected-provider={displayedAgentProvider}
            >
              {subscriptionUsageChip && (
                <div className="mr-auto shrink-0">{subscriptionUsageChip}</div>
              )}
              {providerSettingsControls}
            </div>
          )}
          {!providerSettingsControls && <div className="flex-1" />}
          {startAndSend ? (
            startAndSendButton("chat-panel-no-session-send")
          ) : (
            <button
              type="button"
              data-testid="chat-panel-no-session-send"
              data-pulse={sendPulse ? "true" : undefined}
              onClick={handleSend}
              disabled={!canSend}
              className={classNames(
                "flex h-8 w-8 shrink-0 items-center justify-center rounded-md transition-colors",
                sendPulse && "g5-send-pulse",
                canSend
                  ? "cursor-pointer bg-[var(--g5-info)] text-white hover:brightness-110"
                  : "cursor-not-allowed bg-[var(--g5-bg2)] text-[var(--g5-text2)]/35",
              )}
              title="Send (Enter)"
            >
              <ArrowRight size={16} />
            </button>
          )}
        </div>
        <div className="px-4">
          {launchResolutionMonitor}
          {launchResolutionErrorNotice}
          {claudeModelAdjustmentNotice}
        </div>
      </div>
    ) : (
      <div className="shrink-0 border-t border-[var(--g5-stroke)] p-2">
        {emptyComposerFeedback}
        {composerControlRow}
        <div
          className={classNames(
            "flex items-center gap-2 rounded-lg bg-[var(--g5-bg0)]",
            "border border-[var(--g5-stroke)] px-2 py-1.5",
            "focus-within:border-[var(--g5-info)] transition-colors",
          )}
        >
          {referenceButton}
          <GatedControl
            verb="send messages"
            reason={effectiveComposerDisabledReason ?? null}
            testId="chat-composer-disabled-reason"
            className="min-w-0 flex-1"
          >
            <textarea
              ref={textareaRef}
              data-testid="chat-input"
              value={inputValue}
              onChange={handleComposerChange}
              onPointerDown={leavePromptHistory}
              onKeyDown={handleKeyDown}
              onPaste={handlePasteAttachments}
              placeholder={
                effectiveComposerDisabledReason ??
                composerPlaceholder ??
                "Ask about this codebase…"
              }
              rows={1}
              disabled={inputDisabled}
              className={classNames(
                "min-h-7 w-full flex-1 resize-none bg-transparent py-1 text-[13px] leading-5",
                "text-[var(--g5-text1)] placeholder:text-[var(--g5-text2)]/50",
                "scrollbar-thin scrollbar-thumb-[var(--g5-bg2)]",
              )}
            />
          </GatedControl>
          {startAndSend ? (
            startAndSendButton("chat-panel-no-session-send")
          ) : (
            <button
              type="button"
              data-testid="chat-panel-no-session-send"
              data-pulse={sendPulse ? "true" : undefined}
              onClick={handleSend}
              disabled={!canSend}
              className={classNames(
                "shrink-0 rounded-md p-1.5",
                sendPulse && "g5-send-pulse",
                canSend
                  ? "cursor-pointer bg-[var(--g5-info)] text-white"
                  : "cursor-not-allowed text-[var(--g5-text2)]/30",
                "transition-colors",
              )}
              title="Send (Enter)"
            >
              <Send size={16} />
            </button>
          )}
        </div>
      </div>
    );

  // -------------------------------------------------------------------------
  // Empty state (no session) — show input directly, auto-create on send
  // -------------------------------------------------------------------------
  if (!session) {
    if (layout === "center") {
      return (
        <div className="relative h-full bg-[var(--g5-bg1)]">
          {/* Pop out from the landing too, so the floating panel is one step
              away whether or not a session exists (workspaces
              g5.workspace.ui.chat-corner-button). Outside the scroll box so
              it stays put when the landing scrolls. */}
          {onDetach && (
            <button
              type="button"
              data-testid="chat-center-landing-detach"
              onClick={onDetach}
              className="absolute right-3 top-3 z-10 rounded p-1 text-[var(--g5-text2)] transition-colors hover:bg-white/5 hover:text-[var(--g5-text0)] cursor-pointer"
              title="Pop out"
            >
              <ExternalLink size={14} />
            </button>
          )}
          <div className="h-full overflow-y-auto">
            <div className="mx-auto flex min-h-full w-full max-w-5xl flex-col px-8 pb-12 pt-14">
              {seedHeaderBlock}
              <div>
                <p className="text-[11px] font-bold uppercase tracking-[0.18em] text-[var(--g5-info)]">
                  Your workspace is ready
                </p>
                <h1 className="mt-3 text-[28px] font-semibold leading-tight tracking-[-0.025em] text-[var(--g5-text0)]">
                  Start with a conversation.
                </h1>
                <p className="mt-2 text-sm leading-6 text-[var(--g5-text2)]">
                  Ask G5 to investigate, plan, explain, or make a change. The
                  whole workspace is in reach.
                </p>
              </div>

              {emptyComposer}

              <div className="mt-4 flex flex-wrap items-center gap-2">
                <span className="mr-1 text-[10px] font-semibold uppercase tracking-[0.12em] text-[var(--g5-text2)]">
                  Try asking
                </span>
                {CENTER_CHAT_STARTERS.map((starter) => (
                  <button
                    key={starter.label}
                    type="button"
                    disabled={inputDisabled}
                    title={effectiveComposerDisabledReason ?? undefined}
                    className={classNames(
                      "rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-3 py-1.5",
                      "text-xs text-[var(--g5-text1)] transition-colors",
                      "hover:border-[var(--g5-info)]/60 hover:bg-[var(--g5-bg2)] hover:text-[var(--g5-text0)]",
                      "disabled:cursor-not-allowed disabled:opacity-50",
                    )}
                    onClick={() => {
                      setInputValue(starter.prompt);
                      requestAnimationFrame(() => textareaRef.current?.focus());
                    }}
                    data-testid="chat-starter-prompt"
                  >
                    {starter.label}
                  </button>
                ))}
              </div>

              <section className="mt-10" aria-labelledby="recent-chats-heading">
                <div className="flex items-center justify-between border-b border-[var(--g5-stroke)] pb-2">
                  <h2
                    id="recent-chats-heading"
                    className="text-[10px] font-semibold uppercase tracking-[0.14em] text-[var(--g5-text2)]"
                  >
                    Recent conversations
                  </h2>
                  {sessions.length > 0 &&
                    sessionHistoryPicker("View all", {
                      testId: "chat-center-history",
                      maxWidthClass: "max-w-none",
                      align: "right",
                    })}
                </div>
                {recentSessions.length > 0 ? (
                  <div>
                    {recentSessions.map((recentSession) => (
                      <button
                        key={recentSession.id}
                        type="button"
                        className={classNames(
                          "flex w-full items-center gap-3 border-b border-[var(--g5-stroke)]/70 px-1 py-3 text-left",
                          "text-xs text-[var(--g5-text1)] transition-colors",
                          "hover:bg-[var(--g5-bg0)] hover:text-[var(--g5-text0)]",
                        )}
                        onClick={() => onSelectSession(recentSession.id)}
                        data-testid="chat-center-recent-session"
                      >
                        <ChatActivityIndicator
                          activity={recentSession.activity}
                        />
                        <span
                          className="min-w-0 flex-1 truncate"
                          title={sessionDisplayTitle(recentSession)}
                        >
                          {sessionDisplayTitle(recentSession)}
                        </span>
                        <span className="shrink-0 text-[10px] text-[var(--g5-text2)]">
                          {chatRecentTimeLabel(recentSession, nowMs)}
                        </span>
                      </button>
                    ))}
                  </div>
                ) : (
                  <p className="border-b border-[var(--g5-stroke)]/70 py-4 text-xs text-[var(--g5-text2)]">
                    Your recent conversations will appear here.
                  </p>
                )}
              </section>
            </div>
          </div>
        </div>
      );
    }

    return (
      <div className="flex flex-col h-full bg-[var(--g5-bg1)]">
        {/* Header: keep chat history reachable even before the next session exists. */}
        <div className="flex items-center gap-2 px-3 py-2 border-b border-[var(--g5-stroke)] shrink-0">
          <span className="text-xs font-semibold uppercase tracking-wide text-[var(--g5-text2)]">
            {panelChromeLabel}
          </span>
          <PhaseScopePill scope={phaseScope} />
          {sessionHistoryPicker("History", {
            testId: "chat-history-header",
            maxWidthClass: "max-w-[180px]",
          })}
          <Chip state={scopeChipState(scopeType)} text={scopeLabel} />
          <div className="flex-1" />
          {onDetach && (
            <button
              type="button"
              data-testid="chat-panel-no-session-detach"
              onClick={onDetach}
              className="p-1 rounded hover:bg-white/5 text-[var(--g5-text2)] hover:text-[var(--g5-text0)] transition-colors cursor-pointer"
              title="Pop out"
            >
              <ExternalLink size={14} />
            </button>
          )}
          {onClose && (
            <button
              type="button"
              data-testid="chat-panel-no-session-close"
              onClick={onClose}
              className="p-1 rounded hover:bg-white/5 text-[var(--g5-text2)] hover:text-[var(--g5-text0)] transition-colors cursor-pointer"
              title="Close panel"
            >
              <X size={14} />
            </button>
          )}
        </div>

        {seedHeaderBlock && (
          <div className="shrink-0 px-3 pt-3">{seedHeaderBlock}</div>
        )}

        {/* Empty state message */}
        <div className="flex-1 flex flex-col items-center justify-center gap-3 px-6 py-6 overflow-y-auto">
          <div className="flex flex-col items-center gap-2 text-center">
            <div className="flex h-10 w-10 items-center justify-center rounded-full border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] text-[var(--g5-text2)]">
              <MessageSquare size={18} />
            </div>
            <div className="text-sm font-semibold text-[var(--g5-text0)]">
              Start Chat
            </div>
          </div>
        </div>

        {/* Input area — always shown so user can just start typing */}
        {emptyComposer}
      </div>
    );
  }

  // -------------------------------------------------------------------------
  // Active session
  // -------------------------------------------------------------------------
  return (
    <QuestionCardFocusContext.Provider value={questionCardFocus}>
      <QuestionAnswerResendContext.Provider value={resendQuestionAnswer}>
        <div className="flex flex-col h-full min-h-0 bg-[var(--g5-bg1)]">
          {/* Header: V2-style — "Chat" label + history pill + new session + close */}
          <div className="flex items-center gap-2 px-3 py-2 border-b border-[var(--g5-stroke)] shrink-0">
            <span className="text-xs font-semibold uppercase tracking-wide text-[var(--g5-text2)]">
              {panelChromeLabel}
            </span>
            <PhaseScopePill scope={phaseScope} />

            {/* Session title + compact age subtext, then its rename button */}
            {activeSessionTitleControl}

            {/* New session */}
            {onCreateSession && (
              <button
                type="button"
                data-testid="new-session-header"
                onClick={onCreateSession}
                className="p-1.5 rounded hover:bg-white/5 text-[var(--g5-text2)] hover:text-[var(--g5-text0)] transition-colors cursor-pointer"
                title="New session"
              >
                <Plus size={18} strokeWidth={2.25} />
              </button>
            )}

            {/* Popout / detach */}
            {onDetach && (
              <button
                type="button"
                data-testid="chat-panel-detach"
                onClick={onDetach}
                className="p-1 rounded hover:bg-white/5 text-[var(--g5-text2)] hover:text-[var(--g5-text0)] transition-colors cursor-pointer"
                title="Pop out"
              >
                <ExternalLink size={14} />
              </button>
            )}

            {/* Close panel */}
            {onClose && (
              <button
                type="button"
                data-testid="chat-panel-close"
                onClick={onClose}
                className="p-1 rounded hover:bg-white/5 text-[var(--g5-text2)] hover:text-[var(--g5-text0)] transition-colors cursor-pointer"
                title="Close panel"
              >
                <X size={14} />
              </button>
            )}
          </div>

          {/* Message stream (scrollable) */}
          <div
            ref={messageListRef}
            data-testid="chat-messages"
            data-chat-scroll-root=""
            onScroll={handleScroll}
            className={classNames(
              // overflow-anchor:none — the D38 retention layout effect is
              // the single deterministic compensator; native scroll
              // anchoring must not stack a second adjustment on prepends.
              "flex-1 min-h-0 overflow-y-auto [overflow-anchor:none]",
              "scrollbar-thin scrollbar-thumb-[var(--g5-bg2)] scrollbar-track-transparent",
            )}
          >
            {/* Load-earlier history affordance (chat_panel_runtime D35/D38,
              issues #2333 and chat d45027de): pinned at the top of the stream
              above the first turn. Its presence signals older content from
              EITHER source — timeline entries (possibly the front of the top
              visible turn) or persisted messages — is not yet loaded, so a
              window-truncated top turn is never presented as complete. A
              near-top scroll also auto-loads each eligible source once. */}
            {(hasEarlierHistory || hasOlderMessages) && (
              <div
                className="flex justify-center py-2"
                data-testid="chat-load-earlier-history"
              >
                <button
                  type="button"
                  data-testid="chat-load-earlier-history-button"
                  onClick={triggerScrollbackLoads}
                  // Disabled only while NEITHER source is eligible to fire
                  // now (chat_panel_runtime D38, PR #2435 review): one busy
                  // source must not block the other idle source's load.
                  disabled={
                    !(
                      hasEarlierHistory &&
                      !isLoadingEarlierHistory &&
                      !!onLoadEarlierHistory
                    ) &&
                    !(
                      hasOlderMessages &&
                      !isLoadingOlderMessages &&
                      !!onLoadOlderMessages
                    )
                  }
                  aria-label="Load earlier chat history"
                  className={classNames(
                    "flex items-center gap-1.5 px-3 py-1 text-xs font-medium rounded-full",
                    "border border-[var(--g5-stroke)] text-[var(--g5-text2)]",
                    "hover:bg-white/5 hover:text-[var(--g5-text0)] transition-colors",
                    "disabled:cursor-default cursor-pointer",
                  )}
                >
                  <History
                    size={12}
                    className={
                      isLoadingEarlierHistory || isLoadingOlderMessages
                        ? "animate-pulse"
                        : undefined
                    }
                  />
                  {isLoadingEarlierHistory || isLoadingOlderMessages
                    ? "Loading earlier history…"
                    : "Load earlier history"}
                </button>
              </div>
            )}
            {messages.length === 0 &&
            panelTimelineEntries.length === 0 &&
            !hasVisibleCodexAuth ? (
              <div className="flex items-center justify-center h-full">
                <p className="text-xs text-[var(--g5-text2)]">
                  No messages yet
                </p>
              </div>
            ) : (
              <UnifiedTimeline
                messages={messages}
                timelineEntries={panelTimelineEntries}
                agentProvider={displayedAgentProvider}
                activeTaskSummary={activeTaskSummary}
                isStreaming={isStreaming}
                onCitationClick={onOpenFile ? handleCitationClick : undefined}
                onForkSession={onForkSession}
                sessionId={activeSessionId}
                onResolveAgentApproval={onResolveAgentApproval}
                onEndBackgroundWait={onEndBackgroundWait}
                timelineCoverage={timelineCoverage}
                earlierActivity={earlierActivity}
                activityOverride={turnActivityOverride}
                onOpenOntologyChanges={onOpenOntologyChanges}
                onOpenOntologyFile={
                  onOpenFile
                    ? (filePath, repoId, line, statementId) =>
                        statementId && onOpenOntologyStatement
                          ? onOpenOntologyStatement(
                              filePath,
                              repoId,
                              statementId,
                            )
                          : onOpenFile(
                              filePath,
                              line ? [line, line] : undefined,
                              repoId,
                            )
                    : undefined
                }
              />
            )}
            {showCodexAuth && activeSessionId && (
              <CodexDeviceAuth
                sessionId={activeSessionId}
                terminalFailures={panelTimelineEntries.flatMap((entry) => {
                  const failure = codexAuthFailureFromEntry(entry);
                  return failure ? [failure] : [];
                })}
                onLayoutChange={handleCodexAuthLayoutChange}
              />
            )}
          </div>

          {/* Input area */}
          <div className="border-t border-[var(--g5-stroke)] p-2 shrink-0">
            {composerNotice}
            {composerControlRow}
            {providerThinking && (
              <div
                className="mb-2 flex items-center gap-2 px-1 text-xs text-[var(--g5-text2)]"
                data-testid="provider-thinking-indicator"
              >
                <span className="relative flex h-2 w-2 shrink-0">
                  <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-[var(--g5-active)] opacity-75" />
                  <span className="relative inline-flex rounded-full h-2 w-2 bg-[var(--g5-active)]" />
                </span>
                <span>Thinking...</span>
              </div>
            )}

            {/* Textarea + Send */}
            {queuedInputsTray}
            {referenceTray}
            {attachmentTray}
            {attachmentErrorNotice}
            <div
              className={classNames(
                "flex items-center gap-2 rounded-lg",
                "bg-[var(--g5-bg0)] border border-[var(--g5-stroke)] px-2 py-1.5",
                "focus-within:border-[var(--g5-info)] transition-colors",
              )}
            >
              {referenceButton}
              <GatedControl
                verb="send messages"
                reason={effectiveComposerDisabledReason ?? null}
                testId="chat-composer-disabled-reason"
                className="min-w-0 flex-1"
              >
                <textarea
                  ref={textareaRef}
                  data-testid="chat-input"
                  value={inputValue}
                  onChange={handleComposerChange}
                  onPointerDown={leavePromptHistory}
                  onKeyDown={handleKeyDown}
                  onPaste={handlePasteAttachments}
                  placeholder={activePlaceholder}
                  rows={1}
                  disabled={inputDisabled}
                  className={classNames(
                    "min-h-7 w-full flex-1 resize-none bg-transparent py-1 text-[13px] leading-5",
                    "text-[var(--g5-text1)]",
                    "placeholder-[var(--g5-text2)]/50",
                    "scrollbar-thin scrollbar-thumb-[var(--g5-bg2)]",
                  )}
                />
              </GatedControl>

              {/* Action buttons: stop (when streaming) + send (always) */}
              <div className="flex items-center gap-1 shrink-0">
                {isStreaming && !stopControlSuppressed && onStopStreaming && (
                  <button
                    type="button"
                    onClick={onStopStreaming}
                    disabled={stopRequesting}
                    aria-busy={stopRequesting || undefined}
                    className={classNames(
                      "p-1 rounded-md",
                      "text-[var(--g5-text2)] hover:text-[var(--g5-text0)] hover:bg-[var(--g5-bg2)]",
                      "transition-colors",
                      stopRequesting
                        ? "cursor-progress opacity-60"
                        : "cursor-pointer",
                    )}
                    title={
                      chatStopActivityLabel(sessionStopState) ??
                      "Stop agent (Esc)"
                    }
                    data-testid="stop-streaming-button"
                    data-stop-phase={sessionStopState?.phase}
                  >
                    <Square size={14} />
                  </button>
                )}
                {startAndSend ? (
                  startAndSendButton("chat-panel-send")
                ) : (
                  <button
                    type="button"
                    data-testid="chat-panel-send"
                    data-pulse={sendPulse ? "true" : undefined}
                    onClick={handleSend}
                    disabled={!canSend}
                    className={classNames(
                      "p-1.5 rounded-md",
                      sendPulse && "g5-send-pulse",
                      canSend
                        ? "bg-[var(--g5-info)] text-white cursor-pointer"
                        : "text-[var(--g5-text2)]/30 cursor-not-allowed",
                      "transition-colors",
                    )}
                    title="Send (Enter)"
                  >
                    <Send size={16} />
                  </button>
                )}
              </div>
            </div>
            {sessionStopState?.phase === "failed" && (
              <div
                role="alert"
                data-testid="chat-stop-error"
                className="mt-1 flex items-center gap-2 text-[12px] text-[var(--g5-warn)]"
              >
                <span className="truncate">
                  Stop failed
                  {sessionStopState.error ? `: ${sessionStopState.error}` : ""}
                </span>
                {onStopStreaming && (
                  <button
                    type="button"
                    onClick={onStopStreaming}
                    data-testid="chat-stop-retry"
                    className="rounded border border-current px-1.5 py-0.5 text-[11px] hover:bg-[var(--g5-bg2)]"
                  >
                    Retry
                  </button>
                )}
              </div>
            )}
          </div>
        </div>
      </QuestionAnswerResendContext.Provider>
    </QuestionCardFocusContext.Provider>
  );
});
