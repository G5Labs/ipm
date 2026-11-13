// =============================================================================
// G5 Client — Home personal panel
// =============================================================================
// The caller's personal workspace on Home (personal_workspace
// g5.personal.home) below the width at which Home shows its Chat side panel:
// status, the admin-visibility notice, their personal conversations and a
// composer that sends without choosing a workspace, working while the VM is
// stopped. The panel builds the same live gate the workbench does
// (`useWorkspaceLiveGateValue`) and handles Start for its own workspace, so a
// stopped composer offers Start and send (g5.personal.start-and-send) and
// every reason reads as in the workbench. Files and the full conversation
// open in the workbench at `/{org}/me`.
// =============================================================================

import {
  ExternalLink,
  MessageSquare,
  Plus,
  Send,
  UserRound,
} from "lucide-react";
import { useId, useMemo, useState } from "react";
import { Link } from "react-router-dom";

import { useResolveAgentLaunch } from "../../hooks/useAgentLaunch";
import {
  useChatSessions,
  useSendChatMessage,
  useStartChatMessage,
} from "../../hooks/useChat";
import { ApiError } from "../../lib/api";
import {
  configuredAgentLaunchPreferences,
  type ChatSessionDTO,
} from "../../lib/types";
import { chatLandingQuery } from "../../lib/workspaceLanding";
import {
  GatedControl,
  liveGateCanStart,
  liveGateReason,
  WorkspaceLiveGateProvider,
} from "../../lib/workspaceLiveGate";
import { personalWorkspaceMePath } from "../../lib/workspacePagePath";
import {
  vmControlAdmission,
  vmControlStatus,
} from "../../lib/workspaceVmControls";
import { describeCapacityWait } from "../workspace/vmCapacityWait";
import { classNames } from "../ui/UI";
import { PersonalHomeProvider } from "./PersonalHomeContext";
import { usePersonalHome, type PersonalHomeState } from "./personalHomeState";
import { StartAndSendNotice } from "../chat/StartAndSendNotice";
import { PersonalVmStatus } from "./PersonalVmStatus";

/** Conversations listed before "Open in workspace" takes over. */
const PERSONAL_SESSIONS_SHOWN = 6;

export interface PersonalPanelProps {
  orgId: string;
  orgSlug: string;
  userId: string;
  /** The caller administers the organization (gate wording only). */
  orgAdmin: boolean;
  /**
   * Home shows the full Chat, status and notice in its side panel on wide
   * screens, so this panel is for narrow ones only.
   */
  chatInSidePanel?: boolean;
  className?: string;
}

function sessionLabel(session: ChatSessionDTO): string {
  return session.title?.trim() || "Untitled conversation";
}

export function PersonalPanel(props: PersonalPanelProps) {
  const shared = usePersonalHome();
  if (shared) {
    return (
      <PersonalPanelBody
        state={shared}
        chatInSidePanel={props.chatInSidePanel === true}
        className={props.className}
      />
    );
  }
  return (
    <PersonalHomeProvider
      orgId={props.orgId}
      orgSlug={props.orgSlug}
      userId={props.userId}
      orgAdmin={props.orgAdmin}
    >
      <PersonalPanel {...props} />
    </PersonalHomeProvider>
  );
}

