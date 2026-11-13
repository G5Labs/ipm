// =============================================================================
// G5 Workspace Page — TopBar
// =============================================================================
// Fixed 44px horizontal strip at the top of the workspace page under the
// classic layouts. Provides all global workspace controls: sidebar toggle, G5
// branding, workspace switching, user presence, notifications, settings, and
// chat toggle. The alignment layout renders no title bar (workbench
// g5.workbench.shell-chrome): the tab strip and the ribbon carry its controls.
//
// On mobile (below lg breakpoint): shows simplified header with Smartphone
// icon, "Mobile" badge, and notification bell.
// =============================================================================

import {
  Activity as ActivityIcon,
  CircleDot,
  CircleUserRound,
  Menu,
  MessageSquare,
  Server,
  Smartphone,
  Settings,
} from "lucide-react";
import React from "react";
import type { WorkspaceDetail, WorkspaceSummary } from "../../lib/types";
import type { WorkspaceUiCapabilities } from "../../lib/capabilities";
import { NotificationBell } from "../layout/NotificationBell";
import { PlatformIssueReportTrigger } from "../platform-issues/PlatformIssueReportButton";
import { classNames } from "../ui/UI";
import { WorkspaceSwitcher } from "./WorkspaceSwitcher";

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface TopBarProps {
  /** Current workspace detail */
  workspace: WorkspaceDetail | null;
  /** Organization ID for workspace switching */
  orgId: string;
  /** Organization slug (route segment) used to build workspace row anchor hrefs */
  orgSlug: string;
  /** Whether the left sidebar is currently open */
  leftSidebarOpen: boolean;
  /** Toggle left sidebar visibility */
  onToggleLeftSidebar: () => void;
  /** Whether the right chat panel is currently open */
  rightPanelOpen: boolean;
  /** Toggle right chat panel visibility */
  onToggleChat: () => void;
  /**
   * Whether chat is available in this workspace. When false (e.g. a read-only
   * workspace, where the right chat panel is not rendered and chat writes are
   * blocked server-side), the chat toggle is hidden so it is not a dead
   * affordance. Defaults to true.
   * @ref software_architecture/client/workspace-page/top-bar#D27
   */
  chatAvailable?: boolean;
  /**
   * Whether to render the desktop chat toggle. Central-chat workspace layouts
   * keep chat available through the center tab and hide this duplicate control
   * while that tab strip is shown (workspaces g5.workspace.ui.center-tabs).
   * Defaults to true.
   */
  showChatToggle?: boolean;
  /** Open settings tab */
  onOpenSettings: () => void;
  /** Open the retained org worker-host list and provisioning page */
  onOpenWorkerHosts?: () => void;
  /** Open account/org management flyout */
  onOpenOrgManagement: () => void;
  /** Open notifications flyout */
  onOpenNotifications: () => void;
  /** Navigate to a different workspace */
  onSelectWorkspace: (workspaceId: string) => void;
  /** Create a new workspace */
  onNewWorkspace: () => void;
  /** Fork the current workspace */
  onForkCurrentWorkspace?: () => void;
  /** Open the read-only Workspace Details page for the current workspace */
  onOpenWorkspaceDetails?: () => void;
  /** Fork a specific workspace chosen from the switcher dropdown */
  onForkWorkspace?: (workspace: WorkspaceSummary) => void;
  /** Remove a specific workspace chosen from the switcher dropdown */
  onRemoveWorkspace?: (workspace: WorkspaceSummary) => void;
  /** UI-only gate for whether the switcher can show remove controls */
  canRemoveWorkspace?: (workspace: WorkspaceSummary) => boolean;
  /** Active collaborators from workspace presence */
  collaborators: Array<{
    id: string;
    userId: string;
    name: string;
    avatar?: string;
    color: string;
  }>;
  /** Unread notification count */
  unreadNotificationCount: number;
  /** Whether on mobile layout (below lg breakpoint) */
  isMobile: boolean;
  /** UI capability gates for workspace creation/fork affordances */
  capabilities?: WorkspaceUiCapabilities;
  /** The current workspace is a review workspace (reviewer-owned fork). */
  isReviewWorkspace?: boolean;
  /**
   * Whether the page has learned yet if the current workspace is a review
   * copy; until it has, its switcher row offers no Fork.
   */
  reviewIdentityKnown?: boolean;
  /**
   * Open the workspace Activity (primary issue discussion with workspace
   * context). The caller passes it only while the `external_issues` feature
   * is on; omitted hides the entrypoint.
   */
  onOpenActivity?: () => void;
  /**
   * The workspace's primary external issue, shown as a chip after the
   * workspace switcher. `state` is null when the live summary is unknown.
   * Omitted/null hides the chip (no primary, or feature off).
   */
  primaryIssue?: TopBarPrimaryIssue | null;
}

