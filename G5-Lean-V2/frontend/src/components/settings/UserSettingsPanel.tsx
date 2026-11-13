// =============================================================================
// G5 Settings Panel — UserSettingsPanel
// =============================================================================
// Right-aligned Account flyout: Organizations content + Sign Out footer.
// Controlled by `open` prop; closed via Escape, backdrop click, or X button.
// Workspace/preference settings now live on the full-page WorkspaceSettingsPage
// (WorkspaceSettingsContent); this flyout is account-scoped only.
// =============================================================================

import { X } from "lucide-react";
import React, { useRef } from "react";
import { useAuth } from "../../hooks/useAuth";
import { useOrgs } from "../../hooks/useOrg";
import { useInstanceInfo } from "../../hooks/useSettings";
import { isGithubAppEnabled } from "../../lib/githubAppFlags";
import {
  getWorkspaceUiCapabilities,
  type WorkspaceUiCapabilities,
} from "../../lib/capabilities";
import {
  Button,
  Chip,
  IconButton,
  SectionTitle,
  classNames,
  useDialogFocus,
} from "../ui/UI";
import { GithubConnectionCard } from "./GithubConnectionCard";

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface UserSettingsPanelProps {
  /** Whether the panel is open */
  open: boolean;
  /** Callback to close the panel */
  onClose: () => void;
  /** Current workspace ID (retained for consumer call-sites; account-only flyout) */
  workspaceId: string;
  /** UI capability gates for cloud/project workflow affordances */
  capabilities?: WorkspaceUiCapabilities;
}

// ---------------------------------------------------------------------------
// UserSettingsPanel Component
// ---------------------------------------------------------------------------

