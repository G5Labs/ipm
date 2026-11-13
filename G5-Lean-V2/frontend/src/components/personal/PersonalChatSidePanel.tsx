// =============================================================================
// G5 Client — Home's personal chat side panel
// =============================================================================
// The personal workspace's Chat beside Home (personal_workspace
// g5.personal.home): the workbench's own `ChatPanel`, fed by
// `useWorkspaceChatController` and gated by the workbench's live gate, so it
// behaves as Chat does in the workspace: sessions, streaming replies and tool
// activity, queued inputs, approvals, Stop, and Start and send while the VM is
// stopped (g5.personal.start-and-send). Its header carries the VM status and
// the admin-visibility notice (g5.personal.access); files stay in the
// workbench. The panel collapses to a slim rail; its open state is kept
// per browser.
// =============================================================================

import { ExternalLink, MessageSquare, PanelRightClose } from "lucide-react";
import React, { useCallback, useMemo, useState } from "react";
import { Link, useNavigate } from "react-router-dom";

import { useUserSettings } from "../../hooks/useUserSettings";
import { useWorkspaceChatController } from "../../hooks/useWorkspaceChatController";
import {
  WorkspaceLiveGateProvider,
  liveGateCanStart,
  liveGateReason,
} from "../../lib/workspaceLiveGate";
import { vmControlStatus } from "../../lib/workspaceVmControls";
import { ChatPanel } from "../chat/ChatPanel";
import { describeCapacityWait } from "../workspace/vmCapacityWait";
import { StartAndSendNotice } from "../chat/StartAndSendNotice";
import { PersonalVmStatus } from "./PersonalVmStatus";
import { usePersonalHome } from "./personalHomeState";

/** Per-browser open state of the side panel. */
export const PERSONAL_CHAT_OPEN_KEY = "g5.home.personal-chat.open";

function readOpen(): boolean {
  try {
    return window.localStorage.getItem(PERSONAL_CHAT_OPEN_KEY) !== "false";
  } catch {
    return true;
  }
}

export interface PersonalChatSidePanelProps {
  /** Text the onboarding tour puts in the composer (ui g5.ui.onboarding-tour). */
  composerSeed?: { token: number; text: string } | null;
  /** Toasts a failed send or action, as the workbench does. */
  onNotify?: (kind: "error", title: string, message: string) => void;
  /** The signed-in person, whose prompt history this composer recalls. */
  userId?: string;
}