export interface TopBarPrimaryIssue {
  displayKey: string;
  title: string | null;
  state: "open" | "closed" | null;
  onOpen: () => void;
}

// ---------------------------------------------------------------------------
// MAX_VISIBLE_AVATARS
// ---------------------------------------------------------------------------

const MAX_VISIBLE_AVATARS = 3;

// ---------------------------------------------------------------------------
// CollaboratorAvatars — connected user presence indicators
// ---------------------------------------------------------------------------

const CollaboratorAvatars: React.FC<{
  collaborators: TopBarProps["collaborators"];
}> = ({ collaborators }) => {
  if (collaborators.length === 0) return null;

  const visible = collaborators.slice(0, MAX_VISIBLE_AVATARS);
  const overflow = collaborators.length - MAX_VISIBLE_AVATARS;

  return (
    <div data-testid="user-avatar" className="flex items-center -space-x-1.5">
      {visible.map((c) => {
        // Defensive fallback: awareness/user payloads can be partially populated.
        const safeName =
          typeof c.name === "string" && c.name.length > 0 ? c.name : "Unknown";
        const fallbackInitial = safeName.charAt(0).toUpperCase();
        return (
          <span
            key={c.id}
            className={classNames(
              "w-6 h-6 rounded-full flex items-center justify-center",
              "text-[10px] font-medium text-white",
              "border-2 border-[var(--g5-bg0)]",
              "shrink-0",
            )}
            style={{ backgroundColor: c.color + "30", color: c.color }}
            title={safeName}
          >
            {c.avatar && c.avatar.startsWith("http") ? (
              <img
                src={c.avatar}
                alt={safeName}
                className="w-full h-full rounded-full object-cover"
              />
            ) : (
              c.avatar || fallbackInitial
            )}
          </span>
        );
      })}
      {overflow > 0 && (
        <span
          className={classNames(
            "w-6 h-6 rounded-full flex items-center justify-center",
            "text-[10px] font-medium",
            "bg-[var(--g5-bg1)] text-[var(--g5-text2)]",
            "border-2 border-[var(--g5-bg0)]",
            "shrink-0",
          )}
        >
          +{overflow}
        </span>
      )}
      <span className="text-[10px] text-[var(--g5-text2)] pl-2.5">
        {collaborators.length} online
      </span>
    </div>
  );
};

// ---------------------------------------------------------------------------
// PrimaryIssueChip — the workspace's primary external issue
// ---------------------------------------------------------------------------

const PrimaryIssueChip: React.FC<{ issue: TopBarPrimaryIssue }> = ({
  issue,
}) => {
  const stateLabel =
    issue.state === "open"
      ? "Open"
      : issue.state === "closed"
        ? "Closed"
        : null;
  const accessibleName = [
    `Primary issue ${issue.displayKey}`,
    stateLabel,
    issue.title,
  ]
    .filter(Boolean)
    .join(", ");
  return (
    <button
      type="button"
      data-testid="top-bar-primary-issue"
      className={classNames(
        "inline-flex h-7 min-w-0 max-w-[260px] shrink items-center gap-1.5 px-2",
        "rounded-full border border-[var(--g5-stroke)] bg-[var(--g5-bg1)]",
        "text-xs font-medium text-[var(--g5-text0)]",
        "hover:bg-[var(--g5-bg2)] transition-colors duration-150 cursor-pointer",
        "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]",
      )}
      onClick={issue.onOpen}
      title={issue.title ?? issue.displayKey}
      aria-label={accessibleName}
    >
      <CircleDot
        size={13}
        aria-hidden="true"
        className={classNames(
          "shrink-0",
          issue.state === "closed"
            ? "text-[var(--g5-text2)]"
            : "text-[var(--g5-ok)]",
        )}
      />
      <span className="truncate">{issue.displayKey}</span>
      {stateLabel && (
        <span
          data-testid="top-bar-primary-issue-state"
          className="shrink-0 text-[10px] uppercase tracking-wide text-[var(--g5-text2)]"
        >
          {stateLabel}
        </span>
      )}
    </button>
  );
};

// ---------------------------------------------------------------------------
// TopBar Component
// ---------------------------------------------------------------------------