export const UserSettingsPanel: React.FC<UserSettingsPanelProps> = ({
  open,
  onClose,
  workspaceId: _workspaceId,
  capabilities = getWorkspaceUiCapabilities(),
}) => {
  const instanceInfo = useInstanceInfo();
  const githubAppEnabled = isGithubAppEnabled(
    instanceInfo.status === "ok" ? instanceInfo.data : null,
  );
  // The per-org link cluster (Members / Approvals / Workers / GitHub App /
  // Invite) collapsed into the Admin hub page (`?view=admin`); this flyout now
  // shows Open Home and, for admins, Show Admin Page per org. The `capabilities` prop
  // stays for call-site compatibility.
  void capabilities;

  // Auth context (Sign Out footer + identity copy)
  const { user, logout } = useAuth();

  // Data hooks
  const orgsState = useOrgs();

  // Anchored right-edge flyout, not a centered dialog: it keeps its own
  // layout and backdrop and takes only the shared dialog keyboard/focus
  // lifecycle — Escape, Tab trap, and focus restore (design-system D15).
  // The panel stays mounted while closed (slide transition), so the hook is
  // gated on `open`: while disabled it is fully inert — no dialog-stack
  // entry, no listeners, no focus capture — and each open runs a fresh
  // capture/restore cycle. Priority 40 mirrors the flyout's z-40 backdrop.
  const panelRef = useRef<HTMLDivElement>(null);
  useDialogFocus(panelRef, { onClose, enabled: open, priority: 40 });

  // ---------------------------------------------------------------------------
  // Render: Organizations
  // ---------------------------------------------------------------------------

  const renderOrganizations = () => {
    return (
      <div className="flex flex-col gap-4">
        {/* No Create Organization entry: only platform operators create
            organizations (g5.organization.creation). */}
        <SectionTitle>Organizations</SectionTitle>

        {orgsState.status === "loading" && (
          <p className="text-sm text-[var(--g5-text2)]">
            Loading organizations…
          </p>
        )}

        {orgsState.status === "error" && (
          <p className="text-sm text-[var(--g5-blocked)]">
            Failed to load organizations.
          </p>
        )}

        {orgsState.status === "ok" && (
          <>
            {orgsState.data.map((orgSlim) => {
              const roleLabel =
                orgSlim.role.charAt(0).toUpperCase() + orgSlim.role.slice(1);
              return (
                <div
                  key={orgSlim.orgId}
                  className={classNames(
                    "rounded-[var(--g5-radius-md)] border border-[var(--g5-stroke)]",
                    "bg-[var(--g5-bg1)] p-4",
                  )}
                >
                  {/* Header row: org name + role badge */}
                  <div className="flex items-center gap-2">
                    <span
                      data-testid="org-name"
                      className="text-sm font-semibold text-[var(--g5-text0)]"
                    >
                      {orgSlim.orgName}
                    </span>
                    <Chip state="muted" text={roleLabel} />
                  </div>

                  {/* At most two per-org destinations: the org Home (the
                      default landing, g5.ui.sign-in-landing; for the platform
                      operator org this is the platform console,
                      g5.organization.platform-operator.home) and, for
                      admins only (g5.settings.admin-page), the org Admin
                      hub. Every other org configuration surface (Members /
                      Approvals / Workers / GitHub App / Invite) now lives on
                      the Admin page. */}
                  <div className="flex items-center gap-4 mt-3">
                    <a
                      href={`/${orgSlim.orgSlug}/home`}
                      data-testid="org-open-home-link"
                      className="text-xs text-[var(--g5-info)] hover:underline"
                    >
                      Open Home →
                    </a>
                    {orgSlim.role === "admin" && (
                      <a
                        href={`/${orgSlim.orgSlug}/base?view=admin`}
                        data-testid="org-show-admin-link"
                        className="text-xs text-[var(--g5-info)] hover:underline"
                      >
                        Show Admin Page →
                      </a>
                    )}
                  </div>
                </div>
              );
            })}
            {orgsState.data.length === 0 && (
              <p className="text-sm text-[var(--g5-text2)]">
                No organizations found.
              </p>
            )}
          </>
        )}
      </div>
    );
  };

  // ---------------------------------------------------------------------------
  // Main render
  // ---------------------------------------------------------------------------

  return (
    <>
      {/* Backdrop */}
      {open && (
        <div
          className="fixed inset-0 z-40 bg-black/30 backdrop-blur-sm"
          onClick={onClose}
          aria-hidden="true"
        />
      )}

      {/* Panel */}
      <div
        // The hook is gated on `open` (enabled option), so the ref can stay
        // attached across the slide transition.
        ref={panelRef}
        tabIndex={-1}
        className={classNames(
          "fixed top-0 right-0 z-50 h-full",
          "w-full lg:w-[480px]",
          "bg-[var(--g5-bg1)] border-l border-[var(--g5-stroke)]",
          "flex flex-col",
          "transition-transform duration-300 ease-in-out",
          open ? "translate-x-0" : "translate-x-full",
        )}
        role={open ? "dialog" : undefined}
        aria-modal={open ? true : undefined}
        aria-hidden={!open || undefined}
        aria-label="Account"
      >
        {/* Header */}
        <div className="flex items-center justify-between px-5 py-4 border-b border-[var(--g5-stroke)]">
          <h2 className="text-base font-semibold text-[var(--g5-text0)]">
            Account
          </h2>
          <IconButton onClick={onClose} title="Close settings">
            <X className="w-4 h-4" />
          </IconButton>
        </div>

        {/* Content (scrollable) */}
        <div className="flex-1 overflow-y-auto px-5 py-4">
          {open && githubAppEnabled && (
            <GithubConnectionCard
              returnTo={`${window.location.pathname}${window.location.search}`}
            />
          )}
          {renderOrganizations()}
        </div>

        {/* Sign Out footer — account-scope action.
            Owned by settings-panel ontology block I7. */}
        <div className="flex items-center justify-between gap-3 px-5 py-3 border-t border-[var(--g5-stroke)] bg-[var(--g5-bg1)]">
          <div className="min-w-0 text-xs text-[var(--g5-text2)] truncate">
            Signed in as{" "}
            <span className="text-[var(--g5-text0)]">
              {user?.email ?? "this account"}
            </span>
          </div>
          <Button
            kind="secondary"
            onClick={logout}
            data-testid="settings-sign-out"
          >
            Sign Out
          </Button>
        </div>
      </div>
    </>
  );
};
