/**
 * InviteDialog — Modal dialog for inviting a single member to the organization.
 *
 * Opened from OrgMembersPage's "[Invite +]" button. Provides email input,
 * role selector with privilege summary tooltips, and sends the invitation
 * via the useInviteOrgMember mutation hook.
 *
 * @module org/InviteDialog
 */

import { useState, useEffect, useRef, useCallback } from "react";
import { Mail, Loader2, AlertCircle, X } from "lucide-react";

import { Button, Modal } from "../ui/UI";
import { useInviteOrgMember } from "../../hooks/useOrg";

import type { InviteMemberResponseDTO } from "../../lib/types";

/* ------------------------------------------------------------------ */
/*  Props                                                              */
/* ------------------------------------------------------------------ */

export interface InviteDialogProps {
  /** Whether the dialog is open */
  open: boolean;
  /** Called to close the dialog */
  onClose: () => void;
  /** Org ID to invite the member to */
  orgId: string;
  /** Called when invitation is successfully sent */
  onInviteSent: (result: InviteMemberResponseDTO) => void;
}

/* ------------------------------------------------------------------ */
/*  Role descriptions                                                  */
/* ------------------------------------------------------------------ */

const ROLE_DESCRIPTIONS: Record<string, string> = {
  admin: "Can manage members, billing, and all workspace settings",
  member: "Can create and edit workspaces, run tasks",
  viewer: "Can view workspaces and read content",
};

/* ------------------------------------------------------------------ */
/*  Component                                                          */
/* ------------------------------------------------------------------ */

export function InviteDialog({
  open,
  onClose,
  orgId,
  onInviteSent,
}: InviteDialogProps) {
  const [email, setEmail] = useState("");
  const [role, setRole] = useState<"admin" | "member" | "viewer">("member");
  const [error, setError] = useState<string | null>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);

  const inviteMember = useInviteOrgMember();
  const emailInputRef = useRef<HTMLInputElement>(null);

  /* ---- reset state when dialog opens ---- */
  useEffect(() => {
    if (open) {
      setEmail("");
      setRole("member");
      setError(null);
      setIsSubmitting(false);
    }
  }, [open]);

  /* ---- validate email ---- */
  const isEmailValid = email.includes("@");

  /* ---- submit handler ---- */
  const handleSubmit = useCallback(async () => {
    if (!isEmailValid) {
      setError("Please enter a valid email address.");
      return;
    }

    setIsSubmitting(true);
    setError(null);

    try {
      const result = await inviteMember.mutateAsync({
        orgId,
        email: email.trim(),
        role,
      });
      // The API can return an immediate membership or a pending invitation.
      onInviteSent(result);
      onClose();
    } catch (err: any) {
      const message =
        err?.message ?? "Failed to send invitation. Please try again.";
      // Handle specific error codes
      if (err?.status === 409) {
        setError("User is already a member of this organization.");
      } else if (err?.status === 422) {
        setError("Invalid email address.");
      } else {
        setError(message);
      }
    } finally {
      setIsSubmitting(false);
    }
  }, [email, role, orgId, isEmailValid, inviteMember, onInviteSent, onClose]);

  /* ---- render ---- */
  if (!open) return null;

  return (
    <Modal
      onClose={onClose}
      ariaLabel="Invite member"
      size="sm"
      closeDisabled={isSubmitting}
      initialFocusRef={emailInputRef}
      className="p-6"
    >
      {/* Header */}
      <div className="flex items-center justify-between mb-4">
        <h2 className="text-lg font-semibold text-[var(--g5-text0)] flex items-center gap-2">
          <Mail className="w-5 h-5" />
          Invite Member
        </h2>
        <button
          data-testid="invite-dialog-close"
          onClick={onClose}
          disabled={isSubmitting}
          className="p-1 text-[var(--g5-text2)] hover:text-[var(--g5-text0)] disabled:opacity-50"
          title="Close"
          aria-label="Close dialog"
        >
          <X className="w-5 h-5" />
        </button>
      </div>

      <div className="space-y-4">
        {/* Email input */}
        <div>
          <label
            htmlFor="invite-email"
            className="block text-sm text-[var(--g5-text1)] mb-1"
          >
            Email address
          </label>
          <input
            ref={emailInputRef}
            id="invite-email"
            type="email"
            value={email}
            onChange={(e) => {
              setEmail(e.target.value);
              if (error) setError(null);
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !isSubmitting) handleSubmit();
            }}
            placeholder="colleague@example.com"
            autoComplete="email"
            className="
                w-full px-3 py-2
                bg-[var(--g5-bg0)] text-[var(--g5-text0)]
                border border-[var(--g5-stroke)]
                rounded-[var(--g5-radius-sm)]
                placeholder:text-[var(--g5-text2)]
                focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]
              "
          />
        </div>

        {/* Role selector */}
        <div>
          <label className="block text-sm text-[var(--g5-text1)] mb-2">
            Role
          </label>
          <div className="space-y-2">
            {(["admin", "member", "viewer"] as const).map((r) => (
              <label
                key={r}
                className={`
                    flex items-start gap-3 p-3 rounded-[var(--g5-radius-sm)] cursor-pointer
                    border transition-colors
                    ${
                      role === r
                        ? "border-[var(--g5-info)] bg-[var(--g5-info)]/5"
                        : "border-[var(--g5-stroke)] bg-[var(--g5-bg0)] hover:border-[var(--g5-text2)]"
                    }
                  `}
              >
                <input
                  data-testid="invite-dialog-role-radio"
                  type="radio"
                  name="invite-role"
                  value={r}
                  checked={role === r}
                  onChange={() => setRole(r)}
                  className="mt-0.5 accent-[var(--g5-info)]"
                />
                <div>
                  <div className="text-sm font-medium text-[var(--g5-text0)] capitalize">
                    {r}
                  </div>
                  <div className="text-xs text-[var(--g5-text2)]">
                    {ROLE_DESCRIPTIONS[r]}
                  </div>
                </div>
              </label>
            ))}
          </div>
        </div>

        {/* Error message */}
        {error && (
          <div className="flex items-center gap-2 text-sm text-[var(--g5-blocked)]">
            <AlertCircle className="w-4 h-4 flex-shrink-0" />
            {error}
          </div>
        )}

        {/* Action buttons */}
        <div className="flex justify-end gap-2 pt-2">
          <Button
            data-testid="invite-dialog-cancel"
            kind="secondary"
            onClick={onClose}
            disabled={isSubmitting}
          >
            Cancel
          </Button>
          <Button
            data-testid="invite-dialog-submit"
            kind="primary"
            disabled={isSubmitting || !email.trim()}
            onClick={handleSubmit}
          >
            {isSubmitting ? (
              <span className="flex items-center gap-2">
                <Loader2 className="w-4 h-4 animate-spin" />
                Sending…
              </span>
            ) : (
              "Send Invite"
            )}
          </Button>
        </div>
      </div>
    </Modal>
  );
}
