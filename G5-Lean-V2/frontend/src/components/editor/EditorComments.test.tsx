// Anchored comments in the editors (`g5.documents.comments`): a new thread
// is anchored to exactly the text on screen, controls follow what the viewer
// may do, a failure keeps the draft, and review cards stay visible.

import React from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createCommentAnchor, sha256Utf8 } from "../../lib/commentAnchors";
import type { CommentDTO, ReviewSuggestion } from "../../lib/types";
import { server } from "../../test/msw/server";
import type { SuggestionOverlayProps } from "../workspace/ReviewSuggestionOverlay";
import { CodeEditor } from "./CodeEditor";
import { loadMonacoReact } from "./loadMonaco";

// Hold the lazy module unresolved for fallback cases; native cases explicitly
// complete loading. Read-only authority no longer selects a different engine.
const monacoLoading = vi.hoisted(() => ({
  ready: false,
  waiting: [] as Array<() => void>,
}));
vi.mock("./loadMonaco", () => ({
  loadMonacoReact: async () => {
    if (!monacoLoading.ready)
      await new Promise<void>((resolve) => monacoLoading.waiting.push(resolve));
    return import("@monaco-editor/react");
  },
}));
function finishMonacoLoading() {
  monacoLoading.ready = true;
  for (const resolve of monacoLoading.waiting.splice(0)) resolve();
}
beforeEach(() => {
  monacoLoading.ready = false;
  monacoLoading.waiting.length = 0;
});

// Monaco's rows as a test sets them: where each line's row starts (padding
// and view zones above it included), the height of the whole text, and the
// listeners told when that height changes.
const monacoRows = vi.hoisted(() => ({
  top: (line: number) => (line - 1) * 20,
  contentHeight: 0,
  sizeListeners: [] as Array<() => void>,
  /** Scroll offsets the editor was set to. */
  scrolledTo: [] as number[],
  /** Lines the editor was asked to reveal in the center of its viewport. */
  revealedInCenter: [] as number[],
}));

afterEach(() => {
  monacoRows.top = (line: number) => (line - 1) * 20;
  monacoRows.contentHeight = 0;
  monacoRows.sizeListeners.length = 0;
  monacoRows.scrolledTo.length = 0;
  monacoRows.revealedInCenter.length = 0;
});

// A Monaco stand-in with a caret: selecting in the textarea reports the
// caret line the way Monaco's onDidChangeCursorPosition does, scrolling it
// reports the offset the way onDidScrollChange does, and its rows are
// `monacoRows`.
vi.mock("@monaco-editor/react", async () => {
  const ReactModule = await import("react");
  type CursorListener = (event: {
    position: { lineNumber: number; column: number };
  }) => void;
  const MockMonaco = ({
    value,
    onChange,
    onMount,
  }: {
    value: string;
    onChange: (value: string) => void;
    onMount?: (editor: unknown, monaco: unknown) => void;
  }) => {
    const editorRef = ReactModule.useRef<HTMLTextAreaElement>(null);
    const listeners = ReactModule.useRef<CursorListener[]>([]);
    const scrollListeners = ReactModule.useRef<
      Array<(event: { scrollTop: number; scrollTopChanged: boolean }) => void>
    >([]);
    const zones = ReactModule.useRef(new Map<string, HTMLElement>());
    ReactModule.useEffect(() => {
      onMount?.(
        {
          addCommand: () => undefined,
          getModel: () => null,
          onDidChangeCursorPosition: (listener: CursorListener) => {
            listeners.current.push(listener);
            return { dispose: () => undefined };
          },
          onDidScrollChange: (
            listener: (event: {
              scrollTop: number;
              scrollTopChanged: boolean;
            }) => void,
          ) => {
            scrollListeners.current.push(listener);
            return { dispose: () => undefined };
          },
          getScrollTop: () => editorRef.current?.scrollTop ?? 0,
          getTopForLineNumber: (line: number) => monacoRows.top(line),
          getContentHeight: () => monacoRows.contentHeight,
          setScrollTop: (top: number) => {
            monacoRows.scrolledTo.push(top);
          },
          revealLineInCenter: (line: number) => {
            monacoRows.revealedInCenter.push(line);
          },
          onDidContentSizeChange: (listener: () => void) => {
            monacoRows.sizeListeners.push(listener);
            return { dispose: () => undefined };
          },
          onDidLayoutChange: () => ({ dispose: () => undefined }),
          // View zones: each zone's node is placed in the page.
          changeViewZones: (
            callback: (accessor: {
              addZone: (zone: { domNode: HTMLElement }) => string;
              removeZone: (id: string) => void;
              layoutZone: (id: string) => void;
            }) => void,
          ) => {
            callback({
              addZone: (zone) => {
                document.body.appendChild(zone.domNode);
                zones.current.set(`zone-${zones.current.size}`, zone.domNode);
                return `zone-${zones.current.size - 1}`;
              },
              removeZone: (id) => {
                zones.current.get(id)?.remove();
                zones.current.delete(id);
              },
              layoutZone: () => undefined,
            });
          },
        },
        {
          KeyMod: { CtrlCmd: 0 },
          KeyCode: { KeyS: 0 },
          editor: { defineTheme: () => undefined, setTheme: () => undefined },
        },
      );
    }, []);
    return (
      <textarea
        data-testid="mock-monaco-editor"
        ref={editorRef}
        value={value}
        onChange={(event: React.ChangeEvent<HTMLTextAreaElement>) =>
          onChange(event.currentTarget.value)
        }
        onScroll={(event: React.UIEvent<HTMLTextAreaElement>) => {
          for (const listener of scrollListeners.current) {
            listener({
              scrollTop: event.currentTarget.scrollTop,
              scrollTopChanged: true,
            });
          }
        }}
        onSelect={(event: React.SyntheticEvent<HTMLTextAreaElement>) => {
          const element = event.currentTarget;
          const lineNumber = element.value
            .slice(0, element.selectionStart ?? 0)
            .split("\n").length;
          for (const listener of listeners.current) {
            listener({ position: { lineNumber, column: 1 } });
          }
        }}
      />
    );
  };
  return {
    default: MockMonaco,
    loader: {
      init: () =>
        Promise.resolve({
          editor: { defineTheme: () => undefined, setTheme: () => undefined },
        }),
    },
  };
});

