// =============================================================================
// G5 Settings — WorkspaceSettingsContent
// =============================================================================
// Full-page settings body holding every workspace/preference section relocated
// out of the (now Account-only) UserSettingsPanel flyout:
//   - Theme selector + dark/light mode
//   - Default model
//   - Editor preferences (all 11 controls)
//   - SCM Connections (+ OAuth callback handling on mount)
//   - Personal and owner-only workspace runtime configuration and accounts
//   - Workspace execution-policy settings form
//
// Personal preferences retain their existing persistence. Workspace writes
// require current ownership; organization controls live in Admin. `workspaceId` is optional;
// when absent, the workspace-scoped runtime-config editor and the execution
// policy form are hidden (and their hooks run with an empty id, which their
// `enabled` guards already treat as a no-op).
// =============================================================================

import { AlertCircle, Check, Loader2, Moon, Sun, Trash2 } from "lucide-react";
import React, { useCallback, useEffect, useState } from "react";
import { useParams, useSearchParams } from "react-router-dom";
import { useQueryClient } from "@tanstack/react-query";
import { api } from "../../lib/api";
import { routes } from "../../lib/routes";
import { THEME_OPTIONS } from "../../lib/defaults";
import {
  clearScmOAuthParams,
  scmConnectionErrorMessage,
} from "../../lib/scmOAuth";
import { useCurrentUser } from "../../hooks/useAuth";
import { useWorkspace } from "../../hooks/useWorkspace";
import {
  useOrgScopedConfig,
  useUpdateSettings,
  useUpdateUserScopedConfig,
  useUpdateWorkspaceScopedConfig,
  useUserScopedConfig,
  useWorkspaceScopedConfig,
  useWorkspaceSettings,
  useGuidedJourneyEnabled,
  useProviderAccountsEnabled,
} from "../../hooks/useSettings";
import { useUserSettings } from "../../hooks/useUserSettings";
import {
  getWorkspaceUiCapabilities,
  type WorkspaceUiCapabilities,
} from "../../lib/capabilities";
import {
  HOME_ROWS_PER_TILE_OPTIONS,
  type ActionBarStyle,
  CHAT_BUBBLE_LIMIT_MAX,
  CHAT_BUBBLE_LIMIT_MIN,
  type ChatBubbleStyle,
  type EditorSettings,
  type HomeRowsPerTile,
  type PhaseBarStyle,
  type SidebarDisplayMode,
  type SubscriptionUsageWindowSetting,
  type WorkspaceSettings,
  type CurrentUserDTO,
} from "../../lib/types";
import { Button, Chip, IconButton, classNames } from "../ui/UI";
import { useOnboardingTour } from "../onboarding/onboardingTourContext";
import { GithubPatConnect } from "../scm/GithubPatConnect";
import { ProviderAccountsSection } from "./ProviderAccountsSection";
import { RuntimeEnvConfig } from "./RuntimeEnvConfig";
import { SettingsForm } from "./SettingsForm";
import {
  SettingsPageShell,
  SettingsRow,
  SettingsSection,
} from "./SettingsPageShell";

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface WorkspaceSettingsContentProps {
  /** Optional current workspace id; when absent, workspace-scoped sections are hidden */
  workspaceId?: string | null;
  capabilities?: WorkspaceUiCapabilities;
}

// ---------------------------------------------------------------------------
// Toggle Switch component
// ---------------------------------------------------------------------------

function Toggle({
  checked,
  onChange,
  ariaLabel,
  ariaDescribedBy,
  disabled = false,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  ariaLabel: string;
  ariaDescribedBy?: string;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      data-testid="workspace-settings-toggle"
      role="switch"
      aria-checked={checked}
      aria-label={ariaLabel}
      aria-describedby={ariaDescribedBy}
      disabled={disabled}
      className={classNames(
        "relative inline-flex h-6 w-10 shrink-0 rounded-full border-2 border-transparent",
        "transition-colors duration-200 ease-in-out cursor-pointer disabled:cursor-not-allowed disabled:opacity-50",
        "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)] focus-visible:ring-offset-1",
        checked ? "bg-[var(--g5-info)]" : "bg-[var(--g5-stroke)]",
      )}
      onClick={() => onChange(!checked)}
      onKeyDown={(e) => {
        if (e.key === " " || e.key === "Enter") {
          e.preventDefault();
          onChange(!checked);
        }
      }}
    >
      <span
        className={classNames(
          "pointer-events-none inline-block h-5 w-5 rounded-full bg-white shadow-sm",
          "transition-transform duration-200 ease-in-out",
          checked ? "translate-x-4" : "translate-x-0",
        )}
      />
    </button>
  );
}