function PersonalPanelBody({
  state,
  chatInSidePanel,
  className,
}: {
  state: PersonalHomeState;
  chatInSidePanel: boolean;
  className?: string;
}) {
  const headingId = useId();
  const {
    orgSlug,
    personal,
    workspaceId,
    workspace,
    detail,
    vm,
    live,
    start: handleStart,
    starting,
    startError,
  } = state;
  const resume = { isLoading: starting };
  if (personal.status === "error") {
    const refused =
      personal.error instanceof ApiError && personal.error.status === 403;
    return (
      <section
        data-testid="home-personal-panel"
        aria-labelledby={headingId}
        className={classNames(
          "rounded-[var(--g5-radius-lg)] border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] p-4",
          className,
        )}
      >
        <h2
          id={headingId}
          className="text-sm font-semibold text-[var(--g5-text0)]"
        >
          Personal workspace
        </h2>
        <p
          role={refused ? "note" : "alert"}
          className="mt-1 text-xs text-[var(--g5-text2)]"
        >
          {refused
            ? "Viewers have no personal workspace in this organization."
            : "Your personal workspace could not be loaded. Reload Home to try again."}
        </p>
      </section>
    );
  }

  const mePath = personalWorkspaceMePath(orgSlug);
  const status = vmControlStatus(vm);
  const { canStart } = vmControlAdmission(status, {
    canControl: live.isOwner,
    isBase: false,
  });

  return (
    <WorkspaceLiveGateProvider value={live.gate}>
      <section
        data-testid="home-personal-panel"
        aria-labelledby={headingId}
        className={classNames(
          "rounded-[var(--g5-radius-lg)] border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] p-4",
          chatInSidePanel ? "lg:hidden" : undefined,
          className,
        )}
      >
        <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
          <UserRound size={16} className="text-[var(--g5-info)]" aria-hidden />
          <h2
            id={headingId}
            className="text-sm font-semibold text-[var(--g5-text0)]"
          >
            Personal workspace
          </h2>
          {workspaceId && (
            <PersonalVmStatus
              status={status}
              testId="home-personal-vm-status"
            />
          )}
          {canStart && (
            <button
              type="button"
              data-testid="home-personal-start"
              onClick={() => void handleStart()}
              disabled={resume.isLoading}
              className="rounded border border-[var(--g5-stroke)] px-2 py-0.5 text-xs font-medium text-[var(--g5-text0)] hover:bg-[var(--g5-bg2)] disabled:opacity-60"
              title="Starts your personal workspace's VM"
            >
              Start
            </button>
          )}
          {startError && (
            <span role="alert" className="text-xs text-[var(--g5-blocked)]">
              {startError}
            </span>
          )}
          <Link
            to={mePath}
            data-testid="home-personal-open"
            className="ml-auto inline-flex items-center gap-1 text-xs font-medium text-[var(--g5-info)] hover:underline"
          >
            Open in workspace
            <ExternalLink size={12} aria-hidden />
          </Link>
        </div>
        <p
          role="note"
          data-testid="home-personal-admin-notice"
          className="mt-1 text-xs text-[var(--g5-text2)]"
        >
          Only you can change it. Organization administrators can see your
          personal work.
        </p>

        {personal.status === "loading" || !detail ? (
          <p
            role="status"
            className="mt-3 text-xs text-[var(--g5-text2)]"
            data-testid="home-personal-loading"
          >
            {workspace.status === "error"
              ? "Your personal workspace could not be read."
              : "Loading your personal workspace…"}
          </p>
        ) : (
          <div className="mt-3">
            <PersonalConversations
              workspaceId={workspaceId}
              orgSlug={orgSlug}
              mePath={mePath}
              startAndSend={live.isOwner && liveGateCanStart(live.kind)}
              disabledReason={
                live.isOwner && liveGateCanStart(live.kind)
                  ? null
                  : liveGateReason(live.kind, "send messages", {
                      workspaceNoun: live.workspaceNoun,
                    })
              }
              waitReason={
                vm.status === "ok" && vm.data.status === "queued_for_capacity"
                  ? describeCapacityWait(vm.data.lastError)
                  : null
              }
            />
          </div>
        )}
      </section>
    </WorkspaceLiveGateProvider>
  );
}

// ---------------------------------------------------------------------------
// Conversations and composer
// ---------------------------------------------------------------------------

/**
 * Whether a send queued its message behind the Start it recorded
 * (`g5.personal.start-and-send`). A send to a running VM, or a Start that
 * attached to one, is delivered at once.
 */
function startRecorded(start: { status: string } | null | undefined) {
  return !!start && start.status !== "running";
}

