import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import {
  beginFileSave,
  draftPerson,
  fileDraftKey,
  fileEditing,
  fileViewMode,
  saveFileEditing,
  saveFileViewMode,
  getFileDraft,
  hasFileDrafts,
  newFileDraftId,
  resetFileDraftsForTests,
  retainFileDraftsFor,
  setFileDraft,
  updateFileDraft,
  useFileDraft,
  workspaceFileDrafts,
  type FileDraft,
} from "./fileDrafts";

function draft(content: string, id = newFileDraftId()): FileDraft {
  return {
    id,
    role: "owner",
    content,
    baseline: { content: "base\n", hash: "h1", vmId: "vm-1" },
  };
}

const ALEX = "alex";
const BEA = "bea";
const keyA = fileDraftKey({
  person: ALEX,
  workspaceId: "ws-1",
  repoId: "repo-1",
  path: "docs/a.md",
});

afterEach(() => resetFileDraftsForTests());

describe("FILES drafts", () => {
  it("retains the selected view without a draft or model, isolated to the person", () => {
    saveFileViewMode(keyA, "source");
    expect(fileViewMode(keyA)).toBe("source");
    expect(hasFileDrafts()).toBe(false);
    retainFileDraftsFor(ALEX);
    expect(fileViewMode(keyA)).toBe("source");
    setFileDraft(keyA, draft("edited"));
    saveFileViewMode(keyA, "preview");
    setFileDraft(keyA, null);
    expect(fileViewMode(keyA)).toBe("preview");
    retainFileDraftsFor(BEA);
    expect(fileViewMode(keyA)).toBeUndefined();
  });

  it("retains whether the person was editing, isolated to the person", () => {
    expect(fileEditing(keyA)).toBeUndefined();
    saveFileEditing(keyA, true);
    expect(fileEditing(keyA)).toBe(true);
    saveFileViewMode(keyA, "source");
    saveFileEditing(keyA, false);
    expect(fileEditing(keyA)).toBe(false);
    expect(fileViewMode(keyA)).toBe("source");
    retainFileDraftsFor(ALEX);
    expect(fileEditing(keyA)).toBe(false);
    retainFileDraftsFor(BEA);
    expect(fileEditing(keyA)).toBeUndefined();
  });

  it("keeps a draft for its person, workspace, repository and path", () => {
    const other = fileDraftKey({
      person: ALEX,
      workspaceId: "ws-1",
      repoId: "repo-2",
      path: "docs/a.md",
    });
    setFileDraft(keyA, draft("edited\n"));
    expect(getFileDraft(keyA)?.content).toBe("edited\n");
    expect(getFileDraft(other)).toBeNull();
    expect(workspaceFileDrafts(ALEX, "ws-1").map((e) => e.path)).toEqual([
      "docs/a.md",
    ]);
    expect(workspaceFileDrafts(ALEX, "ws-2")).toEqual([]);
    expect(workspaceFileDrafts(BEA, "ws-1")).toEqual([]);
  });

  it("never lets a late completion change or recreate a dropped or newer draft", () => {
    const first = draft("one\n");
    setFileDraft(keyA, first);
    // Discarded while a Save or Push was in flight: nothing comes back.
    setFileDraft(keyA, null);
    expect(updateFileDraft(keyA, first.id, () => null)).toBe(false);
    expect(
      updateFileDraft(keyA, first.id, (current) => ({
        ...current,
        content: "late\n",
      })),
    ).toBe(false);
    expect(getFileDraft(keyA)).toBeNull();

    // A newer draft started meanwhile is not the one the completion knew.
    const newer = draft("two\n");
    setFileDraft(keyA, newer);
    expect(updateFileDraft(keyA, first.id, () => null)).toBe(false);
    expect(getFileDraft(keyA)).toBe(newer);
    expect(updateFileDraft(keyA, newer.id, () => null)).toBe(true);
    expect(getFileDraft(keyA)).toBeNull();
  });

  it("names the person of a principal, so an org switch keeps their drafts", () => {
    expect(draftPerson("alex:org-1")).toBe(ALEX);
    expect(draftPerson("alex:org-2")).toBe(ALEX);
    expect(draftPerson(null)).toBeNull();
    setFileDraft(keyA, draft("alex\n"));
    retainFileDraftsFor(draftPerson("alex:org-2"));
    expect(getFileDraft(keyA)?.content).toBe("alex\n");
  });

  it("drops every draft of another person, and all on sign-out", () => {
    const keyB = fileDraftKey({
      person: BEA,
      workspaceId: "ws-1",
      repoId: "repo-1",
      path: "docs/a.md",
    });
    setFileDraft(keyA, draft("alex\n"));
    setFileDraft(keyB, draft("bea\n"));
    retainFileDraftsFor(ALEX);
    expect(getFileDraft(keyA)).not.toBeNull();
    expect(getFileDraft(keyB)).toBeNull();
    retainFileDraftsFor(null);
    expect(getFileDraft(keyA)).toBeNull();
    expect(hasFileDrafts()).toBe(false);
  });

  it("asks before the tab closes only while a draft exists", () => {
    const ask = () => {
      const event = new Event("beforeunload", { cancelable: true });
      window.dispatchEvent(event);
      return event.defaultPrevented;
    };
    expect(ask()).toBe(false);
    setFileDraft(keyA, draft("edited\n"));
    expect(ask()).toBe(true);
    // Navigating within G5 keeps the draft, so a later reload still asks.
    expect(ask()).toBe(true);
    setFileDraft(keyA, null);
    expect(ask()).toBe(false);
  });

  it("gives views the current draft as a live value", () => {
    const { result } = renderHook(() => useFileDraft(keyA));
    expect(result.current).toBeNull();
    const next = draft("live\n");
    act(() => setFileDraft(keyA, next));
    expect(result.current).toBe(next);
    act(() => setFileDraft(keyA, null));
    expect(result.current).toBeNull();
  });
});

describe("shared Save coordination", () => {
  it("serializes writers and acknowledges only submitted text while later typing survives", () => {
    const original = draft("submitted");
    setFileDraft(keyA, original);
    const settle = beginFileSave(keyA, original, "h1")!;
    expect(beginFileSave(keyA, getFileDraft(keyA)!, "h1")).toBeNull();
    updateFileDraft(keyA, original.id, (current) => ({
      ...current,
      content: "later",
    }));
    settle({ content: "submitted", hash: "saved", vmId: "vm-1" });
    expect(getFileDraft(keyA)).toMatchObject({
      content: "later",
      baseline: { content: "submitted", hash: "saved" },
    });
    settle({ content: "stale second settlement", hash: "wrong", vmId: "vm-1" });
    expect(getFileDraft(keyA)?.baseline.hash).toBe("saved");
  });
  it("retains failures and conflict recovery, and never settles into a replacement draft", () => {
    const original = draft("mine");
    const settle = beginFileSave(keyA, original, "h1")!;
    settle(null, "File changed", {
      currentHash: "external",
      currentContent: "theirs",
    });
    expect(getFileDraft(keyA)).toMatchObject({
      content: "mine",
      saveError: "File changed",
      saveConflict: { currentHash: "external" },
    });
    const retry = beginFileSave(keyA, getFileDraft(keyA)!, "external")!;
    const replacement = draft("replacement");
    setFileDraft(keyA, replacement);
    retry({ content: "mine", hash: "late", vmId: "vm-1" });
    expect(getFileDraft(keyA)).toBe(replacement);
  });
});
