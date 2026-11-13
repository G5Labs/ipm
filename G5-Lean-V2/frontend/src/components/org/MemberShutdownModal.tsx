/**
 * MemberShutdownModal — per-workspace progress while a member's workspaces
 * are being stopped (`g5.lifecycle.stop.by-user`), polled from
 * `GET /orgs/:orgId/members/:userId/shutdown`. Shared by the Members page and
 * Home's Stop my workspaces.
 *
 * @module org/MemberShutdownModal
 */

import React from "react";

import { Button, Modal } from "../ui/UI";
import { useMemberShutdown } from "../../hooks/useOrg";

export function MemberShutdownModal({
  orgId,
  userId,
  title,
  onClose,
}: {
  orgId: string;
  userId: string;
  title: string;
  onClose: () => void;
}) {
  const shutdown = useMemberShutdown(orgId, userId);
  return (
    <Modal
      onClose={onClose}
      ariaLabel="Workspace shutdown progress"
      size="sm"
      className="p-6"
    >
      <h3 className="text-lg font-semibold mb-3">{title}</h3>
      {shutdown.status === "ok" ? (
        <ul className="space-y-2">
          {shutdown.data.map((workspace) => (
            <li key={workspace.workspaceId}>
              {workspace.name}:{" "}
              {workspace.state === "stopped"
                ? "Stopped"
                : workspace.state === "stopping"
                  ? "Stopping; awaiting confirmation"
                  : "Pending"}
              {workspace.blocker && workspace.state !== "stopped" && (
                <span className="block text-xs text-[var(--g5-warn)]">
                  Blocked: {workspace.blocker}
                </span>
              )}
            </li>
          ))}
          {shutdown.data.length === 0 && <li>No owned workspaces.</li>}
        </ul>
      ) : (
        <p>
          {shutdown.status === "error"
            ? "Unable to load shutdown progress."
            : "Loading…"}
        </p>
      )}
      <Button kind="secondary" className="mt-4" onClick={onClose}>
        Close
      </Button>
    </Modal>
  );
}
