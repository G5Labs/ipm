import { describe, expect, it } from "vitest";
import {
  selectAll,
  reconcileSelection,
  sourceApplyInput,
  sourceChildren,
  sourcePushedSelection,
  type SourceMenu,
  type SourceRepo,
} from "./sourceControlSelection";
const commit = (sha: string) => ({
  sha,
  summary: sha,
  authorName: "Owner",
  committedAt: 1,
  parents: ["base"],
});
const repo = (repoId: string, extra: Partial<SourceRepo> = {}): SourceRepo => ({
  repoId,
  name: repoId,
  base: "base",
  pushed: "pushed",
  commits: [commit("one"), commit("two")],
  truncated: false,
  live: { selectable: true },
  error: null,
  ...extra,
});
const menu = (...repos: SourceRepo[]): SourceMenu => ({
  fingerprint: "fingerprint",
  repos,
  private: true,
});
describe("captured pushed selections", () => {
  it("selects captured pushed commits without private work or unrelated repositories", () => {
    const captured = sourcePushedSelection(
      menu(repo("code"), repo("docs"), repo("other")),
      { code: "pushed", docs: "base" },
    );
    expect(captured).toEqual({
      code: { checked: ["one", "two"], followAll: false },
      docs: { checked: [], followAll: false },
      other: { checked: [], followAll: false },
    });
  });
  it("keeps only the captured tip's known ancestry, including merged parents", () => {
    const commits = [
      ["new", ["merge"]],
      ["merge", ["left", "right"]],
      ["left", ["first"]],
      ["right", ["first"]],
      ["first", ["base"]],
    ].map(([sha, parents]) => ({
      ...commit(sha as string),
      parents: parents as string[],
    }));
    expect(
      sourcePushedSelection(menu(repo("code", { commits, pushed: "new" })), {
        code: "merge",
      }).code.checked,
    ).toEqual(["merge", "left", "right", "first"]);
  });
  it("retains an older missing captured tip explicitly in a truncated menu", () => {
    expect(
      sourcePushedSelection(menu(repo("code", { truncated: true })), {
        code: "older-tip",
      }).code,
    ).toEqual({ checked: ["older-tip"], followAll: false });
  });
});
describe("repository selections at a Save checkpoint", () => {
  it("starts all available changes selected independently by repository", () => {
    const before = menu(repo("code"), repo("docs"));
    const all = selectAll(before);
    expect(all.code.checked).toEqual(["unpushed", "one", "two"]);
    all.code = { checked: ["two"], followAll: false };
    const input = sourceApplyInput(before, all);
    expect(input.repos[0]).toMatchObject({
      commits: ["two"],
      includeUnpushed: false,
    });
    expect(input.repos[1]).toMatchObject({
      commits: ["one", "two"],
      includeUnpushed: true,
    });
  });
  it("fully selected repos follow new commits and custom repos preserve exclusions", () => {
    const before = menu(repo("code"), repo("docs"));
    const selection = selectAll(before);
    selection.docs = { checked: ["one"], followAll: false };
    const after = menu(
      repo("code", { commits: [commit("new"), commit("one"), commit("two")] }),
      repo("docs", {
        commits: [commit("new-doc"), commit("one"), commit("two")],
      }),
    );
    const result = reconcileSelection(before, after, selection);
    expect(result.code.checked).toContain("new");
    expect(result.docs.checked).toEqual(["one"]);
  });
  it("preserves custom exclusions without inferring which local commits were published", () => {
    const before = menu(repo("code"));
    const selection = {
      code: { checked: ["unpushed", "two"], followAll: false },
    };
    const after = menu(
      repo("code", {
        commits: [
          commit("unrelated"),
          commit("local"),
          commit("one"),
          commit("two"),
        ],
        live: { selectable: true },
      }),
    );
    expect(reconcileSelection(before, after, selection).code.checked).toEqual([
      "unpushed",
      "two",
    ]);
    after.repos[0].live = { selectable: true };
    expect(reconcileSelection(before, after, selection).code.checked).toEqual([
      "unpushed",
      "two",
    ]);
  });
  it("retains unverified selections and offers the live option without enumeration", () => {
    const before = menu(repo("code"));
    const after = menu(repo("code", { commits: [], live: null }));
    expect(
      reconcileSelection(before, after, {
        code: { checked: ["one"], followAll: false },
      }).code.checked,
    ).toEqual(["one"]);
    expect(
      sourceChildren(
        repo("code", {
          live: { selectable: true },
        }),
      )[0],
    ).toBe("unpushed");
    expect(
      sourceApplyInput(before, { code: { checked: [], followAll: false } }),
    ).toEqual({ fingerprint: before.fingerprint, repos: [] });
  });
});

it("does not guess newly published selections from formerly dirty paths", () => {
  const before = menu(repo("code"));
  const after = menu(
    repo("code", {
      live: null,
      commits: [
        { ...commit("saved"), paths: ["file"] },
        { ...commit("unrelated"), paths: ["other"] },
        commit("local"),
        commit("one"),
        commit("two"),
      ],
    }),
  );
  expect(
    reconcileSelection(before, after, {
      code: { checked: ["unpushed", "two"], followAll: false },
    }).code.checked,
  ).toEqual(["two"]);
});

it("keeps the live option selectable without inspecting local content", () => {
  const indexOnly = repo("code", {
    live: { selectable: true },
  });
  expect(sourceChildren(indexOnly)).toContain("unpushed");
});
it("selects initially unavailable children on recovery until the user clears them", () => {
  const before = menu(
    repo("code", { error: "Unavailable", commits: [], live: null }),
  );
  const after = menu(repo("code"));
  const selection = selectAll(before);
  expect(selection.code.checked).toEqual([]);
  expect(reconcileSelection(before, after, selection).code.checked).toEqual([
    "unpushed",
    "one",
    "two",
  ]);
  selection.code = { checked: [], followAll: false };
  expect(reconcileSelection(before, after, selection).code.checked).toEqual([]);
});

it("All changes can restore a valid empty pushed range after opt-in history", () => {
  const captured = menu(
    repo("code", { base: "pushed", commits: [], live: null }),
  );
  expect(sourceApplyInput(captured, selectAll(captured)).repos).toMatchObject([
    { repoId: "code", allPushed: true, commits: [], includeUnpushed: false },
  ]);
});

it("keeps the new merge base opt-in when a fully selected range advances", () => {
  const before = menu(repo("code"));
  const after = menu(repo("code", { base: "one", commits: [commit("two")] }));
  expect(
    reconcileSelection(before, after, selectAll(before)).code.checked,
  ).toEqual(["unpushed", "two"]);
  expect(
    reconcileSelection(before, after, {
      code: { checked: ["one"], followAll: false },
    }).code.checked,
  ).toEqual(["one"]);
});