// ---------------------------------------------------------------------------
// WorkspaceSettingsContent Component
// ---------------------------------------------------------------------------

/** The settings page's selects share one look; only the width differs. */
function settingsSelectClass(width: string): string {
  return classNames(
    "px-2 py-1 text-sm",
    width,
    "rounded-[var(--g5-radius-sm)]",
    "bg-[var(--g5-bg2)] text-[var(--g5-text0)]",
    "border border-[var(--g5-stroke)]",
    "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]",
  );
}

export const WorkspaceSettingsContent: React.FC<
  WorkspaceSettingsContentProps
> = ({ workspaceId, capabilities = getWorkspaceUiCapabilities() }) => {
  const params = useParams<{ org?: string; workspace?: string }>();
  const [searchParams, setSearchParams] = useSearchParams();
  const queryClient = useQueryClient();
  const onboardingTour = useOnboardingTour();
  const showCloudWorkspaceWorkflows = capabilities.cloudWorkspaceWorkflows;

  // Workspace-scoped hooks must run unconditionally (React hook rules); the
  // empty-id case is handled by each hook's `enabled` guard, exactly as the
  // original panel relied on. The RENDER of workspace-scoped sections is gated
  // on a truthy `workspaceId` below.
  const workspaceIdForHooks = workspaceId ?? "";

  // Data hooks
  const currentUserState = useCurrentUser();
  const { settings: userSettings, update: updateUserSettings } =
    useUserSettings();
  // The Guided section's kill-switch (ui g5.ui.guided-toggle).
  const guidedJourneyEnabled = useGuidedJourneyEnabled();
  // Provider-accounts kill-switch (external_agent_provider_contract D14,
  // settings-page D2/D16): the Accounts section renders only while the
  // `provider_accounts` instance feature is advertised.
  const providerAccountsEnabled = useProviderAccountsEnabled();
  const wsSettingsState = useWorkspaceSettings(workspaceIdForHooks);
  const updateWsSettings = useUpdateSettings(workspaceIdForHooks);
  // §38.33: Do not silently fall back to orgs[0] when slug doesn't match.
  const activeOrg =
    currentUserState.status === "ok"
      ? (currentUserState.data.orgs.find((org) => org.orgSlug === params.org) ??
        (params.org ? null : currentUserState.data.orgs[0]))
      : null;
  const orgId = activeOrg?.orgId ?? "";
  const workspaceState = useWorkspace(workspaceIdForHooks);
  const canEditWorkspaceConfig =
    currentUserState.status === "ok" &&
    workspaceState.status === "ok" &&
    workspaceState.data.ownerUserId === currentUserState.data.id;
  const userScopedConfig = useUserScopedConfig(orgId);
  const orgScopedConfig = useOrgScopedConfig(orgId);
  const workspaceScopedConfig = useWorkspaceScopedConfig(
    workspaceIdForHooks,
    orgId,
  );
  const updateUserScopedConfig = useUpdateUserScopedConfig(orgId);
  const updateWorkspaceScopedConfig = useUpdateWorkspaceScopedConfig(
    workspaceIdForHooks,
    orgId,
  );

  // Workspace settings form state
  const [wsErrors, setWsErrors] = useState<Record<string, string>>({});

  // SCM connection state/handlers
  const [deletingScmProvider, setDeletingScmProvider] = useState<string | null>(
    null,
  );
  const [scmConnectError, setScmConnectError] = useState<string | null>(null);

  // Account-wide notification preference (users g5.user.notifications). The
  // server defaults it on, so a response that omits it reads as on.
  const emailOnApprovalRequest =
    currentUserState.status === "ok"
      ? (currentUserState.data.emailOnApprovalRequest ?? true)
      : true;
  const [savingApprovalEmail, setSavingApprovalEmail] = useState(false);
  const [approvalEmailError, setApprovalEmailError] = useState(false);
  const toggleApprovalEmail = useCallback(
    async (next: boolean) => {
      setSavingApprovalEmail(true);
      setApprovalEmailError(false);
      try {
        await api.patch<CurrentUserDTO>(routes.users.me, {
          emailOnApprovalRequest: next,
        });
        await queryClient.invalidateQueries({ queryKey: ["current-user"] });
      } catch {
        setApprovalEmailError(true);
      } finally {
        setSavingApprovalEmail(false);
      }
    },
    [queryClient],
  );

  const stopVmAfterMergeDefault =
    currentUserState.status === "ok"
      ? (currentUserState.data.stopVmAfterMergeDefault ?? false)
      : false;
  const [savingStopDefault, setSavingStopDefault] = useState(false);
  const [stopDefaultError, setStopDefaultError] = useState(false);
  const toggleStopDefault = useCallback(
    async (next: boolean) => {
      setSavingStopDefault(true);
      setStopDefaultError(false);
      try {
        const updated = await api.patch<CurrentUserDTO>(routes.users.me, {
          stopVmAfterMergeDefault: next,
        });
        queryClient.setQueryData<CurrentUserDTO>(
          ["current-user"],
          (previous) =>
            previous
              ? {
                  ...previous,
                  stopVmAfterMergeDefault: updated.stopVmAfterMergeDefault,
                }
              : updated,
        );
        await Promise.all([
          queryClient.invalidateQueries({ queryKey: ["current-user"] }),
          queryClient.invalidateQueries({
            queryKey: ["workspace-pull-requests"],
          }),
        ]);
      } catch {
        setStopDefaultError(true);
      } finally {
        setSavingStopDefault(false);
      }
    },
    [queryClient],
  );

  // SCM-OAuth callback handling — runs on mount of this page (consumes
  // `scm_connected`/`scm_error` query params and clears them).
  useEffect(() => {
    const scmConnected = searchParams.get("scm_connected");
    const scmError = searchParams.get("scm_error");

    if (scmConnected === "true") {
      setScmConnectError(null);
      void queryClient.invalidateQueries({ queryKey: ["current-user"] });
      setSearchParams(clearScmOAuthParams(searchParams), { replace: true });
    } else if (scmError) {
      setScmConnectError(scmConnectionErrorMessage(scmError));
      setSearchParams(clearScmOAuthParams(searchParams), { replace: true });
    }
  }, [queryClient, searchParams, setSearchParams]);

  const handleSaveWsSettings = useCallback(
    async (patch: Partial<WorkspaceSettings>) => {
      try {
        setWsErrors({});
        await updateWsSettings.mutateAsync(patch);
      } catch (err) {
        setWsErrors({
          form:
            err instanceof Error
              ? err.message
              : "Failed to save workspace settings.",
        });
      }
    },
    [updateWsSettings],
  );

  const handleDeleteScmConnection = useCallback(
    async (provider: string) => {
      const confirmed = window.confirm(
        `Delete your ${provider} SCM connection?`,
      );

      if (!confirmed) {
        return;
      }

      setScmConnectError(null);
      setDeletingScmProvider(provider);

      try {
        await api.delete(`/api/v1/scm/${provider}/disconnect`);
        await queryClient.invalidateQueries({ queryKey: ["current-user"] });
      } catch (err) {
        setScmConnectError(
          err instanceof Error
            ? err.message
            : `Failed to delete ${provider} connection.`,
        );
      } finally {
        setDeletingScmProvider(null);
      }
    },
    [queryClient],
  );

  // ---------------------------------------------------------------------------
  // Render: SCM Connections
  // ---------------------------------------------------------------------------

  const renderScmConnections = () => {
    const connections =
      currentUserState.status === "ok"
        ? currentUserState.data.scmConnections
        : [];
    const hasGithub = connections.some((conn) => conn.provider === "github");

    return (
      <div className="space-y-3" data-testid="settings-scm-connections">
        {!hasGithub && currentUserState.status === "ok" && (
          <GithubPatConnect
            onConnected={() =>
              queryClient.invalidateQueries({ queryKey: ["current-user"] })
            }
          />
        )}

        <div>
          {currentUserState.status === "loading" && (
            <p className="text-sm text-[var(--g5-text2)]">
              Loading SCM connections…
            </p>
          )}

          {currentUserState.status === "error" && (
            <p className="text-sm text-[var(--g5-blocked)]">
              Failed to load SCM connections.
            </p>
          )}

          {currentUserState.status === "ok" && connections.length > 0 && (
            <ul className="space-y-2">
              {connections.map((conn) => (
                <li
                  key={`${conn.provider}-${conn.providerUsername}`}
                  className="flex items-center justify-between gap-3 py-2 px-3 rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg1)]"
                >
                  <div className="min-w-0">
                    <div className="text-sm font-medium text-[var(--g5-text0)] capitalize">
                      {conn.provider}
                    </div>
                    <div className="text-xs text-[var(--g5-text2)] truncate">
                      @{conn.providerUsername}
                    </div>
                  </div>
                  <div className="flex shrink-0 items-center gap-2">
                    <Chip
                      state={conn.hasInstallation ? "ok" : "muted"}
                      text={
                        conn.hasInstallation ? "Connected" : "No Installation"
                      }
                    />
                    <IconButton
                      title={`Delete ${conn.provider} SCM connection`}
                      disabled={deletingScmProvider === conn.provider}
                      onClick={() =>
                        void handleDeleteScmConnection(conn.provider)
                      }
                    >
                      {deletingScmProvider === conn.provider ? (
                        <Loader2 className="w-4 h-4 animate-spin" />
                      ) : (
                        <Trash2 className="w-4 h-4" />
                      )}
                    </IconButton>
                  </div>
                </li>
              ))}
            </ul>
          )}

          {currentUserState.status === "ok" && connections.length === 0 && (
            <p className="text-sm text-[var(--g5-text2)]">
              No SCM connections configured.
            </p>
          )}

          {scmConnectError && (
            <div className="flex items-center gap-2 mt-3 text-sm text-[var(--g5-blocked)]">
              <AlertCircle className="w-4 h-4" />
              {scmConnectError}
            </div>
          )}
        </div>
      </div>
    );
  };

  // ---------------------------------------------------------------------------
  // Render: Appearance rows (theme, mode, layout) and the Editor row
  // ---------------------------------------------------------------------------

  const renderTheme = () => (
    <div data-testid="theme-selector" className="grid grid-cols-3 gap-2">
      {THEME_OPTIONS.map((theme) => {
        const isActive = userSettings.themeName === theme.name;
        return (
          <button
            key={theme.name}
            type="button"
            data-testid="settings-theme-option"
            className={classNames(
              "relative flex flex-col items-center gap-1.5 p-3",
              "rounded-[var(--g5-radius-md)] border cursor-pointer",
              "transition-colors duration-150",
              "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]",
              isActive
                ? "border-[var(--g5-info)] bg-[var(--g5-bg2)]"
                : "border-[var(--g5-stroke)] bg-[var(--g5-bg1)] hover:bg-[var(--g5-bg2)]",
            )}
            onClick={() => updateUserSettings({ themeName: theme.name })}
            aria-label={`Select ${theme.label} theme`}
          >
            <span
              className="w-3 h-3 rounded-full"
              style={{ backgroundColor: theme.accent }}
            />
            <span className="text-xs text-[var(--g5-text1)] truncate max-w-full">
              {theme.label}
            </span>
            {isActive && (
              <span className="absolute top-1 right-1 text-[var(--g5-info)]">
                <Check className="w-3.5 h-3.5" />
              </span>
            )}
          </button>
        );
      })}
    </div>
  );

  const renderMode = () => (
    <div
      data-testid="mode-toggle"
      className="flex items-center justify-between"
    >
      <span className="text-sm text-[var(--g5-text0)]">Dark mode</span>
      <div className="flex items-center gap-2">
        <Sun className="w-4 h-4 text-[var(--g5-text2)]" />
        <Toggle
          checked={userSettings.themeMode === "dark"}
          onChange={(dark) =>
            updateUserSettings({ themeMode: dark ? "dark" : "light" })
          }
          ariaLabel="Toggle dark/light mode"
        />
        <Moon className="w-4 h-4 text-[var(--g5-text2)]" />
      </div>
    </div>
  );

  // Layout preferences — left-sidebar display mode (settings-page D12;
  // left-sidebar D38), the workbench header, Home paging and Guided.
  const renderLayout = () => (
    <div className="flex flex-col gap-3">
      <div className="flex items-center justify-between">
        <span className="text-sm text-[var(--g5-text0)]">Left sidebar</span>
        <select
          className={settingsSelectClass("w-40")}
          data-testid="sidebar-display-mode-select"
          value={userSettings.sidebarDisplayMode}
          onChange={(e) =>
            updateUserSettings({
              sidebarDisplayMode: e.target.value as SidebarDisplayMode,
            })
          }
          aria-label="Left sidebar display mode"
        >
          <option value="expanded">Full sidebar</option>
          <option value="collapse-to-rail">
            Full sidebar, icon rail when collapsed
          </option>
          <option value="expand-on-hover">Expand on hover</option>
          <option value="rail">Icon rail only</option>
        </select>
      </div>

      {/* Organization Home paging (workbench g5.workbench.home-workspaces):
                every Home tile shows this many rows before "Show more". */}
      <div className="flex items-center justify-between">
        <span className="text-sm text-[var(--g5-text0)]">
          Rows per Home tile
        </span>
        <select
          className={settingsSelectClass("w-40")}
          data-testid="home-rows-per-tile-select"
          value={String(userSettings.homeRowsPerTile)}
          onChange={(e) =>
            updateUserSettings({
              homeRowsPerTile: Number(e.target.value) as HomeRowsPerTile,
            })
          }
          aria-label="Rows per Home tile"
        >
          {HOME_ROWS_PER_TILE_OPTIONS.map((size) => (
            <option key={size} value={String(size)}>
              {size}
            </option>
          ))}
        </select>
      </div>

      {/* Workbench header styles (ui g5.workbench.header-styles):
                how the phase bar and the action bar render. */}
      <div className="flex items-center justify-between">
        <span className="text-sm text-[var(--g5-text0)]">Phase bar</span>
        <select
          className={settingsSelectClass("w-64")}
          data-testid="phase-bar-style-select"
          value={userSettings.phaseBarStyle}
          onChange={(e) =>
            updateUserSettings({
              phaseBarStyle: e.target.value as PhaseBarStyle,
            })
          }
          aria-label="Phase bar style"
        >
          <option value="extended">Extended — name and deliverable</option>
          <option value="minimal">Minimal — name only</option>
          <option value="vertical">
            Vertical — column beside the action bar
          </option>
        </select>
      </div>
      <div className="flex items-center justify-between">
        <span className="text-sm text-[var(--g5-text0)]">Action bar</span>
        <select
          className={settingsSelectClass("w-64")}
          data-testid="action-bar-style-select"
          value={userSettings.actionBarStyle}
          onChange={(e) =>
            updateUserSettings({
              actionBarStyle: e.target.value as ActionBarStyle,
            })
          }
          aria-label="Action bar style"
        >
          <option value="extended-icons">
            Extended icons — icon with label
          </option>
          <option value="minimal-icons">
            Minimal icons — icon only, label on hover
          </option>
          <option value="extended-text">
            Extended text — grouped text buttons
          </option>
          <option value="simplified">
            Simplified — one row of icon and label
          </option>
          <option value="dynamic">
            Dynamic — icons shrink to fit, then scroll
          </option>
        </select>
      </div>

      {/* Chat bubble stack (ui g5.workspace.ui.chat-bubble-stack): the
          faces above the Chat corner button and how many show before the
          rest fold into a count. */}
      <div className="flex items-center justify-between">
        <span className="text-sm text-[var(--g5-text0)]">Chat bubbles</span>
        <select
          className={settingsSelectClass("w-64")}
          data-testid="chat-bubble-style-select"
          value={userSettings.chatBubbleStyle}
          onChange={(e) =>
            updateUserSettings({
              chatBubbleStyle: e.target.value as ChatBubbleStyle,
            })
          }
          aria-label="Chat bubbles"
        >
          <option value="off">Off — the Chat button alone</option>
          <option value="still">Still — one face per state</option>
          <option value="animated">
            Animated — faces alternate two frames
          </option>
          <option value="3d">3D — robot heads</option>
        </select>
      </div>
      <div className="flex items-center justify-between">
        <span className="text-sm text-[var(--g5-text0)]">
          Chat bubbles shown
        </span>
        {/* The input commits only a whole number within range; anything else
            leaves the setting as it was. */}
        <input
          type="number"
          className={settingsSelectClass("w-24")}
          data-testid="chat-bubble-limit-input"
          min={CHAT_BUBBLE_LIMIT_MIN}
          max={CHAT_BUBBLE_LIMIT_MAX}
          step={1}
          value={userSettings.chatBubbleLimit}
          onChange={(e) => {
            const limit = Number(e.target.value);
            if (
              Number.isInteger(limit) &&
              limit >= CHAT_BUBBLE_LIMIT_MIN &&
              limit <= CHAT_BUBBLE_LIMIT_MAX
            ) {
              updateUserSettings({ chatBubbleLimit: limit });
            }
          }}
          aria-label="Chat bubbles shown"
        />
      </div>

      {/* Guided experience (ui g5.ui.guided-toggle): the Guided
                section of the left sidebar, off by default; offered only
                while that section can exist (`guided_journey` feature). */}
      {guidedJourneyEnabled && (
        <div
          className="flex items-center justify-between"
          data-testid="guided-journey-setting"
        >
          <span className="text-sm text-[var(--g5-text0)]">
            Guided experience
          </span>
          <Toggle
            checked={userSettings.showGuidedJourney}
            onChange={(v) => updateUserSettings({ showGuidedJourney: v })}
            ariaLabel="Show the guided experience"
          />
        </div>
      )}

      {/* Center tab strip visibility (workspaces
                g5.workspace.ui.center-tabs): the Chat/Activity/document tab
                row above the center panel stays hidden until turned on. */}
      <div className="flex items-center justify-between">
        <span className="text-sm text-[var(--g5-text0)]">Workspace tabs</span>
        <Toggle
          checked={userSettings.showTabBar}
          onChange={(v) => updateUserSettings({ showTabBar: v })}
          ariaLabel="Show workspace tabs"
        />
      </div>

      {/* Personal display preference for live Claude and Codex usage. */}
      <div className="flex items-center justify-between">
        <span className="text-sm text-[var(--g5-text0)]">
          Subscription usage window
        </span>
        <select
          className={settingsSelectClass("w-40")}
          data-testid="subscription-usage-window-select"
          value={userSettings.subscriptionUsageWindow}
          onChange={(e) =>
            updateUserSettings({
              subscriptionUsageWindow: e.target
                .value as SubscriptionUsageWindowSetting,
            })
          }
          aria-label="Subscription usage window"
        >
          <option value="7d">7-day weekly limit</option>
          <option value="5h">5-hour window</option>
          <option value="off">Never display</option>
        </select>
      </div>
    </div>
  );

  // Editor settings — local preferences, always shown.
  const renderEditor = () => (
    <div className="flex flex-col gap-3">
      {/* Boolean toggles */}
      {(
        [
          ["rainbowBrackets", "Rainbow brackets"],
          ["codeFolding", "Code folding"],
          ["showMinimap", "Show minimap"],
          ["indentGuides", "Indent guides"],
          ["bracketPairGuides", "Bracket pair guides"],
          ["ligatures", "Font ligatures"],
          ["stickyScroll", "Sticky scroll"],
        ] as const
      ).map(([key, label]) => (
        <div key={key} className="flex items-center justify-between">
          <span className="text-sm text-[var(--g5-text0)]">{label}</span>
          <Toggle
            checked={userSettings.editor?.[key] ?? false}
            onChange={(v) =>
              updateUserSettings({
                editor: { ...userSettings.editor, [key]: v },
              })
            }
            ariaLabel={`Toggle ${label}`}
          />
        </div>
      ))}

      {/* Word wrap dropdown */}
      <div className="flex items-center justify-between">
        <span className="text-sm text-[var(--g5-text0)]">Word wrap</span>
        <select
          className={settingsSelectClass("w-32")}
          data-testid="editor-word-wrap-select"
          value={userSettings.editor?.wordWrap ?? "off"}
          onChange={(e) =>
            updateUserSettings({
              editor: {
                ...userSettings.editor,
                wordWrap: e.target.value as EditorSettings["wordWrap"],
              },
            })
          }
          aria-label="Word wrap"
        >
          <option value="off">Off</option>
          <option value="on">On</option>
          <option value="wordWrapColumn">Column</option>
          <option value="bounded">Bounded</option>
        </select>
      </div>

      {/* Render whitespace dropdown */}
      <div className="flex items-center justify-between">
        <span className="text-sm text-[var(--g5-text0)]">
          Render whitespace
        </span>
        <select
          className={settingsSelectClass("w-32")}
          data-testid="editor-render-whitespace-select"
          value={userSettings.editor?.renderWhitespace ?? "selection"}
          onChange={(e) =>
            updateUserSettings({
              editor: {
                ...userSettings.editor,
                renderWhitespace: e.target
                  .value as EditorSettings["renderWhitespace"],
              },
            })
          }
          aria-label="Render whitespace"
        >
          <option value="none">None</option>
          <option value="boundary">Boundary</option>
          <option value="selection">Selection</option>
          <option value="trailing">Trailing</option>
          <option value="all">All</option>
        </select>
      </div>

      {/* Font size */}
      <div className="flex items-center justify-between">
        <span className="text-sm text-[var(--g5-text0)]">Font size</span>
        <input
          type="number"
          min="10"
          max="24"
          className={classNames(
            "w-16 px-2 py-1 text-sm text-center",
            "rounded-[var(--g5-radius-sm)]",
            "bg-[var(--g5-bg2)] text-[var(--g5-text0)]",
            "border border-[var(--g5-stroke)]",
            "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]",
          )}
          value={userSettings.editor?.editorFontSize ?? 14}
          onChange={(e) =>
            updateUserSettings({
              editor: {
                ...userSettings.editor,
                editorFontSize: parseInt(e.target.value) || 14,
              },
            })
          }
          aria-label="Editor font size"
        />
      </div>

      {/* Tab size */}
      <div className="flex items-center justify-between">
        <span className="text-sm text-[var(--g5-text0)]">Tab size</span>
        <select
          className={settingsSelectClass("w-16")}
          data-testid="editor-tab-size-select"
          value={userSettings.editor?.editorTabSize ?? 4}
          onChange={(e) =>
            updateUserSettings({
              editor: {
                ...userSettings.editor,
                editorTabSize: parseInt(
                  e.target.value,
                ) as EditorSettings["editorTabSize"],
              },
            })
          }
          aria-label="Editor tab size"
        >
          <option value="2">2</option>
          <option value="4">4</option>
          <option value="8">8</option>
        </select>
      </div>
    </div>
  );

  // ---------------------------------------------------------------------------
  // Render: Runtime config + credentials (cloud + org gated)
  // ---------------------------------------------------------------------------

  // The Defaults card (settings-page D19): per-agent default-model selects
  // are user preferences and are always shown subject to the provider UI
  // gate; the default-agent select, the account selects, and the
  // custom-variable editor are gated on cloud workflows + a resolved org.
  const renderRuntimeConfig = () => (
    <RuntimeEnvConfig
      orgId={orgId}
      workspaceId={workspaceId}
      org={orgScopedConfig}
      workspace={workspaceScopedConfig}
      user={userScopedConfig}
      updateWorkspace={updateWorkspaceScopedConfig.mutateAsync}
      updateUser={updateUserScopedConfig.mutateAsync}
      savingWorkspace={updateWorkspaceScopedConfig.isLoading}
      savingUser={updateUserScopedConfig.isLoading}
      canEditOrg={false}
      canEditWorkspace={canEditWorkspaceConfig}
      defaultAgentModels={{
        claude_code: userSettings.defaultAgentProviderChoices.claude_code.model,
        codex: userSettings.defaultAgentProviderChoices.codex.model,
      }}
      onDefaultAgentModelChange={(harness, model) =>
        updateUserSettings({
          defaultAgentProviderChoices: {
            ...userSettings.defaultAgentProviderChoices,
            [harness]: {
              ...userSettings.defaultAgentProviderChoices[harness],
              model,
            },
          },
        })
      }
      showScopedConfig={showCloudWorkspaceWorkflows && !!orgId}
    />
  );

  // ---------------------------------------------------------------------------
  // Main render — one sheet, one scroll, index on the left (g5.ui.settings-page)
  // ---------------------------------------------------------------------------

  const showAccounts = providerAccountsEnabled && !!orgId;
  const showWorkspaceExecutionPolicy =
    showCloudWorkspaceWorkflows &&
    !!workspaceId &&
    wsSettingsState.status === "ok";

  const orgSlug = activeOrg?.orgSlug ?? params.org ?? null;
  const isOrgAdmin = activeOrg?.role === "admin";

  return (
    <SettingsPageShell
      view="settings"
      crumbs={[orgSlug, params.workspace]}
      title="Settings"
      scopeLine="Your settings. They follow you into every workspace."
      indexTitle="Your settings"
      indexFooter={
        // Effective configuration resolves org → workspace → user
        // (config_service `resolve_effective`), so a value set here wins
        // over the organization's. The Admin link is for those who can open it.
        <>
          {isOrgAdmin && orgSlug && (
            <>
              Organization settings are in{" "}
              <a
                href={`/${orgSlug}/base?view=admin`}
                data-testid="settings-admin-link"
                className="underline underline-offset-2 hover:text-[var(--g5-text0)]"
              >
                Admin
              </a>
              .{" "}
            </>
          )}
          <span data-testid="settings-priority-note">
            Your settings on this page take priority over the organization
            settings.
          </span>
        </>
      }
    >
      {/* ─────────────────────────── 1. Appearance ─────────────────────── */}
      <SettingsSection
        id="appearance"
        title="Appearance"
        description="How G5 looks and which panels a workspace shows."
      >
        <SettingsRow
          title="Theme"
          description="Colors for every workspace."
          testId="settings-row-theme"
        >
          {renderTheme()}
        </SettingsRow>
        <SettingsRow
          title="Mode"
          description="Light or dark."
          testId="settings-row-mode"
        >
          {renderMode()}
        </SettingsRow>
        <SettingsRow
          title="Layout"
          description="The sidebar, the workbench header, Home paging and the Guided panel."
        >
          {renderLayout()}
        </SettingsRow>
        <SettingsRow
          title="Tour"
          description="The guided walk through Settings, the scratchpad and your first workspace (ui g5.ui.onboarding-tour)."
          testId="settings-row-tour"
        >
          <Button
            kind="secondary"
            data-testid="onboarding-replay"
            onClick={onboardingTour.replay}
          >
            Replay the tour
          </Button>
        </SettingsRow>
      </SettingsSection>

      {/* ─────────────────────────── 2. Accounts ────────────────────────── */}
      {/* Second on purpose: accounts matter more than the rest. Named
          provider accounts (settings-page D16), gated on the provider_accounts
          instance feature; needs a resolved org because the accounts API is
          org-scoped. */}
      {showAccounts && (
        <SettingsSection
          id="accounts"
          title="Accounts"
          testId="settings-section-accounts"
          description="Subscriptions and API keys your agents bill to, for you and for workspaces you own."
        >
          <ProviderAccountsSection
            orgId={orgId}
            workspaceId={workspaceId}
            canEditOrg={false}
            canEditWorkspace={canEditWorkspaceConfig}
          />
        </SettingsSection>
      )}

      {/* ─────────────────────────── 3. Defaults ────────────────────────── */}
      {/* Always rendered: the per-agent default-model selects are user
          preferences. The default-agent select, account selects, and
          custom-variable editor inside are gated on cloud workflows (see
          showScopedConfig). */}
      <SettingsSection
        id="defaults"
        title="Defaults"
        description="The default agent, each agent's model and account, and custom variables, for you and for workspaces you own. Your first account becomes the default for its agent."
      >
        {renderRuntimeConfig()}
      </SettingsSection>

      {/* ──────────────────────────── 4. Editor ─────────────────────────── */}
      <SettingsSection
        id="editor"
        title="Editor"
        description="Preferences for files you open in a workspace."
      >
        <SettingsRow
          title="Code editor"
          description="Guides, wrapping and type."
        >
          {renderEditor()}
        </SettingsRow>
      </SettingsSection>

      {/* ──────────────────────────── 5. Access ─────────────────────────── */}
      <SettingsSection
        id="access"
        title="Access"
        description="Connections G5 uses on your behalf."
      >
        <SettingsRow
          title="SCM connections"
          description="Git hosts you connected with OAuth or a token."
        >
          {renderScmConnections()}
        </SettingsRow>
      </SettingsSection>

      {/* ──────────────────────── 6. Notifications ──────────────────────── */}
      <SettingsSection
        id="notifications"
        title="Notifications"
        description="Emails G5 sends you about your work."
      >
        <SettingsRow
          title="Approval requests"
          description="Email me when someone requests my approval, and when a reviewer approves or requests changes on a workspace I own. On by default."
          testId="settings-row-approval-email"
        >
          <div className="flex flex-col gap-1">
            <Toggle
              checked={emailOnApprovalRequest}
              onChange={(next) => {
                if (!savingApprovalEmail) {
                  void toggleApprovalEmail(next);
                }
              }}
              ariaLabel="Email me about approval requests and decisions"
            />
            {approvalEmailError && (
              <span className="text-xs text-[var(--g5-blocked)]">
                Could not save. Try again.
              </span>
            )}
          </div>
        </SettingsRow>
      </SettingsSection>

      {/* ───────────────────────── 7. Execution policy ──────────────────── */}
      {showCloudWorkspaceWorkflows && (
        <SettingsSection
          id="execution"
          title="Execution policy"
          description="How workspace VMs run."
        >
          <SettingsRow
            title="Stop VM after merge"
            description={
              <span id="stop-vm-default-help">
                Default for all your workspaces. You can change it in Prepare to
                Ship.
              </span>
            }
            testId="settings-row-stop-vm-default"
          >
            <div className="flex items-center justify-between gap-4">
              <div>
                <p
                  className="text-sm font-medium text-[var(--g5-text0)]"
                  aria-live="polite"
                >
                  {savingStopDefault
                    ? "Saving…"
                    : currentUserState.status === "loading"
                      ? "Loading…"
                      : currentUserState.status === "error"
                        ? "Could not load your default."
                        : stopVmAfterMergeDefault
                          ? "On by default"
                          : "Off by default"}
                </p>
                {currentUserState.status === "ok" && (
                  <p className="mt-1 text-xs text-[var(--g5-text2)]">
                    {stopVmAfterMergeDefault
                      ? "Stop workspaces after a successful merge."
                      : "Keep workspaces running after merge."}
                  </p>
                )}
              </div>
              <Toggle
                checked={stopVmAfterMergeDefault}
                disabled={savingStopDefault || currentUserState.status !== "ok"}
                onChange={(next) => void toggleStopDefault(next)}
                ariaLabel="Stop VM after merge"
                ariaDescribedBy="stop-vm-default-help"
              />
            </div>
            {stopDefaultError && (
              <p role="alert" className="mt-2 text-xs text-[var(--g5-blocked)]">
                Could not save. Try again.
              </p>
            )}
          </SettingsRow>
          {showWorkspaceExecutionPolicy && (
            <SettingsRow
              title="Idle timeout"
              description="This workspace stops after this long without activity. Only its owner can change it."
            >
              {!canEditWorkspaceConfig && (
                <p className="mb-3 text-sm text-[var(--g5-text2)]">
                  Only the workspace owner can change workspace configuration.
                </p>
              )}
              <fieldset disabled={!canEditWorkspaceConfig}>
                <SettingsForm
                  settings={wsSettingsState.data}
                  onSave={handleSaveWsSettings}
                  saving={updateWsSettings.isLoading}
                  errors={wsErrors}
                />
              </fieldset>
            </SettingsRow>
          )}
        </SettingsSection>
      )}
    </SettingsPageShell>
  );
};