export function PersonalChatSidePanel({
  onNotify,
  userId,
  composerSeed,
}: PersonalChatSidePanelProps) {
  const state = usePersonalHome();
  const navigate = useNavigate();
  const { settings: userSettings } = useUserSettings();
  const [open, setOpen] = useState(readOpen);
  const setOpenPersisted = useCallback((next: boolean) => {
    setOpen(next);
    try {
      window.localStorage.setItem(PERSONAL_CHAT_OPEN_KEY, String(next));
    } catch {
      // Storage may be unavailable; the panel still toggles.
    }
  }, []);

  const workspaceId = state?.workspaceId ?? "";
  const promptHistoryScope = useMemo(
    () => (userId && workspaceId ? { userId, workspaceId } : undefined),
    [userId, workspaceId],
  );
  const live = state?.live;
  const ready = !!state?.detail && !!live?.isOwner;
  const startAndSend = !!live && live.isOwner && liveGateCanStart(live.kind);
  const chat = useWorkspaceChatController({
    workspaceId,
    orgId: state?.orgId ?? "",
    enabled: ready,
    writable: ready && (live?.writable ?? false),
    canControlLiveTurn: ready && (live?.canManageDurableOwnerState ?? false),
    startAndSend,
    onError: onNotify
      ? (title, message) => onNotify("error", title, message)
      : undefined,
  });

  // Viewers own no personal workspace; the main panel says so.
  if (!state || state.personal.status === "error") return null;
  const { mePath, vm } = state;

  if (!open) {
    return (
      <aside
        data-testid="home-personal-chat-collapsed"
        aria-label="Personal chat"
        className="hidden shrink-0 border-l border-[var(--g5-stroke)] bg-[var(--g5-bg1)] lg:flex"
      >
        <button
          type="button"
          data-testid="home-personal-chat-open"
          onClick={() => setOpenPersisted(true)}
          className="flex h-full w-10 flex-col items-center gap-2 pt-4 text-[var(--g5-text1)] hover:bg-[var(--g5-bg2)] hover:text-[var(--g5-text0)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]"
          aria-label="Open personal chat"
          title="Open personal chat"
        >
          <MessageSquare size={16} aria-hidden />
          <span className="text-[11px] font-medium [writing-mode:vertical-rl]">
            Personal chat
          </span>
        </button>
      </aside>
    );
  }

  const composerDisabledReason =
    (startAndSend || !live
      ? null
      : liveGateReason(live.kind, "send messages", {
          workspaceNoun: live.workspaceNoun,
        })) ??
    (chat.pendingApprovalIds.length > 0
      ? "Resolve the pending approval before sending more steering."
      : null);
  const composerNotice = (
    <StartAndSendNotice
      waitReason={
        vm.status === "ok" && vm.data.status === "queued_for_capacity"
          ? describeCapacityWait(vm.data.lastError)
          : null
      }
    />
  );

  return (
    <aside
      data-testid="home-personal-chat"
      aria-label="Personal chat"
      className="hidden w-[min(480px,40vw)] shrink-0 flex-col border-l border-[var(--g5-stroke)] bg-[var(--g5-bg1)] lg:flex"
    >
      <div className="flex items-center gap-2 border-b border-[var(--g5-stroke)] px-3 py-2">
        <MessageSquare
          size={14}
          className="text-[var(--g5-info)]"
          aria-hidden
        />
        <h2 className="text-sm font-semibold text-[var(--g5-text0)]">
          Personal chat
        </h2>
        {workspaceId && (
          <PersonalVmStatus
            status={vmControlStatus(vm)}
            testId="home-personal-chat-vm-status"
          />
        )}
        <Link
          to={mePath}
          data-testid="home-personal-chat-open-workspace"
          className="ml-auto inline-flex items-center gap-1 text-xs font-medium text-[var(--g5-info)] hover:underline"
        >
          Open in workspace
          <ExternalLink size={12} aria-hidden />
        </Link>
        <button
          type="button"
          data-testid="home-personal-chat-close"
          onClick={() => setOpenPersisted(false)}
          className="rounded p-1 text-[var(--g5-text2)] hover:bg-[var(--g5-bg2)] hover:text-[var(--g5-text0)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]"
          aria-label="Collapse personal chat"
          title="Collapse personal chat"
        >
          <PanelRightClose size={14} aria-hidden />
        </button>
      </div>
      <p
        role="note"
        data-testid="home-personal-chat-admin-notice"
        className="border-b border-[var(--g5-stroke)] px-3 py-1.5 text-[11px] text-[var(--g5-text2)]"
      >
        Only you can change your personal workspace. Organization administrators
        can see your personal work.
      </p>
      <div className="flex min-h-0 flex-1 flex-col">
        {!ready || !live ? (
          <p
            role="status"
            data-testid="home-personal-chat-loading"
            className="p-4 text-xs text-[var(--g5-text2)]"
          >
            Loading your personal workspace…
          </p>
        ) : (
          <WorkspaceLiveGateProvider value={live.gate}>
            <ChatPanel
              key={workspaceId}
              layout="panel"
              promptHistoryScope={promptHistoryScope}
              onAcknowledgeResult={chat.onAcknowledgeResult}
              session={chat.session}
              messages={chat.messages}
              isStreaming={chat.isStreaming}
              stopControlSuppressed={chat.stopControlSuppressed}
              stopState={chat.stopState}
              liveTailState={chat.liveTailState}
              timelineEntries={chat.timelineEntries}
              hasEarlierHistory={chat.hasEarlierHistory}
              isLoadingEarlierHistory={chat.isLoadingEarlierHistory}
              onLoadEarlierHistory={chat.onLoadEarlierHistory}
              timelineCoverage={chat.timelineCoverage}
              earlierHistoryError={chat.earlierHistoryError}
              hasOlderMessages={chat.hasOlderMessages}
              isLoadingOlderMessages={chat.isLoadingOlderMessages}
              onLoadOlderMessages={chat.onLoadOlderMessages}
              activeTaskSummary={chat.activeTaskSummary}
              scopeType="workspace"
              scopeRef={workspaceId}
              scopeLabel="Personal workspace"
              sessions={chat.sessions}
              selectedAgentProvider={chat.selectedAgentProvider}
              availableAgentProviders={chat.availableAgentProviders}
              providerAccountsEnabled={chat.providerAccountsEnabled}
              providerAccounts={chat.providerAccounts}
              providerAccountAssignments={chat.providerAccountAssignments}
              runtimeVariablesAvailable={chat.runtimeVariablesAvailable}
              onSelectAgentProvider={chat.onSelectAgentProvider}
              resolveAgentLaunch={chat.resolveAgentLaunch}
              defaultClaudeModel={chat.defaultClaudeModel}
              defaultAgentProviderChoices={
                userSettings.defaultAgentProviderChoices
              }
              subscriptionUsageWindow={userSettings.subscriptionUsageWindow}
              workspaceSubscriptionUsage={chat.workspaceSubscriptionUsage}
              setupError={chat.setupError}
              composerDisabledReason={composerDisabledReason}
              startAndSend={startAndSend}
              composerSeed={composerSeed ?? null}
              composerNotice={composerNotice}
              canManageSessionInputs={live.canManageDurableOwnerState}
              onResolveAgentApproval={chat.onResolveAgentApproval}
              onSendMessage={chat.onSendMessage}
              onStopStreaming={chat.onStopStreaming}
              onEndBackgroundWait={chat.onEndBackgroundWait}
              onForkSession={chat.onForkSession}
              onSummarize={() => {}}
              onCreateSession={chat.onCreateSession}
              onSelectSession={chat.onSelectSession}
              onRenameSession={chat.onRenameSession}
              // A cited file opens in the personal workbench.
              onOpenFile={(filePath) =>
                navigate(
                  `${mePath}/file/${filePath
                    .replace(/^\.\//, "")
                    .split("/")
                    .map(encodeURIComponent)
                    .join("/")}`,
                )
              }
            />
          </WorkspaceLiveGateProvider>
        )}
      </div>
    </aside>
  );
}
