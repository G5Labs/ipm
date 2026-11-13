// Home's Stop my workspaces: stop by user for the caller themself
// (`POST /orgs/:orgId/members/:userId/stop-workspaces`,
// g5.lifecycle.stop.by-user) — the same hold the Members page's own-row
// action sets. Every workspace the caller owns in the organization saves and
// stops, and none starts until all have stopped. While the hold is pending
// (the roster's shutdownPending) the control opens its progress instead of
// asking again; a repeated request would only join it. It sits in the Running
// compute tile's header, sized like that tile's row controls.

import { useQueryClient } from "@tanstack/react-query";
import { Power } from "lucide-react";
import React, { useState } from "react";

import { runningComputeQueryKey } from "../../hooks/useHome";
import { useStopMemberWorkspaces } from "../../hooks/useOrg";
import { MemberShutdownModal } from "../org/MemberShutdownModal";
import { Button, Modal, classNames } from "../ui/UI";
import { HOME_ROW_ACTION_CLASSES } from "./HomeTileParts";

export interface HomeStopMyWorkspacesProps {
  orgId: string;
  orgName: string;
  userId: string;
  shutdownPending: boolean;
  onNotify: (kind: "info" | "error", title: string, message: string) => void;
}

export function HomeStopMyWorkspaces({
  orgId,
  orgName,
  userId,
  shutdownPending,
  onNotify,
}: HomeStopMyWorkspacesProps) {
  const queryClient = useQueryClient();
  const stop = useStopMemberWorkspaces();
  const [confirming, setConfirming] = useState(false);
  const [showProgress, setShowProgress] = useState(false);

  const confirmStop = async () => {
    try {
      await stop.mutateAsync({ orgId, userId });
      setShowProgress(true);
    } catch (error) {
      onNotify(
        "error",
        "Could not stop your workspaces",
        error instanceof Error ? error.message : String(error),
      );
    } finally {
      setConfirming(false);
      void queryClient.invalidateQueries({
        queryKey: runningComputeQueryKey(orgId),
      });
    }
  };

  return (
    <>
      <button
        type="button"
        data-testid="home-stop-my-workspaces"
        data-pending={shutdownPending ? "true" : undefined}
        onClick={() =>
          shutdownPending ? setShowProgress(true) : setConfirming(true)
        }
        className={classNames(
          HOME_ROW_ACTION_CLASSES,
          "inline-flex h-6 items-center gap-1 whitespace-nowrap",
        )}
      >
        <Power aria-hidden="true" className="h-3 w-3" />
        {shutdownPending ? "Stopping my workspaces…" : "Stop my workspaces"}
      </button>
      {confirming && (
        <Modal
          onClose={() => setConfirming(false)}
          ariaLabel="Stop all your workspaces?"
          size="sm"
          className="p-6"
        >
          <h3 className="text-[var(--g5-text0)] text-lg font-semibold mb-2">
            Stop all your workspaces?
          </h3>
          <p className="text-[var(--g5-text1)] mb-4">
            Every workspace you own in {orgName} stops after saving, and none
            can start until all have stopped. Access is unchanged and nothing
            restarts automatically.
          </p>
          <div className="flex justify-end gap-2">
            <Button
              data-testid="home-stop-my-workspaces-cancel"
              kind="secondary"
              onClick={() => setConfirming(false)}
            >
              Cancel
            </Button>
            <button
              type="button"
              data-testid="home-stop-my-workspaces-confirm"
              disabled={stop.isLoading}
              className="bg-[var(--g5-blocked)] text-white px-4 py-2 rounded-[var(--g5-radius-sm)] hover:opacity-90"
              onClick={() => void confirmStop()}
            >
              Stop workspaces
            </button>
          </div>
        </Modal>
      )}
      {showProgress && (
        <MemberShutdownModal
          orgId={orgId}
          userId={userId}
          title="Stopping your workspaces"
          onClose={() => setShowProgress(false)}
        />
      )}
    </>
  );
}
