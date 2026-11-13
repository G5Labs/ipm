import type { AsyncState, WorkspaceDetail } from "./types";

/**
 * Whether a workspace accepts writes to comment and review-suggestion
 * threads. A read-only or freezing workspace, and Base, accept none
 * (`g5.documents.comments.authority`, `g5.documents.comments.replies`).
 */
export function workspaceAcceptsThreadWrites(
  detail: Pick<WorkspaceDetail, "isBase" | "readOnly"> & {
    mutationPolicyTransition?: string | null;
  },
): boolean {
  return (
    !detail.isBase &&
    detail.readOnly !== true &&
    detail.mutationPolicyTransition !== "freezing"
  );
}

/** Whether a source's threads may be written from here, and why not. */
export interface SourceThreadWrites {
  writable: boolean;
  /** Shown where a reply box would be; empty when writable. */
  closedNote: string;
}

/**
 * Thread controls follow the source's known state: only a loaded, writable
 * source offers them. While its state is loading, or could not be loaded,
 * none are offered (`g5.documents.comments.controls`).
 */
export function sourceThreadWrites(
  detail: AsyncState<WorkspaceDetail>,
): SourceThreadWrites {
  if (detail.status === "loading") {
    return {
      writable: false,
      closedNote: "Replies open once the workspace's state has loaded.",
    };
  }
  if (detail.status === "error") {
    return {
      writable: false,
      closedNote: "Replies are off: the workspace's state could not be loaded.",
    };
  }
  if (workspaceAcceptsThreadWrites(detail.data)) {
    return { writable: true, closedNote: "" };
  }
  return {
    writable: false,
    closedNote: detail.data.isBase
      ? "Base is read-only, so its discussions accept no replies."
      : "The workspace is read-only, so its discussions accept no replies.",
  };
}
