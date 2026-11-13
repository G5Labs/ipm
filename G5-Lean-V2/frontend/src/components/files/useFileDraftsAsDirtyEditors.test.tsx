import React, { useEffect } from "react";
import { act, renderHook, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { renderApp } from "../../test/render";
import { createMockUser } from "../../test/fixtures";
import { RequireAuth, useRenderedPrincipal } from "../../hooks/useAuth";
import { CodeEditor } from "../editor/CodeEditor";
import {
  dirtyEditorPaths,
  dropDirtyEdits,
  resetDirtyEditorsForTests,
} from "../editor/dirtyEditors";
import {
  draftPerson,
  fileDraftKey,
  getFileDraft,
  newFileDraftId,
  setFileDraft,
  type FileDraft,
} from "./fileDrafts";
import { useFileDraftsAsDirtyEditors } from "./useFileDraftsAsDirtyEditors";

const draft = (content: string): FileDraft => ({
  id: newFileDraftId(),
  role: "owner",
  content,
  baseline: { content: "base\n", hash: "h1", vmId: "vm-1" },
});

afterEach(() => resetDirtyEditorsForTests());

describe("FILES drafts as the page's unsaved editors", () => {
  it("lists this workspace's drafts by path, and a Discard drops every same-path draft", () => {
    const key = (workspaceId: string, repoId: string) =>
      fileDraftKey({ person: "p", workspaceId, repoId, path: "docs/a.md" });
    const { unmount } = renderHook(() =>
      useFileDraftsAsDirtyEditors("p", "ws-1"),
    );
    act(() => {
      setFileDraft(key("ws-1", "repo-1"), draft("one\n"));
      setFileDraft(key("ws-1", "repo-2"), draft("two\n"));
      setFileDraft(key("ws-2", "repo-1"), draft("elsewhere\n"));
    });
    expect(dirtyEditorPaths()).toEqual(["docs/a.md"]);

    // A review copy's Discard rewrites the file: its drafts must go too.
    act(() => {
      dropDirtyEdits(["docs/a.md"]);
    });
    expect(getFileDraft(key("ws-1", "repo-1"))).toBeNull();
    expect(getFileDraft(key("ws-1", "repo-2"))).toBeNull();
    expect(getFileDraft(key("ws-2", "repo-1"))).not.toBeNull();

    act(() => setFileDraft(key("ws-1", "repo-1"), draft("again\n")));
    expect(dirtyEditorPaths()).toEqual(["docs/a.md"]);
    unmount();
    expect(dirtyEditorPaths()).toEqual([]);
  });

  it("keeps a retained draft listed while a clean editor shows the same path", async () => {
    const key = fileDraftKey({
      person: "p",
      workspaceId: "ws-1",
      repoId: "repo-1",
      path: "docs/a.md",
    });
    const controls: { show: (next: boolean) => void } = {
      show: () => undefined,
    };
    const Page = () => {
      const [showOther, setShowOther] = React.useState(false);
      useEffect(() => {
        controls.show = setShowOther;
      }, []);
      useFileDraftsAsDirtyEditors("p", "ws-1");
      // The same path in another repository (or in PROJECT DOCS), clean.
      return showOther ? (
        <CodeEditor
          filePath="docs/a.md"
          content="other\n"
          language="markdown"
          editable={false}
          showComments={false}
          comments={[]}
        />
      ) : null;
    };
    act(() => setFileDraft(key, draft("unsaved\n")));
    renderApp(<Page />);
    expect(dirtyEditorPaths()).toEqual(["docs/a.md"]);
    act(() => controls.show(true));
    expect(await screen.findByTestId("code-editor")).toBeInTheDocument();
    expect(dirtyEditorPaths()).toEqual(["docs/a.md"]);
    act(() => controls.show(false));
    expect(dirtyEditorPaths()).toEqual(["docs/a.md"]);

    act(() => {
      dropDirtyEdits(["docs/a.md"]);
    });
    expect(getFileDraft(key)).toBeNull();
  });
});

describe("FILES drafts and the signed-in person", () => {
  it("drops another person's drafts when a page is shown", async () => {
    const foreign = fileDraftKey({
      person: "someone-else",
      workspaceId: "ws-1",
      repoId: "repo-1",
      path: "docs/a.md",
    });
    setFileDraft(foreign, draft("theirs\n"));
    const seen: { principal: string | null } = { principal: null };
    const Probe = () => {
      const principal = useRenderedPrincipal();
      useEffect(() => {
        seen.principal = principal;
      }, [principal]);
      return null;
    };
    renderApp(
      <RequireAuth>
        <Probe />
      </RequireAuth>,
      { user: createMockUser() },
    );
    await waitFor(() => expect(seen.principal).not.toBeNull());
    expect(getFileDraft(foreign)).toBeNull();

    const own = fileDraftKey({
      person: draftPerson(seen.principal)!,
      workspaceId: "ws-1",
      repoId: "repo-1",
      path: "docs/a.md",
    });
    act(() => setFileDraft(own, draft("mine\n")));
    expect(getFileDraft(own)?.content).toBe("mine\n");
  });
});