function withQueryClient(ui: React.ReactElement) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return <QueryClientProvider client={client}>{ui}</QueryClientProvider>;
}

async function storedComment(
  content: string,
  lineNumber: number,
  overrides: Partial<CommentDTO> = {},
): Promise<CommentDTO> {
  const anchor = await createCommentAnchor(content, lineNumber);
  return {
    id: "comment-1",
    workspaceId: "ws-1",
    authorId: "user-owner",
    authorName: "Olive Owner",
    authorAvatarUrl: null,
    scope: "line",
    repoId: "repo-1",
    filePath: "src/main.ts",
    lineNumber,
    lineHash: anchor.lineHash,
    contentHash: anchor.contentHash,
    anchorContext: anchor.anchorContext,
    commitSha: null,
    content: "Why this line?",
    resolved: false,
    resolvedAt: null,
    originBranch: "main",
    replyCount: 0,
    createdAt: "2026-09-30T10:00:00Z",
    updatedAt: "2026-09-30T10:00:00Z",
    ...overrides,
  };
}

function suggestion(overrides: Partial<ReviewSuggestion>): ReviewSuggestion {
  return {
    id: "sug-1",
    publicationId: "pub-1",
    findingId: null,
    repoId: "repo-1",
    path: "src/main.ts",
    kind: "comment",
    beforeLines: "second();\n",
    afterLines: null,
    anchorStart: 2,
    contentHash: null,
    anchorContext: null,
    authorKind: "human",
    authorLabel: "rita",
    note: "Consider renaming.",
    commentId: "thread-1",
    status: "open",
    statusBy: null,
    statusAt: null,
    supersedes: null,
    adoptedFrom: null,
    createdAt: "2026-09-30T10:00:00Z",
    ...overrides,
  };
}

function overlay(
  suggestions: ReviewSuggestion[],
  extra: Partial<SuggestionOverlayProps> = {},
): SuggestionOverlayProps {
  return {
    workspaceId: "ws-1",
    suggestions,
    viewerOwnsWorkspace: false,
    viewerUserId: "user-reviewer",
    ownerVmRunning: false,
    sourceWritable: true,
    ...extra,
  };
}

