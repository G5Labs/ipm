import { describe, expect, it } from "vitest";
import {
  activityLabel,
  activitySummary,
  backgroundTaskKindLabel,
  formatElapsedWait,
} from "./chatActivity";

describe("conversation activity labels", () => {
  it("uses supplied descriptions, file basenames, then command previews", () => {
    expect(
      activityLabel("Bash", {
        description: "Check the changes",
        command: "git diff",
      }),
    ).toBe("Check the changes");
    expect(
      activityLabel("Read", {
        description: "Read",
        path: "C:\\project\\report.md",
      }),
    ).toBe("Read report.md");
    expect(activityLabel("command_execution", { command: "npm test" })).toBe(
      "npm test",
    );
    expect(activityLabel("file_change", { path: "/work/ChatPanel.tsx" })).toBe(
      "Edit ChatPanel.tsx",
    );
  });

  it("summarizes repeated reads and known successful files without guessing diff counts", () => {
    const read = {
      toolName: "Read",
      display: { path: "/tmp/earliest.txt" },
      status: "done",
    };
    expect(activitySummary([read, read, read])).toBe("Read earliest.txt");
    const edit = {
      toolName: "file_change",
      display: { paths: ["one.ts", "two.ts"] },
      status: "done",
    };
    const command = {
      toolName: "command_execution",
      display: { command: "npm test" },
      status: "done",
    };
    expect(activitySummary([edit, command])).toBe(
      "Ran 1 command, Updated 2 files",
    );
    expect(activitySummary([{ ...edit, status: "error" }, command])).toBe(
      "Ran 1 command, 1 file operation",
    );
  });

  it("does not describe unfinished commands as completed", () => {
    expect(
      activitySummary([
        { toolName: "Bash", display: {}, status: "done" },
        { toolName: "Bash", display: {}, status: "ended" },
      ]),
    ).toBe("2 commands");
  });
});

describe("background wait presentation", () => {
  it("formats the elapsed wait in seconds, then minutes, then hours", () => {
    expect(formatElapsedWait(0)).toBe("0s");
    expect(formatElapsedWait(42_900)).toBe("42s");
    expect(formatElapsedWait(65_000)).toBe("1m 05s");
    expect(formatElapsedWait(3_599_999)).toBe("59m 59s");
    expect(formatElapsedWait(3_600_000)).toBe("1h 00m");
    expect(formatElapsedWait(7_380_000)).toBe("2h 03m");
    expect(formatElapsedWait(26 * 3_600_000)).toBe("26h 00m");
    // A start ahead of the local clock never reads as negative.
    expect(formatElapsedWait(-5_000)).toBe("0s");
    expect(formatElapsedWait(Number.NaN)).toBe("0s");
  });

  it("labels background work by its reported kind", () => {
    expect(backgroundTaskKindLabel("shell")).toBe("Command");
    expect(backgroundTaskKindLabel("agent")).toBe("Agent");
    expect(backgroundTaskKindLabel("other")).toBe("Task");
  });
});
