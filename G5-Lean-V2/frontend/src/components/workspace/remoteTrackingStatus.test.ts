import { describe, expect, it } from "vitest";
import type {
  BranchTrackingRepo,
  WorkspaceMergeRunSummary,
} from "../../lib/types";
import {
  deriveRemoteTrackingStatus,
  remoteTrackingLabel,
} from "./remoteTrackingStatus";

function repo(overrides: Partial<BranchTrackingRepo> = {}): BranchTrackingRepo {
  return {
    repoId: "repo-1",
    repoName: "demo-repo",
    repoBranchName: "main",
    trackingMode: "remote_tracking",
    headCommit: "deadbeef",
    remoteBranch: "main",
    // baseline == head ⇒ healthy unless a case overrides it.
    baselineCommit: "deadbeef",
    ...overrides,
  } as BranchTrackingRepo;
}

function resetRun(overrides: Partial<WorkspaceMergeRunSummary> = {}) {
  return {
    id: "merge-run-1",
    repoId: "repo-1",
    sourceKind: "remote_tracking",
    phase: "reset_below_baseline",
    status: "waiting_for_human",
    ...overrides,
  } as WorkspaceMergeRunSummary;
}

describe("deriveRemoteTrackingStatus", () => {
  it("names a pending workspace rewrite distinctly from new commits", () => {
    const ahead = repo({ headCommit: "cafef00d", baselineCommit: "deadbeef" });
    expect(deriveRemoteTrackingStatus([ahead], [])?.label).toBe(
      "Ahead — local commits to push",
    );
    expect(
      deriveRemoteTrackingStatus([{ ...ahead, rewritePending: true }], [])
        ?.label,
    ).toBe("Ahead — rewritten history to push");
  });

  it("returns null when there are no remote-tracking rows", () => {
    expect(deriveRemoteTrackingStatus([], [])).toBeNull();
  });

  it("is healthy when head, baseline and advisory agree", () => {
    const status = deriveRemoteTrackingStatus(
      [repo({ remoteTipAdvisorySha: "deadbeef" })],
      [],
    );
    expect(status).toMatchObject({
      state: "healthy",
      label: "Tracking healthy",
    });
  });

  it("is behind when the advisory moved past the baseline", () => {
    const behind = repo({ remoteTipAdvisorySha: "cafebabe" });
    const status = deriveRemoteTrackingStatus([behind], []);
    expect(status?.state).toBe("behind");
    expect(status?.behindRepo).toBe(behind);
    expect(status?.label).not.toMatch(/\d/);
  });

  it("is ahead, not behind, for unpushed local commits", () => {
    const status = deriveRemoteTrackingStatus(
      [repo({ baselineCommit: "0000", remoteTipAdvisorySha: "0000" })],
      [],
    );
    expect(status).toMatchObject({
      state: "ahead",
      label: "Ahead — local commits to push",
    });
  });

  it("ranks behind above ahead", () => {
    const status = deriveRemoteTrackingStatus(
      [repo({ baselineCommit: "0000", remoteTipAdvisorySha: "cafebabe" })],
      [],
    );
    expect(status?.state).toBe("behind");
  });

  it("ranks a pause above behind and names its reason", () => {
    const status = deriveRemoteTrackingStatus(
      [
        repo({
          syncPausedReason: "diverged",
          remoteTipAdvisorySha: "cafebabe",
        }),
      ],
      [],
    );
    expect(status).toMatchObject({
      state: "blocked",
      pauseReason: "diverged",
      label: "Paused — merge required",
    });
  });

  it("treats a watermark pause with its own reset run as review", () => {
    const status = deriveRemoteTrackingStatus(
      [repo({ syncPausedReason: "watermark_violation" })],
      [resetRun()],
    );
    expect(status?.state).toBe("review");
    expect(status?.pauseReason).toBe("watermark_violation");
  });

  it.each([
    ["no merge run", []],
    ["a non-reset phase", [resetRun({ phase: "workspace_rebase_preflight" })]],
    ["another repo's reset run", [resetRun({ repoId: "repo-other" })]],
  ])("keeps a watermark pause blocked with %s", (_label, runs) => {
    const status = deriveRemoteTrackingStatus(
      [repo({ syncPausedReason: "watermark_violation" })],
      runs as WorkspaceMergeRunSummary[],
    );
    expect(status).toMatchObject({
      state: "blocked",
      label: "Paused — provider history rewritten",
    });
  });
});

describe("remoteTrackingLabel", () => {
  it.each([
    ["auth_required", "Authentication required"],
    ["push_failed", "Push failed"],
    ["fetch_failed", "Fetch failed"],
    ["merge_run:abc", "Paused — merge required"],
    ["org_binding_mismatch", "Paused — cross-org binding mismatch"],
    ["something_new", "Paused — something new"],
  ])("names the %s pause in words", (reason, label) => {
    expect(remoteTrackingLabel("blocked", reason)).toBe(label);
  });
});