/**
 * Fixed 44px top bar with all global workspace controls.
 *
 * Layout regions:
 * - Left: Sidebar toggle (Menu icon) + "G5" brand text
 * - Center-left: WorkspaceSwitcher dropdown
 * - Right: Collaborator avatars, Notification bell,
 *          Settings gear, Chat toggle
 *
 * Mobile: Smartphone icon + "Mobile" badge and notification bell.
 */
export const TopBar: React.FC<TopBarProps> = ({
  workspace,
  orgId,
  orgSlug,
  leftSidebarOpen,
  onToggleLeftSidebar,
  rightPanelOpen,
  onToggleChat,
  chatAvailable = true,
  showChatToggle = true,
  onOpenSettings,
  onOpenWorkerHosts,
  onOpenOrgManagement,
  onOpenNotifications,
  onSelectWorkspace,
  onNewWorkspace,
  onForkCurrentWorkspace,
  onOpenWorkspaceDetails,
  onForkWorkspace,
  onRemoveWorkspace,
  canRemoveWorkspace,
  collaborators,
  unreadNotificationCount,
  isMobile,
  capabilities,
  isReviewWorkspace = false,
  reviewIdentityKnown = true,
  onOpenActivity,
  primaryIssue,
}) => {
  const currentWorkspaceSummary: WorkspaceSummary | null = workspace
    ? {
        id: workspace.id,
        name: workspace.name,
        description: workspace.description,
        isBase: workspace.isBase,
        readOnly: workspace.readOnly,
        createdBy: workspace.createdBy,
        clonedFromId: workspace.clonedFromId,
        branches: workspace.branches,
        tasksCompleted: 0,
        totalTasks: 0,
        runningTime: null,
        cost: 0,
        concurrentTasks: 0,
        lastUpdated: workspace.updatedAt,
        createdAt: workspace.createdAt,
      }
    : null;

  // -------------------------------------------------------------------------
  // Mobile Layout
  // -------------------------------------------------------------------------

  if (isMobile) {
    return (
      <div
        data-surface="titlebar"
        className={classNames(
          "bg-[var(--g5-bg0)]/80 backdrop-blur",
          "border-b border-[var(--g5-stroke)]",
        )}
      >
        <div className="h-11 flex items-center px-3 gap-2">
          {/* Mobile indicator */}
          <Smartphone size={16} className="text-[var(--g5-text1)] shrink-0" />
          <span
            className={classNames(
              "text-[10px] font-medium uppercase tracking-wide",
              "bg-[var(--g5-bg1)] text-[var(--g5-text2)]",
              "px-1.5 py-0.5 rounded-[var(--g5-radius-sm)]",
            )}
          >
            Mobile
          </span>

          <div className="flex-1" />

          {/* Notification bell */}
          <NotificationBell
            unreadCount={unreadNotificationCount}
            onClick={onOpenNotifications}
          />

          <PlatformIssueReportTrigger placement="topbar" />
        </div>
      </div>
    );
  }

  // -------------------------------------------------------------------------
  // Desktop Layout
  // -------------------------------------------------------------------------

  return (
    <div
      data-surface="titlebar"
      className={classNames(
        "h-11 flex items-center px-3 gap-2",
        "bg-[var(--g5-bg0)]/80 backdrop-blur",
        "border-b border-[var(--g5-stroke)]",
        "relative z-40",
      )}
    >
      {/* Left region: Sidebar toggle + G5 brand */}
      <div className="flex items-center gap-2 shrink-0">
        <button
          type="button"
          className={classNames(
            "inline-flex items-center justify-center",
            "w-8 h-8",
            "rounded-[var(--g5-radius-sm)]",
            "text-[var(--g5-text1)]",
            "hover:bg-[var(--g5-bg1)] hover:text-[var(--g5-text0)]",
            "transition-colors duration-150",
            "cursor-pointer",
            "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]",
          )}
          onClick={onToggleLeftSidebar}
          data-testid="top-bar-toggle-left-sidebar"
          title={leftSidebarOpen ? "Close sidebar" : "Open sidebar"}
          aria-label={leftSidebarOpen ? "Close sidebar" : "Open sidebar"}
        >
          <Menu size={18} />
        </button>

        <span className="font-bold text-[var(--g5-text0)] text-sm select-none">
          G5
        </span>
      </div>

      {/* Center-left: WorkspaceSwitcher */}
      <WorkspaceSwitcher
        currentWorkspace={currentWorkspaceSummary}
        orgId={orgId}
        orgSlug={orgSlug}
        onSelectWorkspace={onSelectWorkspace}
        onNewWorkspace={onNewWorkspace}
        onForkCurrentWorkspace={onForkCurrentWorkspace}
        onOpenWorkspaceDetails={onOpenWorkspaceDetails}
        onForkWorkspace={onForkWorkspace}
        onRemoveWorkspace={onRemoveWorkspace}
        canRemoveWorkspace={canRemoveWorkspace}
        capabilities={capabilities}
        currentWorkspaceIsReview={isReviewWorkspace}
        reviewIdentityKnown={reviewIdentityKnown}
      />

      {onOpenActivity && (
        <button
          type="button"
          data-testid="top-bar-activity"
          className={classNames(
            "inline-flex items-center justify-center",
            "w-8 h-8 shrink-0",
            "rounded-[var(--g5-radius-sm)]",
            "text-[var(--g5-text1)]",
            "hover:bg-[var(--g5-bg1)] hover:text-[var(--g5-text0)]",
            "transition-colors duration-150",
            "cursor-pointer",
            "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]",
          )}
          onClick={onOpenActivity}
          title="Activity"
          aria-label="Activity"
        >
          <ActivityIcon size={15} />
        </button>
      )}
      {primaryIssue && <PrimaryIssueChip issue={primaryIssue} />}

      <div className="flex-1" />

      {/* Right region: Avatars, Workers, Notifications, Settings, Org, Chat */}
      <div className="flex items-center gap-1 shrink-0 ml-auto">
        {/* Connected user avatars */}
        <CollaboratorAvatars collaborators={collaborators} />

        {onOpenWorkerHosts && (
          <button
            type="button"
            data-testid="top-bar-worker-hosts"
            className={classNames(
              "inline-flex items-center justify-center",
              "w-8 h-8",
              "rounded-[var(--g5-radius-sm)]",
              "text-[var(--g5-text1)]",
              "hover:bg-[var(--g5-bg1)] hover:text-[var(--g5-text0)]",
              "transition-colors duration-150",
              "cursor-pointer",
              "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]",
            )}
            onClick={onOpenWorkerHosts}
            title="Workers"
            aria-label="Workers"
          >
            <Server size={16} />
          </button>
        )}

        {/* Notification bell */}
        <NotificationBell
          unreadCount={unreadNotificationCount}
          onClick={onOpenNotifications}
        />

        <PlatformIssueReportTrigger placement="topbar" />

        {/* Settings button — opens the central preferences/settings tab */}
        <button
          type="button"
          data-testid="settings-button"
          className={classNames(
            "inline-flex items-center justify-center",
            "w-8 h-8",
            "rounded-[var(--g5-radius-sm)]",
            "text-[var(--g5-text1)]",
            "hover:bg-[var(--g5-bg1)] hover:text-[var(--g5-text0)]",
            "transition-colors duration-150",
            "cursor-pointer",
            "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]",
          )}
          onClick={onOpenSettings}
          title="Settings"
          aria-label="Settings"
        >
          <Settings size={16} />
        </button>

        {/* Org management button — opens the Account/Organizations flyout */}
        <button
          type="button"
          data-testid="org-management-button"
          className={classNames(
            "inline-flex items-center justify-center",
            "w-8 h-8",
            "rounded-[var(--g5-radius-sm)]",
            "text-[var(--g5-text1)]",
            "hover:bg-[var(--g5-bg1)] hover:text-[var(--g5-text0)]",
            "transition-colors duration-150",
            "cursor-pointer",
            "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]",
          )}
          onClick={onOpenOrgManagement}
          title="Org management"
          aria-label="Org management"
        >
          <CircleUserRound size={16} />
        </button>

        {/* Chat toggle — hidden when chat is unavailable (e.g. read-only
            workspace); @ref .../top-bar#D27 */}
        {chatAvailable && showChatToggle && (
          <button
            type="button"
            className={classNames(
              "inline-flex items-center justify-center",
              "w-8 h-8",
              "rounded-[var(--g5-radius-sm)]",
              rightPanelOpen
                ? "text-[var(--g5-info)] bg-[var(--g5-bg1)]"
                : "text-[var(--g5-text1)]",
              "hover:bg-[var(--g5-bg1)] hover:text-[var(--g5-text0)]",
              "transition-colors duration-150",
              "cursor-pointer",
              "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]",
            )}
            onClick={onToggleChat}
            data-testid="toggle-right-panel"
            title={rightPanelOpen ? "Close chat" : "Open chat"}
            aria-label={rightPanelOpen ? "Close chat" : "Open chat"}
          >
            <MessageSquare size={16} />
          </button>
        )}
      </div>
    </div>
  );
};