describe("CodeEditor comments", () => {
  it("anchors a new thread to the edited text at Monaco's caret line", async () => {
    finishMonacoLoading();
    const onAddComment = vi.fn().mockResolvedValue(undefined);
    render(
      <CodeEditor
        filePath="src/main.ts"
        content={"alpha();\nbeta();\ngamma();\ntarget();\ndelta();\n"}
        contentHash="file-v1"
        repoOid="oid-base"
        language="typescript"
        editable
        onSave={vi.fn()}
        showComments
        onToggleComments={vi.fn()}
        comments={[]}
        onAddComment={onAddComment}
      />,
    );
    const editor = (await screen.findByTestId(
      "mock-monaco-editor",
    )) as HTMLTextAreaElement;
    // Unsaved: the first two lines are deleted above the target.
    const edited = "gamma();\ntarget();\ndelta();\n";
    fireEvent.change(editor, { target: { value: edited } });
    const caret = edited.indexOf("target");
    editor.setSelectionRange(caret, caret);
    fireEvent.select(editor);

    fireEvent.contextMenu(editor);
    fireEvent.click(
      screen.getByTestId("code-editor-context-add-comment-button"),
    );
    const user = userEvent.setup();
    await user.type(screen.getByLabelText("Comment"), "Why target?");
    await user.click(screen.getByTestId("draft-comment-save"));

    await waitFor(() => expect(onAddComment).toHaveBeenCalledTimes(1));
    expect(onAddComment).toHaveBeenCalledWith({
      anchor: {
        lineNumber: 2,
        lineHash: await sha256Utf8("target();"),
        contentHash: await sha256Utf8(edited),
        anchorContext: (await createCommentAnchor(edited, 2)).anchorContext,
      },
      content: "Why target?",
      baseOid: "oid-base",
      id: expect.stringMatching(/^[0-9a-f-]{36}$/),
    });
    await waitFor(() =>
      expect(screen.queryByLabelText("Comment")).not.toBeInTheDocument(),
    );
  });

  it("keeps the draft and says why when starting a thread fails, sending once while in flight", async () => {
    let rejectAdd!: (error: Error) => void;
    const onAddComment = vi.fn(
      () =>
        new Promise((_resolve, reject) => {
          rejectAdd = reject;
        }),
    );
    render(
      <CodeEditor
        filePath="src/main.ts"
        content={"first();\nsecond();\n"}
        language="typescript"
        editable={false}
        showComments
        onToggleComments={vi.fn()}
        comments={[]}
        onAddComment={onAddComment}
      />,
    );
    fireEvent.mouseEnter(screen.getByTestId("code-editor-gutter-line-2"));
    fireEvent.click(screen.getByTestId("code-editor-add-comment-button"));
    const user = userEvent.setup();
    const draft = screen.getByLabelText("Comment");
    await user.type(draft, "Keep me");
    // Ctrl+Enter twice, then the button: one submission while in flight.
    fireEvent.keyDown(draft, { key: "Enter", ctrlKey: true });
    fireEvent.keyDown(draft, { key: "Enter", ctrlKey: true });
    await user.click(screen.getByTestId("draft-comment-save"));
    await waitFor(() => expect(onAddComment).toHaveBeenCalledTimes(1));
    // While it is being sent the draft waits for the answer: its words are
    // locked, and neither Cancel nor Escape closes it.
    expect(draft).toHaveAttribute("readonly");
    expect(screen.getByTestId("draft-comment-cancel")).toBeDisabled();
    fireEvent.keyDown(draft, { key: "Escape" });
    expect(screen.getByLabelText("Comment")).toBe(draft);

    await act(async () => {
      rejectAdd(new Error("Only the workspace owner can start a thread"));
    });
    expect(await screen.findByTestId("draft-comment-error")).toHaveTextContent(
      "Only the workspace owner can start a thread",
    );
    expect(screen.getByLabelText("Comment")).toHaveValue("Keep me");
  });

  it("keeps a draft being sent on its line, so a refused one is retried there", async () => {
    const sends: Array<{
      resolve: (value: unknown) => void;
      reject: (error: Error) => void;
    }> = [];
    const onAddComment = vi.fn(
      (_draft: unknown) =>
        new Promise((resolve, reject) => sends.push({ resolve, reject })),
    );
    render(
      <CodeEditor
        filePath="src/main.ts"
        content={"first();\nsecond();\nthird();\nfourth();\n"}
        language="typescript"
        editable={false}
        showComments
        onToggleComments={vi.fn()}
        comments={[]}
        onAddComment={onAddComment}
      />,
    );
    const openOn = (line: number) => {
      const gutter = screen.getByTestId(`code-editor-gutter-line-${line}`);
      fireEvent.mouseEnter(gutter);
      fireEvent.click(
        within(gutter).getByTestId("code-editor-add-comment-button"),
      );
    };
    const user = userEvent.setup();
    openOn(2);
    await user.type(screen.getByLabelText("Comment"), "On second");
    await user.click(screen.getByTestId("draft-comment-save"));
    await waitFor(() => expect(onAddComment).toHaveBeenCalledTimes(1));

    // Another line is chosen while it is sent: nothing moves.
    openOn(4);
    await act(async () => {
      sends[0].reject(new Error("The service is restarting"));
    });
    expect(await screen.findByTestId("draft-comment-error")).toHaveTextContent(
      "The service is restarting",
    );
    expect(screen.getByLabelText("Comment")).toHaveValue("On second");

    // The retry goes where the words were written for.
    await user.click(screen.getByTestId("draft-comment-save"));
    await waitFor(() => expect(onAddComment).toHaveBeenCalledTimes(2));
    expect(onAddComment.mock.calls[1][0]).toEqual(
      onAddComment.mock.calls[0][0],
    );
    expect(onAddComment.mock.calls[1][0]).toMatchObject({
      content: "On second",
      anchor: { lineNumber: 2, lineHash: await sha256Utf8("second();") },
    });
    await act(async () => {
      sends[1].resolve(undefined);
    });
    await waitFor(() =>
      expect(screen.queryByLabelText("Comment")).not.toBeInTheDocument(),
    );

    // Answered: another line can be chosen, and gets its own words.
    openOn(4);
    const draft = screen.getByLabelText("Comment");
    expect(draft).toHaveValue("");
    await user.type(draft, "On fourth");
    await user.click(screen.getByTestId("draft-comment-save"));
    await waitFor(() => expect(onAddComment).toHaveBeenCalledTimes(3));
    expect(onAddComment.mock.calls[2][0]).not.toMatchObject({
      id: (onAddComment.mock.calls[0][0] as { id: string }).id,
    });
    expect(onAddComment.mock.calls[2][0]).toMatchObject({
      content: "On fourth",
      anchor: { lineNumber: 4, lineHash: await sha256Utf8("fourth();") },
    });
  });

  it("keeps a reply being sent, its refusal and an open draft while the comments are hidden", async () => {
    const content = "first();\nsecond();\nthird();\n";
    const comment = await storedComment(content, 2);
    let refuse!: (error: Error) => void;
    const onReplyToComment = vi.fn(
      (_threadId: string, _content: string) =>
        new Promise((_resolve, reject) => {
          refuse = reject;
        }),
    );
    const client = new QueryClient();
    const editor = (showComments: boolean) => (
      <QueryClientProvider client={client}>
        <CodeEditor
          filePath="src/main.ts"
          content={content}
          language="typescript"
          editable={false}
          showComments={showComments}
          onToggleComments={vi.fn()}
          comments={[comment]}
          onReplyToComment={onReplyToComment}
          onAddComment={vi.fn()}
        />
      </QueryClientProvider>
    );
    const { rerender } = render(editor(true));
    const user = userEvent.setup();
    const thread = await screen.findByTestId("comment-thread-comment-1");
    await user.click(
      within(thread).getByTestId("comment-thread-toggle-expand"),
    );
    const reply = within(thread).getByPlaceholderText("Reply…");
    await user.type(reply, "Sent, then hidden");
    fireEvent.keyDown(reply, { key: "Enter" });
    await waitFor(() => expect(onReplyToComment).toHaveBeenCalledTimes(1));
    const gutter = screen.getByTestId("code-editor-gutter-line-3");
    fireEvent.mouseEnter(gutter);
    fireEvent.click(
      within(gutter).getByTestId("code-editor-add-comment-button"),
    );
    const draft = screen.getByLabelText("Comment");
    await user.type(draft, "Draft kept");

    // Hidden while the reply is sent, refused meanwhile, shown again.
    rerender(editor(false));
    expect(screen.getByTestId("code-editor-comment-margin")).not.toBeVisible();
    await act(async () => {
      refuse(new Error("The service is restarting"));
    });
    rerender(editor(true));
    expect(screen.getByTestId("comment-thread-comment-1")).toBe(thread);
    expect(
      within(thread).getByTestId("comment-thread-error"),
    ).toHaveTextContent("The service is restarting");
    expect(reply).toHaveValue("Sent, then hidden");
    expect(screen.getByLabelText("Comment")).toBe(draft);
    expect(draft).toHaveValue("Draft kept");
    expect(onReplyToComment).toHaveBeenCalledTimes(1);
  });

  it("offers no Add comment control to a viewer who may not start a thread", () => {
    render(
      <CodeEditor
        filePath="src/main.ts"
        content={"first();\nsecond();\n"}
        language="typescript"
        editable={false}
        showComments
        onToggleComments={vi.fn()}
        comments={[]}
        onReplyToComment={vi.fn()}
      />,
    );
    fireEvent.mouseEnter(screen.getByTestId("code-editor-gutter-line-2"));
    expect(
      screen.queryByTestId("code-editor-add-comment-button"),
    ).not.toBeInTheDocument();
    fireEvent.contextMenu(screen.getByTestId("code-line-2"));
    expect(
      screen.queryByTestId("code-editor-context-add-comment-button"),
    ).not.toBeInTheDocument();
  });

  it("shows a line's comment marker and add control beside its suggestion marker", async () => {
    const content = "first();\nsecond();\nthird();\n";
    const comment = await storedComment(content, 2);
    render(
      withQueryClient(
        <CodeEditor
          filePath="src/main.ts"
          content={content}
          language="typescript"
          editable={false}
          showComments={false}
          onToggleComments={vi.fn()}
          comments={[comment]}
          onAddComment={vi.fn()}
          suggestionOverlay={overlay([
            suggestion({ id: "sug-2", anchorStart: 2 }),
            suggestion({
              id: "sug-3",
              anchorStart: 3,
              beforeLines: "third();\n",
            }),
          ])}
        />,
      ),
    );
    const second = screen.getByTestId("code-editor-gutter-line-2");
    await waitFor(() =>
      expect(
        within(second).getByTestId("code-editor-view-comments-button"),
      ).toBeInTheDocument(),
    );
    expect(
      within(second).getByTestId("suggestion-gutter-marker"),
    ).toBeInTheDocument();
    const third = screen.getByTestId("code-editor-gutter-line-3");
    fireEvent.mouseEnter(third);
    expect(
      within(third).getByTestId("suggestion-gutter-marker"),
    ).toBeInTheDocument();
    expect(
      within(third).getByTestId("code-editor-add-comment-button"),
    ).toBeInTheDocument();
  });

  it("lets the reviewer withdraw their suggestion from its card in the file", () => {
    render(
      withQueryClient(
        <CodeEditor
          filePath="src/main.ts"
          content={"first();\nsecond();\n"}
          language="typescript"
          editable={false}
          showComments={false}
          onToggleComments={vi.fn()}
          comments={[]}
          suggestionOverlay={overlay([
            suggestion({ id: "sug-mine", reviewerUserId: "user-reviewer" }),
            suggestion({
              id: "sug-theirs",
              anchorStart: 1,
              beforeLines: "first();\n",
              reviewerUserId: "user-other",
            }),
          ])}
        />,
      ),
    );
    expect(
      screen.getByTestId("review-suggestion-withdraw-sug-mine"),
    ).toBeInTheDocument();
    expect(
      screen.queryByTestId("review-suggestion-withdraw-sug-theirs"),
    ).not.toBeInTheDocument();
  });

  it("lists threads from the top in the text's order, the unplaced last, whatever the text shows", async () => {
    finishMonacoLoading();
    const content =
      Array.from({ length: 150 }, (_, i) => `row ${i + 1}`).join("\n") + "\n";
    // The API lists newest first: the thread on row 20 is the newest.
    const early = await storedComment(content, 20, {
      id: "early-thread",
      createdAt: "2026-09-30T12:00:00Z",
    });
    const lost = await storedComment(content, 30, {
      id: "lost-thread",
      lineHash: await sha256Utf8("gone"),
      anchorContext: null,
      createdAt: "2026-09-30T11:00:00Z",
    });
    const far = await storedComment(content, 150, {
      id: "far-thread",
      createdAt: "2026-09-30T10:00:00Z",
    });
    render(
      withQueryClient(
        <CodeEditor
          filePath="src/main.ts"
          content={content}
          language="plaintext"
          editable
          showComments
          onToggleComments={vi.fn()}
          comments={[early, lost, far]}
          onAddComment={vi.fn()}
        />,
      ),
    );
    const editor = (await screen.findByTestId(
      "mock-monaco-editor",
    )) as HTMLTextAreaElement;
    const margin = screen.getByTestId("code-editor-comment-margin");
    const listed = () =>
      Array.from(
        margin.querySelectorAll<HTMLElement>("[data-comment-thread-id]"),
      ).map((el) => el.dataset.commentThreadId);
    await waitFor(() =>
      expect(listed()).toEqual(["early-thread", "far-thread", "lost-thread"]),
    );
    expect(
      within(margin)
        .getAllByTestId("comment-thread-anchor")
        .map((el) => el.textContent),
    ).toEqual(["L20", "L150", "Orphaned anchor · was L30"]);

    // The list is its own scroll container: the text's scroll leaves it.
    editor.scrollTop = 2000;
    fireEvent.scroll(editor);
    expect(margin.scrollTop).toBe(0);
    expect(
      screen.queryByTestId("code-editor-comment-margin-extent"),
    ).toBeNull();

    // A draft opens first in the list, above the threads.
    const caret = content.indexOf("row 100\n");
    editor.setSelectionRange(caret, caret);
    fireEvent.select(editor);
    fireEvent.contextMenu(editor);
    fireEvent.click(
      screen.getByTestId("code-editor-context-add-comment-button"),
    );
    const draft = screen.getByLabelText("Comment");
    expect(
      draft.compareDocumentPosition(
        screen.getByTestId("comment-thread-early-thread"),
      ) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it("scrolls Monaco so an expanded card's row is its first line, and not on collapse", async () => {
    finishMonacoLoading();
    const content =
      Array.from({ length: 150 }, (_, i) => `row ${i + 1}`).join("\n") + "\n";
    const far = await storedComment(content, 150, { id: "far-thread" });
    const lost = await storedComment(content, 30, {
      id: "lost-thread",
      lineHash: await sha256Utf8("gone"),
      anchorContext: null,
    });
    render(
      withQueryClient(
        <CodeEditor
          filePath="src/main.ts"
          content={content}
          language="plaintext"
          editable
          showComments
          comments={[far, lost]}
        />,
      ),
    );
    await screen.findByTestId("mock-monaco-editor");
    const card = await screen.findByTestId("comment-thread-far-thread");
    await waitFor(() => expect(card).toHaveTextContent("L150"));
    fireEvent.click(within(card).getByTestId("comment-thread-toggle-expand"));
    // The row's own offset, as Monaco reports it: a reveal at the top would
    // keep sticky-scroll room above the row.
    expect(monacoRows.scrolledTo).toEqual([monacoRows.top(150)]);
    fireEvent.click(within(card).getByTestId("comment-thread-toggle-expand"));
    expect(monacoRows.scrolledTo).toHaveLength(1);
    // An orphaned card, once placed as such, has no row to reveal.
    const orphan = screen.getByTestId("comment-thread-lost-thread");
    await waitFor(() => expect(orphan).toHaveTextContent("Orphaned anchor"));
    fireEvent.click(within(orphan).getByTestId("comment-thread-toggle-expand"));
    expect(monacoRows.scrolledTo).toHaveLength(1);
  });

  it("jumps to the same source line again when a later jump names it", async () => {
    // Keep navigation and its delayed highlight reset inside the test clock;
    // real highlight timers can otherwise outlive the jsdom environment.
    vi.useFakeTimers();
    finishMonacoLoading();
    const content =
      Array.from({ length: 150 }, (_, i) => `row ${i + 1}`).join("\n") + "\n";
    const ui = (seq: number) =>
      withQueryClient(
        <CodeEditor
          filePath="src/main.ts"
          content={content}
          language="plaintext"
          editable
          showComments={false}
          comments={[]}
          scrollToLine={20}
          scrollToLineSeq={seq}
        />,
      );
    const { rerender } = render(ui(1));
    await act(async () => {
      await loadMonacoReact();
    });
    expect(screen.getByTestId("mock-monaco-editor")).toBeInTheDocument();
    await act(() => vi.advanceTimersByTimeAsync(200));
    expect(monacoRows.revealedInCenter).toEqual([20]);
    // The same jump, rendered again, is not repeated; a new one is.
    rerender(ui(1));
    await act(() => vi.advanceTimersByTimeAsync(200));
    expect(monacoRows.revealedInCenter).toEqual([20]);
    rerender(ui(2));
    await act(() => vi.advanceTimersByTimeAsync(200));
    expect(monacoRows.revealedInCenter).toEqual([20, 20]);
    await act(() => vi.advanceTimersByTimeAsync(3000));
  });

  it("scrolls the read-only viewer to an expanded card's row, below the suggestion cards above it", async () => {
    const content =
      Array.from({ length: 150 }, (_, i) => `row ${i + 1}`).join("\n") + "\n";
    const far = await storedComment(content, 150, { id: "far-thread" });
    const scrolled: ScrollToOptions[] = [];
    const scrollTo = Element.prototype.scrollTo;
    Element.prototype.scrollTo = function (
      this: Element,
      options?: ScrollToOptions | number,
    ) {
      if (typeof options === "object") scrolled.push(options);
    } as typeof Element.prototype.scrollTo;
    try {
      render(
        withQueryClient(
          <CodeEditor
            filePath="src/main.ts"
            content={content}
            language="plaintext"
            editable={false}
            showComments
            onToggleComments={vi.fn()}
            comments={[far]}
          />,
        ),
      );
      const code = await screen.findByTestId("code-editor-scroll");
      const card = await screen.findByTestId("comment-thread-far-thread");
      await waitFor(() => expect(card).toHaveTextContent("L150"));
      code.scrollTop = 500;
      fireEvent.scroll(code);
      fireEvent.click(within(card).getByTestId("comment-thread-toggle-expand"));
      expect(scrolled).toEqual([{ top: 149 * 20, behavior: "smooth" }]);
    } finally {
      Element.prototype.scrollTo = scrollTo;
    }
  });

  it("keeps a card's typed reply when the text changes and the card moves to another line", async () => {
    server.use(
      http.get(
        "*/api/v1/workspaces/ws-1/review/suggestions/sug-1/replies",
        () => HttpResponse.json({ data: { replies: [] } }),
      ),
    );
    const client = new QueryClient();
    const editor = (content: string) => (
      <QueryClientProvider client={client}>
        <CodeEditor
          filePath="src/main.ts"
          content={content}
          language="typescript"
          editable={false}
          showComments={false}
          onToggleComments={vi.fn()}
          comments={[]}
          suggestionOverlay={overlay([suggestion({ id: "sug-1" })], {
            sourceWritable: true,
          })}
        />
      </QueryClientProvider>
    );
    const { rerender } = render(editor("first();\nsecond();\nthird();\n"));
    const user = userEvent.setup();
    await user.click(
      await screen.findByTestId("review-suggestion-replies-toggle-sug-1"),
    );
    const input = await screen.findByTestId(
      "review-suggestion-reply-input-sug-1",
    );
    await user.type(input, "Keep me");
    expect(screen.getByTestId("suggestion-zone-after-2")).toContainElement(
      input,
    );

    // Two lines are added above: the card follows its line, with its words.
    rerender(editor("zero();\nnew();\nfirst();\nsecond();\nthird();\n"));
    await waitFor(() =>
      expect(screen.getByTestId("suggestion-zone-after-4")).toContainElement(
        input,
      ),
    );
    expect(screen.getByTestId("review-suggestion-reply-input-sug-1")).toBe(
      input,
    );
    expect(input).toHaveValue("Keep me");
  });

  it("keeps a card's typed reply as Monaco loads and editing authority changes", async () => {
    server.use(
      http.get(
        "*/api/v1/workspaces/ws-1/review/suggestions/sug-1/replies",
        () => HttpResponse.json({ data: { replies: [] } }),
      ),
    );
    const client = new QueryClient();
    const editor = (editable: boolean) => (
      <QueryClientProvider client={client}>
        <CodeEditor
          filePath="src/main.ts"
          content={"first();\nsecond();\nthird();\n"}
          language="typescript"
          editable={editable}
          onSave={vi.fn()}
          showComments={false}
          onToggleComments={vi.fn()}
          comments={[]}
          suggestionOverlay={overlay([suggestion({ id: "sug-1" })], {
            sourceWritable: true,
          })}
        />
      </QueryClientProvider>
    );
    const { rerender } = render(editor(false));
    const user = userEvent.setup();
    await user.click(
      await screen.findByTestId("review-suggestion-replies-toggle-sug-1"),
    );
    const input = await screen.findByTestId(
      "review-suggestion-reply-input-sug-1",
    );
    await user.type(input, "Keep me");

    // Loading completes: Monaco adopts the existing card and its draft.
    await act(async () => finishMonacoLoading());
    rerender(editor(true));
    await screen.findByTestId("mock-monaco-editor");
    await waitFor(() =>
      expect(screen.getByTestId("suggestion-zone-sug-1")).toContainElement(
        input,
      ),
    );
    expect(input).toHaveValue("Keep me");

    // Losing write authority keeps the native viewer and the same card.
    rerender(editor(false));
    await waitFor(() =>
      expect(screen.getByTestId("suggestion-zone-sug-1")).toContainElement(
        input,
      ),
    );
    expect(input).toHaveValue("Keep me");
  });

  it("marks a review comment whose line text was never recorded, and a blank line among blank twins", async () => {
    const { rerender } = render(
      withQueryClient(
        <CodeEditor
          filePath="src/main.ts"
          content={"// added\nfirst();\nsecond();\n"}
          language="typescript"
          editable={false}
          showComments={false}
          onToggleComments={vi.fn()}
          comments={[]}
          suggestionOverlay={overlay([
            suggestion({ id: "sug-untexted", beforeLines: null }),
          ])}
        />,
      ),
    );
    expect(
      await screen.findByTestId("suggestion-anchor-note-sug-untexted"),
    ).toHaveTextContent("Not confirmed here");

    rerender(
      withQueryClient(
        <CodeEditor
          filePath="src/main.ts"
          content={"a();\n\nb();\n\nc();\n"}
          language="typescript"
          editable={false}
          showComments={false}
          onToggleComments={vi.fn()}
          comments={[]}
          suggestionOverlay={overlay([
            suggestion({ id: "sug-blank", beforeLines: "" }),
          ])}
        />,
      ),
    );
    expect(
      await screen.findByTestId("suggestion-anchor-note-sug-blank"),
    ).toHaveTextContent("occur several times");
  });

  it("marks twin suggestion lines that nothing published tells apart", async () => {
    render(
      withQueryClient(
        <CodeEditor
          filePath="src/main.ts"
          content={"same();\nother();\nsame();\n"}
          language="typescript"
          editable={false}
          showComments={false}
          onToggleComments={vi.fn()}
          comments={[]}
          suggestionOverlay={overlay([
            suggestion({
              id: "sug-twin",
              anchorStart: 1,
              beforeLines: "same();\n",
              anchorContext: null,
            }),
          ])}
        />,
      ),
    );
    expect(
      await screen.findByTestId("suggestion-anchor-note-sug-twin"),
    ).toHaveTextContent("occur several times");
  });
});

it("starts a fresh code comment identity after cancelling a failed thread", async () => {
  const onAddComment = vi
    .fn()
    .mockRejectedValueOnce(new Error("Save failed"))
    .mockResolvedValueOnce(undefined);
  render(
    <CodeEditor
      filePath="src/main.ts"
      content={"first();\nsecond();\n"}
      language="typescript"
      editable={false}
      showComments
      onToggleComments={vi.fn()}
      comments={[]}
      onAddComment={onAddComment}
    />,
  );
  const user = userEvent.setup();
  const open = (line: number) => {
    const gutter = screen.getByTestId(`code-editor-gutter-line-${line}`);
    fireEvent.mouseEnter(gutter);
    fireEvent.click(
      within(gutter).getByTestId("code-editor-add-comment-button"),
    );
  };
  open(1);
  await user.type(screen.getByLabelText("Comment"), "First thread");
  await user.click(screen.getByTestId("draft-comment-save"));
  await screen.findByTestId("draft-comment-error");
  await user.click(screen.getByTestId("draft-comment-cancel"));
  open(2);
  await user.type(screen.getByLabelText("Comment"), "Second thread");
  await user.click(screen.getByTestId("draft-comment-save"));
  await waitFor(() => expect(onAddComment).toHaveBeenCalledTimes(2));
  expect(onAddComment.mock.calls[1][0].id).not.toEqual(
    onAddComment.mock.calls[0][0].id,
  );
  expect(onAddComment.mock.calls[1][0].anchor.lineNumber).toBe(2);
});

describe("CodeEditor comments on the chosen line", () => {
  const text = "first();\nsecond();\nthird();\nfourth();\nfifth();\n";

  function renderReadOnly(onAddComment = vi.fn().mockResolvedValue(undefined)) {
    render(
      <CodeEditor
        filePath="src/main.ts"
        content={text}
        language="typescript"
        editable={false}
        showComments
        onToggleComments={vi.fn()}
        comments={[]}
        onAddComment={onAddComment}
      />,
    );
    return onAddComment;
  }

  async function submitDraft(onAddComment: ReturnType<typeof vi.fn>) {
    const user = userEvent.setup();
    await user.type(await screen.findByLabelText("Comment"), "On this line");
    await user.click(screen.getByTestId("draft-comment-save"));
    await waitFor(() => expect(onAddComment).toHaveBeenCalledTimes(1));
    return onAddComment.mock.calls[0][0].anchor;
  }

  it("anchors a right-click Add comment to the row clicked, not an earlier caret", async () => {
    const onAddComment = renderReadOnly();
    // An earlier click leaves the caret on line 5.
    fireEvent.click(screen.getByTestId("code-editor-gutter-line-5"));
    fireEvent.contextMenu(screen.getByTestId("code-line-3"));
    fireEvent.click(
      screen.getByTestId("code-editor-context-add-comment-button"),
    );
    const anchor = await submitDraft(onAddComment);
    expect(anchor.lineNumber).toBe(3);
    expect(anchor.lineHash).toBe(await sha256Utf8("third();"));
  });

  it("anchors a selection's Comment to the selected row", async () => {
    // jsdom lays nothing out: give ranges a box to position the popover.
    Object.defineProperty(Range.prototype, "getBoundingClientRect", {
      configurable: true,
      value: () => new DOMRect(10, 10, 40, 16),
    });
    const onAddComment = renderReadOnly();
    const row = screen.getByTestId("code-line-4");
    const textNode = document
      .createTreeWalker(row, NodeFilter.SHOW_TEXT)
      .nextNode() as Node;
    const range = document.createRange();
    range.setStart(textNode, 0);
    range.setEnd(textNode, Math.min(3, textNode.textContent?.length ?? 0));
    window.getSelection()?.removeAllRanges();
    window.getSelection()?.addRange(range);
    fireEvent.mouseUp(row);
    fireEvent.click(
      await screen.findByTestId("code-editor-selection-comment-button"),
    );
    const anchor = await submitDraft(onAddComment);
    expect(anchor.lineNumber).toBe(4);
    expect(anchor.lineHash).toBe(await sha256Utf8("fourth();"));
    delete (Range.prototype as { getBoundingClientRect?: unknown })
      .getBoundingClientRect;
  });

  it("keeps a draft on its line when lines are added above before it is sent", async () => {
    const onAddComment = vi.fn().mockResolvedValue(undefined);
    const view = render(
      <CodeEditor
        filePath="src/main.ts"
        content={text}
        language="typescript"
        editable={false}
        showComments
        onToggleComments={vi.fn()}
        comments={[]}
        onAddComment={onAddComment}
      />,
    );
    fireEvent.mouseEnter(screen.getByTestId("code-editor-gutter-line-3"));
    fireEvent.click(screen.getByTestId("code-editor-add-comment-button"));
    // The file changes on screen while the draft is open.
    const changed = `added();\n${text}`;
    view.rerender(
      <CodeEditor
        filePath="src/main.ts"
        content={changed}
        language="typescript"
        editable={false}
        showComments
        onToggleComments={vi.fn()}
        comments={[]}
        onAddComment={onAddComment}
      />,
    );
    const anchor = await submitDraft(onAddComment);
    expect(anchor.lineNumber).toBe(4);
    expect(anchor.lineHash).toBe(await sha256Utf8("third();"));
    expect(anchor.contentHash).toBe(await sha256Utf8(changed));
  });

  it("keeps the draft and says why when its line is gone before it is sent", async () => {
    const onAddComment = vi.fn().mockResolvedValue(undefined);
    const view = render(
      <CodeEditor
        filePath="src/main.ts"
        content={text}
        language="typescript"
        editable={false}
        showComments
        onToggleComments={vi.fn()}
        comments={[]}
        onAddComment={onAddComment}
      />,
    );
    fireEvent.mouseEnter(screen.getByTestId("code-editor-gutter-line-3"));
    fireEvent.click(screen.getByTestId("code-editor-add-comment-button"));
    view.rerender(
      <CodeEditor
        filePath="src/main.ts"
        content={"first();\nsecond();\nfourth();\nfifth();\n"}
        language="typescript"
        editable={false}
        showComments
        onToggleComments={vi.fn()}
        comments={[]}
        onAddComment={onAddComment}
      />,
    );
    const user = userEvent.setup();
    await user.type(await screen.findByLabelText("Comment"), "Keep me");
    await user.click(screen.getByTestId("draft-comment-save"));
    expect(await screen.findByTestId("draft-comment-error")).toHaveTextContent(
      "The commented line changed",
    );
    expect(screen.getByLabelText("Comment")).toHaveValue("Keep me");
    expect(onAddComment).not.toHaveBeenCalled();
  });

  it("offers no review controls on a read-only source, and Withdraw to a reviewer who owns it", () => {
    const { rerender } = render(
      withQueryClient(
        <CodeEditor
          filePath="src/main.ts"
          content={"first();\nsecond();\n"}
          language="typescript"
          editable={false}
          showComments={false}
          onToggleComments={vi.fn()}
          comments={[]}
          suggestionOverlay={overlay(
            [suggestion({ id: "sug-mine", reviewerUserId: "user-reviewer" })],
            { viewerOwnsWorkspace: true, sourceWritable: false },
          )}
        />,
      ),
    );
    expect(
      screen.queryByTestId("review-suggestion-withdraw-sug-mine"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("review-suggestion-reject-sug-mine"),
    ).not.toBeInTheDocument();

    rerender(
      withQueryClient(
        <CodeEditor
          filePath="src/main.ts"
          content={"first();\nsecond();\n"}
          language="typescript"
          editable={false}
          showComments={false}
          onToggleComments={vi.fn()}
          comments={[]}
          suggestionOverlay={overlay(
            [suggestion({ id: "sug-mine", reviewerUserId: "user-reviewer" })],
            { viewerOwnsWorkspace: true, sourceWritable: true },
          )}
        />,
      ),
    );
    expect(
      screen.getByTestId("review-suggestion-withdraw-sug-mine"),
    ).toBeInTheDocument();
  });
});