function PersonalConversations({
  workspaceId,
  orgSlug,
  mePath,
  startAndSend,
  disabledReason,
  waitReason,
}: {
  workspaceId: string;
  orgSlug: string;
  mePath: string;
  startAndSend: boolean;
  disabledReason: string | null;
  waitReason: string | null;
}) {
  const ids = useId();
  const sessions = useChatSessions(workspaceId, "workspace");
  const sendMessage = useSendChatMessage();
  const startChat = useStartChatMessage();
  const resolveAgentLaunch = useResolveAgentLaunch(workspaceId);
  const [selectedSessionId, setSelectedSessionId] = useState<string | null>(
    null,
  );
  const [draft, setDraft] = useState("");
  const [sending, setSending] = useState(false);
  const [sendError, setSendError] = useState<string | null>(null);
  const [lastSent, setLastSent] = useState<{
    sessionId: string;
    queued: boolean;
  } | null>(null);

  const sorted = useMemo(
    () =>
      sessions.status === "ok"
        ? [...sessions.data].sort(
            (left, right) =>
              new Date(right.updatedAt).getTime() -
              new Date(left.updatedAt).getTime(),
          )
        : [],
    [sessions],
  );
  const selected =
    sorted.find((session) => session.id === selectedSessionId) ?? null;
  const blocked = disabledReason !== null;
  const canSend = !blocked && !sending && draft.trim().length > 0;

  const send = async () => {
    const content = draft.trim();
    if (!canSend || !content) return;
    setSending(true);
    setSendError(null);
    const startCompute = startAndSend ? true : undefined;
    try {
      if (selected) {
        const response = await sendMessage.mutateAsync({
          sessionId: selected.id,
          content,
          startCompute,
        });
        setLastSent({
          sessionId: selected.id,
          queued: startRecorded(response.startCompute),
        });
      } else {
        const resolved = await resolveAgentLaunch.mutateAsync(
          configuredAgentLaunchPreferences(),
        );
        const started = await startChat.mutateAsync({
          workspaceId,
          content,
          scopeType: "workspace",
          scopeRef: workspaceId,
          agentLaunch: {
            preferences: configuredAgentLaunchPreferences(),
            confirmedResolutionId: resolved.resolutionId,
          },
          startCompute,
        });
        setSelectedSessionId(started.session.id);
        setLastSent({
          sessionId: started.session.id,
          queued: startRecorded(
            started.startCompute ?? started.message?.startCompute,
          ),
        });
      }
      setDraft("");
    } catch (error) {
      setSendError(
        error instanceof Error && error.message
          ? error.message
          : "Could not send your message.",
      );
    } finally {
      setSending(false);
    }
  };

  const shown = sorted.slice(0, PERSONAL_SESSIONS_SHOWN);
  return (
    <div className="flex min-w-0 flex-col gap-2">
      <div className="flex items-center justify-between">
        <h3
          id={`${ids}-conversations`}
          className="text-[11px] font-semibold uppercase tracking-[0.12em] text-[var(--g5-text2)]"
        >
          Conversations
        </h3>
        <button
          type="button"
          data-testid="home-personal-new-conversation"
          onClick={() => {
            setSelectedSessionId(null);
            setLastSent(null);
          }}
          aria-pressed={selected === null}
          className="inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-xs text-[var(--g5-text1)] hover:bg-[var(--g5-bg2)]"
        >
          <Plus size={12} aria-hidden />
          New conversation
        </button>
      </div>
      <ul
        aria-labelledby={`${ids}-conversations`}
        data-testid="home-personal-conversations"
        className="flex flex-col gap-0.5"
      >
        {sessions.status === "loading" && (
          <li role="status" className="text-xs text-[var(--g5-text2)]">
            Loading conversations…
          </li>
        )}
        {sessions.status === "error" && (
          <li role="alert" className="text-xs text-[var(--g5-blocked)]">
            Could not load your conversations.
          </li>
        )}
        {sessions.status === "ok" && sorted.length === 0 && (
          <li className="text-xs text-[var(--g5-text2)]">
            No conversations yet. Ask anything below.
          </li>
        )}
        {shown.map((session) => (
          <li key={session.id} className="flex min-w-0 items-center gap-1">
            <button
              type="button"
              data-testid="home-personal-conversation"
              aria-pressed={session.id === selectedSessionId}
              onClick={() => {
                setSelectedSessionId(session.id);
                setLastSent(null);
              }}
              className={classNames(
                "flex min-w-0 flex-1 items-center gap-2 rounded px-2 py-1 text-left text-xs",
                session.id === selectedSessionId
                  ? "bg-[var(--g5-bg2)] text-[var(--g5-text0)]"
                  : "text-[var(--g5-text1)] hover:bg-[var(--g5-bg2)]",
              )}
            >
              <MessageSquare
                size={12}
                className="shrink-0 text-[var(--g5-text2)]"
                aria-hidden
              />
              <span className="truncate">{sessionLabel(session)}</span>
            </button>
            <Link
              to={`${mePath}${chatLandingQuery(session.id)}`}
              aria-label={`Open ${sessionLabel(session)} in your personal workspace`}
              title="Open in workspace"
              className="shrink-0 rounded p-1 text-[var(--g5-text2)] hover:bg-[var(--g5-bg2)] hover:text-[var(--g5-text0)]"
            >
              <ExternalLink size={12} aria-hidden />
            </Link>
          </li>
        ))}
        {sorted.length > shown.length && (
          <li>
            <Link
              to={mePath}
              className="px-2 text-xs text-[var(--g5-info)] hover:underline"
            >
              All {sorted.length} conversations
            </Link>
          </li>
        )}
      </ul>

      <StartAndSendNotice waitReason={waitReason} />
      <div className="flex flex-col gap-2 rounded-lg border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] p-2 focus-within:border-[var(--g5-info)]">
        <label htmlFor={`${ids}-composer`} className="sr-only">
          {selected
            ? `Message ${sessionLabel(selected)}`
            : "Message a new personal conversation"}
        </label>
        <GatedControl
          verb="send messages"
          reason={disabledReason}
          testId="home-personal-composer-disabled-reason"
          className="w-full"
        >
          <textarea
            id={`${ids}-composer`}
            data-testid="home-personal-composer"
            value={draft}
            rows={3}
            disabled={blocked || sending}
            placeholder={
              disabledReason ??
              (selected
                ? `Continue “${sessionLabel(selected)}”…`
                : "Ask anything — no workspace to choose")
            }
            onChange={(event) => {
              setDraft(event.target.value);
              setSendError(null);
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter" && !event.shiftKey) {
                event.preventDefault();
                void send();
              }
            }}
            className="block w-full resize-none bg-transparent px-1 text-sm text-[var(--g5-text0)] placeholder:text-[var(--g5-text2)]/70 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-[var(--g5-info)] disabled:cursor-not-allowed"
          />
        </GatedControl>
        <div className="flex items-center gap-2">
          <span className="min-w-0 flex-1 truncate text-[11px] text-[var(--g5-text2)]">
            {selected
              ? `Continues ${sessionLabel(selected)}`
              : "Starts a new conversation"}
          </span>
          <button
            type="button"
            data-testid={
              startAndSend
                ? "home-personal-start-and-send"
                : "home-personal-send"
            }
            onClick={() => void send()}
            disabled={!canSend}
            title={
              startAndSend
                ? "Starts your personal workspace and sends this message once it is running (Enter)"
                : "Send (Enter)"
            }
            className={classNames(
              "inline-flex h-7 shrink-0 items-center gap-1.5 rounded-md px-2.5 text-xs font-medium transition-colors",
              canSend
                ? "cursor-pointer bg-[var(--g5-info)] text-white hover:brightness-110"
                : "cursor-not-allowed bg-[var(--g5-bg2)] text-[var(--g5-text2)]/60",
            )}
          >
            <Send size={12} aria-hidden />
            {startAndSend ? "Start and send" : "Send"}
          </button>
        </div>
      </div>
      {sendError && (
        <p role="alert" className="text-xs text-[var(--g5-blocked)]">
          {sendError}
        </p>
      )}
      {lastSent && !sendError && (
        <p
          role="status"
          data-testid="home-personal-sent"
          className="text-xs text-[var(--g5-text2)]"
        >
          {lastSent.queued
            ? "Queued; it is sent once your personal workspace is running. "
            : "Sent. "}
          <Link
            to={`/${orgSlug}/me${chatLandingQuery(lastSent.sessionId)}`}
            className="text-[var(--g5-info)] hover:underline"
          >
            Open the conversation
          </Link>
        </p>
      )}
    </div>
  );
}
