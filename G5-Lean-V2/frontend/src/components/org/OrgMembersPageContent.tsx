/**
 * OrgMembersPageContent — Embeddable inner content for managing org members.
 *
 * Renders WITHOUT the standalone `<AppShell>` chrome so it can be embedded
 * either as a standalone route (via `OrgMembersPage`, which wraps it in
 * `<AppShell>`) or as a center-panel tab inside `WorkspacePage`. It reads the
 * `:org` slug from `useParams`, which is present on both the standalone
 * `/:org/settings/members` route and the in-workspace `/:org/:workspace` route.
 *
 * Requires org:admin scope for mutating actions (route guard handled by
 * RequireAuth; the table/dialogs additionally gate on the resolved role).
 *
 * Features:
 *   - Member table with role management (dropdown per row)
 *   - Remove member with confirmation modal
 *   - Pending invitations section with resend/revoke
 *   - Invite button opens InviteDialog
 *
 * @module org/OrgMembersPageContent
 */

import React, { useState, useCallback, useMemo } from "react";
import { useParams } from "react-router-dom";
import { useQueryClient } from "@tanstack/react-query";
import {
  UserPlus,
  Trash2,
  RefreshCw,
  X,
  AlertCircle,
  Users,
} from "lucide-react";

import { Button, Chip, Modal } from "../ui/UI";
import { invitationEmailLabel } from "../../lib/invitationEmail";
import { useAuth } from "../../hooks/useAuth";
import {
  useOrgMembers,
  useOrgInvitations,
  useUpdateMemberRole,
  useRemoveMember,
  useSetMemberActive,
  useStopMemberWorkspaces,
  useResendInvitation,
  useRevokeInvitation,
} from "../../hooks/useOrg";
import { InviteDialog } from "./InviteDialog";
import { MemberShutdownModal } from "./MemberShutdownModal";

import type {
  InviteMemberResponseDTO,
  OrgMemberDTO,
  OrgInvitationDTO,
} from "../../lib/types";
import type { State } from "../ui/UI";

/* ------------------------------------------------------------------ */
/*  Exported sub-component props                                       */
/* ------------------------------------------------------------------ */

export interface MemberTableProps {
  members: OrgMemberDTO[];
  currentUserId: string;
  currentUserRole: "admin" | "member" | "viewer";
  onChangeRole: (userId: string, newRole: string) => void;
  onRemove: (userId: string) => void;
  onActivity: (member: OrgMemberDTO) => void;
  onShutdown: (member: OrgMemberDTO) => void;
  onStopWorkspaces: (member: OrgMemberDTO) => void;
}

export interface PendingInvitationsTableProps {
  invitations: OrgInvitationDTO[];
  onResend: (invitationId: string) => void;
  onRevoke: (invitationId: string) => void;
}

/* ------------------------------------------------------------------ */
/*  Helpers                                                            */
/* ------------------------------------------------------------------ */

const ROLE_PRIORITY: Record<string, number> = {
  admin: 1,
  member: 2,
  viewer: 3,
};

const ROLE_STATE_MAP: Record<string, State> = {
  admin: "info",
  member: "ok",
  viewer: "muted",
};

function formatRelativeTime(isoDate: string): string {
  const diff = new Date(isoDate).getTime() - Date.now();
  const days = Math.floor(diff / (1000 * 60 * 60 * 24));
  const hours = Math.floor(diff / (1000 * 60 * 60));
  if (days > 0) return `expires in ${days}d`;
  if (hours > 0) return `expires in ${hours}h`;
  return "expiring soon";
}

function sortMembers(members: OrgMemberDTO[]): OrgMemberDTO[] {
  return [...members].sort((a, b) => {
    const roleDiff =
      (ROLE_PRIORITY[a.role] ?? 99) - (ROLE_PRIORITY[b.role] ?? 99);
    if (roleDiff !== 0) return roleDiff;
    return a.displayName.localeCompare(b.displayName);
  });
}

/* ------------------------------------------------------------------ */
/*  Confirmation Modal                                                 */
/* ------------------------------------------------------------------ */

