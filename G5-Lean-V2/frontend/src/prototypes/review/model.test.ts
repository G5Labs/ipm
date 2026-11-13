import { describe, expect, it } from "vitest";
import {
  appendEvent,
  changedFiles,
  changedRepos,
  configuredReviewers,
  currentRevision,
  comparison,
  contentModes,
  initialState,
  latestDecision,
  mayDecide,
  prior,
  pushedRecord,
  sameVector,
  pushDocs,
  requestReview,
  ribbonActions,
  scopes,
  submitDecision,
} from "./model";

describe("review prototype invariants", () => {
  it("has no requests or decisions before PR submission, even if an action is called directly", () => {
    const s = initialState("rahul-test");
    expect(s.decisions).toEqual([]);
    expect(s.events.some((e) => ["request", "decision"].includes(e.kind))).toBe(
      false,
    );
    expect(
      requestReview(s, "rahul-test", "owner", "business", ["James Moore"]),
    ).toBe(s);
    expect(
      submitDecision(s, "rahul-test", "reviewer", {
        scope: "business",
        result: "approved",
        body: "",
        revision: s.pushed,
      }),
    ).toBe(s);
  });
  it("excludes the owner from every configured approval type", () => {
    for (const scope of scopes) {
      expect(configuredReviewers("rahul-test", scope.id)).not.toContain(
        "Rahul",
      );
      expect(configuredReviewers("preview-test", scope.id)).not.toContain(
        "Kaihui Zheng",
      );
    }
  });
  it("allows stopped durable decisions only for addressed reviewers, once per request", () => {
    const s = initialState("preview-test");
    expect(s.running).toBe(false);
    expect(mayDecide(s, "preview-test", "reviewer", "security")).toBe(true);
    for (const role of ["owner", "viewer"] as const)
      expect(mayDecide(s, "preview-test", role, "security")).toBe(false);
    expect(mayDecide(s, "preview-test", "reviewer", "business")).toBe(true);
    const d = {
      scope: "security",
      result: "approved" as const,
      body: "Meets the contract.",
      revision: s.pushed,
    };
    const done = submitDecision(s, "preview-test", "reviewer", d);
    expect(done.requests.security).toEqual(["Ben Vandiver"]);
    expect(done.decisions).toHaveLength(2);
    expect(submitDecision(done, "preview-test", "reviewer", d)).toBe(done);
  });
  it("detects a docs-only vector change even when the application head is unchanged", () => {
    const s = initialState("preview-test");
    expect(changedRepos(prior, s.pushed)).toEqual(["docs"]);
    expect(changedFiles(prior, s.pushed).map((f) => f.id)).toEqual([
      "requirements",
      "ontology",
    ]);
  });
  it("derives each checkpoint from a formal decision for that person and scope", () => {
    const s = initialState("preview-test"),
      before = latestDecision(s, "Rahul", "security");
    expect(before?.result).toBe("changes");
    expect(latestDecision(s, "Rahul", "business")).toBeUndefined();
    expect(latestDecision(s, "Ben Vandiver", "security")).toBeUndefined();
    const commented = appendEvent(s, {
      kind: "comment",
      who: "Rahul",
      body: "Reading now",
    });
    expect(latestDecision(commented, "Rahul", "security")).toBe(before);
  });
  it("keeps the captured vector immutable after a concurrent push and excludes current dirty work", () => {
    const s = initialState("preview-test"),
      captured = structuredClone(s.pushed);
    const moved = {
      ...s,
      pushed: { ...s.pushed, heads: { ...s.pushed.heads, docs: "9999999" } },
    };
    const done = submitDecision(moved, "preview-test", "reviewer", {
      scope: "security",
      result: "approved",
      body: "",
      revision: captured,
    });
    const decision = latestDecision(done, "Rahul", "security")!;
    expect(decision.revision.heads.docs).toBe("5ac97e2");
    captured.heads.docs = "0000000";
    expect(decision.revision.heads.docs).toBe("5ac97e2");
    expect(currentRevision(s.pushed, { code: "dirty edit" }).files.code).toBe(
      "dirty edit",
    );
    expect(decision.revision.files.code).not.toBe("dirty edit");
  });
  it("accepts only configured reviewers and keeps separate requests across phases", () => {
    let s = { ...initialState("rahul-test"), pr: true };
    for (const scope of scopes)
      s = requestReview(s, "rahul-test", "owner", scope.id, [
        "James Moore",
        scope.other,
        "Rahul",
        "Unconfigured",
      ]);
    expect(s.requests).toEqual({
      business: ["James Moore", "David Chang"],
      security: ["James Moore", "Ben Vandiver"],
      release: ["James Moore", "Ziniu Wu"],
    });
    s = submitDecision(s, "rahul-test", "reviewer", {
      scope: "business",
      result: "changes",
      body: "Clarify retry ownership",
      revision: s.pushed,
    });
    expect(s.requests.security).toContain("James Moore");
    expect(s.requests.business).toEqual(["David Chang"]);
  });
  it("removes the repeated Approvals ribbon action in every phase", () => {
    for (const groups of Object.values(ribbonActions))
      expect(
        groups.flatMap((g) => g.actions).some((a) => a.target === "approvals"),
      ).toBe(false);
  });
  it("distinguishes uncommitted work from all unpushed work and unchanged files", () => {
    const s = { ...initialState("preview-test"), running: true };
    expect(comparison(s, "owner", "all").files.map((f) => f.id)).toContain(
      "guide",
    );
    expect(
      comparison(s, "owner", "uncommitted").files.map((f) => f.id),
    ).toEqual(["requirements", "code"]);
    expect(comparison(s, "owner", "unpushed").files.map((f) => f.id)).toEqual([
      "requirements",
      "code",
      "runbook",
    ]);
    expect(comparison(s, "owner", "base").files.map((f) => f.id)).not.toContain(
      "guide",
    );
  });
  it("never exposes current content to non-owners or a stopped owner", () => {
    const s = {
      ...initialState("preview-test"),
      running: true,
      edits: { code: "PRIVATE" },
    };
    for (const role of ["reviewer", "viewer"] as const) {
      expect(contentModes(role, true, true)).toEqual([
        "all",
        "base",
        "parent",
        "last",
      ]);
      for (const mode of [
        "all",
        "uncommitted",
        "unpushed",
        "base",
        "parent",
        "last",
      ] as const)
        expect(comparison(s, role, mode).after.files.code).not.toContain(
          "PRIVATE",
        );
    }
    expect(
      comparison({ ...s, running: false }, "owner", "unpushed").effectiveMode,
    ).toBe("base");
  });
  it("keeps saved base separate from the current parent's pushed vector", () => {
    const s = initialState("preview-test");
    expect(changedRepos(s.base, s.parent)).toEqual(["docs"]);
    expect(comparison(s, "viewer", "base").before.files.runbook).not.toContain(
      "Parent update",
    );
    expect(comparison(s, "viewer", "parent").before.files.runbook).toContain(
      "Parent update",
    );
  });
  it("general feedback never completes a request or advances a checkpoint", () => {
    const s = initialState("preview-test");
    const after = appendEvent(s, {
      kind: "comment",
      who: "Alex Chen",
      body: "Consider a recovery walkthrough.",
      feedback: true,
    });
    expect(after.requests).toBe(s.requests);
    expect(after.decisions).toBe(s.decisions);
    expect(latestDecision(after, "Alex Chen", "security")).toBeUndefined();
    expect(
      submitDecision(after, "preview-test", "viewer", {
        scope: "security",
        result: "approved",
        body: "",
        revision: s.pushed,
      }),
    ).toBe(after);
  });
  it("appends a complete vector on push and preserves unchanged member metadata and earlier revisions", () => {
    const state = initialState("preview-test");
    const previous = pushedRecord(state)!;
    expect(previous.commits["G5-Lean"].pushedAt).not.toBe(
      previous.commits["G5-Lean"].committedAt,
    );
    const updated = pushDocs(state, "preview-test", "2026-10-12T16:00:00Z");
    const pushed = pushedRecord(updated)!;
    expect(pushed.recordedAt).toBe("2026-10-12T16:00:00Z");
    expect(pushed.commits.docs.pushedAt).toBe("2026-10-12T16:00:00Z");
    expect(pushed.revision).toBe(updated.pushed);
    expect(changedRepos(previous.revision, pushed.revision)).toEqual(["docs"]);
    expect(pushed.commits["G5-Lean"]).toBe(previous.commits["G5-Lean"]);
    expect(updated.history.slice(1)).toEqual(state.history);
    expect(state.history).toHaveLength(10);
    expect(
      pushedRecord({
        ...state,
        pushed: {
          ...state.pushed,
          heads: { ...state.pushed.heads, docs: "unknown" },
        },
      }),
    ).toBeUndefined();
  });
  it("records complete consistent vectors, including prior review, saved base and inherited history", () => {
    const state = initialState("preview-test");
    expect(state.history[0].revision).toEqual(state.pushed);
    expect(
      state.history.find((entry) => sameVector(entry.revision, prior))
        ?.revision,
    ).toEqual(prior);
    const baseIndex = state.history.findIndex((entry) =>
      sameVector(entry.revision, state.base),
    );
    expect(baseIndex).toBeGreaterThan(0);
    expect(baseIndex).toBeLessThan(state.history.length - 1);
    for (const [index, entry] of state.history.entries()) {
      expect(Object.keys(entry.revision.heads)).toEqual(["G5-Lean", "docs"]);
      const previous = state.history[index + 1];
      if (!previous) continue;
      expect(new Date(entry.recordedAt).getTime()).toBeGreaterThan(
        new Date(previous.recordedAt).getTime(),
      );
      for (const repo of ["G5-Lean", "docs"] as const) {
        if (entry.revision.heads[repo] === previous.revision.heads[repo])
          expect(entry.commits[repo]).toBe(previous.commits[repo]);
      }
    }
  });
});
