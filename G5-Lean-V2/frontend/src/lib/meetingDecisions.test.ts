import { describe, expect, it } from "vitest";
import {
  appendIssueReference,
  isMeetingDecisionCard,
  meetingCardShape,
  meetingDecisionIssueOption,
  meetingIssueBody,
  meetingTaskKind,
  recordedIssueReference,
} from "./meetingDecisions";

import { meetingOptions, meetingTaskOptions } from "../test/questionFixtures";

const legacy = meetingOptions();
const task = meetingTaskOptions();

describe("meetingCardShape", () => {
  it("tells the task set from the legacy set, and both from ordinary questions", () => {
    const set = { setId: "meeting-x" };
    expect(meetingCardShape(set, { options: task, multiSelect: false })).toBe(
      "task",
    );
    expect(meetingCardShape(set, { options: legacy, multiSelect: false })).toBe(
      "legacy",
    );
    expect(
      meetingCardShape(
        { setId: "plan" },
        { options: task, multiSelect: false },
      ),
    ).toBeNull();
    expect(
      meetingCardShape(set, { options: task, multiSelect: true }),
    ).toBeNull();
    expect(
      meetingCardShape(set, { options: task.slice(0, 2), multiSelect: false }),
    ).toBeNull();
    expect(
      isMeetingDecisionCard(set, { options: task, multiSelect: false }),
    ).toBe(true);
    expect(
      isMeetingDecisionCard(set, { options: legacy, multiSelect: false }),
    ).toBe(true);
  });
});

describe("meetingTaskKind", () => {
  it("names attach and detach, nothing else", () => {
    expect(meetingTaskKind("attach")).toBe("attach");
    expect(meetingTaskKind("detach")).toBe("detach");
    expect(meetingTaskKind("reject")).toBeNull();
    expect(meetingTaskKind("accept")).toBeNull();
  });
});

describe("meetingDecisionIssueOption", () => {
  it("names the selected Delay or Out of scope option of a legacy meeting set", () => {
    const set = { setId: "meeting-kickoff-2026-10-15" };
    expect(
      meetingDecisionIssueOption(set, { options: legacy, multiSelect: false }, [
        "delay",
      ])?.label,
    ).toBe("Delay");
    expect(
      meetingDecisionIssueOption(set, { options: legacy, multiSelect: false }, [
        "out-of-scope",
      ])?.id,
    ).toBe("out-of-scope");
  });
  it("offers nothing for Accept, Reject, no selection, another set, other options or a task set", () => {
    const set = { setId: "meeting-kickoff" };
    for (const selected of [["accept"], ["reject"], []]) {
      expect(
        meetingDecisionIssueOption(
          set,
          { options: legacy, multiSelect: false },
          selected,
        ),
      ).toBeNull();
    }
    expect(
      meetingDecisionIssueOption(
        { setId: "plan" },
        { options: legacy, multiSelect: false },
        ["delay"],
      ),
    ).toBeNull();
    expect(
      meetingDecisionIssueOption(
        set,
        { options: [{ id: "delay", label: "Delay" }], multiSelect: false },
        ["delay"],
      ),
    ).toBeNull();
    expect(
      meetingDecisionIssueOption(set, { options: task, multiSelect: false }, [
        "detach",
      ]),
    ).toBeNull();
  });
});

describe("meetingIssueBody", () => {
  it("joins context, the decision and the meeting", () => {
    expect(
      meetingIssueBody(
        { context: "Detail: the overview goes away." },
        { id: "delay", label: "Delay" },
        " after the launch ",
        "Kickoff 2026-10-15",
      ),
    ).toBe(
      "Detail: the overview goes away.\n\nDecision: Delay — after the launch\n\nFrom meeting: Kickoff 2026-10-15",
    );
    expect(
      meetingIssueBody(
        {},
        { id: "out-of-scope", label: "Out of scope" },
        "",
        undefined,
      ),
    ).toBe("Decision: Out of scope");
  });
  it("carries only the comment for a task option, since the task is the decision", () => {
    expect(
      meetingIssueBody(
        { context: "Detail." },
        {
          id: "attach",
          label: "Create a task and attach to the current workspace",
        },
        "land it in AGENTS.md",
        "Standup",
      ),
    ).toBe(
      "Detail.\n\nDecision: land it in AGENTS.md\n\nFrom meeting: Standup",
    );
    expect(
      meetingIssueBody(
        { context: "Detail." },
        { id: "detach", label: "x" },
        "  ",
        "Standup",
      ),
    ).toBe("Detail.\n\nFrom meeting: Standup");
  });
});

describe("issue references in the comment", () => {
  const issue = {
    displayKey: "acme/api#77",
    url: "https://github.com/acme/api/issues/77",
  };
  it("appends the reference once", () => {
    expect(appendIssueReference("", issue)).toBe(
      "acme/api#77 https://github.com/acme/api/issues/77",
    );
    expect(appendIssueReference("after the launch\n", issue)).toBe(
      "after the launch\nacme/api#77 https://github.com/acme/api/issues/77",
    );
    const once = appendIssueReference("x", issue);
    expect(appendIssueReference(once, issue)).toBe(once);
  });
  it("finds the reference an earlier press recorded, and nothing in prose", () => {
    expect(
      recordedIssueReference(
        "after the launch\nacme/api#77 https://github.com/acme/api/issues/77",
      ),
    ).toEqual(issue);
    expect(
      recordedIssueReference(
        "acme/api#77 https://github.com/acme/api/issues/77",
      ),
    ).toEqual(issue);
    expect(recordedIssueReference("see acme/api#77 for details")).toBeNull();
    expect(recordedIssueReference("")).toBeNull();
  });
});
