import { describe, expect, it } from "vitest";

import {
  previewEndingFromOutcome,
  previewEndingSentence,
  previewRunHasLog,
  previewRunTitle,
  previewStageWord,
} from "./previewRunWords";

describe("previewRunWords (ui g5.ui.activity.preview-row)", () => {
  it("words every live stage, naming the port while serving", () => {
    expect(previewStageWord("queued")).toBe("Queued");
    expect(previewStageWord("starting")).toBe("Starting");
    expect(previewStageWord("building")).toBe("Building");
    expect(previewStageWord("serving", 3000)).toBe("Serving on port 3000");
    expect(previewStageWord("serving", null)).toBe("Serving");
    expect(previewStageWord("closing")).toBe("Closing");
  });

  it("titles the run by its profile, with the ending verb once ended", () => {
    expect(previewRunTitle({ profile: "full-stack" })).toBe(
      "Preview · full-stack",
    );
    expect(previewRunTitle({ profile: null })).toBe(
      "Preview · default profile",
    );
    expect(
      previewRunTitle({ profile: "full-stack", ending: { kind: "closed" } }),
    ).toBe("Preview closed · full-stack");
    expect(
      previewRunTitle({
        profile: "full-stack",
        ending: { kind: "did_not_start" },
      }),
    ).toBe("Preview did not start · full-stack");
  });

  it("gives every ending one sentence in the run log's words", () => {
    expect(previewEndingSentence({ kind: "ended" })).toBe(
      "The application exited on its own",
    );
    expect(previewEndingSentence({ kind: "closed", close: "clean" })).toBe(
      "Closed by you",
    );
    expect(previewEndingSentence({ kind: "closed", close: "failed" })).toBe(
      "Closed by you, but the cleanup command failed",
    );
    expect(previewEndingSentence({ kind: "closed", close: "killed" })).toBe(
      "Closed by you, but cleanup did not finish in time and was killed",
    );
    expect(
      previewEndingSentence({ kind: "stopped", reason: "idle_stop" }),
    ).toBe("Stopped because the workspace went idle");
    expect(
      previewEndingSentence({ kind: "stopped", reason: "host_drain" }),
    ).toBe("Stopped while its host was drained");
    expect(previewEndingSentence({ kind: "stopped", cause: "unstarted" })).toBe(
      "Stopped before it started",
    );
    expect(previewEndingSentence({ kind: "stopped" })).toBe("Stopped");
    expect(previewEndingSentence({ kind: "closed", close: "lost" })).toBe(
      "Closed by you, but the workspace VM was lost before cleanup finished",
    );
    expect(previewEndingSentence({ kind: "stopped", close: "lost" })).toBe(
      "Closed, but the workspace VM was lost before cleanup finished",
    );
    expect(
      previewEndingSentence({
        kind: "stopped",
        reason: "idle_stop",
        close: "killed",
      }),
    ).toBe(
      "Stopped because the workspace went idle, but cleanup did not finish in time and was killed",
    );
    expect(previewEndingSentence({ kind: "stopped", close: "failed" })).toBe(
      "Closed, but the cleanup command failed",
    );
    expect(previewEndingSentence({ kind: "stopped", close: "killed" })).toBe(
      "Closed, but cleanup did not finish in time and was killed",
    );
    expect(
      previewEndingSentence({ kind: "stopped", reason: "something_new" }),
    ).toBe("Stopped: something new");
    expect(
      previewEndingSentence({
        kind: "failed",
        cause: "app_exited",
        exitCode: 7,
      }),
    ).toBe("Application exited with status 7");
    expect(
      previewEndingSentence({
        kind: "failed",
        cause: "app_exited",
        exitCode: null,
        signal: 9,
      }),
    ).toBe("Application exited with signal 9");
    expect(
      previewEndingSentence({
        kind: "failed",
        cause: "module",
        message: "profile full-stack has no command",
      }),
    ).toBe("Preview failed: profile full-stack has no command");
    expect(previewEndingSentence({ kind: "failed", cause: "module" })).toBe(
      "Preview failed: unknown reason",
    );
    expect(previewEndingSentence({ kind: "failed", cause: "timed_out" })).toBe(
      "Exceeded its time limit",
    );
    expect(
      previewEndingSentence({ kind: "failed", cause: "limit_exceeded" }),
    ).toBe("Exceeded its output limit");
    expect(
      previewEndingSentence({ kind: "failed", cause: "interrupted" }),
    ).toBe("Interrupted: supervision of the run was lost");
    expect(
      previewEndingSentence({
        kind: "failed",
        cause: "vm_lost",
        reason: "vm_lost",
      }),
    ).toBe("Lost with the workspace VM");
    expect(
      previewEndingSentence({
        kind: "failed",
        cause: "vm_lost",
        reason: "idle_stop",
      }),
    ).toBe(
      "Lost with the workspace VM while it was being stopped because the workspace went idle",
    );
    expect(
      previewEndingSentence({
        kind: "did_not_start",
        message: "Conflict: a preview is already running",
      }),
    ).toBe("Could not be started: Conflict: a preview is already running");
  });

  it("knows which runs have a log to open (g5.preview.run-log.ended)", () => {
    expect(previewRunHasLog({ stage: "building" })).toBe(true);
    expect(
      previewRunHasLog({ ending: { kind: "failed", cause: "module" } }),
    ).toBe(true);
    expect(previewRunHasLog({ ending: { kind: "did_not_start" } })).toBe(false);
    expect(
      previewRunHasLog({ ending: { kind: "failed", cause: "vm_lost" } }),
    ).toBe(false);
    expect(
      previewRunHasLog({ ending: { kind: "closed", close: "lost" } }),
    ).toBe(false);
    expect(
      previewRunHasLog({ ending: { kind: "stopped", close: "failed" } }),
    ).toBe(true);
    expect(
      previewRunHasLog({ ending: { kind: "closed", close: "killed" } }),
    ).toBe(true);
    expect(
      previewRunHasLog({ ending: { kind: "closed", close: "clean" } }),
    ).toBe(false);
    expect(previewRunHasLog({ ending: { kind: "closed" } })).toBe(false);
    expect(
      previewRunHasLog({ ending: { kind: "stopped", reason: "idle_stop" } }),
    ).toBe(false);
    expect(previewRunHasLog({ ending: { kind: "ended" } })).toBe(false);
    expect(previewRunHasLog({})).toBe(false);
  });

  it("reads a module outcome payload into the same ending shape for the run log", () => {
    expect(
      previewEndingFromOutcome({
        status: "failed",
        result: { status: "app_exited", app_exit_code: 7, app_signal: null },
      }),
    ).toEqual({
      kind: "failed",
      cause: "app_exited",
      exitCode: 7,
      signal: null,
    });
    expect(
      previewEndingFromOutcome({
        status: "failed",
        result: { message: "no command" },
      }),
    ).toEqual({ kind: "failed", cause: "module", message: "no command" });
    expect(previewEndingFromOutcome({ status: "timed_out" })).toEqual({
      kind: "failed",
      cause: "timed_out",
    });
    expect(previewEndingFromOutcome({ status: "limit_exceeded" })).toEqual({
      kind: "failed",
      cause: "limit_exceeded",
    });
    expect(
      previewEndingFromOutcome({
        status: "interrupted",
        detail: { reason: "supervision_lost" },
      }),
    ).toEqual({ kind: "failed", cause: "interrupted" });
    expect(
      previewEndingFromOutcome({
        status: "cancelled",
        detail: { close: "failed" },
      }),
    ).toEqual({ kind: "stopped", reason: undefined, close: "failed" });
    expect(previewEndingFromOutcome({ status: "succeeded" })).toEqual({
      kind: "ended",
    });
    expect(previewEndingFromOutcome({ status: "unknown" })).toBeNull();
    expect(previewEndingFromOutcome(null)).toBeNull();
  });
});