function ConfirmModal({
  title,
  message,
  confirmLabel,
  confirmDisabled = false,
  onConfirm,
  onCancel,
}: {
  title: string;
  message: string;
  confirmLabel: string;
  confirmDisabled?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  return (
    <Modal onClose={onCancel} ariaLabel={title} size="sm" className="p-6">
      <h3 className="text-[var(--g5-text0)] text-lg font-semibold mb-2">
        {title}
      </h3>
      <p className="text-[var(--g5-text1)] mb-4">{message}</p>
      <div className="flex justify-end gap-2">
        <Button
          data-testid="org-members-confirm-cancel"
          kind="secondary"
          onClick={onCancel}
        >
          Cancel
        </Button>
        <button
          data-testid="org-members-confirm-accept"
          disabled={confirmDisabled}
          className="bg-[var(--g5-blocked)] text-white px-4 py-2 rounded-[var(--g5-radius-sm)] hover:opacity-90"
          onClick={onConfirm}
        >
          {confirmLabel}
        </button>
      </div>
    </Modal>
  );
}

/* ------------------------------------------------------------------ */
/*  MemberTable                                                        */
/* ------------------------------------------------------------------ */

function MemberTable({
  members,
  currentUserId,
  currentUserRole,
  onChangeRole,
  onRemove,
  onActivity,
  onShutdown,
  onStopWorkspaces,
}: MemberTableProps) {
  const sorted = useMemo(() => sortMembers(members), [members]);
  const isAdmin = currentUserRole === "admin";

  return (
    <table className="w-full border-collapse">
      <thead>
        <tr className="bg-[var(--g5-bg2)] text-left text-xs uppercase tracking-wide text-[var(--g5-text2)]">
          <th className="px-4 py-3 border-b border-[var(--g5-stroke)]">
            Status
          </th>
          <th className="px-4 py-3 border-b border-[var(--g5-stroke)]">
            Name / Email
          </th>
          <th className="px-4 py-3 border-b border-[var(--g5-stroke)]">Role</th>
          <th className="px-4 py-3 border-b border-[var(--g5-stroke)]">
            Joined
          </th>
          <th className="px-4 py-3 border-b border-[var(--g5-stroke)]">
            Actions
          </th>
        </tr>
      </thead>
      <tbody>
        {sorted.map((member) => {
          const isSelf = member.userId === currentUserId;
          const canChangeRole = isAdmin && !isSelf;
          const canRemove = isAdmin && !isSelf;
          // Stop by user: the member themself or an admin.
          const canStopWorkspaces =
            (isAdmin || isSelf) &&
            !member.deactivatedAt &&
            !member.shutdownPending &&
            member.role !== "viewer";

          return (
            <tr
              key={member.userId}
              className="bg-[var(--g5-bg1)] border-b border-[var(--g5-stroke)] hover:bg-[var(--g5-bg2)] transition-colors"
            >
              {/* Status dot */}
              <td className="px-4 py-3">
                <Chip
                  state={member.deactivatedAt ? "muted" : "ok"}
                  text={member.deactivatedAt ? "Inactive" : "Active"}
                />
                {member.shutdownPending &&
                  (isAdmin || isSelf ? (
                    <button
                      data-testid="org-members-shutdown-progress"
                      className="block text-xs text-[var(--g5-info)] mt-1"
                      onClick={() => onShutdown(member)}
                    >
                      Shutdown pending
                    </button>
                  ) : (
                    <div className="text-xs text-[var(--g5-text2)] mt-1">
                      Shutdown pending
                    </div>
                  ))}
              </td>

              {/* Name / Email */}
              <td className="px-4 py-3">
                <div className="text-sm text-[var(--g5-text0)] font-medium">
                  {member.displayName}
                  {isSelf && (
                    <span className="text-[var(--g5-text2)] ml-1">(you)</span>
                  )}
                </div>
                <div className="text-xs text-[var(--g5-text2)]">
                  {member.email}
                </div>
              </td>

              {/* Role dropdown */}
              <td className="px-4 py-3">
                {canChangeRole ? (
                  <select
                    data-testid="org-members-role-select"
                    value={member.role}
                    onChange={(e) =>
                      onChangeRole(member.userId, e.target.value)
                    }
                    className="
                      text-sm px-2 py-1
                      bg-[var(--g5-bg0)] text-[var(--g5-text0)]
                      border border-[var(--g5-stroke)]
                      rounded-[var(--g5-radius-sm)]
                      focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]
                    "
                    aria-label={`Change role for ${member.displayName}`}
                  >
                    <option value="admin">Admin</option>
                    <option value="member">Member</option>
                    <option value="viewer">Viewer</option>
                  </select>
                ) : (
                  <Chip
                    state={ROLE_STATE_MAP[member.role] ?? "muted"}
                    text={member.role}
                  />
                )}
              </td>

              {/* Joined date */}
              <td className="px-4 py-3 text-sm text-[var(--g5-text1)]">
                {new Date(member.joinedAt).toLocaleDateString()}
              </td>

              {/* Actions */}
              <td className="px-4 py-3">
                {canStopWorkspaces && (
                  <button
                    data-testid="org-members-stop-workspaces"
                    className="px-2 py-1 text-sm text-[var(--g5-info)]"
                    onClick={() => onStopWorkspaces(member)}
                    aria-label={`Stop all workspaces of ${member.displayName}`}
                  >
                    Stop workspaces
                  </button>
                )}
                {canRemove && (
                  <button
                    data-testid="org-members-change-activity"
                    className="px-2 py-1 text-sm text-[var(--g5-info)]"
                    onClick={() => onActivity(member)}
                    aria-label={`${member.deactivatedAt ? "Reactivate" : "Deactivate"} ${member.displayName}`}
                  >
                    {member.deactivatedAt ? "Reactivate" : "Deactivate"}
                  </button>
                )}
                {canRemove && (
                  <button
                    disabled={member.shutdownPending}
                    data-testid="org-members-remove-member"
                    onClick={() => onRemove(member.userId)}
                    className="p-1.5 text-[var(--g5-text2)] hover:text-[var(--g5-blocked)] transition-colors"
                    title={`Remove ${member.displayName}`}
                    aria-label={`Remove ${member.displayName}`}
                  >
                    <Trash2 className="w-4 h-4" />
                  </button>
                )}
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}

/* ------------------------------------------------------------------ */
/*  InvitationEmailStatus                                              */
/* ------------------------------------------------------------------ */

const EMAIL_TONE_CLASS = {
  muted: "text-[var(--g5-text2)]",
  ok: "text-[var(--g5-ok)]",
  warn: "text-[var(--g5-warn)]",
} as const;

function InvitationEmailStatus({
  delivery,
}: {
  delivery: OrgInvitationDTO["emailDelivery"];
}) {
  const label = invitationEmailLabel(delivery);
  if (!label) return null;
  return (
    <span
      data-testid="org-members-invite-email-status"
      className={EMAIL_TONE_CLASS[label.tone]}
    >
      {" "}
      · {label.text}
    </span>
  );
}

/* ------------------------------------------------------------------ */
/*  PendingInvitationsTable                                            */
/* ------------------------------------------------------------------ */

function PendingInvitationsTable({
  invitations,
  onResend,
  onRevoke,
}: PendingInvitationsTableProps) {
  const pendingInvitations = invitations.filter(
    (inv) => inv.status === "pending",
  );

  if (pendingInvitations.length === 0) return null;

  return (
    <div className="mt-8">
      <h3 className="text-sm font-semibold text-[var(--g5-text0)] uppercase tracking-wide mb-3">
        Pending Invitations ({pendingInvitations.length})
      </h3>

      <div className="space-y-2">
        {pendingInvitations.map((inv) => (
          <div
            key={inv.id}
            className="
              flex items-center justify-between
              bg-[var(--g5-bg1)] border border-[var(--g5-stroke)]
              rounded-[var(--g5-radius-md)] px-4 py-3
            "
          >
            <div className="flex items-center gap-3">
              <span className="text-lg" title="Pending">
                ⏳
              </span>
              <div>
                <div className="text-sm text-[var(--g5-text0)]">
                  {inv.email}
                </div>
                <div className="text-xs text-[var(--g5-text2)]">
                  <Chip
                    state={ROLE_STATE_MAP[inv.role] ?? "muted"}
                    text={inv.role}
                  />{" "}
                  · {formatRelativeTime(inv.expiresAt)}
                  <InvitationEmailStatus delivery={inv.emailDelivery} />
                </div>
              </div>
            </div>

            <div className="flex items-center gap-2">
              <button
                data-testid="org-members-resend-invite"
                onClick={() => onResend(inv.id)}
                className="p-1.5 text-[var(--g5-text2)] hover:text-[var(--g5-info)] transition-colors"
                title={`Resend invitation to ${inv.email}`}
                aria-label={`Resend invitation to ${inv.email}`}
              >
                <RefreshCw className="w-4 h-4" />
              </button>
              <button
                data-testid="org-members-revoke-invite"
                onClick={() => onRevoke(inv.id)}
                className="p-1.5 text-[var(--g5-text2)] hover:text-[var(--g5-blocked)] transition-colors"
                title={`Revoke invitation for ${inv.email}`}
                aria-label={`Revoke invitation for ${inv.email}`}
              >
                <X className="w-4 h-4" />
              </button>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Skeleton rows (loading state)                                      */
/* ------------------------------------------------------------------ */

function SkeletonTable() {
  return (
    <div className="space-y-3">
      {Array.from({ length: 5 }, (_, i) => (
        <div
          key={i}
          className="h-14 bg-[var(--g5-bg2)] rounded-[var(--g5-radius-sm)] animate-pulse"
        />
      ))}
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Main Content Component                                             */
/* ------------------------------------------------------------------ */

/**
 * Embeddable inner content for the org-members admin surface. Renders the
 * header, member table, pending invitations, and dialogs WITHOUT any
 * `<AppShell>` chrome.
 */
export function OrgMembersPageContent() {
  const { org } = useParams<{ org: string }>();
  const { user, hasScope } = useAuth();
  const queryClient = useQueryClient();

  // Derive orgId from user's orgs list using the route param (slug)
  const currentOrg = useMemo(
    () => user?.orgs.find((o) => o.orgSlug === org),
    [user, org],
  );
  const orgId = currentOrg?.orgId ?? "";
  const orgName = currentOrg?.orgName ?? org ?? "";
  const currentUserRole = (currentOrg?.role ?? "viewer") as
    | "admin"
    | "member"
    | "viewer";
  const currentUserId = user?.id ?? "";
  // Check both JWT scope and user role — the role from the user profile
  // is always accurate once auth completes, while JWT scopes may not
  // propagate through the React state cycle on every render.
  const isAdmin = hasScope("org:admin") || currentUserRole === "admin";

  // Data fetching
  const membersState = useOrgMembers(orgId);
  const invitationsState = useOrgInvitations(orgId);

  // Mutations
  const updateRole = useUpdateMemberRole();
  const removeMember = useRemoveMember();
  const setMemberActive = useSetMemberActive();
  const [confirmActivity, setConfirmActivity] = useState<OrgMemberDTO | null>(
    null,
  );
  const [shutdownMember, setShutdownMember] = useState<OrgMemberDTO | null>(
    null,
  );
  const stopMemberWorkspaces = useStopMemberWorkspaces();
  const [confirmStop, setConfirmStop] = useState<OrgMemberDTO | null>(null);
  const resendInvitation = useResendInvitation();
  const revokeInvitation = useRevokeInvitation();

  // UI state
  const [inviteDialogOpen, setInviteDialogOpen] = useState(false);
  const [confirmRemove, setConfirmRemove] = useState<{
    userId: string;
    name: string;
    email: string;
  } | null>(null);
  const [confirmRevoke, setConfirmRevoke] = useState<{
    invitationId: string;
    email: string;
  } | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);

  /* ---- handlers ---- */

  const handleChangeRole = useCallback(
    (userId: string, newRole: string) => {
      setActionError(null);
      updateRole.mutateAsync({ orgId, userId, role: newRole }).catch((err) => {
        setActionError(
          err instanceof Error ? err.message : "Failed to update role",
        );
      });
    },
    [orgId, updateRole],
  );

  const handleRemoveClick = useCallback(
    (userId: string) => {
      const member =
        membersState.status === "ok"
          ? membersState.data.find((m) => m.userId === userId)
          : null;
      if (member) {
        setConfirmRemove({
          userId,
          name: member.displayName,
          email: member.email,
        });
      }
    },
    [membersState],
  );

  const handleConfirmRemove = useCallback(() => {
    if (confirmRemove) {
      removeMember.mutateAsync({ orgId, userId: confirmRemove.userId }).then(
        () => setConfirmRemove(null),
        (err) => {
          setActionError(
            err instanceof Error ? err.message : "Failed to remove member",
          );
          setConfirmRemove(null);
        },
      );
    }
  }, [confirmRemove, orgId, removeMember]);

  const handleResend = useCallback(
    (invitationId: string) => {
      setActionError(null);
      resendInvitation.mutateAsync({ orgId, invitationId }).catch((err) => {
        setActionError(
          err instanceof Error ? err.message : "Failed to resend invitation",
        );
      });
    },
    [orgId, resendInvitation],
  );

  const handleRevokeClick = useCallback(
    (invitationId: string) => {
      const invitation =
        invitationsState.status === "ok"
          ? invitationsState.data.find((inv) => inv.id === invitationId)
          : null;
      if (invitation) {
        setConfirmRevoke({ invitationId, email: invitation.email });
      }
    },
    [invitationsState],
  );

  const handleConfirmRevoke = useCallback(() => {
    if (confirmRevoke) {
      revokeInvitation
        .mutateAsync({
          orgId,
          invitationId: confirmRevoke.invitationId,
        })
        .then(
          () => setConfirmRevoke(null),
          (err) => {
            setActionError(
              err instanceof Error
                ? err.message
                : "Failed to revoke invitation",
            );
            setConfirmRevoke(null);
          },
        );
    }
  }, [confirmRevoke, orgId, revokeInvitation]);

  const handleInviteSent = useCallback((_result: InviteMemberResponseDTO) => {
    // Invitation sent — the query cache will be invalidated by the mutation hook
    setInviteDialogOpen(false);
  }, []);

  /* ---- render ---- */

  return (
    <div className="max-w-4xl mx-auto px-6 py-8">
      {/* Page header */}
      <div className="flex items-center justify-between mb-6">
        <div className="flex items-center gap-3">
          <Users className="w-6 h-6 text-[var(--g5-text1)]" />
          <h1 className="text-xl font-semibold text-[var(--g5-text0)]">
            Members — {orgName}
          </h1>
        </div>
        {isAdmin && (
          <Button
            data-testid="org-members-open-invite"
            kind="primary"
            onClick={() => setInviteDialogOpen(true)}
          >
            <UserPlus className="w-4 h-4 mr-1 inline" />
            Invite +
          </Button>
        )}
      </div>

      {/* Action error banner */}
      {actionError && (
        <div className="mb-4 flex items-center gap-2 rounded-md border border-[var(--g5-blocked)] bg-[var(--g5-bg1)] px-4 py-2 text-sm text-[var(--g5-blocked)]">
          <AlertCircle className="w-4 h-4 shrink-0" />
          <span className="flex-1">{actionError}</span>
          <button
            data-testid="org-members-dismiss-error"
            onClick={() => setActionError(null)}
            className="text-[var(--g5-text2)] hover:text-[var(--g5-text0)]"
          >
            &times;
          </button>
        </div>
      )}

      {/* Member table */}
      {membersState.status === "loading" && <SkeletonTable />}

      {membersState.status === "error" && (
        <div className="flex flex-col items-center gap-3 py-12 text-center">
          <AlertCircle className="w-8 h-8 text-[var(--g5-blocked)]" />
          <p className="text-[var(--g5-text1)]">Failed to load members.</p>
          <Button
            data-testid="org-members-retry"
            kind="secondary"
            onClick={() => {
              queryClient.invalidateQueries({
                queryKey: ["org-members", orgId],
              });
            }}
          >
            Retry
          </Button>
        </div>
      )}

      {membersState.status === "ok" && (
        <>
          <div className="border border-[var(--g5-stroke)] rounded-[var(--g5-radius-md)] overflow-hidden">
            <MemberTable
              members={membersState.data}
              currentUserId={currentUserId}
              currentUserRole={currentUserRole}
              onChangeRole={handleChangeRole}
              onRemove={handleRemoveClick}
              onActivity={setConfirmActivity}
              onShutdown={setShutdownMember}
              onStopWorkspaces={setConfirmStop}
            />
          </div>

          {membersState.data.length === 0 && (
            <p className="text-center text-[var(--g5-text2)] py-8">
              No members yet.
            </p>
          )}
        </>
      )}

      {/* Pending invitations */}
      {invitationsState.status === "ok" && (
        <PendingInvitationsTable
          invitations={invitationsState.data}
          onResend={handleResend}
          onRevoke={handleRevokeClick}
        />
      )}

      {/* Invite dialog */}
      <InviteDialog
        open={inviteDialogOpen}
        onClose={() => setInviteDialogOpen(false)}
        orgId={orgId}
        onInviteSent={handleInviteSent}
      />

      {shutdownMember && (
        <MemberShutdownModal
          orgId={orgId}
          userId={shutdownMember.userId}
          title={`Workspace shutdown — ${shutdownMember.displayName}`}
          onClose={() => setShutdownMember(null)}
        />
      )}
      {/* Confirmation modals */}
      {confirmStop && (
        <ConfirmModal
          confirmDisabled={stopMemberWorkspaces.isLoading}
          title={`Stop all workspaces of ${confirmStop.displayName}?`}
          message={`Every workspace ${confirmStop.userId === currentUserId ? "you own" : `${confirmStop.displayName} owns`} in ${orgName} stops after saving, and none can start until all have stopped. Access is unchanged and nothing restarts automatically.`}
          confirmLabel="Stop workspaces"
          onCancel={() => setConfirmStop(null)}
          onConfirm={() => {
            setActionError(null);
            const member = confirmStop;
            void stopMemberWorkspaces
              .mutateAsync({ orgId, userId: member.userId })
              .then(
                () => {
                  setConfirmStop(null);
                  setShutdownMember(member);
                },
                (error: unknown) => {
                  setConfirmStop(null);
                  setActionError(
                    error instanceof Error
                      ? error.message
                      : "Unable to stop the member's workspaces",
                  );
                },
              );
          }}
        />
      )}
      {confirmActivity && (
        <ConfirmModal
          confirmDisabled={setMemberActive.isLoading}
          title={`${confirmActivity.deactivatedAt ? "Reactivate" : "Deactivate"} ${confirmActivity.displayName}?`}
          message={
            confirmActivity.deactivatedAt
              ? `Restore access to ${orgName} with their existing role now. Workspaces stay stopped until deliberately started. Any pending shutdown must finish before a workspace can start.`
              : `Disable access to ${orgName}, including existing sessions, CLI credentials and API keys. Their workspaces will stop. Membership, role, ownership and history stay intact. Other organizations are unaffected.`
          }
          confirmLabel={
            confirmActivity.deactivatedAt ? "Reactivate" : "Deactivate"
          }
          onCancel={() => setConfirmActivity(null)}
          onConfirm={() => {
            setActionError(null);
            void setMemberActive
              .mutateAsync({
                orgId,
                userId: confirmActivity.userId,
                active: !!confirmActivity.deactivatedAt,
                activityRevision: confirmActivity.activityRevision,
              })
              .then(
                () => setConfirmActivity(null),
                (error: unknown) => {
                  setConfirmActivity(null);
                  setActionError(
                    error instanceof Error
                      ? error.message
                      : "Unable to change membership activity",
                  );
                },
              );
          }}
        />
      )}

      {confirmRemove && (
        <ConfirmModal
          title={`Remove ${confirmRemove.name}?`}
          message={`Remove ${confirmRemove.email} from ${orgName}? They will lose access to all workspaces.`}
          confirmLabel="Remove"
          onConfirm={handleConfirmRemove}
          onCancel={() => setConfirmRemove(null)}
        />
      )}

      {confirmRevoke && (
        <ConfirmModal
          title="Revoke invitation?"
          message={`Revoke the pending invitation for ${confirmRevoke.email}? This action cannot be undone.`}
          confirmLabel="Revoke"
          onConfirm={handleConfirmRevoke}
          onCancel={() => setConfirmRevoke(null)}
        />
      )}
    </div>
  );
}
