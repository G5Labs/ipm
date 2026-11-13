// Anchored comments at the WorkspacePage–editor boundary
// (`g5.documents.comments`, `.identity`, `.authority`, `.replies`,
// `.controls`): the page lists a file's threads by repository and path,
// starts a thread from the anchor the editor built on the text on screen,
// and hands each viewer only the controls they may use.

import {
  act,
  fireEvent,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { Route, Routes } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { MOCK_WORKSPACE_DETAILS } from "../data/mock";
import { setApiToken } from "../lib/api";
import { createCommentAnchor, sha256Utf8 } from "../lib/commentAnchors";
import type { CommentDTO, ReviewSuggestion } from "../lib/types";
import { GETTING_STARTED_AUTO_OPENED_STORAGE_KEY } from "../lib/userGuide";
import { createMockUser } from "../test/fixtures";
import { server } from "../test/msw/server";
import { renderApp } from "../test/render";
import { seedUserSettings } from "../test/userSettings";
import { WorkspacePage } from "./WorkspacePage";

vi.mock("../components/execution/WorkspaceTerminalPanel", () => ({
  WorkspaceTerminalPanel: () => <div />,
}));

// A Monaco stand-in with a caret, a model, view zones and a glyph margin.
// Selecting in the textarea reports the caret line the way Monaco's
// onDidChangeCursorPosition does; view zones keep their DOM nodes below the
// text, so suggestion cards placed in zones render; with a glyph margin
// (FILES' read-only Monaco), each line has a button that reports a
// glyph-margin mouse-down, the way a click in Monaco's margin does.
vi.mock("@monaco-editor/react", async () => {
  const ReactModule = await import("react");
  type CursorListener = (event: {
    position: { lineNumber: number; column: number };
  }) => void;
  type MouseListener = (event: {
    target: { type: number; position: { lineNumber: number } | null };
  }) => void;
  const GUTTER_GLYPH_MARGIN = 2;
  const disposable = { dispose: () => undefined };
  const MockMonaco = ({
    value: controlledValue,
    defaultValue = "",
    onChange,
    onMount,
    options,
  }: {
    value?: string;
    defaultValue?: string;
    onChange: (value: string) => void;
    onMount?: (editor: unknown, monaco: unknown) => void;
    options?: { readOnly?: boolean; glyphMargin?: boolean };
  }) => {
    const [modelValue, setModelValue] = ReactModule.useState(defaultValue);
    const value = controlledValue ?? modelValue;
    const listeners = ReactModule.useRef<CursorListener[]>([]);
    const mouseDown = ReactModule.useRef<MouseListener[]>([]);
    const zones = ReactModule.useRef<HTMLDivElement>(null);
    const zoneNodes = ReactModule.useRef(new Map<string, HTMLElement>());
    const valueRef = ReactModule.useRef(value);
    valueRef.current = value;
    const model = ReactModule.useRef({
      getValue: () => valueRef.current,
      getEOL: () => (valueRef.current.includes("\r\n") ? "\r\n" : "\n"),
      pushEOL: () => undefined,
      getFullModelRange: () => ({}),
      pushStackElement: () => undefined,
      pushEditOperations: (_before: unknown, edits: { text: string }[]) => {
        valueRef.current = edits[0].text;
        setModelValue(edits[0].text);
        onChange(edits[0].text);
      },
      setValue: (text: string) => {
        valueRef.current = text;
        setModelValue(text);
      },
      isDisposed: () => false,
      dispose: () => undefined,
      onDidChangeContent: () => disposable,
      uri: { path: "/mock" },
    });
    ReactModule.useEffect(() => {
      let nextZone = 0;
      onMount?.(
        {
          addCommand: () => undefined,
          revealLineInCenter: () => undefined,
          getModel: () => model.current,
          onDidChangeCursorPosition: (listener: CursorListener) => {
            listeners.current.push(listener);
            return disposable;
          },
          onDidChangeModelContent: () => disposable,
          onMouseDown: (listener: MouseListener) => {
            mouseDown.current.push(listener);
            return disposable;
          },
          onMouseMove: () => disposable,
          onMouseLeave: () => disposable,
          createDecorationsCollection: () => ({
            set: () => undefined,
            clear: () => undefined,
          }),
          changeViewZones: (
            change: (accessor: {
              addZone: (zone: { domNode: HTMLElement }) => string;
              removeZone: (id: string) => void;
              layoutZone: (id: string) => void;
            }) => void,
          ) =>
            change({
              addZone: (zone) => {
                nextZone += 1;
                const id = `zone-${nextZone}`;
                zoneNodes.current.set(id, zone.domNode);
                zones.current?.appendChild(zone.domNode);
                return id;
              },
              removeZone: (id) => {
                zoneNodes.current.get(id)?.remove();
                zoneNodes.current.delete(id);
              },
              layoutZone: () => undefined,
            }),
        },
        {
          KeyMod: { CtrlCmd: 0 },
          KeyCode: { KeyS: 0 },
          Range: class {},
          editor: {
            defineTheme: () => undefined,
            setTheme: () => undefined,
            MouseTargetType: { GUTTER_GLYPH_MARGIN },
          },
        },
      );
    }, []);
    const lineCount = value.split("\n").length;
    return ReactModule.createElement(
      "div",
      null,
      ReactModule.createElement("textarea", {
        "data-testid": "mock-monaco-editor",
        value,
        readOnly: options?.readOnly === true,
        onChange: (event: React.ChangeEvent<HTMLTextAreaElement>) => {
          setModelValue(event.currentTarget.value);
          valueRef.current = event.currentTarget.value;
          onChange(event.currentTarget.value);
        },
        onSelect: (event: React.SyntheticEvent<HTMLTextAreaElement>) => {
          const element = event.currentTarget;
          const lineNumber = element.value
            .slice(0, element.selectionStart ?? 0)
            .split("\n").length;
          for (const listener of listeners.current) {
            listener({ position: { lineNumber, column: 1 } });
          }
        },
      }),
      options?.glyphMargin
        ? Array.from({ length: lineCount }, (_, index) =>
            ReactModule.createElement("button", {
              key: index,
              type: "button",
              "data-testid": `mock-monaco-glyph-${index + 1}`,
              onClick: () => {
                for (const listener of mouseDown.current) {
                  listener({
                    target: {
                      type: GUTTER_GLYPH_MARGIN,
                      position: { lineNumber: index + 1 },
                    },
                  });
                }
              },
            }),
          )
        : null,
      ReactModule.createElement("div", {
        ref: zones,
        "data-testid": "mock-monaco-zones",
      }),
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

const WORKSPACE_ID = "task-graph-rework";
const FILE_PATH = "src/main.ts";
const FILE_TEXT = "alpha();\nbeta();\ntarget();\n";

interface Served {
  fileRepoIds: string[];
  listQueries: URLSearchParams[];
  created: Record<string, unknown>[];
  replies: Array<{ commentId: string; body: Record<string, unknown> }>;
}

function serveWorkspace({
  ownerUserId = "user-alex",
  vmStatus = "running",
  comments = [],
}: {
  ownerUserId?: string;
  vmStatus?: "running" | "stopped";
  comments?: CommentDTO[];
} = {}): Served {
  const served: Served = {
    fileRepoIds: [],
    listQueries: [],
    created: [],
    replies: [],
  };
  server.use(
    // One repository, so its branch is the content branch.
    http.get("/api/v1/workspaces/:id", () =>
      HttpResponse.json({
        data: {
          ...MOCK_WORKSPACE_DETAILS[WORKSPACE_ID],
          ownerUserId,
          branches: MOCK_WORKSPACE_DETAILS[WORKSPACE_ID].branches.slice(0, 1),
        },
      }),
    ),
    http.get("/api/v1/workspaces/:id/vm", () =>
      HttpResponse.json({
        data: {
          orgId: "org-g5labs",
          workspaceId: WORKSPACE_ID,
          ownerUserId,
          status: vmStatus,
          hostId: vmStatus === "running" ? "host-test" : null,
          vmId: vmStatus === "running" ? `vm-${WORKSPACE_ID}` : null,
          epoch: 1,
          imageRef: "g5-test:latest",
          moduleCatalogVersion: "test",
          startIntent: null,
          startDeadlineAt: null,
          startedAt: "2026-09-30T08:00:00Z",
          lastActivityAt: "2026-09-30T08:00:00Z",
          stopReason: null,
          stopOperationId: null,
          stopDeadlineAt: null,
          pendingStopReason: null,
          pendingStopDeadlineAt: null,
          nudgedAt: null,
          lastFlushId: null,
          unflushedLossAt: null,
          lastError: null,
        },
      }),
    ),
    http.get("/api/v1/workspaces/:id/files/*", ({ request }) => {
      served.fileRepoIds.push(
        new URL(request.url).searchParams.get("repoId") ?? "",
      );
      return HttpResponse.json({
        data: {
          path: FILE_PATH,
          content: FILE_TEXT,
          size: FILE_TEXT.length,
          encoding: "utf-8",
          hash: "file-v1",
          source:
            ownerUserId === "user-alex" && vmStatus === "running"
              ? "live"
              : "committed",
          repoOid: "oid-base",
          snapshotAt: null,
          vmId: vmStatus === "running" ? `vm-${WORKSPACE_ID}` : null,
        },
      });
    }),
    http.get("/api/v1/workspaces/:wsId/comments", ({ request }) => {
      served.listQueries.push(new URL(request.url).searchParams);
      return HttpResponse.json({ data: comments });
    }),
    http.post("/api/v1/workspaces/:wsId/comments", async ({ request }) => {
      const body = (await request.json()) as Record<string, unknown>;
      served.created.push(body);
      return HttpResponse.json(
        {
          data: {
            id: "comment-new",
            workspaceId: WORKSPACE_ID,
            authorId: "user-alex",
            authorName: "Alex Chen",
            authorAvatarUrl: null,
            resolved: false,
            resolvedAt: null,
            originBranch: "main",
            replyCount: 0,
            createdAt: "2026-09-30T10:00:00Z",
            updatedAt: "2026-09-30T10:00:00Z",
            ...body,
          },
        },
        { status: 201 },
      );
    }),
    http.get("/api/v1/comments/:commentId/replies", () =>
      HttpResponse.json({ data: [] }),
    ),
    http.post(
      "/api/v1/comments/:commentId/replies",
      async ({ params, request }) => {
        const body = (await request.json()) as Record<string, unknown>;
        served.replies.push({ commentId: params.commentId as string, body });
        return HttpResponse.json(
          {
            data: {
              id: `reply-${served.replies.length}`,
              commentId: params.commentId,
              authorId: "user-alex",
              authorName: "Alex Chen",
              authorAvatarUrl: null,
              content: body.content,
              createdAt: "2026-09-30T10:05:00Z",
            },
          },
          { status: 201 },
        );
      },
    ),
  );
  return served;
}

async function storedComment(lineNumber: number): Promise<CommentDTO> {
  const anchor = await createCommentAnchor(FILE_TEXT, lineNumber);
  return {
    id: "comment-1",
    workspaceId: WORKSPACE_ID,
    authorId: "user-other",
    authorName: "Olive Owner",
    authorAvatarUrl: null,
    scope: "line",
    repoId: "repo-g5-client",
    filePath: FILE_PATH,
    lineNumber,
    lineHash: anchor.lineHash,
    contentHash: anchor.contentHash,
    anchorContext: anchor.anchorContext,
    commitSha: null,
    content: "Is beta needed?",
    resolved: false,
    resolvedAt: null,
    originBranch: "main",
    replyCount: 0,
    createdAt: "2026-09-30T09:00:00Z",
    updatedAt: "2026-09-30T09:00:00Z",
  };
}

async function renderFileTab() {
  localStorage.setItem(
    `g5:tabs:${WORKSPACE_ID}`,
    JSON.stringify({
      tabs: [{ type: "file", filePath: FILE_PATH, name: "main.ts" }],
      activeTabId: `file:${FILE_PATH}`,
    }),
  );
  const view = renderApp(
    <Routes>
      <Route path="/:org/:workspace/*" element={<WorkspacePage />} />
    </Routes>,
    { route: `/org-g5labs/${WORKSPACE_ID}`, user: createMockUser() },
  );
  fireEvent.click(await screen.findByRole("tab", { name: /main\.ts/ }));
  return view;
}

/**
 * Clicks a line's glyph until its draft opens: the editor ignores glyph
 * clicks until commenting is wired, which can lag the glyph under load.
 */
async function startThreadAt(line: number): Promise<HTMLElement> {
  return waitFor(() => {
    const open = screen.queryByLabelText("Comment");
    if (open) return open;
    fireEvent.click(screen.getByTestId(`mock-monaco-glyph-${line}`));
    return screen.getByLabelText("Comment");
  });
}

/** The file tab beside another file's, for switching away and back. */
async function renderFileTabBesideAnother() {
  localStorage.setItem(
    `g5:tabs:${WORKSPACE_ID}`,
    JSON.stringify({
      tabs: [
        { type: "file", filePath: FILE_PATH, name: "main.ts" },
        { type: "file", filePath: "src/other.ts", name: "other.ts" },
      ],
      activeTabId: `file:${FILE_PATH}`,
    }),
  );
  const view = renderApp(
    <Routes>
      <Route path="/:org/:workspace/*" element={<WorkspacePage />} />
    </Routes>,
    { route: `/org-g5labs/${WORKSPACE_ID}`, user: createMockUser() },
  );
  fireEvent.click(await screen.findByRole("tab", { name: /main\.ts/ }));
  return view;
}

/** The file tab beside the workspace's Approvals tab, where Reviews is. */
async function renderFileTabBesideApprovals() {
  localStorage.setItem(
    `g5:tabs:${WORKSPACE_ID}`,
    JSON.stringify({
      tabs: [
        { type: "file", filePath: FILE_PATH, name: "main.ts" },
        { type: "workspace_approvals", name: "Approvals" },
      ],
      activeTabId: `file:${FILE_PATH}`,
    }),
  );
  const view = renderApp(
    <Routes>
      <Route path="/:org/:workspace/*" element={<WorkspacePage />} />
    </Routes>,
    { route: `/org-g5labs/${WORKSPACE_ID}`, user: createMockUser() },
  );
  fireEvent.click(await screen.findByRole("tab", { name: /main\.ts/ }));
  return view;
}

/** The open-tab strip's tab whose label matches `name`. */
function centerTab(name: RegExp): HTMLElement {
  const tab = screen
    .getAllByTestId("center-content-tab")
    .find((candidate) => name.test(candidate.textContent ?? ""));
  if (!tab) throw new Error(`no open tab named ${name}`);
  return tab;
}

/**
 * Switches to the other file's tab, so `shown` (a test id) leaves the page,
 * settles `whileAway` there, and switches back.
 */
async function switchAwayAndBack(shown: string, whileAway: () => void) {
  fireEvent.click(centerTab(/other\.ts/));
  await waitFor(() => expect(screen.queryByTestId(shown)).toBeNull());
  whileAway();
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 50));
  });
  fireEvent.click(centerTab(/main\.ts/));
}

describe("WorkspacePage anchored comments", () => {
  beforeEach(() => {
    localStorage.clear();
    // The center tab bar is hidden by default (workspaces
    // g5.workspace.ui.center-tabs); this suite drives the tabs, so it opts in.
    seedUserSettings({ showTabBar: true });
    localStorage.setItem(GETTING_STARTED_AUTO_OPENED_STORAGE_KEY, "1");
    vi.mocked(window.matchMedia).mockImplementation((query) => ({
      matches: query.includes("min-width: 1024px"),
      media: query,
      onchange: null,
      addListener: vi.fn(),
      removeListener: vi.fn(),
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      dispatchEvent: vi.fn(),
    }));
  });

  it("starts a thread on the file from the anchor built on the unsaved text on screen", async () => {
    const served = serveWorkspace();
    await renderFileTab();
    const editor = (await screen.findByTestId(
      "mock-monaco-editor",
    )) as HTMLTextAreaElement;
    await waitFor(() => expect(editor).toHaveValue(FILE_TEXT));
    // FILES opens read-first; the owner edits after Edit.
    fireEvent.click(await screen.findByTestId("file-view-edit"));

    // Unsaved: two lines inserted above the line being commented on.
    const edited = `one();\ntwo();\n${FILE_TEXT}`;
    fireEvent.change(editor, { target: { value: edited } });
    const caret = edited.indexOf("target");
    editor.setSelectionRange(caret, caret);
    fireEvent.select(editor);
    fireEvent.contextMenu(editor);
    fireEvent.click(
      screen.getByTestId("code-editor-context-add-comment-button"),
    );
    const user = userEvent.setup();
    await user.type(await screen.findByLabelText("Comment"), "Why target?");
    await user.click(screen.getByTestId("draft-comment-save"));

    await waitFor(() => expect(served.created).toHaveLength(1));
    const repoId = served.fileRepoIds[0];
    expect(repoId).toBeTruthy();
    expect(served.created[0]).toEqual({
      id: expect.stringMatching(/^[0-9a-f-]{36}$/),
      scope: "line",
      content: "Why target?",
      repoId,
      filePath: FILE_PATH,
      lineNumber: 5,
      lineHash: await sha256Utf8("target();"),
      contentHash: await sha256Utf8(edited),
      anchorContext: (await createCommentAnchor(edited, 5)).anchorContext,
    });
    // The file's threads are listed by repository and path; a branch never
    // selects them.
    expect(served.listQueries.length).toBeGreaterThan(0);
    for (const query of served.listQueries) {
      expect(query.get("repoId")).toBe(repoId);
      expect(query.get("filePath")).toBe(FILE_PATH);
      expect(query.has("branch")).toBe(false);
    }
  });

  it.each([
    { who: "another person signs in", sub: "user-bob", sent: false },
    { who: "the same person's token is renewed", sub: "user-alex", sent: true },
  ])(
    "starts a thread only for whoever submitted it when $who while it is sent",
    async ({ sub, sent }) => {
      const served = serveWorkspace();
      await renderFileTab();
      const editor = (await screen.findByTestId(
        "mock-monaco-editor",
      )) as HTMLTextAreaElement;
      await waitFor(() => expect(editor).toHaveValue(FILE_TEXT));
      // The anchor's hashing waits until it is released.
      const realDigest = crypto.subtle.digest.bind(crypto.subtle);
      let release!: () => void;
      const gate = new Promise<void>((resolve) => {
        release = resolve;
      });
      const digest = vi
        .spyOn(crypto.subtle, "digest")
        .mockImplementation(async (algorithm, data) => {
          await gate;
          return realDigest(algorithm, data);
        });
      try {
        const caret = FILE_TEXT.indexOf("target");
        editor.setSelectionRange(caret, caret);
        fireEvent.select(editor);
        fireEvent.contextMenu(editor);
        fireEvent.click(
          screen.getByTestId("code-editor-context-add-comment-button"),
        );
        const user = userEvent.setup();
        await user.type(
          await screen.findByLabelText("Comment"),
          "Alex's words",
        );
        await user.click(screen.getByTestId("draft-comment-save"));

        // While the anchor is computed, another tab signs in, or the same
        // person's token is renewed.
        const encode = (value: object) =>
          btoa(JSON.stringify(value))
            .replace(/\+/g, "-")
            .replace(/\//g, "_")
            .replace(/=+$/, "");
        act(() =>
          setApiToken(
            `${encode({ alg: "HS256" })}.${encode({ sub, org: "org-g5labs", scopes: [] })}.renewed`,
          ),
        );
        release();
        if (sent) {
          await waitFor(() => expect(served.created).toHaveLength(1));
          expect(served.created[0]).toMatchObject({ content: "Alex's words" });
        } else {
          // Bob's page holds nothing Alex typed, and Alex's thread, whose
          // anchor was still being computed, is not started under Bob's token.
          expect(screen.queryByDisplayValue("Alex's words")).toBeNull();
          await act(async () => {
            await new Promise((resolve) => setTimeout(resolve, 100));
          });
          expect(digest).toHaveBeenCalled();
          expect(served.created).toHaveLength(0);
        }
      } finally {
        digest.mockRestore();
      }
    },
  );

  it.each([
    { who: "another person's token arrives", sub: "user-bob", sent: false },
    { who: "the same person's token is renewed", sub: "user-alex", sent: true },
  ])(
    "sends replies only for the principal the page was shown to when $who before the page catches up",
    async ({ sub, sent }) => {
      const repoId = MOCK_WORKSPACE_DETAILS[WORKSPACE_ID].branches[0].repoId;
      const suggestion: ReviewSuggestion = {
        id: "sug-1",
        publicationId: "pub-1",
        findingId: null,
        repoId,
        path: FILE_PATH,
        kind: "comment",
        beforeLines: "beta();\n",
        afterLines: null,
        anchorStart: 2,
        contentHash: null,
        anchorContext: null,
        authorKind: "human",
        authorLabel: "rita",
        note: "Is beta needed?",
        commentId: "thread-1",
        status: "open",
        statusBy: null,
        statusAt: null,
        supersedes: null,
        adoptedFrom: null,
        createdAt: "2026-09-30T10:00:00Z",
      };
      const answered: unknown[] = [];
      const served = serveWorkspace({
        vmStatus: "stopped",
        comments: [await storedComment(2)],
      });
      server.use(
        http.get("/api/v1/workspaces/:wsId/review/suggestions", () =>
          HttpResponse.json({ data: { suggestions: [suggestion] } }),
        ),
        http.get(
          "/api/v1/workspaces/:wsId/review/suggestions/:id/replies",
          () => HttpResponse.json({ data: { replies: [] } }),
        ),
        http.post(
          "/api/v1/workspaces/:wsId/review/suggestions/:id/replies",
          async ({ request }) => {
            answered.push(await request.json());
            return HttpResponse.json({ data: { replies: [] } });
          },
        ),
      );
      await renderFileTab();
      const user = userEvent.setup();
      await user.click(
        await screen.findByTestId("code-editor-toggle-comments-button"),
      );
      const thread = await screen.findByTestId("comment-thread-comment-1");
      await user.click(
        within(thread).getByTestId("comment-thread-toggle-expand"),
      );
      const threadReply = within(thread).getByPlaceholderText("Reply…");
      await user.type(threadReply, "Alex's reply");
      const card = await screen.findByTestId("review-suggestion-sug-1");
      await user.click(
        within(card).getByTestId("review-suggestion-replies-toggle-sug-1"),
      );
      const cardReply = await within(card).findByTestId(
        "review-suggestion-reply-input-sug-1",
      );
      await user.type(cardReply, "Alex's answer");

      // The token arrives from outside React, as a refresh's does, and both
      // replies are sent before the page has re-rendered for it.
      const encode = (value: object) =>
        btoa(JSON.stringify(value))
          .replace(/\+/g, "-")
          .replace(/\//g, "_")
          .replace(/=+$/, "");
      const env = globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean };
      env.IS_REACT_ACT_ENVIRONMENT = false;
      try {
        setApiToken(
          `${encode({ alg: "HS256" })}.${encode({ sub, org: "org-g5labs", scopes: [] })}.renewed`,
        );
      } finally {
        env.IS_REACT_ACT_ENVIRONMENT = true;
      }
      act(() => {
        fireEvent.keyDown(threadReply, { key: "Enter" });
        fireEvent.keyDown(cardReply, { key: "Enter" });
      });

      if (sent) {
        await waitFor(() => expect(served.replies).toHaveLength(1));
        expect(served.replies[0].body).toEqual({ content: "Alex's reply" });
        await waitFor(() =>
          expect(answered).toEqual([{ content: "Alex's answer" }]),
        );
      } else {
        // Once the page catches up, it holds nothing Alex typed, and
        // neither reply was sent under Bob's token.
        await waitFor(() => {
          expect(screen.queryByDisplayValue("Alex's reply")).toBeNull();
          expect(screen.queryByDisplayValue("Alex's answer")).toBeNull();
        });
        await act(async () => {
          await new Promise((resolve) => setTimeout(resolve, 100));
        });
        expect(served.replies).toHaveLength(0);
        expect(answered).toHaveLength(0);
      }
    },
  );

  it.each(
    (
      [
        { action: "resolve", thread: "comment-1" },
        { action: "delete", thread: "comment-2" },
        { action: "reopen", thread: "comment-3" },
      ] as const
    ).flatMap((decision) => [
      {
        ...decision,
        who: "another person's token arrives",
        sub: "user-bob",
        sent: false,
      },
      {
        ...decision,
        who: "the same person's token is renewed",
        sub: "user-alex",
        sent: true,
      },
    ]),
  )(
    "decides by $action only for the principal the page was shown to when $who before the page catches up",
    async ({ action, thread: threadId, sub, sent }) => {
      const repoId = MOCK_WORKSPACE_DETAILS[WORKSPACE_ID].branches[0].repoId;
      const suggestion: ReviewSuggestion = {
        id: "sug-1",
        publicationId: "pub-1",
        findingId: null,
        repoId,
        path: FILE_PATH,
        kind: "comment",
        beforeLines: "beta();\n",
        afterLines: null,
        anchorStart: 2,
        contentHash: null,
        anchorContext: null,
        authorKind: "human",
        authorLabel: "rita",
        note: "Is beta needed?",
        commentId: "thread-1",
        status: "open",
        statusBy: null,
        statusAt: null,
        supersedes: null,
        adoptedFrom: null,
        createdAt: "2026-09-30T10:00:00Z",
      };
      const decided: string[] = [];
      // One thread to resolve, one to delete, and a resolved one to reopen.
      serveWorkspace({
        vmStatus: "stopped",
        comments: [
          await storedComment(1),
          { ...(await storedComment(2)), id: "comment-2" },
          {
            ...(await storedComment(3)),
            id: "comment-3",
            resolved: true,
            resolvedAt: "2026-09-30T09:30:00Z",
          },
        ],
      });
      server.use(
        http.get("/api/v1/workspaces/:wsId/review/suggestions", () =>
          HttpResponse.json({ data: { suggestions: [suggestion] } }),
        ),
        http.patch(
          "/api/v1/comments/:commentId",
          async ({ params, request }) => {
            const { resolved } = (await request.json()) as {
              resolved: boolean;
            };
            decided.push(
              `${resolved ? "resolve" : "reopen"} ${String(params.commentId)}`,
            );
            return HttpResponse.json({ data: {} });
          },
        ),
        http.delete("/api/v1/comments/:commentId", ({ params }) => {
          decided.push(`delete ${String(params.commentId)}`);
          return new HttpResponse(null, { status: 204 });
        }),
        http.post(
          "/api/v1/workspaces/:wsId/review/suggestions/:id/resolve",
          ({ params }) => {
            decided.push(`resolve ${String(params.id)}`);
            return HttpResponse.json({
              data: { suggestion: { ...suggestion, status: "resolved" } },
            });
          },
        ),
      );
      await renderFileTab();
      const user = userEvent.setup();
      await user.click(
        await screen.findByTestId("code-editor-toggle-comments-button"),
      );
      const thread = await screen.findByTestId(`comment-thread-${threadId}`);
      await user.click(
        within(thread).getByTestId("comment-thread-toggle-expand"),
      );
      // Words the page clears once it catches up with another person.
      await user.type(within(thread).getByPlaceholderText("Reply…"), "Note");
      if (action === "delete") {
        await user.click(within(thread).getByTestId("comment-thread-delete"));
      }
      const control = within(thread).getByTestId(
        action === "resolve"
          ? "comment-thread-resolve"
          : action === "delete"
            ? "comment-thread-delete-confirm"
            : "comment-thread-unresolve",
      );
      const card = await screen.findByTestId("review-suggestion-sug-1");

      // The token arrives from outside React, as a refresh's does, and the
      // decisions are clicked before the page has re-rendered for it.
      const encode = (value: object) =>
        btoa(JSON.stringify(value))
          .replace(/\+/g, "-")
          .replace(/\//g, "_")
          .replace(/=+$/, "");
      const env = globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean };
      env.IS_REACT_ACT_ENVIRONMENT = false;
      try {
        setApiToken(
          `${encode({ alg: "HS256" })}.${encode({ sub, org: "org-g5labs", scopes: [] })}.renewed`,
        );
      } finally {
        env.IS_REACT_ACT_ENVIRONMENT = true;
      }
      act(() => {
        fireEvent.click(control);
        fireEvent.click(
          within(card).getByTestId("review-suggestion-resolve-sug-1"),
        );
      });

      if (sent) {
        await waitFor(() =>
          expect([...decided].sort()).toEqual(
            [`${action} ${threadId}`, "resolve sug-1"].sort(),
          ),
        );
      } else {
        // Once the page catches up, neither decision was sent under Bob's
        // token.
        await waitFor(() =>
          expect(screen.queryByDisplayValue("Note")).toBeNull(),
        );
        await act(async () => {
          await new Promise((resolve) => setTimeout(resolve, 100));
        });
        expect(decided).toHaveLength(0);
      }
    },
  );

  it("lets the owner start a thread while the workspace is stopped", async () => {
    const served = serveWorkspace({ vmStatus: "stopped" });
    await renderFileTab();
    // FILES draws read-only Monaco: its glyph margin starts a thread.
    await startThreadAt(3);
    const user = userEvent.setup();
    await user.type(await screen.findByLabelText("Comment"), "Rename it");
    await user.click(screen.getByTestId("draft-comment-save"));

    await waitFor(() => expect(served.created).toHaveLength(1));
    expect(served.created[0]).toMatchObject({
      lineNumber: 3,
      lineHash: await sha256Utf8("target();"),
      contentHash: await sha256Utf8(FILE_TEXT),
      commitSha: "oid-base",
    });
  });

  it("lets a reader start a pushed-file thread and reply, without moderation controls", async () => {
    const served = serveWorkspace({
      ownerUserId: "user-other",
      comments: [await storedComment(2)],
    });
    await renderFileTab();
    await startThreadAt(3);
    const user = userEvent.setup();
    await user.type(await screen.findByLabelText("Comment"), "Reader question");
    await user.click(screen.getByTestId("draft-comment-save"));
    await waitFor(() => expect(served.created).toHaveLength(1));
    expect(served.created[0]).toMatchObject({
      content: "Reader question",
      lineNumber: 3,
      commitSha: "oid-base",
      contentHash: await sha256Utf8(FILE_TEXT),
    });
    const thread = await screen.findByTestId("comment-thread-comment-1");
    expect(thread).toBeVisible();
    await user.click(
      within(thread).getByTestId("comment-thread-toggle-expand"),
    );
    expect(
      within(thread).queryByTestId("comment-thread-resolve"),
    ).not.toBeInTheDocument();
    expect(
      within(thread).queryByTestId("comment-thread-delete"),
    ).not.toBeInTheDocument();

    const reply = within(thread).getByPlaceholderText("Reply…");
    expect(reply).toBeVisible();
    await user.type(reply, "Yes, for the retry path");
    // Two Enters while the first is in flight send one reply.
    fireEvent.keyDown(reply, { key: "Enter" });
    fireEvent.keyDown(reply, { key: "Enter" });
    await waitFor(() => expect(served.replies).toHaveLength(1));
    expect(served.replies[0]).toEqual({
      commentId: "comment-1",
      body: { content: "Yes, for the retry path" },
    });
    await waitFor(() => expect(reply).toHaveValue(""));
  });
  it("keeps pushed-file commenting out of a reader's suggestion buffer", async () => {
    serveWorkspace({ ownerUserId: "user-other" });
    await renderFileTab();
    await waitFor(() =>
      expect(screen.getByTestId("mock-monaco-glyph-3")).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByTestId("file-view-suggest"));
    await screen.findByTestId("file-view-suggesting");
    expect(screen.queryByTestId("mock-monaco-glyph-3")).not.toBeInTheDocument();
    fireEvent.contextMenu(screen.getByTestId("mock-monaco-editor"));
    expect(
      screen.queryByTestId("code-editor-context-add-comment-button"),
    ).not.toBeInTheDocument();
  });

  it("keeps a typed reply and shows why when deleting the thread is refused", async () => {
    let release: () => void = () => {};
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    serveWorkspace({ comments: [await storedComment(2)] });
    server.use(
      http.delete("/api/v1/comments/:commentId", async () => {
        await gate;
        return HttpResponse.json(
          {
            error: {
              code: "WORKSPACE_STOPPING",
              message: "The workspace is being transferred",
              requestId: "request-delete",
            },
          },
          { status: 409 },
        );
      }),
    );
    await renderFileTab();
    const user = userEvent.setup();
    await user.click(
      await screen.findByTestId("code-editor-toggle-comments-button"),
    );
    const thread = await screen.findByTestId("comment-thread-comment-1");
    await user.click(
      within(thread).getByTestId("comment-thread-toggle-expand"),
    );
    await user.type(
      within(thread).getByPlaceholderText("Reply…"),
      "Half-typed",
    );
    await user.click(within(thread).getByTestId("comment-thread-delete"));
    await user.click(
      within(thread).getByTestId("comment-thread-delete-confirm"),
    );
    // The thread stays while the server decides, and after it refuses.
    expect(screen.getByTestId("comment-thread-comment-1")).toBe(thread);
    release();
    expect(
      await within(thread).findByTestId("comment-thread-error"),
    ).toHaveTextContent("The workspace is being transferred");
    expect(within(thread).getByPlaceholderText("Reply…")).toHaveValue(
      "Half-typed",
    );
  });

  it("keeps a thread and its typed reply when the reply and the list refresh both fail", async () => {
    const unavailable = () =>
      HttpResponse.json(
        {
          error: {
            code: "SERVICE_UNAVAILABLE",
            message: "The service is restarting",
            requestId: "request-down",
          },
        },
        { status: 503 },
      );
    serveWorkspace({ comments: [await storedComment(2)] });
    const view = await renderFileTab();
    const user = userEvent.setup();
    await user.click(
      await screen.findByTestId("code-editor-toggle-comments-button"),
    );
    const thread = await screen.findByTestId("comment-thread-comment-1");
    await user.click(
      within(thread).getByTestId("comment-thread-toggle-expand"),
    );
    // The service goes away: the reply fails, and so does the refresh of
    // the thread list that follows it.
    server.use(
      http.post("/api/v1/comments/:commentId/replies", unavailable),
      http.get("/api/v1/workspaces/:wsId/comments", unavailable),
    );
    const reply = within(thread).getByPlaceholderText("Reply…");
    await user.type(reply, "Keep this");
    fireEvent.keyDown(reply, { key: "Enter" });
    expect(
      await within(thread).findByTestId("comment-thread-error"),
    ).toHaveTextContent("The service is restarting");
    expect(
      await screen.findByTestId("comment-column-notice"),
    ).toHaveTextContent("Could not refresh comments");
    expect(screen.getByTestId("comment-thread-comment-1")).toBe(thread);
    expect(reply).toHaveValue("Keep this");

    // Back: the next refresh succeeds, and nothing typed was lost.
    serveWorkspace({ comments: [await storedComment(2)] });
    await view.queryClient.invalidateQueries();
    await waitFor(() =>
      expect(
        screen.queryByTestId("comment-column-notice"),
      ).not.toBeInTheDocument(),
    );
    expect(screen.getByTestId("comment-thread-comment-1")).toBe(thread);
    expect(reply).toHaveValue("Keep this");
  });

  it("keeps the page, a typed reply and an open draft when the workspace cannot be refreshed", async () => {
    serveWorkspace({
      vmStatus: "stopped",
      comments: [await storedComment(2)],
    });
    // Stopped: the read-only viewer, whose gutter starts a thread.
    const view = await renderFileTab();
    const user = userEvent.setup();
    await user.click(
      await screen.findByTestId("code-editor-toggle-comments-button"),
    );
    const thread = await screen.findByTestId("comment-thread-comment-1");
    await user.click(
      within(thread).getByTestId("comment-thread-toggle-expand"),
    );
    const reply = within(thread).getByPlaceholderText("Reply…");
    await user.type(reply, "Typed before the outage");
    expect(
      within(thread).getByTestId("comment-thread-resolve"),
    ).toBeInTheDocument();
    // A new thread being drafted on line 3 as well.
    // FILES draws read-only Monaco: its glyph margin starts a thread.
    await startThreadAt(3);
    const draft = await screen.findByLabelText("Comment");
    await user.type(draft, "New thread text");

    // The workspace detail cannot be refreshed: the page stays as it was
    // last loaded and says so; nothing typed is lost.
    server.use(
      http.get("/api/v1/workspaces/:id", () =>
        HttpResponse.json(
          {
            error: {
              code: "SERVICE_UNAVAILABLE",
              message: "The service is restarting",
              requestId: "request-down",
            },
          },
          { status: 503 },
        ),
      ),
    );
    await view.queryClient.invalidateQueries({ queryKey: ["workspace"] });
    expect(
      await screen.findByTestId("workspace-refresh-error"),
    ).toHaveTextContent("Can't refresh this workspace");
    expect(
      screen.queryByTestId("workspace-load-error-back-to-projects"),
    ).not.toBeInTheDocument();
    expect(screen.getByTestId("comment-thread-comment-1")).toBe(thread);
    expect(reply).toHaveValue("Typed before the outage");
    expect(screen.getByLabelText("Comment")).toBe(draft);
    expect(draft).toHaveValue("New thread text");
    // The controls are those of the workspace as last loaded; the server
    // decides each action.
    expect(
      within(thread).getByTestId("comment-thread-resolve"),
    ).toBeInTheDocument();

    // Back: the banner goes, and the draft is sent as typed.
    const served = serveWorkspace({
      vmStatus: "stopped",
      comments: [await storedComment(2)],
    });
    await view.queryClient.invalidateQueries({ queryKey: ["workspace"] });
    await waitFor(() =>
      expect(
        screen.queryByTestId("workspace-refresh-error"),
      ).not.toBeInTheDocument(),
    );
    expect(screen.getByTestId("comment-thread-comment-1")).toBe(thread);
    expect(reply).toHaveValue("Typed before the outage");
    await user.click(screen.getByTestId("draft-comment-save"));
    await waitFor(() => expect(served.created).toHaveLength(1));
    expect(served.created[0]).toMatchObject({
      content: "New thread text",
      lineNumber: 3,
    });
  });

  it("shows the load error when access to the workspace is refused on refresh", async () => {
    serveWorkspace({ comments: [await storedComment(2)] });
    const view = await renderFileTab();
    await screen.findByTestId("mock-monaco-editor");
    server.use(
      http.get("/api/v1/workspaces/:id", () =>
        HttpResponse.json(
          {
            error: {
              code: "FORBIDDEN",
              message: "You no longer have access to this workspace",
              requestId: "request-refused",
            },
          },
          { status: 403 },
        ),
      ),
    );
    await view.queryClient.invalidateQueries({ queryKey: ["workspace"] });
    expect(
      await screen.findByTestId("workspace-load-error-back-to-projects"),
    ).toBeInTheDocument();
    expect(screen.queryByTestId("mock-monaco-editor")).not.toBeInTheDocument();
  });

  it.each([
    { failure: "gets no answer", answer: () => HttpResponse.error() },
    {
      failure: "is cut off while it is read",
      answer: () =>
        new HttpResponse(
          new ReadableStream({
            start(controller) {
              controller.enqueue(new TextEncoder().encode('{"data":'));
              controller.error(new TypeError("terminated"));
            },
          }),
          { headers: { "Content-Type": "application/json" } },
        ),
    },
  ])(
    "keeps the file and its unsaved text when refreshing its content $failure",
    async ({ answer }) => {
      serveWorkspace({ comments: [await storedComment(2)] });
      const view = await renderFileTab();
      const editor = (await screen.findByTestId(
        "mock-monaco-editor",
      )) as HTMLTextAreaElement;
      await waitFor(() => expect(editor).toHaveValue(FILE_TEXT));
      fireEvent.click(await screen.findByTestId("file-view-edit"));
      fireEvent.change(editor, {
        target: { value: `${FILE_TEXT}unsaved();\n` },
      });

      // No usable answer from the server: the editor stays, with what was
      // typed.
      server.use(http.get("/api/v1/workspaces/:id/files/*", answer));
      await view.queryClient.invalidateQueries();
      expect(
        await screen.findByTestId("content-refresh-error"),
      ).toHaveTextContent("Could not refresh this file");
      expect(screen.getByTestId("mock-monaco-editor")).toBe(editor);
      expect(editor).toHaveValue(`${FILE_TEXT}unsaved();\n`);

      serveWorkspace({ comments: [await storedComment(2)] });
      await view.queryClient.invalidateQueries();
      await waitFor(() =>
        expect(
          screen.queryByTestId("content-refresh-error"),
        ).not.toBeInTheDocument(),
      );
      expect(screen.getByTestId("mock-monaco-editor")).toBe(editor);
    },
  );

  it("keeps the page when resolving its name again fails, and says not found once the name is gone", async () => {
    serveWorkspace({ comments: [await storedComment(2)] });
    const view = await renderFileTab();
    const editor = await screen.findByTestId("mock-monaco-editor");
    server.use(
      http.get("/api/v1/resolve", () =>
        HttpResponse.json(
          {
            error: {
              code: "SERVICE_UNAVAILABLE",
              message: "The service is restarting",
              requestId: "request-down",
            },
          },
          { status: 503 },
        ),
      ),
    );
    await view.queryClient.invalidateQueries({
      queryKey: ["resolve-workspace"],
    });
    await waitFor(
      () =>
        expect(
          view.queryClient.getQueryState([
            "resolve-workspace",
            "org-g5labs",
            WORKSPACE_ID,
          ])?.status,
        ).toBe("error"),
      { timeout: 4000 },
    );
    expect(
      screen.queryByTestId("workspace-not-found-back-to-projects"),
    ).not.toBeInTheDocument();
    expect(screen.getByTestId("mock-monaco-editor")).toBe(editor);
    expect(screen.getByTestId("workspace-refresh-error")).toBeInTheDocument();

    // The name resolves to nothing now: that is no outage.
    server.use(
      http.get("/api/v1/resolve", () =>
        HttpResponse.json({ data: { orgId: "org-g5labs" } }),
      ),
    );
    await view.queryClient.invalidateQueries({
      queryKey: ["resolve-workspace"],
    });
    expect(
      await screen.findByTestId(
        "workspace-not-found-back-to-projects",
        {},
        { timeout: 4000 },
      ),
    ).toBeInTheDocument();
  });

  it("keeps the editor and its unsaved text when a poll of the workspace's state fails", async () => {
    serveWorkspace({ comments: [await storedComment(2)] });
    const view = await renderFileTab();
    const editor = (await screen.findByTestId(
      "mock-monaco-editor",
    )) as HTMLTextAreaElement;
    await waitFor(() => expect(editor).toHaveValue(FILE_TEXT));
    fireEvent.click(await screen.findByTestId("file-view-edit"));
    fireEvent.change(editor, { target: { value: `${FILE_TEXT}unsaved();\n` } });

    server.use(
      http.get("/api/v1/workspaces/:id/vm", () =>
        HttpResponse.json(
          {
            error: {
              code: "SERVICE_UNAVAILABLE",
              message: "The service is restarting",
              requestId: "request-down",
            },
          },
          { status: 503 },
        ),
      ),
    );
    await view.queryClient.invalidateQueries({ queryKey: ["workspace-vm"] });
    await waitFor(() =>
      expect(
        view.queryClient.getQueryState(["workspace-vm", WORKSPACE_ID])?.status,
      ).toBe("error"),
    );
    // Not a stop: the editor, and what is typed in it, stay; the page says
    // the workspace is shown as last loaded.
    expect(screen.getByTestId("mock-monaco-editor")).toBe(editor);
    expect(editor).toHaveValue(`${FILE_TEXT}unsaved();\n`);
    expect(screen.getByTestId("workspace-refresh-error")).toBeInTheDocument();
  });

  it("keeps an in-file card, its reply being sent and the refusal, when the suggestion is decided meanwhile", async () => {
    const repoId = MOCK_WORKSPACE_DETAILS[WORKSPACE_ID].branches[0].repoId;
    const open: ReviewSuggestion = {
      id: "sug-1",
      publicationId: "pub-1",
      findingId: null,
      repoId,
      path: FILE_PATH,
      kind: "comment",
      beforeLines: "beta();\n",
      afterLines: null,
      anchorStart: 2,
      contentHash: null,
      anchorContext: null,
      authorKind: "human",
      authorLabel: "rita",
      note: "Is beta needed?",
      commentId: "thread-1",
      status: "open",
      statusBy: null,
      statusAt: null,
      supersedes: null,
      adoptedFrom: null,
      createdAt: "2026-09-30T10:00:00Z",
    };
    let current = open;
    const posted: unknown[] = [];
    let refuse!: () => void;
    const held = new Promise<void>((resolve) => {
      refuse = resolve;
    });
    // Stopped: the file shows in the read-only viewer, cards in place.
    serveWorkspace({ vmStatus: "stopped" });
    server.use(
      http.get("/api/v1/workspaces/:wsId/review/suggestions", () =>
        HttpResponse.json({ data: { suggestions: [current] } }),
      ),
      http.get("/api/v1/workspaces/:wsId/review/suggestions/:id/replies", () =>
        HttpResponse.json({ data: { replies: [] } }),
      ),
      http.post(
        "/api/v1/workspaces/:wsId/review/suggestions/:id/replies",
        async ({ request }) => {
          posted.push(await request.json());
          await held;
          return HttpResponse.json(
            {
              error: {
                code: "WORKSPACE_READ_ONLY",
                message: "The workspace became read-only",
                requestId: "request-refused",
              },
            },
            { status: 409 },
          );
        },
      ),
    );
    const view = await renderFileTab();
    const user = userEvent.setup();
    await user.click(
      await screen.findByTestId("code-editor-toggle-comments-button"),
    );
    const card = await screen.findByTestId("review-suggestion-sug-1");
    await user.click(
      within(card).getByTestId("review-suggestion-replies-toggle-sug-1"),
    );
    const reply = await within(card).findByTestId(
      "review-suggestion-reply-input-sug-1",
    );
    await user.type(reply, "Is it?{Enter}");
    await waitFor(() => expect(posted).toHaveLength(1));

    // Rejected by its owner meanwhile: the list no longer offers it as open,
    // but the card being answered stays.
    current = { ...open, status: "rejected" };
    await view.queryClient.invalidateQueries();
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 50));
    });
    expect(screen.getByTestId("review-suggestion-sug-1")).toBe(card);
    expect(
      within(card).getByTestId("review-suggestion-status-sug-1"),
    ).toHaveTextContent("Rejected");
    refuse();
    expect(
      await within(card).findByText("The workspace became read-only"),
    ).toBeInTheDocument();
    expect(reply).toHaveValue("Is it?");
    expect(posted).toHaveLength(1);

    // Its words cleared, the decided card leaves the file.
    await user.clear(reply);
    await waitFor(() =>
      expect(
        screen.queryByTestId("review-suggestion-sug-1"),
      ).not.toBeInTheDocument(),
    );
  });

  it("keeps a reader's reply being sent, and its refusal, when the owner deletes the thread", async () => {
    serveWorkspace({
      ownerUserId: "user-other",
      comments: [await storedComment(2)],
    });
    const posted: unknown[] = [];
    let refuse!: () => void;
    const held = new Promise<void>((resolve) => {
      refuse = resolve;
    });
    server.use(
      http.post("/api/v1/comments/:commentId/replies", async ({ request }) => {
        posted.push(await request.json());
        await held;
        return HttpResponse.json(
          {
            error: {
              code: "NOT_FOUND",
              message: "The thread no longer exists",
              requestId: "request-refused",
            },
          },
          { status: 404 },
        );
      }),
    );
    const view = await renderFileTab();
    const user = userEvent.setup();
    await user.click(
      await screen.findByTestId("code-editor-toggle-comments-button"),
    );
    const thread = await screen.findByTestId("comment-thread-comment-1");
    await user.click(
      within(thread).getByTestId("comment-thread-toggle-expand"),
    );
    const reply = within(thread).getByPlaceholderText("Reply…");
    await user.type(reply, "Yes, for the retry path{Enter}");
    await waitFor(() => expect(posted).toHaveLength(1));

    // The owner deletes it elsewhere: the list no longer holds it, as after
    // `comment.deleted` (`ws.ts`).
    server.use(
      http.get("/api/v1/workspaces/:wsId/comments", () =>
        HttpResponse.json({ data: [] }),
      ),
    );
    view.queryClient.removeQueries({
      queryKey: ["comment-replies", "comment-1"],
      exact: true,
    });
    await view.queryClient.invalidateQueries();
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 50));
    });
    expect(screen.getByTestId("comment-thread-comment-1")).toBe(thread);
    expect(
      within(thread).getByTestId("comment-thread-deleted-label"),
    ).toBeInTheDocument();

    refuse();
    expect(
      await within(thread).findByText("The thread no longer exists"),
    ).toBeInTheDocument();
    expect(
      within(thread).getByTestId("comment-thread-deleted"),
    ).toHaveTextContent("This thread was deleted");
    expect(reply).toHaveValue("Yes, for the retry path");
    expect(
      within(thread).getByTestId("comment-thread-send-reply"),
    ).toBeDisabled();
    expect(posted).toHaveLength(1);

    // Dismissed, the deleted thread leaves.
    await user.click(within(thread).getByTestId("comment-thread-dismiss"));
    await waitFor(() =>
      expect(screen.queryByTestId("comment-thread-comment-1")).toBeNull(),
    );
  });

  it.each([
    { reason: "being sent", sent: true },
    { reason: "typed, not yet sent", sent: false },
  ])(
    "keeps an in-file card and its rejection reason $reason, with no Reject left, when the suggestion is withdrawn meanwhile",
    async ({ sent }) => {
      const repoId = MOCK_WORKSPACE_DETAILS[WORKSPACE_ID].branches[0].repoId;
      const open: ReviewSuggestion = {
        id: "sug-1",
        publicationId: "pub-1",
        findingId: null,
        repoId,
        path: FILE_PATH,
        kind: "comment",
        beforeLines: "beta();\n",
        afterLines: null,
        anchorStart: 2,
        contentHash: null,
        anchorContext: null,
        authorKind: "human",
        authorLabel: "rita",
        note: "Is beta needed?",
        commentId: "thread-1",
        status: "open",
        statusBy: null,
        statusAt: null,
        supersedes: null,
        adoptedFrom: null,
        createdAt: "2026-09-30T10:00:00Z",
      };
      let current = open;
      const posted: unknown[] = [];
      let refuse!: () => void;
      const held = new Promise<void>((resolve) => {
        refuse = resolve;
      });
      serveWorkspace({ vmStatus: "stopped" });
      server.use(
        http.get("/api/v1/workspaces/:wsId/review/suggestions", () =>
          HttpResponse.json({ data: { suggestions: [current] } }),
        ),
        http.post(
          "/api/v1/workspaces/:wsId/review/suggestions/:id/reject",
          async ({ request }) => {
            posted.push(await request.json());
            await held;
            return HttpResponse.json(
              {
                error: {
                  code: "SUGGESTION_DECIDED",
                  message: "The suggestion was withdrawn",
                  requestId: "request-refused",
                },
              },
              { status: 409 },
            );
          },
        ),
      );
      const view = await renderFileTab();
      const user = userEvent.setup();
      await user.click(
        await screen.findByTestId("code-editor-toggle-comments-button"),
      );
      const card = await screen.findByTestId("review-suggestion-sug-1");
      await user.click(
        within(card).getByTestId("review-suggestion-reject-sug-1"),
      );
      const reason = within(card).getByTestId(
        "review-suggestion-reject-reason-sug-1",
      );
      await user.type(reason, "Covered by the retry");
      if (sent) {
        await user.click(
          within(card).getByTestId("review-suggestion-reject-confirm-sug-1"),
        );
        await waitFor(() => expect(posted).toHaveLength(1));
      }

      // Withdrawn by its reviewer meanwhile: the card being decided stays.
      current = { ...open, status: "withdrawn" };
      await view.queryClient.invalidateQueries();
      await act(async () => {
        await new Promise((resolve) => setTimeout(resolve, 50));
      });
      expect(screen.getByTestId("review-suggestion-sug-1")).toBe(card);
      if (sent) {
        refuse();
        expect(
          await within(card).findByText(/The suggestion was withdrawn/),
        ).toBeInTheDocument();
      }
      expect(
        within(card).getByTestId("review-suggestion-status-sug-1"),
      ).toHaveTextContent("Withdrawn");
      expect(reason).toHaveValue("Covered by the retry");
      expect(
        within(card).getByTestId("review-suggestion-reject-confirm-sug-1"),
      ).toBeDisabled();
      expect(posted).toHaveLength(sent ? 1 : 0);

      // Cancelled, the decided card leaves the file.
      await user.click(
        within(card).getByTestId("review-suggestion-reject-cancel-sug-1"),
      );
      await waitFor(() =>
        expect(screen.queryByTestId("review-suggestion-sug-1")).toBeNull(),
      );
    },
  );

  it("keeps an in-file card being answered, with its refusal, after a commit takes it out of the current list", async () => {
    const repoId = MOCK_WORKSPACE_DETAILS[WORKSPACE_ID].branches[0].repoId;
    const card = (id: string, line: number, before: string) =>
      ({
        id,
        publicationId: "pub-1",
        findingId: null,
        repoId,
        path: FILE_PATH,
        kind: "comment",
        beforeLines: before,
        afterLines: null,
        anchorStart: line,
        contentHash: null,
        anchorContext: null,
        authorKind: "human",
        authorLabel: "rita",
        note: `About line ${line}`,
        commentId: `thread-${id}`,
        status: "open",
        statusBy: null,
        statusAt: null,
        supersedes: null,
        adoptedFrom: null,
        createdAt: "2026-09-30T10:00:00Z",
      }) satisfies ReviewSuggestion;
    let current = [
      card("sug-1", 2, "beta();\n"),
      card("sug-2", 3, "target();\n"),
    ];
    const posted: unknown[] = [];
    let refuse!: () => void;
    const held = new Promise<void>((resolve) => {
      refuse = resolve;
    });
    serveWorkspace({ vmStatus: "stopped" });
    server.use(
      http.get("/api/v1/workspaces/:wsId/review/suggestions", () =>
        HttpResponse.json({ data: { suggestions: current } }),
      ),
      http.get("/api/v1/workspaces/:wsId/review/suggestions/:id/replies", () =>
        HttpResponse.json({ data: { replies: [] } }),
      ),
      http.post(
        "/api/v1/workspaces/:wsId/review/suggestions/:id/replies",
        async ({ request }) => {
          posted.push(await request.json());
          await held;
          return HttpResponse.json(
            {
              error: {
                code: "SUGGESTION_OUTDATED",
                message: "The suggestion is about an earlier commit",
                requestId: "request-refused",
              },
            },
            { status: 409 },
          );
        },
      ),
    );
    const view = await renderFileTab();
    const user = userEvent.setup();
    await user.click(
      await screen.findByTestId("code-editor-toggle-comments-button"),
    );
    const answered = await screen.findByTestId("review-suggestion-sug-1");
    expect(screen.getByTestId("review-suggestion-sug-2")).toBeInTheDocument();
    await user.click(
      within(answered).getByTestId("review-suggestion-replies-toggle-sug-1"),
    );
    const reply = await within(answered).findByTestId(
      "review-suggestion-reply-input-sug-1",
    );
    await user.type(reply, "Done in the retry{Enter}");
    await waitFor(() => expect(posted).toHaveLength(1));

    // The owner commits: neither row is about the current commit any more.
    current = [];
    await view.queryClient.invalidateQueries();
    await waitFor(() =>
      expect(screen.queryByTestId("review-suggestion-sug-2")).toBeNull(),
    );
    const kept = screen.getByTestId("review-suggestion-sug-1");
    expect(
      within(kept).getByTestId("review-suggestion-outdated-sug-1"),
    ).toHaveTextContent("From an earlier commit");
    refuse();
    expect(
      await within(kept).findByText(
        "The suggestion is about an earlier commit",
      ),
    ).toBeInTheDocument();
    expect(
      within(kept).getByTestId("review-suggestion-reply-input-sug-1"),
    ).toHaveValue("Done in the retry");
    expect(posted).toHaveLength(1);

    // Its words cleared, the card from the earlier commit leaves the file.
    await user.clear(
      within(kept).getByTestId("review-suggestion-reply-input-sug-1"),
    );
    await waitFor(() =>
      expect(screen.queryByTestId("review-suggestion-sug-1")).toBeNull(),
    );
  });

  it("keeps an in-file card's edited replacement, read-only, when the suggestion is withdrawn before it is accepted", async () => {
    const repoId = MOCK_WORKSPACE_DETAILS[WORKSPACE_ID].branches[0].repoId;
    const open: ReviewSuggestion = {
      id: "sug-1",
      publicationId: "pub-1",
      findingId: null,
      repoId,
      path: FILE_PATH,
      kind: "hunk",
      beforeLines: "beta();\n",
      afterLines: "gamma();\n",
      anchorStart: 2,
      contentHash: null,
      anchorContext: null,
      authorKind: "human",
      authorLabel: "rita",
      note: "Rename it.",
      commentId: "thread-1",
      status: "open",
      statusBy: null,
      statusAt: null,
      supersedes: null,
      adoptedFrom: null,
      createdAt: "2026-09-30T10:00:00Z",
    };
    let current = open;
    serveWorkspace({ vmStatus: "stopped" });
    server.use(
      http.get("/api/v1/workspaces/:wsId/review/suggestions", () =>
        HttpResponse.json({ data: { suggestions: [current] } }),
      ),
    );
    const view = await renderFileTab();
    const user = userEvent.setup();
    await user.click(
      await screen.findByTestId("code-editor-toggle-comments-button"),
    );
    const card = await screen.findByTestId("review-suggestion-sug-1");
    await user.click(
      within(card).getByTestId("review-suggestion-edit-toggle-sug-1"),
    );
    const edit = within(card).getByTestId("review-suggestion-edit-sug-1");
    await user.clear(edit);
    await user.type(edit, "delta();");

    // Withdrawn by its reviewer meanwhile: the edit stays readable.
    current = { ...open, status: "withdrawn" };
    await view.queryClient.invalidateQueries();
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 50));
    });
    expect(screen.getByTestId("review-suggestion-sug-1")).toBe(card);
    const kept = within(card).getByTestId("review-suggestion-edit-sug-1");
    expect(kept).toHaveValue("delta();");
    expect(kept).toHaveAttribute("readonly");
    expect(
      within(card).getByTestId("review-suggestion-edit-closed-sug-1"),
    ).toHaveTextContent("can no longer be accepted");
    expect(
      within(card).queryByTestId("review-suggestion-accept-sug-1"),
    ).toBeNull();

    // Dismissed, the decided card leaves the file.
    await user.click(
      within(card).getByTestId("review-suggestion-edit-dismiss-sug-1"),
    );
    await waitFor(() =>
      expect(screen.queryByTestId("review-suggestion-sug-1")).toBeNull(),
    );
  });

  it("keeps a reply being sent, its refusal and an open draft through a switch to Preview and back to Source", async () => {
    const htmlPath = "docs/page.html";
    const html = "<h1>Title</h1>\n<p>Second line</p>\n<p>Third</p>\n";
    const anchor = await createCommentAnchor(html, 2);
    serveWorkspace({
      vmStatus: "stopped",
      comments: [
        {
          ...(await storedComment(2)),
          filePath: htmlPath,
          lineHash: anchor.lineHash,
          contentHash: anchor.contentHash,
          anchorContext: anchor.anchorContext,
        },
      ],
    });
    const posted: unknown[] = [];
    let refuse!: () => void;
    const held = new Promise<void>((resolve) => {
      refuse = resolve;
    });
    server.use(
      http.get("/api/v1/workspaces/:id/files/*", () =>
        HttpResponse.json({
          data: {
            path: htmlPath,
            content: html,
            size: html.length,
            encoding: "utf-8",
            hash: "file-v1",
            source: "committed",
            repoOid: "oid-base",
            snapshotAt: null,
          },
        }),
      ),
      http.post("/api/v1/comments/:commentId/replies", async ({ request }) => {
        posted.push(await request.json());
        await held;
        return HttpResponse.json(
          {
            error: {
              code: "WORKSPACE_READ_ONLY",
              message: "The workspace became read-only",
              requestId: "request-refused",
            },
          },
          { status: 409 },
        );
      }),
    );
    localStorage.setItem(
      `g5:tabs:${WORKSPACE_ID}`,
      JSON.stringify({
        tabs: [{ type: "file", filePath: htmlPath, name: "page.html" }],
        activeTabId: `file:${htmlPath}`,
      }),
    );
    renderApp(
      <Routes>
        <Route path="/:org/:workspace/*" element={<WorkspacePage />} />
      </Routes>,
      { route: `/org-g5labs/${WORKSPACE_ID}`, user: createMockUser() },
    );
    fireEvent.click(await screen.findByRole("tab", { name: /page\.html/ }));
    const user = userEvent.setup();
    await user.click(await screen.findByTestId("file-view-switch-mode"));
    await user.click(
      await screen.findByTestId("code-editor-toggle-comments-button"),
    );
    const thread = await screen.findByTestId("comment-thread-comment-1");
    await user.click(
      within(thread).getByTestId("comment-thread-toggle-expand"),
    );
    await user.type(
      within(thread).getByPlaceholderText("Reply…"),
      "Keep it{Enter}",
    );
    await waitFor(() => expect(posted).toHaveLength(1));
    // A new thread on line 3, typed and not sent.
    // FILES draws read-only Monaco: its glyph margin starts a thread.
    await startThreadAt(3);
    await user.type(await screen.findByLabelText("Comment"), "Shorter?");

    // The preview keeps the native model and shows the same discussion in
    // its list, the open draft with its words included: it is the file's
    // one draft in either mode.
    await user.click(screen.getByTestId("file-view-switch-mode"));
    expect(await screen.findByTestId("file-view-preview")).toBeVisible();
    const previewPanel = await screen.findByTestId("file-view-preview-threads");
    expect(within(previewPanel).getByLabelText("Comment")).toHaveValue(
      "Shorter?",
    );
    await user.click(screen.getByTestId("file-view-switch-mode"));
    expect(await screen.findByLabelText("Comment")).toHaveValue("Shorter?");
    const again = await screen.findByTestId("comment-thread-comment-1");
    if (!within(again).queryByPlaceholderText("Reply…")) {
      await user.click(
        within(again).getByTestId("comment-thread-toggle-expand"),
      );
    }
    const reply = within(again).getByPlaceholderText("Reply…");
    expect(reply).toHaveValue("Keep it");
    // Still being sent: a second Enter sends nothing.
    fireEvent.keyDown(reply, { key: "Enter" });
    refuse();
    expect(
      await within(again).findByText("The workspace became read-only"),
    ).toBeInTheDocument();
    expect(reply).toHaveValue("Keep it");
    expect(posted).toHaveLength(1);
    expect(screen.getByLabelText("Comment")).toHaveValue("Shorter?");

    // Cancelled, the draft's words go with it: the next draft starts empty.
    await user.click(screen.getByTestId("draft-comment-cancel"));
    // FILES draws read-only Monaco: its glyph margin starts a thread.
    await startThreadAt(3);
    expect(await screen.findByLabelText("Comment")).toHaveValue("");
  });

  it("keeps an in-file card whose resolve is being sent, and its refusal, when the suggestion is withdrawn meanwhile", async () => {
    const repoId = MOCK_WORKSPACE_DETAILS[WORKSPACE_ID].branches[0].repoId;
    const open: ReviewSuggestion = {
      id: "sug-1",
      publicationId: "pub-1",
      findingId: null,
      repoId,
      path: FILE_PATH,
      kind: "comment",
      beforeLines: "beta();\n",
      afterLines: null,
      anchorStart: 2,
      contentHash: null,
      anchorContext: null,
      authorKind: "human",
      authorLabel: "rita",
      note: "Is beta needed?",
      commentId: "thread-1",
      status: "open",
      statusBy: null,
      statusAt: null,
      supersedes: null,
      adoptedFrom: null,
      createdAt: "2026-09-30T10:00:00Z",
    };
    let current = open;
    const posted: string[] = [];
    let refuse!: () => void;
    const held = new Promise<void>((resolve) => {
      refuse = resolve;
    });
    serveWorkspace({ vmStatus: "stopped" });
    server.use(
      http.get("/api/v1/workspaces/:wsId/review/suggestions", () =>
        HttpResponse.json({ data: { suggestions: [current] } }),
      ),
      http.post(
        "/api/v1/workspaces/:wsId/review/suggestions/:id/resolve",
        async ({ params }) => {
          posted.push(String(params.id));
          await held;
          return HttpResponse.json(
            {
              error: {
                code: "SUGGESTION_DECIDED",
                message: "The suggestion was withdrawn",
                requestId: "request-refused",
              },
            },
            { status: 409 },
          );
        },
      ),
    );
    const view = await renderFileTab();
    const user = userEvent.setup();
    await user.click(
      await screen.findByTestId("code-editor-toggle-comments-button"),
    );
    const card = await screen.findByTestId("review-suggestion-sug-1");
    await user.click(
      within(card).getByTestId("review-suggestion-resolve-sug-1"),
    );
    await waitFor(() => expect(posted).toEqual(["sug-1"]));

    // Withdrawn meanwhile: the card being decided stays, with nothing typed.
    current = { ...open, status: "withdrawn" };
    await view.queryClient.invalidateQueries();
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 50));
    });
    expect(screen.getByTestId("review-suggestion-sug-1")).toBe(card);
    refuse();
    expect(
      await within(card).findByText(/The suggestion was withdrawn/),
    ).toBeInTheDocument();
    // The refusal alone keeps it until dismissed.
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 50));
    });
    expect(screen.getByTestId("review-suggestion-sug-1")).toBe(card);
    await user.click(
      within(card).getByTestId("review-suggestion-error-dismiss-sug-1"),
    );
    await waitFor(() =>
      expect(screen.queryByTestId("review-suggestion-sug-1")).toBeNull(),
    );
  });

  it("keeps a deleted thread with its reply's refusal after a tab switch while it was sent", async () => {
    serveWorkspace({
      ownerUserId: "user-other",
      comments: [await storedComment(2)],
    });
    const posted: unknown[] = [];
    let refuse!: () => void;
    const held = new Promise<void>((resolve) => {
      refuse = resolve;
    });
    server.use(
      http.post("/api/v1/comments/:commentId/replies", async ({ request }) => {
        posted.push(await request.json());
        await held;
        return HttpResponse.json(
          {
            error: {
              code: "NOT_FOUND",
              message: "The thread no longer exists",
              requestId: "request-refused",
            },
          },
          { status: 404 },
        );
      }),
    );
    const view = await renderFileTabBesideAnother();
    const user = userEvent.setup();
    await user.click(
      await screen.findByTestId("code-editor-toggle-comments-button"),
    );
    const thread = await screen.findByTestId("comment-thread-comment-1");
    await user.click(
      within(thread).getByTestId("comment-thread-toggle-expand"),
    );
    await user.type(
      within(thread).getByPlaceholderText("Reply…"),
      "Yes, for the retry path{Enter}",
    );
    await waitFor(() => expect(posted).toHaveLength(1));
    server.use(
      http.get("/api/v1/workspaces/:wsId/comments", () =>
        HttpResponse.json({ data: [] }),
      ),
    );
    view.queryClient.removeQueries({
      queryKey: ["comment-replies", "comment-1"],
      exact: true,
    });
    await view.queryClient.invalidateQueries();
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 50));
    });
    expect(
      within(screen.getByTestId("comment-thread-comment-1")).getByTestId(
        "comment-thread-deleted-label",
      ),
    ).toBeInTheDocument();

    // Away on another tab while the refusal arrives, then back.
    await switchAwayAndBack("comment-thread-comment-1", refuse);
    const again = await screen.findByTestId("comment-thread-comment-1");
    expect(
      within(again).getByTestId("comment-thread-deleted-label"),
    ).toBeInTheDocument();
    if (!within(again).queryByPlaceholderText("Reply…")) {
      await user.click(
        within(again).getByTestId("comment-thread-toggle-expand"),
      );
    }
    expect(
      await within(again).findByText("The thread no longer exists"),
    ).toBeInTheDocument();
    expect(within(again).getByPlaceholderText("Reply…")).toHaveValue(
      "Yes, for the retry path",
    );
    expect(posted).toHaveLength(1);
    await user.click(within(again).getByTestId("comment-thread-dismiss"));
    await waitFor(() =>
      expect(screen.queryByTestId("comment-thread-comment-1")).toBeNull(),
    );
  });

  it("keeps a card a commit took out of the list, with its reply's refusal, after a tab switch", async () => {
    const repoId = MOCK_WORKSPACE_DETAILS[WORKSPACE_ID].branches[0].repoId;
    const open: ReviewSuggestion = {
      id: "sug-1",
      publicationId: "pub-1",
      findingId: null,
      repoId,
      path: FILE_PATH,
      kind: "comment",
      beforeLines: "beta();\n",
      afterLines: null,
      anchorStart: 2,
      contentHash: null,
      anchorContext: null,
      authorKind: "human",
      authorLabel: "rita",
      note: "Is beta needed?",
      commentId: "thread-1",
      status: "open",
      statusBy: null,
      statusAt: null,
      supersedes: null,
      adoptedFrom: null,
      createdAt: "2026-09-30T10:00:00Z",
    };
    let current: ReviewSuggestion[] = [open];
    const posted: unknown[] = [];
    let refuse!: () => void;
    const held = new Promise<void>((resolve) => {
      refuse = resolve;
    });
    serveWorkspace({ vmStatus: "stopped" });
    server.use(
      http.get("/api/v1/workspaces/:wsId/review/suggestions", () =>
        HttpResponse.json({ data: { suggestions: current } }),
      ),
      http.get("/api/v1/workspaces/:wsId/review/suggestions/:id/replies", () =>
        HttpResponse.json({ data: { replies: [] } }),
      ),
      http.post(
        "/api/v1/workspaces/:wsId/review/suggestions/:id/replies",
        async ({ request }) => {
          posted.push(await request.json());
          await held;
          return HttpResponse.json(
            {
              error: {
                code: "SUGGESTION_OUTDATED",
                message: "The suggestion is about an earlier commit",
                requestId: "request-refused",
              },
            },
            { status: 409 },
          );
        },
      ),
    );
    const view = await renderFileTabBesideAnother();
    const user = userEvent.setup();
    await user.click(
      await screen.findByTestId("code-editor-toggle-comments-button"),
    );
    const card = await screen.findByTestId("review-suggestion-sug-1");
    await user.click(
      within(card).getByTestId("review-suggestion-replies-toggle-sug-1"),
    );
    await user.type(
      await within(card).findByTestId("review-suggestion-reply-input-sug-1"),
      "Done in the retry{Enter}",
    );
    await waitFor(() => expect(posted).toHaveLength(1));
    current = [];
    await view.queryClient.invalidateQueries();
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 50));
    });

    await switchAwayAndBack("review-suggestion-sug-1", refuse);
    // The card is first laid out in the read-only zone list and moves into
    // the editor's zones once Monaco is up: wait for its settled host, and
    // query it afresh each time.
    const again = () => screen.getByTestId("review-suggestion-sug-1");
    await waitFor(() =>
      expect(
        again().closest('[data-testid="mock-monaco-zones"]'),
      ).not.toBeNull(),
    );
    expect(
      within(again()).getByTestId("review-suggestion-outdated-sug-1"),
    ).toHaveTextContent("From an earlier commit");
    if (!within(again()).queryByTestId("review-suggestion-reply-input-sug-1")) {
      await user.click(
        within(again()).getByTestId("review-suggestion-replies-toggle-sug-1"),
      );
    }
    await waitFor(() =>
      expect(
        within(again()).getByText("The suggestion is about an earlier commit"),
      ).toBeInTheDocument(),
    );
    expect(
      within(again()).getByTestId("review-suggestion-reply-input-sug-1"),
    ).toHaveValue("Done in the retry");
    expect(posted).toHaveLength(1);
  });

  it("locks a rejection reason while it is sent, so later typing cannot be lost", async () => {
    const repoId = MOCK_WORKSPACE_DETAILS[WORKSPACE_ID].branches[0].repoId;
    const open: ReviewSuggestion = {
      id: "sug-1",
      publicationId: "pub-1",
      findingId: null,
      repoId,
      path: FILE_PATH,
      kind: "comment",
      beforeLines: "beta();\n",
      afterLines: null,
      anchorStart: 2,
      contentHash: null,
      anchorContext: null,
      authorKind: "human",
      authorLabel: "rita",
      note: "Is beta needed?",
      commentId: "thread-1",
      status: "open",
      statusBy: null,
      statusAt: null,
      supersedes: null,
      adoptedFrom: null,
      createdAt: "2026-09-30T10:00:00Z",
    };
    const posted: unknown[] = [];
    let accept!: () => void;
    const held = new Promise<void>((resolve) => {
      accept = resolve;
    });
    serveWorkspace({ vmStatus: "stopped" });
    server.use(
      http.get("/api/v1/workspaces/:wsId/review/suggestions", () =>
        HttpResponse.json({ data: { suggestions: [open] } }),
      ),
      http.post(
        "/api/v1/workspaces/:wsId/review/suggestions/:id/reject",
        async ({ request }) => {
          posted.push(await request.json());
          await held;
          return HttpResponse.json({
            data: {
              suggestion: { ...open, status: "rejected" },
              writtenHash: null,
              alreadyApplied: false,
            },
          });
        },
      ),
    );
    await renderFileTab();
    const user = userEvent.setup();
    await user.click(
      await screen.findByTestId("code-editor-toggle-comments-button"),
    );
    const card = await screen.findByTestId("review-suggestion-sug-1");
    await user.click(
      within(card).getByTestId("review-suggestion-reject-sug-1"),
    );
    const reason = within(card).getByTestId(
      "review-suggestion-reject-reason-sug-1",
    );
    await user.type(reason, "Covered");
    await user.click(
      within(card).getByTestId("review-suggestion-reject-confirm-sug-1"),
    );
    await waitFor(() => expect(posted).toEqual([{ reason: "Covered" }]));
    expect(reason).toHaveAttribute("readonly");
    await user.type(reason, " and more");
    expect(reason).toHaveValue("Covered");
    accept();
    // Decided, with nothing more held: the card leaves the file at once.
    await waitFor(() =>
      expect(screen.queryByTestId("review-suggestion-sug-1")).toBeNull(),
    );
    expect(
      screen.queryByTestId("review-suggestion-reject-reason-sug-1"),
    ).toBeNull();
    expect(posted).toHaveLength(1);
  });

  it("lets a cancelled rejection reason go, so a withdrawn card leaves the file", async () => {
    const repoId = MOCK_WORKSPACE_DETAILS[WORKSPACE_ID].branches[0].repoId;
    const open: ReviewSuggestion = {
      id: "sug-1",
      publicationId: "pub-1",
      findingId: null,
      repoId,
      path: FILE_PATH,
      kind: "comment",
      beforeLines: "beta();\n",
      afterLines: null,
      anchorStart: 2,
      contentHash: null,
      anchorContext: null,
      authorKind: "human",
      authorLabel: "rita",
      note: "Is beta needed?",
      commentId: "thread-1",
      status: "open",
      statusBy: null,
      statusAt: null,
      supersedes: null,
      adoptedFrom: null,
      createdAt: "2026-09-30T10:00:00Z",
    };
    let current = open;
    serveWorkspace({ vmStatus: "stopped" });
    server.use(
      http.get("/api/v1/workspaces/:wsId/review/suggestions", () =>
        HttpResponse.json({ data: { suggestions: [current] } }),
      ),
    );
    const view = await renderFileTab();
    const user = userEvent.setup();
    await user.click(
      await screen.findByTestId("code-editor-toggle-comments-button"),
    );
    const card = await screen.findByTestId("review-suggestion-sug-1");
    await user.click(
      within(card).getByTestId("review-suggestion-reject-sug-1"),
    );
    await user.type(
      within(card).getByTestId("review-suggestion-reject-reason-sug-1"),
      "Not needed",
    );
    await user.click(
      within(card).getByTestId("review-suggestion-reject-cancel-sug-1"),
    );

    current = { ...open, status: "withdrawn" };
    await view.queryClient.invalidateQueries();
    await waitFor(() =>
      expect(screen.queryByTestId("review-suggestion-sug-1")).toBeNull(),
    );
  });

  it.each([
    { by: "here", decision: "rejected", status: "Rejected" },
    { by: "elsewhere", decision: "withdrawn", status: "Withdrawn" },
  ])(
    "shows a decision made $by on a card kept from an earlier commit, keeping its typed reply",
    async ({ by, decision, status }) => {
      const repoId = MOCK_WORKSPACE_DETAILS[WORKSPACE_ID].branches[0].repoId;
      const open: ReviewSuggestion = {
        id: "sug-1",
        publicationId: "pub-1",
        findingId: null,
        repoId,
        path: FILE_PATH,
        kind: "comment",
        beforeLines: "beta();\n",
        afterLines: null,
        anchorStart: 2,
        contentHash: null,
        anchorContext: null,
        authorKind: "human",
        authorLabel: "rita",
        note: "Is beta needed?",
        commentId: "thread-1",
        status: "open",
        statusBy: null,
        statusAt: null,
        supersedes: null,
        adoptedFrom: null,
        createdAt: "2026-09-30T10:00:00Z",
      };
      // The current commit's list, and every round's.
      let current: ReviewSuggestion[] = [open];
      let everyRound: ReviewSuggestion[] = [open];
      // Once decided here, every round's list stops answering: the card
      // shows the decision from its own answer.
      let listsHang = false;
      serveWorkspace({ vmStatus: "stopped" });
      server.use(
        http.get(
          "/api/v1/workspaces/:wsId/review/suggestions",
          async ({ request }) => {
            const currentOnly =
              new URL(request.url).searchParams.get("currentOnly") === "true";
            if (listsHang && !currentOnly) await new Promise(() => {});
            return HttpResponse.json({
              data: { suggestions: currentOnly ? current : everyRound },
            });
          },
        ),
        http.get(
          "/api/v1/workspaces/:wsId/review/suggestions/:id/replies",
          () => HttpResponse.json({ data: { replies: [] } }),
        ),
        http.post(
          "/api/v1/workspaces/:wsId/review/suggestions/:id/reject",
          () => {
            listsHang = true;
            return HttpResponse.json({
              data: {
                suggestion: { ...open, status: "rejected" },
                writtenHash: null,
                alreadyApplied: false,
              },
            });
          },
        ),
      );
      const view = await renderFileTab();
      const user = userEvent.setup();
      await user.click(
        await screen.findByTestId("code-editor-toggle-comments-button"),
      );
      const card = await screen.findByTestId("review-suggestion-sug-1");
      await user.click(
        within(card).getByTestId("review-suggestion-replies-toggle-sug-1"),
      );
      await user.type(
        await within(card).findByTestId("review-suggestion-reply-input-sug-1"),
        "Still needed?",
      );
      // A commit takes it out of the current list.
      current = [];
      await view.queryClient.invalidateQueries();
      await waitFor(() =>
        expect(
          within(screen.getByTestId("review-suggestion-sug-1")).getByTestId(
            "review-suggestion-outdated-sug-1",
          ),
        ).toBeInTheDocument(),
      );
      const kept = screen.getByTestId("review-suggestion-sug-1");
      if (by === "here") {
        await user.click(
          within(kept).getByTestId("review-suggestion-reject-sug-1"),
        );
        await user.click(
          within(kept).getByTestId("review-suggestion-reject-confirm-sug-1"),
        );
      } else {
        everyRound = [
          { ...open, status: decision as ReviewSuggestion["status"] },
        ];
        await view.queryClient.invalidateQueries();
      }
      await waitFor(() =>
        expect(
          within(screen.getByTestId("review-suggestion-sug-1")).getByTestId(
            "review-suggestion-status-sug-1",
          ),
        ).toHaveTextContent(status),
      );
      const decided = screen.getByTestId("review-suggestion-sug-1");
      expect(
        within(decided).queryByTestId("review-suggestion-reject-sug-1"),
      ).toBeNull();
      expect(
        within(decided).queryByTestId("review-suggestion-resolve-sug-1"),
      ).toBeNull();
      expect(
        within(decided).getByTestId("review-suggestion-reply-input-sug-1"),
      ).toHaveValue("Still needed?");
    },
  );

  it.each([
    { what: "a thread", thread: true },
    { what: "a suggestion card", thread: false },
  ])(
    "keeps the words of $what's reply readable when the workspace turns read-only while it is sent",
    async ({ thread }) => {
      const repoId = MOCK_WORKSPACE_DETAILS[WORKSPACE_ID].branches[0].repoId;
      const card: ReviewSuggestion = {
        id: "sug-1",
        publicationId: "pub-1",
        findingId: null,
        repoId,
        path: FILE_PATH,
        kind: "comment",
        beforeLines: "beta();\n",
        afterLines: null,
        anchorStart: 2,
        contentHash: null,
        anchorContext: null,
        authorKind: "human",
        authorLabel: "rita",
        note: "Is beta needed?",
        commentId: "thread-1",
        status: "open",
        statusBy: null,
        statusAt: null,
        supersedes: null,
        adoptedFrom: null,
        createdAt: "2026-09-30T10:00:00Z",
      };
      const posted: unknown[] = [];
      let refuse!: () => void;
      const held = new Promise<void>((resolve) => {
        refuse = resolve;
      });
      const refusal = () =>
        HttpResponse.json(
          {
            error: {
              code: "WORKSPACE_READ_ONLY",
              message: "The workspace became read-only",
              requestId: "request-refused",
            },
          },
          { status: 409 },
        );
      serveWorkspace({
        vmStatus: "stopped",
        ownerUserId: "user-other",
        comments: thread ? [await storedComment(2)] : [],
      });
      server.use(
        http.get("/api/v1/workspaces/:wsId/review/suggestions", () =>
          HttpResponse.json({ data: { suggestions: thread ? [] : [card] } }),
        ),
        http.get(
          "/api/v1/workspaces/:wsId/review/suggestions/:id/replies",
          () => HttpResponse.json({ data: { replies: [] } }),
        ),
        http.post(
          "/api/v1/workspaces/:wsId/review/suggestions/:id/replies",
          async ({ request }) => {
            posted.push(await request.json());
            await held;
            return refusal();
          },
        ),
        http.post(
          "/api/v1/comments/:commentId/replies",
          async ({ request }) => {
            posted.push(await request.json());
            await held;
            return refusal();
          },
        ),
      );
      const view = await renderFileTab();
      const user = userEvent.setup();
      await user.click(
        await screen.findByTestId("code-editor-toggle-comments-button"),
      );
      if (thread) {
        const shown = await screen.findByTestId("comment-thread-comment-1");
        await user.click(
          within(shown).getByTestId("comment-thread-toggle-expand"),
        );
        await user.type(
          within(shown).getByPlaceholderText("Reply…"),
          "Yes, for the retry path{Enter}",
        );
      } else {
        const shown = await screen.findByTestId("review-suggestion-sug-1");
        await user.click(
          within(shown).getByTestId("review-suggestion-replies-toggle-sug-1"),
        );
        await user.type(
          await within(shown).findByTestId(
            "review-suggestion-reply-input-sug-1",
          ),
          "Yes, for the retry path{Enter}",
        );
      }
      await waitFor(() => expect(posted).toHaveLength(1));

      // The workspace turns read-only, then the refusal arrives.
      server.use(
        http.get("/api/v1/workspaces/:id", () =>
          HttpResponse.json({
            data: {
              ...MOCK_WORKSPACE_DETAILS[WORKSPACE_ID],
              ownerUserId: "user-other",
              readOnly: true,
            },
          }),
        ),
      );
      await view.queryClient.invalidateQueries();
      await act(async () => {
        await new Promise((resolve) => setTimeout(resolve, 50));
      });
      refuse();
      if (thread) {
        const shown = screen.getByTestId("comment-thread-comment-1");
        expect(
          await within(shown).findByText("The workspace became read-only"),
        ).toBeInTheDocument();
        expect(
          within(shown).getByTestId("comment-thread-kept-reply"),
        ).toHaveValue("Yes, for the retry path");
        expect(within(shown).queryByPlaceholderText("Reply…")).toBeNull();
        await user.click(within(shown).getByTestId("comment-thread-dismiss"));
        expect(
          within(shown).queryByTestId("comment-thread-kept-reply"),
        ).toBeNull();
      } else {
        const shown = screen.getByTestId("review-suggestion-sug-1");
        expect(
          await within(shown).findByText("The workspace became read-only"),
        ).toBeInTheDocument();
        expect(
          within(shown).getByTestId("review-suggestion-kept-reply-sug-1"),
        ).toHaveValue("Yes, for the retry path");
        expect(
          within(shown).queryByTestId("review-suggestion-reply-input-sug-1"),
        ).toBeNull();
        await user.click(
          within(shown).getByTestId("review-suggestion-reply-dismiss-sug-1"),
        );
        expect(
          within(shown).queryByTestId("review-suggestion-kept-reply-sug-1"),
        ).toBeNull();
      }
      expect(posted).toHaveLength(1);
    },
  );

  it("shows a decision on a card of the current commit from its own answer when the list cannot be read again", async () => {
    const repoId = MOCK_WORKSPACE_DETAILS[WORKSPACE_ID].branches[0].repoId;
    const open: ReviewSuggestion = {
      id: "sug-1",
      publicationId: "pub-1",
      findingId: null,
      repoId,
      path: FILE_PATH,
      kind: "comment",
      beforeLines: "beta();\n",
      afterLines: null,
      anchorStart: 2,
      contentHash: null,
      anchorContext: null,
      authorKind: "human",
      authorLabel: "rita",
      note: "Is beta needed?",
      commentId: "thread-1",
      status: "open",
      statusBy: null,
      statusAt: null,
      supersedes: null,
      adoptedFrom: null,
      createdAt: "2026-09-30T10:00:00Z",
    };
    let listsDown = false;
    serveWorkspace({ vmStatus: "stopped" });
    server.use(
      http.get("/api/v1/workspaces/:wsId/review/suggestions", () =>
        listsDown
          ? HttpResponse.json(
              {
                error: {
                  code: "SERVICE_UNAVAILABLE",
                  message: "The service is restarting",
                  requestId: "request-down",
                },
              },
              { status: 503 },
            )
          : HttpResponse.json({ data: { suggestions: [open] } }),
      ),
      http.get("/api/v1/workspaces/:wsId/review/suggestions/:id/replies", () =>
        HttpResponse.json({ data: { replies: [] } }),
      ),
      http.post(
        "/api/v1/workspaces/:wsId/review/suggestions/:id/reject",
        () => {
          listsDown = true;
          return HttpResponse.json({
            data: {
              suggestion: {
                ...open,
                status: "rejected",
                statusAt: "2026-09-30T11:00:00Z",
              },
              writtenHash: null,
              alreadyApplied: false,
            },
          });
        },
      ),
    );
    await renderFileTab();
    const user = userEvent.setup();
    await user.click(
      await screen.findByTestId("code-editor-toggle-comments-button"),
    );
    const card = await screen.findByTestId("review-suggestion-sug-1");
    await user.click(
      within(card).getByTestId("review-suggestion-replies-toggle-sug-1"),
    );
    await user.type(
      await within(card).findByTestId("review-suggestion-reply-input-sug-1"),
      "Still needed?",
    );
    await user.click(
      within(card).getByTestId("review-suggestion-reject-sug-1"),
    );
    await user.click(
      within(card).getByTestId("review-suggestion-reject-confirm-sug-1"),
    );

    expect(
      await screen.findByText(/Could not refresh review suggestions/),
    ).toBeInTheDocument();
    const decided = screen.getByTestId("review-suggestion-sug-1");
    expect(
      within(decided).getByTestId("review-suggestion-status-sug-1"),
    ).toHaveTextContent("Rejected");
    expect(
      within(decided).queryByTestId("review-suggestion-reject-sug-1"),
    ).toBeNull();
    expect(
      within(decided).getByTestId("review-suggestion-reply-input-sug-1"),
    ).toHaveValue("Still needed?");
  });

  it("shows a suggestion withdrawn as the Reviews panel read it, back on a file whose list cannot be refreshed", async () => {
    const repoId = MOCK_WORKSPACE_DETAILS[WORKSPACE_ID].branches[0].repoId;
    const open: ReviewSuggestion = {
      id: "sug-1",
      publicationId: "pub-1",
      findingId: null,
      repoId,
      path: FILE_PATH,
      kind: "comment",
      beforeLines: "beta();\n",
      afterLines: null,
      anchorStart: 2,
      contentHash: null,
      anchorContext: null,
      authorKind: "human",
      authorLabel: "rita",
      note: "Is beta needed?",
      commentId: "thread-1",
      status: "open",
      statusBy: null,
      statusAt: null,
      supersedes: null,
      adoptedFrom: null,
      createdAt: "2026-09-30T10:00:00Z",
    };
    let withdrawn = false;
    let fileListDown = false;
    let readsOfEveryRound = 0;
    serveWorkspace({ vmStatus: "stopped" });
    server.use(
      http.get("/api/v1/workspaces/:wsId/review/suggestions", ({ request }) => {
        if (new URL(request.url).searchParams.get("currentOnly") === "true") {
          return fileListDown
            ? HttpResponse.json(
                {
                  error: {
                    code: "SERVICE_UNAVAILABLE",
                    message: "The service is restarting",
                    requestId: "request-down",
                  },
                },
                { status: 503 },
              )
            : HttpResponse.json({ data: { suggestions: [open] } });
        }
        if (withdrawn) readsOfEveryRound += 1;
        return HttpResponse.json({
          data: {
            suggestions: [
              withdrawn
                ? {
                    ...open,
                    status: "withdrawn",
                    statusAt: "2026-09-30T11:00:00Z",
                  }
                : open,
            ],
          },
        });
      }),
      http.get("/api/v1/workspaces/:wsId/review/suggestions/:id/replies", () =>
        HttpResponse.json({ data: { replies: [] } }),
      ),
    );
    await renderFileTabBesideApprovals();
    const user = userEvent.setup();
    await user.click(
      await screen.findByTestId("code-editor-toggle-comments-button"),
    );
    const card = await screen.findByTestId("review-suggestion-sug-1");
    await user.click(
      within(card).getByTestId("review-suggestion-replies-toggle-sug-1"),
    );
    await user.type(
      await within(card).findByTestId("review-suggestion-reply-input-sug-1"),
      "Still needed?",
    );

    // Withdrawn elsewhere: Reviews reads it, and the file's own list can no
    // longer be read.
    withdrawn = true;
    fileListDown = true;
    fireEvent.click(centerTab(/Approvals/));
    await waitFor(() => expect(readsOfEveryRound).toBeGreaterThan(0));
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 50));
    });
    fireEvent.click(centerTab(/main\.ts/));

    const back = await screen.findByTestId("review-suggestion-sug-1");
    await waitFor(() =>
      expect(
        within(back).getByTestId("review-suggestion-status-sug-1"),
      ).toHaveTextContent("Withdrawn"),
    );
    expect(
      within(back).queryByTestId("review-suggestion-reject-sug-1"),
    ).toBeNull();
    expect(
      within(back).queryByTestId("review-suggestion-resolve-sug-1"),
    ).toBeNull();
    if (!within(back).queryByTestId("review-suggestion-reply-input-sug-1")) {
      await user.click(
        within(back).getByTestId("review-suggestion-replies-toggle-sug-1"),
      );
    }
    expect(
      within(back).getByTestId("review-suggestion-reply-input-sug-1"),
    ).toHaveValue("Still needed?");
  });

  it("keeps a kept card's decision through a sibling's dismissal and a tab switch, while the earlier rounds cannot be read again", async () => {
    const repoId = MOCK_WORKSPACE_DETAILS[WORKSPACE_ID].branches[0].repoId;
    const card = (
      id: string,
      line: number,
      before: string,
    ): ReviewSuggestion => ({
      id,
      publicationId: "pub-1",
      findingId: null,
      repoId,
      path: FILE_PATH,
      kind: "comment",
      beforeLines: before,
      afterLines: null,
      anchorStart: line,
      contentHash: null,
      anchorContext: null,
      authorKind: "human",
      authorLabel: "rita",
      note: `About line ${line}`,
      commentId: `thread-${id}`,
      status: "open",
      statusBy: null,
      statusAt: null,
      supersedes: null,
      adoptedFrom: null,
      createdAt: "2026-09-30T10:00:00Z",
    });
    const first = card("sug-1", 2, "beta();\n");
    const second = card("sug-2", 3, "target();\n");
    let current: ReviewSuggestion[] = [first, second];
    let listsHang = false;
    serveWorkspace({ vmStatus: "stopped" });
    server.use(
      http.get(
        "/api/v1/workspaces/:wsId/review/suggestions",
        async ({ request }) => {
          const currentOnly =
            new URL(request.url).searchParams.get("currentOnly") === "true";
          // Every round's list: read before the decision, then no answer.
          if (listsHang && !currentOnly) await new Promise(() => {});
          return HttpResponse.json({
            data: { suggestions: currentOnly ? current : [first, second] },
          });
        },
      ),
      http.get("/api/v1/workspaces/:wsId/review/suggestions/:id/replies", () =>
        HttpResponse.json({ data: { replies: [] } }),
      ),
      http.post(
        "/api/v1/workspaces/:wsId/review/suggestions/:id/reject",
        () => {
          listsHang = true;
          return HttpResponse.json({
            data: {
              suggestion: {
                ...first,
                status: "rejected",
                statusAt: "2026-09-30T11:00:00Z",
              },
              writtenHash: null,
              alreadyApplied: false,
            },
          });
        },
      ),
    );
    const view = await renderFileTabBesideAnother();
    const user = userEvent.setup();
    await user.click(
      await screen.findByTestId("code-editor-toggle-comments-button"),
    );
    for (const id of ["sug-1", "sug-2"]) {
      const shown = await screen.findByTestId(`review-suggestion-${id}`);
      await user.click(
        within(shown).getByTestId(`review-suggestion-replies-toggle-${id}`),
      );
      await user.type(
        await within(shown).findByTestId(`review-suggestion-reply-input-${id}`),
        `Words on ${id}`,
      );
    }
    // A commit takes both out of the current list.
    current = [];
    await view.queryClient.invalidateQueries();
    await waitFor(() =>
      expect(
        within(screen.getByTestId("review-suggestion-sug-1")).getByTestId(
          "review-suggestion-outdated-sug-1",
        ),
      ).toBeInTheDocument(),
    );
    const kept = screen.getByTestId("review-suggestion-sug-1");
    await user.click(
      within(kept).getByTestId("review-suggestion-reject-sug-1"),
    );
    await user.click(
      within(kept).getByTestId("review-suggestion-reject-confirm-sug-1"),
    );
    const decided = async () => {
      const shown = await screen.findByTestId("review-suggestion-sug-1");
      await waitFor(() =>
        expect(
          within(shown).getByTestId("review-suggestion-status-sug-1"),
        ).toHaveTextContent("Rejected"),
      );
      expect(
        within(shown).queryByTestId("review-suggestion-reject-sug-1"),
      ).toBeNull();
      return shown;
    };
    await decided();

    // The other card's words are cleared: it leaves, and the decision stays.
    await user.clear(
      within(screen.getByTestId("review-suggestion-sug-2")).getByTestId(
        "review-suggestion-reply-input-sug-2",
      ),
    );
    await waitFor(() =>
      expect(screen.queryByTestId("review-suggestion-sug-2")).toBeNull(),
    );
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 50));
    });
    await decided();

    // Away and back: the decision stays, with the words.
    await switchAwayAndBack("review-suggestion-sug-1", () => {});
    const back = await decided();
    if (!within(back).queryByTestId("review-suggestion-reply-input-sug-1")) {
      await user.click(
        within(back).getByTestId("review-suggestion-replies-toggle-sug-1"),
      );
    }
    expect(
      within(back).getByTestId("review-suggestion-reply-input-sug-1"),
    ).toHaveValue("Words on sug-1");
  });

  it("says so when the earlier rounds of a kept card cannot be read", async () => {
    const repoId = MOCK_WORKSPACE_DETAILS[WORKSPACE_ID].branches[0].repoId;
    const open: ReviewSuggestion = {
      id: "sug-1",
      publicationId: "pub-1",
      findingId: null,
      repoId,
      path: FILE_PATH,
      kind: "comment",
      beforeLines: "beta();\n",
      afterLines: null,
      anchorStart: 2,
      contentHash: null,
      anchorContext: null,
      authorKind: "human",
      authorLabel: "rita",
      note: "Is beta needed?",
      commentId: "thread-1",
      status: "open",
      statusBy: null,
      statusAt: null,
      supersedes: null,
      adoptedFrom: null,
      createdAt: "2026-09-30T10:00:00Z",
    };
    let current: ReviewSuggestion[] = [open];
    serveWorkspace({ vmStatus: "stopped" });
    server.use(
      http.get("/api/v1/workspaces/:wsId/review/suggestions", ({ request }) =>
        new URL(request.url).searchParams.get("currentOnly") === "true"
          ? HttpResponse.json({ data: { suggestions: current } })
          : HttpResponse.json(
              {
                error: {
                  code: "UNAVAILABLE",
                  message: "Review rounds are unavailable",
                  requestId: "request-failed",
                },
              },
              { status: 503 },
            ),
      ),
      http.get("/api/v1/workspaces/:wsId/review/suggestions/:id/replies", () =>
        HttpResponse.json({ data: { replies: [] } }),
      ),
    );
    const view = await renderFileTab();
    const user = userEvent.setup();
    await user.click(
      await screen.findByTestId("code-editor-toggle-comments-button"),
    );
    const card = await screen.findByTestId("review-suggestion-sug-1");
    await user.click(
      within(card).getByTestId("review-suggestion-replies-toggle-sug-1"),
    );
    await user.type(
      await within(card).findByTestId("review-suggestion-reply-input-sug-1"),
      "Still needed?",
    );
    current = [];
    await view.queryClient.invalidateQueries();
    expect(
      await screen.findByText(/Could not load the earlier rounds/, undefined, {
        timeout: 4000,
      }),
    ).toBeInTheDocument();
    expect(
      within(screen.getByTestId("review-suggestion-sug-1")).getByTestId(
        "review-suggestion-reply-input-sug-1",
      ),
    ).toHaveValue("Still needed?");
  });

  it("keeps a draft's words readable, and sends nothing, once the workspace turns read-only", async () => {
    const served = serveWorkspace({ vmStatus: "stopped" });
    const view = await renderFileTab();
    // FILES draws read-only Monaco: its glyph margin starts a thread.
    await startThreadAt(3);
    const user = userEvent.setup();
    await user.type(await screen.findByLabelText("Comment"), "Rename it");

    server.use(
      http.get("/api/v1/workspaces/:id", () =>
        HttpResponse.json({
          data: { ...MOCK_WORKSPACE_DETAILS[WORKSPACE_ID], readOnly: true },
        }),
      ),
    );
    await view.queryClient.invalidateQueries();
    expect(await screen.findByTestId("draft-comment-closed")).toHaveTextContent(
      "can no longer start a thread",
    );
    const draft = screen.getByLabelText("Comment");
    expect(draft).toHaveValue("Rename it");
    expect(draft).toHaveAttribute("readonly");
    expect(screen.queryByTestId("draft-comment-save")).toBeNull();
    // The keyboard sends nothing either: no refusal is even asked for.
    fireEvent.keyDown(draft, { key: "Enter", metaKey: true });
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 20));
    });
    expect(screen.queryByTestId("draft-comment-error")).toBeNull();
    await user.click(screen.getByTestId("draft-comment-cancel"));
    expect(screen.queryByLabelText("Comment")).toBeNull();
    expect(served.created).toHaveLength(0);
  });

  it("keeps an in-file suggestion card and its typed reply when refreshing suggestions fails", async () => {
    const repoId = MOCK_WORKSPACE_DETAILS[WORKSPACE_ID].branches[0].repoId;
    const suggestion: ReviewSuggestion = {
      id: "sug-1",
      publicationId: "pub-1",
      findingId: null,
      repoId,
      path: FILE_PATH,
      kind: "comment",
      beforeLines: "beta();\n",
      afterLines: null,
      anchorStart: 2,
      contentHash: null,
      anchorContext: null,
      authorKind: "human",
      authorLabel: "rita",
      note: "Is beta needed?",
      commentId: "thread-1",
      status: "open",
      statusBy: null,
      statusAt: null,
      supersedes: null,
      adoptedFrom: null,
      createdAt: "2026-09-30T10:00:00Z",
    };
    const suggestions = (ok: boolean) =>
      http.get("/api/v1/workspaces/:wsId/review/suggestions", () =>
        ok
          ? HttpResponse.json({ data: { suggestions: [suggestion] } })
          : HttpResponse.json(
              {
                error: {
                  code: "SERVICE_UNAVAILABLE",
                  message: "The service is restarting",
                  requestId: "request-down",
                },
              },
              { status: 503 },
            ),
      );
    // Stopped: the file shows in the read-only viewer, cards in place.
    serveWorkspace({ vmStatus: "stopped" });
    server.use(
      suggestions(true),
      http.get("/api/v1/workspaces/:wsId/review/suggestions/:id/replies", () =>
        HttpResponse.json({ data: { replies: [] } }),
      ),
    );
    const view = await renderFileTab();
    const user = userEvent.setup();
    await user.click(
      await screen.findByTestId("code-editor-toggle-comments-button"),
    );
    const card = await screen.findByTestId("review-suggestion-sug-1");
    await user.click(
      within(card).getByTestId("review-suggestion-replies-toggle-sug-1"),
    );
    const reply = await within(card).findByTestId(
      "review-suggestion-reply-input-sug-1",
    );
    await user.type(reply, "Draft answer");

    server.use(suggestions(false));
    await view.queryClient.invalidateQueries();
    expect(
      await screen.findByTestId("comment-column-notice"),
    ).toHaveTextContent("Could not refresh review suggestions");
    expect(screen.getByTestId("review-suggestion-sug-1")).toBe(card);
    expect(reply).toHaveValue("Draft answer");

    server.use(suggestions(true));
    await view.queryClient.invalidateQueries();
    await waitFor(() =>
      expect(
        screen.queryByTestId("comment-column-notice"),
      ).not.toBeInTheDocument(),
    );
    expect(reply).toHaveValue("Draft answer");
  });
});

const WORKSPACE_UUID = "7f3b1c2e-4d5a-4b6c-8d7e-9f0a1b2c3d4e";
const REPO_UUID = "1a2b3c4d-5e6f-4a7b-8c9d-0e1f2a3b4c5d";

/** Serve one workspace under a UUID id, as repository document handles need. */
function serveUuidWorkspace(): URLSearchParams[] {
  const listQueries: URLSearchParams[] = [];
  const detail = {
    ...MOCK_WORKSPACE_DETAILS[WORKSPACE_ID],
    id: WORKSPACE_UUID,
    ownerUserId: "user-alex",
    branches: [
      {
        ...MOCK_WORKSPACE_DETAILS[WORKSPACE_ID].branches[0],
        workspaceId: WORKSPACE_UUID,
        repoId: REPO_UUID,
      },
    ],
  };
  server.use(
    http.get("/api/v1/resolve", () =>
      HttpResponse.json({
        data: { orgId: "org-g5labs", workspaceId: WORKSPACE_UUID },
      }),
    ),
    http.get("/api/v1/workspaces/:id", () =>
      HttpResponse.json({ data: detail }),
    ),
    http.get("/api/v1/workspaces/:wsId/comments", ({ request }) => {
      listQueries.push(new URL(request.url).searchParams);
      return HttpResponse.json({ data: [] });
    }),
  );
  return listQueries;
}

/**
 * Select the restored content tab (`centerTab`): the workbench landing puts
 * the pinned Workspace (timeline) tab in front (g5.workbench.landing), so
 * the restored tab is the one content tab that is not it.
 */
const RESTORED_TAB = /^(?!Workspace$)/;
async function selectRestoredTab() {
  await waitFor(() => centerTab(RESTORED_TAB));
  fireEvent.click(centerTab(RESTORED_TAB));
}

function renderRestoredTab(tab: Record<string, unknown>, activeTabId: string) {
  localStorage.setItem(
    `g5:tabs:${WORKSPACE_UUID}`,
    JSON.stringify({ tabs: [tab], activeTabId }),
  );
  return renderApp(
    <Routes>
      <Route path="/:org/:workspace/*" element={<WorkspacePage />} />
    </Routes>,
    { route: `/org-g5labs/${WORKSPACE_ID}`, user: createMockUser() },
  );
}

function base64Url(text: string): string {
  return btoa(text).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

describe("WorkspacePage lists a file's threads by the file in every view", () => {
  beforeEach(() => {
    localStorage.clear();
    // The center tab bar is hidden by default (workspaces
    // g5.workspace.ui.center-tabs); this suite drives the tabs, so it opts in.
    seedUserSettings({ showTabBar: true });
    localStorage.setItem(GETTING_STARTED_AUTO_OPENED_STORAGE_KEY, "1");
    vi.mocked(window.matchMedia).mockImplementation((query) => ({
      matches: query.includes("min-width: 1024px"),
      media: query,
      onchange: null,
      addListener: vi.fn(),
      removeListener: vi.fn(),
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      dispatchEvent: vi.fn(),
    }));
  });

  it("uses the document's repository and path in the document view", async () => {
    const listQueries = serveUuidWorkspace();
    const docId = `overlay:${WORKSPACE_UUID}:${REPO_UUID}:${base64Url("docs/plan.md")}`;
    server.use(
      http.get("/api/v1/workspaces/:wsId/files/*", () =>
        HttpResponse.json({
          data: {
            id: docId,
            workspaceId: WORKSPACE_UUID,
            repoId: REPO_UUID,
            filePath: "docs/plan.md",
            name: "plan.md",
            title: "Plan",
            content: "# Plan\n\nBody.\n",
            hash: "doc-v1",
            encoding: "utf-8",
            size: 100,
            source: "live",
            repoOid: null,
            snapshotAt: null,
          },
        }),
      ),
    );
    renderRestoredTab({ type: "doc", docId, name: "plan.md" }, `doc:${docId}`);
    await selectRestoredTab();
    await waitFor(() =>
      expect(
        listQueries.some((query) => query.get("filePath") === "docs/plan.md"),
      ).toBe(true),
    );
    for (const query of listQueries.filter((q) => q.has("filePath"))) {
      expect(query.get("repoId")).toBe(REPO_UUID);
      expect(query.get("filePath")).toBe("docs/plan.md");
      expect(query.has("branch")).toBe(false);
    }
  });

  it("keeps a document on screen when refreshing it fails", async () => {
    serveUuidWorkspace();
    const docId = `overlay:${WORKSPACE_UUID}:${REPO_UUID}:${base64Url("docs/plan.md")}`;
    server.use(
      http.get("/api/v1/workspaces/:wsId/files/*", () =>
        HttpResponse.json({
          data: {
            id: docId,
            workspaceId: WORKSPACE_UUID,
            repoId: REPO_UUID,
            filePath: "docs/plan.md",
            name: "plan.md",
            title: "Plan",
            content: "# Plan\n\nBody.\n",
            hash: "doc-v1",
            encoding: "utf-8",
            size: 100,
            source: "live",
            repoOid: null,
            snapshotAt: null,
          },
        }),
      ),
    );
    const view = renderRestoredTab(
      { type: "doc", docId, name: "plan.md" },
      `doc:${docId}`,
    );
    await selectRestoredTab();
    const body = await screen.findByText("Body.");

    server.use(
      http.get("/api/v1/workspaces/:wsId/files/*", () => HttpResponse.error()),
    );
    await view.queryClient.invalidateQueries({ queryKey: ["workspace-file"] });
    expect(
      await screen.findByTestId("content-refresh-error"),
    ).toBeInTheDocument();
    expect(screen.getByText("Body.")).toBe(body);
  });

  it("keeps a document's card, its reply being sent and the refusal, when the suggestion is decided meanwhile", async () => {
    serveUuidWorkspace();
    const docId = `overlay:${WORKSPACE_UUID}:${REPO_UUID}:${base64Url("docs/plan.md")}`;
    const open: ReviewSuggestion = {
      id: "sug-doc",
      publicationId: "pub-1",
      findingId: null,
      repoId: REPO_UUID,
      path: "docs/plan.md",
      kind: "comment",
      beforeLines: "Body.\n",
      afterLines: null,
      anchorStart: 3,
      contentHash: null,
      anchorContext: null,
      authorKind: "human",
      authorLabel: "rita",
      note: "Say more.",
      commentId: "thread-doc",
      status: "open",
      statusBy: null,
      statusAt: null,
      supersedes: null,
      adoptedFrom: null,
      createdAt: "2026-09-30T10:00:00Z",
    };
    let current = open;
    const posted: unknown[] = [];
    let refuse!: () => void;
    const held = new Promise<void>((resolve) => {
      refuse = resolve;
    });
    server.use(
      http.get("/api/v1/workspaces/:wsId/files/*", () =>
        HttpResponse.json({
          data: {
            id: docId,
            workspaceId: WORKSPACE_UUID,
            repoId: REPO_UUID,
            filePath: "docs/plan.md",
            name: "plan.md",
            title: "Plan",
            content: "# Plan\n\nBody.\n",
            hash: "doc-v1",
            encoding: "utf-8",
            size: 100,
            source: "live",
            repoOid: null,
            snapshotAt: null,
          },
        }),
      ),
      http.get("/api/v1/workspaces/:wsId/review/suggestions", () =>
        HttpResponse.json({ data: { suggestions: [current] } }),
      ),
      http.get("/api/v1/workspaces/:wsId/review/suggestions/:id/replies", () =>
        HttpResponse.json({ data: { replies: [] } }),
      ),
      http.post(
        "/api/v1/workspaces/:wsId/review/suggestions/:id/replies",
        async ({ request }) => {
          posted.push(await request.json());
          await held;
          return HttpResponse.json(
            {
              error: {
                code: "WORKSPACE_READ_ONLY",
                message: "The workspace became read-only",
                requestId: "request-refused",
              },
            },
            { status: 409 },
          );
        },
      ),
    );
    const view = renderRestoredTab(
      { type: "doc", docId, name: "plan.md" },
      `doc:${docId}`,
    );
    await selectRestoredTab();
    const card = await screen.findByTestId("review-suggestion-sug-doc");
    const user = userEvent.setup();
    await user.click(
      within(card).getByTestId("review-suggestion-replies-toggle-sug-doc"),
    );
    const reply = await within(card).findByTestId(
      "review-suggestion-reply-input-sug-doc",
    );
    await user.type(reply, "Is it?{Enter}");
    await waitFor(() => expect(posted).toHaveLength(1));

    current = { ...open, status: "withdrawn" };
    await view.queryClient.invalidateQueries();
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 50));
    });
    expect(screen.getByTestId("review-suggestion-sug-doc")).toBe(card);
    refuse();
    expect(
      await within(card).findByText("The workspace became read-only"),
    ).toBeInTheDocument();
    expect(reply).toHaveValue("Is it?");
    expect(posted).toHaveLength(1);
  });

  it("keeps a document's only thread, a reply being sent and the refusal, when the thread is deleted elsewhere", async () => {
    serveUuidWorkspace();
    const docId = `overlay:${WORKSPACE_UUID}:${REPO_UUID}:${base64Url("docs/plan.md")}`;
    const docText = "# Plan\n\nBody.\n";
    const anchor = await createCommentAnchor(docText, 3);
    const thread: CommentDTO = {
      ...(await storedComment(2)),
      id: "comment-doc",
      workspaceId: WORKSPACE_UUID,
      repoId: REPO_UUID,
      filePath: "docs/plan.md",
      lineNumber: 3,
      lineHash: anchor.lineHash,
      contentHash: anchor.contentHash,
      anchorContext: anchor.anchorContext,
    };
    let listed = [thread];
    const posted: unknown[] = [];
    let refuse!: () => void;
    const held = new Promise<void>((resolve) => {
      refuse = resolve;
    });
    server.use(
      http.get("/api/v1/workspaces/:wsId/comments", () =>
        HttpResponse.json({ data: listed }),
      ),
      http.get("/api/v1/comments/:commentId/replies", () =>
        HttpResponse.json({ data: [] }),
      ),
      http.post("/api/v1/comments/:commentId/replies", async ({ request }) => {
        posted.push(await request.json());
        await held;
        return HttpResponse.json(
          {
            error: {
              code: "NOT_FOUND",
              message: "The thread no longer exists",
              requestId: "request-refused",
            },
          },
          { status: 404 },
        );
      }),
      http.get("/api/v1/workspaces/:wsId/files/*", () =>
        HttpResponse.json({
          data: {
            id: docId,
            workspaceId: WORKSPACE_UUID,
            repoId: REPO_UUID,
            filePath: "docs/plan.md",
            name: "plan.md",
            title: "Plan",
            content: docText,
            hash: "doc-v1",
            encoding: "utf-8",
            size: 100,
            source: "live",
            repoOid: null,
            snapshotAt: null,
          },
        }),
      ),
    );
    const view = renderRestoredTab(
      { type: "doc", docId, name: "plan.md" },
      `doc:${docId}`,
    );
    await selectRestoredTab();
    // Opening a thread scrolls the document to its block; jsdom has no
    // element scrolling.
    Object.defineProperty(HTMLElement.prototype, "scrollTo", {
      value: vi.fn(),
      configurable: true,
      writable: true,
    });
    const user = userEvent.setup();
    await user.click(
      await screen.findByTestId("code-editor-toggle-comments-button"),
    );
    const shown = await screen.findByTestId("comment-thread-comment-doc");
    await user.click(within(shown).getByTestId("comment-thread-toggle-expand"));
    const reply = within(shown).getByPlaceholderText("Reply…");
    await user.type(reply, "Say which part{Enter}");
    await waitFor(() => expect(posted).toHaveLength(1));

    // Deleted in another tab: the document has no thread left, as after
    // `comment.deleted` (`ws.ts`).
    listed = [];
    view.queryClient.removeQueries({
      queryKey: ["comment-replies", "comment-doc"],
      exact: true,
    });
    await view.queryClient.invalidateQueries();
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 50));
    });
    expect(screen.getByTestId("comment-thread-comment-doc")).toBe(shown);

    refuse();
    expect(
      await within(shown).findByText("The thread no longer exists"),
    ).toBeInTheDocument();
    expect(reply).toHaveValue("Say which part");
    expect(posted).toHaveLength(1);
    await user.click(within(shown).getByTestId("comment-thread-dismiss"));
    await waitFor(() =>
      expect(screen.queryByTestId("comment-thread-comment-doc")).toBeNull(),
    );
  });

  it("keeps a document's card, its rejection reason and the refusal, when the suggestion is withdrawn meanwhile", async () => {
    serveUuidWorkspace();
    const docId = `overlay:${WORKSPACE_UUID}:${REPO_UUID}:${base64Url("docs/plan.md")}`;
    const open: ReviewSuggestion = {
      id: "sug-doc",
      publicationId: "pub-1",
      findingId: null,
      repoId: REPO_UUID,
      path: "docs/plan.md",
      kind: "comment",
      beforeLines: "Body.\n",
      afterLines: null,
      anchorStart: 3,
      contentHash: null,
      anchorContext: null,
      authorKind: "human",
      authorLabel: "rita",
      note: "Say more.",
      commentId: "thread-doc",
      status: "open",
      statusBy: null,
      statusAt: null,
      supersedes: null,
      adoptedFrom: null,
      createdAt: "2026-09-30T10:00:00Z",
    };
    let current = open;
    const posted: unknown[] = [];
    let refuse!: () => void;
    const held = new Promise<void>((resolve) => {
      refuse = resolve;
    });
    server.use(
      http.get("/api/v1/workspaces/:wsId/files/*", () =>
        HttpResponse.json({
          data: {
            id: docId,
            workspaceId: WORKSPACE_UUID,
            repoId: REPO_UUID,
            filePath: "docs/plan.md",
            name: "plan.md",
            title: "Plan",
            content: "# Plan\n\nBody.\n",
            hash: "doc-v1",
            encoding: "utf-8",
            size: 100,
            source: "live",
            repoOid: null,
            snapshotAt: null,
          },
        }),
      ),
      http.get("/api/v1/workspaces/:wsId/review/suggestions", () =>
        HttpResponse.json({ data: { suggestions: [current] } }),
      ),
      http.post(
        "/api/v1/workspaces/:wsId/review/suggestions/:id/reject",
        async ({ request }) => {
          posted.push(await request.json());
          await held;
          return HttpResponse.json(
            {
              error: {
                code: "SUGGESTION_DECIDED",
                message: "The suggestion was withdrawn",
                requestId: "request-refused",
              },
            },
            { status: 409 },
          );
        },
      ),
    );
    const view = renderRestoredTab(
      { type: "doc", docId, name: "plan.md" },
      `doc:${docId}`,
    );
    await selectRestoredTab();
    const card = await screen.findByTestId("review-suggestion-sug-doc");
    const user = userEvent.setup();
    await user.click(
      within(card).getByTestId("review-suggestion-reject-sug-doc"),
    );
    const reason = within(card).getByTestId(
      "review-suggestion-reject-reason-sug-doc",
    );
    await user.type(reason, "Out of scope");
    await user.click(
      within(card).getByTestId("review-suggestion-reject-confirm-sug-doc"),
    );
    await waitFor(() => expect(posted).toHaveLength(1));

    current = { ...open, status: "withdrawn" };
    await view.queryClient.invalidateQueries();
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 50));
    });
    expect(screen.getByTestId("review-suggestion-sug-doc")).toBe(card);
    refuse();
    expect(
      await within(card).findByText(/The suggestion was withdrawn/),
    ).toBeInTheDocument();
    expect(reason).toHaveValue("Out of scope");
    expect(
      within(card).getByTestId("review-suggestion-reject-confirm-sug-doc"),
    ).toBeDisabled();
    expect(posted).toHaveLength(1);
  });

  it.each([
    { what: "a commit takes it out of the current list", commit: true },
    { what: "it is withdrawn with an edited replacement", commit: false },
  ])(
    "keeps a document's card being answered or edited when $what",
    async ({ commit }) => {
      serveUuidWorkspace();
      const docId = `overlay:${WORKSPACE_UUID}:${REPO_UUID}:${base64Url("docs/plan.md")}`;
      const open: ReviewSuggestion = {
        id: "sug-doc",
        publicationId: "pub-1",
        findingId: null,
        repoId: REPO_UUID,
        path: "docs/plan.md",
        kind: commit ? "comment" : "hunk",
        beforeLines: "Body.\n",
        afterLines: commit ? null : "More body.\n",
        anchorStart: 3,
        contentHash: null,
        anchorContext: null,
        authorKind: "human",
        authorLabel: "rita",
        note: "Say more.",
        commentId: "thread-doc",
        status: "open",
        statusBy: null,
        statusAt: null,
        supersedes: null,
        adoptedFrom: null,
        createdAt: "2026-09-30T10:00:00Z",
      };
      let current: ReviewSuggestion[] = [open];
      server.use(
        http.get("/api/v1/workspaces/:wsId/files/*", () =>
          HttpResponse.json({
            data: {
              id: docId,
              workspaceId: WORKSPACE_UUID,
              repoId: REPO_UUID,
              filePath: "docs/plan.md",
              name: "plan.md",
              title: "Plan",
              content: "# Plan\n\nBody.\n",
              hash: "doc-v1",
              encoding: "utf-8",
              size: 100,
              source: "live",
              repoOid: null,
              snapshotAt: null,
            },
          }),
        ),
        http.get("/api/v1/workspaces/:wsId/review/suggestions", () =>
          HttpResponse.json({ data: { suggestions: current } }),
        ),
        http.get(
          "/api/v1/workspaces/:wsId/review/suggestions/:id/replies",
          () => HttpResponse.json({ data: { replies: [] } }),
        ),
      );
      const view = renderRestoredTab(
        { type: "doc", docId, name: "plan.md" },
        `doc:${docId}`,
      );
      await selectRestoredTab();
      const card = await screen.findByTestId("review-suggestion-sug-doc");
      const user = userEvent.setup();
      if (commit) {
        await user.click(
          within(card).getByTestId("review-suggestion-replies-toggle-sug-doc"),
        );
        await user.type(
          await within(card).findByTestId(
            "review-suggestion-reply-input-sug-doc",
          ),
          "Which part?",
        );
        current = [];
      } else {
        await user.click(
          within(card).getByTestId("review-suggestion-edit-toggle-sug-doc"),
        );
        const edit = within(card).getByTestId("review-suggestion-edit-sug-doc");
        await user.clear(edit);
        await user.type(edit, "Much more body.");
        current = [{ ...open, status: "withdrawn" }];
      }
      await view.queryClient.invalidateQueries();
      await act(async () => {
        await new Promise((resolve) => setTimeout(resolve, 50));
      });
      const kept = screen.getByTestId("review-suggestion-sug-doc");
      if (commit) {
        expect(kept).toBe(card);
        expect(
          within(kept).getByTestId("review-suggestion-outdated-sug-doc"),
        ).toHaveTextContent("From an earlier commit");
        expect(
          within(kept).getByTestId("review-suggestion-reply-input-sug-doc"),
        ).toHaveValue("Which part?");
      } else {
        expect(
          within(kept).getByTestId("review-suggestion-edit-sug-doc"),
        ).toHaveValue("Much more body.");
        expect(
          within(kept).getByTestId("review-suggestion-edit-closed-sug-doc"),
        ).toBeInTheDocument();
        expect(
          within(kept).getByTestId("review-suggestion-edit-sug-doc"),
        ).toHaveAttribute("readonly");
        expect(
          within(kept).queryByTestId("review-suggestion-accept-sug-doc"),
        ).toBeNull();
      }
    },
  );

  it("keeps a document draft's words readable, and sends nothing, once the workspace turns read-only", async () => {
    serveUuidWorkspace();
    const docId = `overlay:${WORKSPACE_UUID}:${REPO_UUID}:${base64Url("docs/plan.md")}`;
    const created: unknown[] = [];
    server.use(
      http.get("/api/v1/workspaces/:wsId/files/*", () =>
        HttpResponse.json({
          data: {
            id: docId,
            workspaceId: WORKSPACE_UUID,
            repoId: REPO_UUID,
            filePath: "docs/plan.md",
            name: "plan.md",
            title: "Plan",
            content: "# Plan\n\nBody.\n",
            hash: "doc-v1",
            encoding: "utf-8",
            size: 100,
            source: "live",
            repoOid: null,
            snapshotAt: null,
          },
        }),
      ),
      http.post("/api/v1/workspaces/:wsId/comments", async ({ request }) => {
        created.push(await request.json());
        return HttpResponse.json({ data: {} }, { status: 201 });
      }),
    );
    const view = renderRestoredTab(
      { type: "doc", docId, name: "plan.md" },
      `doc:${docId}`,
    );
    await selectRestoredTab();
    await userEvent.click(await screen.findByTestId("file-view-switch-mode"));
    await startThreadAt(3);
    const user = userEvent.setup();
    await user.type(await screen.findByLabelText("Comment"), "Say more");

    server.use(
      http.get("/api/v1/workspaces/:id", () =>
        HttpResponse.json({
          data: {
            ...MOCK_WORKSPACE_DETAILS[WORKSPACE_ID],
            id: WORKSPACE_UUID,
            ownerUserId: "user-alex",
            readOnly: true,
            branches: [
              {
                ...MOCK_WORKSPACE_DETAILS[WORKSPACE_ID].branches[0],
                workspaceId: WORKSPACE_UUID,
                repoId: REPO_UUID,
              },
            ],
          },
        }),
      ),
    );
    await view.queryClient.invalidateQueries();
    expect(await screen.findByTestId("draft-comment-closed")).toHaveTextContent(
      "can no longer start a thread",
    );
    const draft = screen.getByLabelText("Comment");
    expect(draft).toHaveValue("Say more");
    expect(draft).toHaveAttribute("readonly");
    expect(screen.queryByTestId("draft-comment-save")).toBeNull();
    await user.click(screen.getByTestId("draft-comment-cancel"));
    expect(screen.queryByLabelText("Comment")).toBeNull();
    expect(created).toHaveLength(0);
  });

  /** One file (HTML by default) with a thread and a suggestion on line 2, seen by a reader. */
  async function serveHtmlFileToReader(
    filePath: string,
    html = "<h1>Title</h1>\n<p>Second line</p>\n<p>Third</p>\n",
  ) {
    const anchor = await createCommentAnchor(html, 2);
    const thread: CommentDTO = {
      id: "html-thread-1",
      workspaceId: WORKSPACE_UUID,
      authorId: "user-other",
      authorName: "Olive Owner",
      authorAvatarUrl: null,
      scope: "line",
      repoId: REPO_UUID,
      filePath,
      lineNumber: 2,
      lineHash: anchor.lineHash,
      contentHash: anchor.contentHash,
      anchorContext: anchor.anchorContext,
      commitSha: null,
      content: "Is this paragraph needed?",
      resolved: false,
      resolvedAt: null,
      originBranch: "main",
      replyCount: 0,
      createdAt: "2026-09-30T09:00:00Z",
      updatedAt: "2026-09-30T09:00:00Z",
    };
    // A later thread on a later line: the API lists it first.
    const laterAnchor = await createCommentAnchor(html, 3);
    const laterThread: CommentDTO = {
      ...thread,
      id: "html-thread-2",
      lineNumber: 3,
      lineHash: laterAnchor.lineHash,
      anchorContext: laterAnchor.anchorContext,
      content: "Keep the third?",
      createdAt: "2026-09-30T09:30:00Z",
      updatedAt: "2026-09-30T09:30:00Z",
    };
    const suggestion: ReviewSuggestion = {
      id: "sug-html",
      publicationId: "pub-1",
      findingId: null,
      repoId: REPO_UUID,
      path: filePath,
      kind: "comment",
      beforeLines: html.split("\n")[1],
      afterLines: null,
      anchorStart: 2,
      contentHash: null,
      anchorContext: null,
      authorKind: "human",
      authorLabel: "rita",
      note: "Shorten this.",
      commentId: "thread-sug-html",
      status: "open",
      statusBy: null,
      statusAt: null,
      supersedes: null,
      adoptedFrom: null,
      createdAt: "2026-09-30T10:00:00Z",
    };
    const replies: unknown[] = [];
    serveUuidWorkspace();
    server.use(
      http.get("/api/v1/workspaces/:id", () =>
        HttpResponse.json({
          data: {
            ...MOCK_WORKSPACE_DETAILS[WORKSPACE_ID],
            id: WORKSPACE_UUID,
            ownerUserId: "user-other",
            branches: [
              {
                ...MOCK_WORKSPACE_DETAILS[WORKSPACE_ID].branches[0],
                workspaceId: WORKSPACE_UUID,
                repoId: REPO_UUID,
              },
            ],
          },
        }),
      ),
      http.get("/api/v1/workspaces/:wsId/comments", ({ request }) =>
        HttpResponse.json({
          data:
            new URL(request.url).searchParams.get("filePath") === filePath
              ? [laterThread, thread]
              : [],
        }),
      ),
      http.get("/api/v1/workspaces/:wsId/review/suggestions", () =>
        HttpResponse.json({ data: { suggestions: [suggestion] } }),
      ),
      http.get("/api/v1/comments/:commentId/replies", () =>
        HttpResponse.json({ data: [] }),
      ),
      http.post("/api/v1/comments/:commentId/replies", async ({ request }) => {
        replies.push(await request.json());
        return HttpResponse.json(
          {
            data: {
              id: "reply-1",
              commentId: "html-thread-1",
              authorId: "user-alex",
              authorName: "Alex Chen",
              authorAvatarUrl: null,
              content: "Yes",
              createdAt: "2026-09-30T10:05:00Z",
            },
          },
          { status: 201 },
        );
      }),
    );
    return { html, replies };
  }

  async function expectHtmlThreadsBesidePreview(
    replies: unknown[],
    panelTestId = "file-view-preview-threads",
  ) {
    await selectRestoredTab();
    await userEvent.click(
      await screen.findByTestId("code-editor-toggle-comments-button"),
    );
    const panel = await screen.findByTestId(panelTestId);
    const thread = await within(panel).findByTestId(
      "comment-thread-html-thread-1",
    );
    // The thread names its line of the source.
    await waitFor(() =>
      expect(
        within(thread).getByTestId("comment-thread-anchor"),
      ).toHaveTextContent("L2"),
    );
    // Listed by line, not newest first: the later thread on line 3 follows.
    await waitFor(() =>
      expect(
        Array.from(
          panel.querySelectorAll<HTMLElement>("[data-comment-thread-id]"),
        ).map((el) => el.dataset.commentThreadId),
      ).toEqual(["html-thread-1", "html-thread-2"]),
    );
    expect(
      within(panel)
        .getAllByTestId("comment-thread-anchor")
        .map((el) => el.textContent),
    ).toEqual(["L2", "L3"]);
    expect(screen.getByTestId("review-suggestion-sug-html")).toBeVisible();
    const user = userEvent.setup();
    await user.click(
      within(thread).getByTestId("comment-thread-toggle-expand"),
    );
    expect(
      within(thread).queryByTestId("comment-thread-resolve"),
    ).not.toBeInTheDocument();
    expect(
      within(thread).queryByTestId("comment-thread-delete"),
    ).not.toBeInTheDocument();
    const reply = within(thread).getByPlaceholderText("Reply…");
    await user.type(reply, "Keep it");
    fireEvent.keyDown(reply, { key: "Enter" });
    await waitFor(() => expect(replies).toEqual([{ content: "Keep it" }]));
  }

  it.each(["doc"] as const)(
    "lets a reader comment on pushed text in the native %s editor",
    async (surface) => {
      const filePath = "docs/review.md";
      const content = "# Plan\n\nPushed paragraph.\n";
      await serveHtmlFileToReader(filePath, content);
      const docId = `overlay:${WORKSPACE_UUID}:${REPO_UUID}:${base64Url(filePath)}`;
      const created: Record<string, unknown>[] = [];
      server.use(
        http.get(
          surface === "doc"
            ? "/api/v1/workspaces/:wsId/files/*"
            : "/api/v1/workspaces/:wsId/ontology/:nodeId/content",
          () =>
            HttpResponse.json({
              data: {
                id: docId,
                nodeId: "node-pushed",
                workspaceId: WORKSPACE_UUID,
                repoId: REPO_UUID,
                filePath,
                name: "review.md",
                content,
                hash: "doc-v1",
                encoding: "utf-8",
                size: 100,
                source: "committed",
                repoOid: "oid-pushed",
                snapshotAt: null,
                draftDirty: false,
              },
            }),
        ),
        http.post("/api/v1/workspaces/:wsId/comments", async ({ request }) => {
          const body = (await request.json()) as Record<string, unknown>;
          created.push(body);
          return HttpResponse.json(
            { data: { ...(await storedComment(3)), ...body } },
            { status: 201 },
          );
        }),
      );
      renderRestoredTab(
        surface === "doc"
          ? { type: "doc", docId, name: "review.md" }
          : {
              type: "ontology",
              nodeId: "node-pushed",
              nodeType: "document",
              name: "review.md",
              filePath,
            },
        surface === "doc" ? `doc:${docId}` : "ontology:node-pushed",
      );
      await selectRestoredTab();
      await userEvent.click(await screen.findByTestId("file-view-switch-mode"));
      await startThreadAt(3);
      const user = userEvent.setup();
      await user.type(
        await screen.findByLabelText("Comment"),
        "Native reader comment",
      );
      await user.click(screen.getByTestId("draft-comment-save"));
      await waitFor(() => expect(created).toHaveLength(1));
      expect(created[0]).toMatchObject({
        repoId: REPO_UUID,
        filePath,
        lineNumber: 3,
        commitSha: "oid-pushed",
        contentHash: await sha256Utf8(content),
      });
    },
  );

  it("lets a reader comment on selected text of the rendered pushed document", async () => {
    const filePath = "docs/review.md";
    const content = "# Plan\n\nPushed paragraph.\n";
    await serveHtmlFileToReader(filePath, content);
    const docId = `overlay:${WORKSPACE_UUID}:${REPO_UUID}:${base64Url(filePath)}`;
    const created: Record<string, unknown>[] = [];
    server.use(
      http.get("/api/v1/workspaces/:wsId/files/*", () =>
        HttpResponse.json({
          data: {
            id: docId,
            nodeId: "node-pushed",
            workspaceId: WORKSPACE_UUID,
            repoId: REPO_UUID,
            filePath,
            name: "review.md",
            content,
            hash: "doc-v1",
            encoding: "utf-8",
            size: 100,
            source: "committed",
            repoOid: "oid-pushed",
            snapshotAt: null,
            draftDirty: false,
          },
        }),
      ),
      http.post("/api/v1/workspaces/:wsId/comments", async ({ request }) => {
        const body = (await request.json()) as Record<string, unknown>;
        created.push(body);
        return HttpResponse.json(
          { data: { ...(await storedComment(3)), ...body } },
          { status: 201 },
        );
      }),
    );
    renderRestoredTab(
      { type: "doc", docId, name: "review.md" },
      `doc:${docId}`,
    );
    await selectRestoredTab();
    // Reading the rendered document: select the paragraph and take Comment.
    const host = await screen.findByTestId("document-preview-content");
    const paragraph = await within(host).findByText("Pushed paragraph.");
    const range = document.createRange();
    range.selectNodeContents(paragraph);
    const selection = window.getSelection()!;
    selection.removeAllRanges();
    selection.addRange(range);
    fireEvent.mouseUp(host);
    fireEvent.click(
      await screen.findByTestId("document-preview-selection-comment-button"),
    );
    // The draft opens in the panel; the rendered document stays.
    const panel = await screen.findByTestId("file-view-preview-threads");
    const card = await within(panel).findByTestId("comment-draft-card");
    expect(screen.getByTestId("markdown-preview")).toBeInTheDocument();
    const user = userEvent.setup();
    await user.type(
      within(card).getByLabelText("Comment"),
      "Rendered reader comment",
    );
    await user.click(within(card).getByTestId("draft-comment-save"));
    await waitFor(() => expect(created).toHaveLength(1));
    expect(created[0]).toMatchObject({
      repoId: REPO_UUID,
      filePath,
      lineNumber: 3,
      commitSha: "oid-pushed",
      contentHash: await sha256Utf8(content),
      content: "> Pushed paragraph.\n\nRendered reader comment",
    });
  });

  it("lists an HTML document's threads and cards beside its preview", async () => {
    const filePath = "docs/page.html";
    const { html, replies } = await serveHtmlFileToReader(filePath);
    const docId = `overlay:${WORKSPACE_UUID}:${REPO_UUID}:${base64Url(filePath)}`;
    server.use(
      http.get("/api/v1/workspaces/:wsId/files/*", () =>
        HttpResponse.json({
          data: {
            id: docId,
            workspaceId: WORKSPACE_UUID,
            repoId: REPO_UUID,
            filePath,
            name: "page.html",
            title: "Page",
            content: html,
            hash: "doc-v1",
            encoding: "utf-8",
            size: 100,
            source: "committed",
            repoOid: "oid-pushed",
            snapshotAt: null,
          },
        }),
      ),
    );
    renderRestoredTab(
      { type: "doc", docId, name: "page.html" },
      `doc:${docId}`,
    );
    await expectHtmlThreadsBesidePreview(replies);
  });

  it("lists a Markdown file's threads and cards beside its FILES Preview", async () => {
    const filePath = "docs/guide.md";
    const markdown = "# Guide\nSecond line\nThird\n";
    const { replies } = await serveHtmlFileToReader(filePath, markdown);
    server.use(
      http.get("/api/v1/workspaces/:id/files/*", () =>
        HttpResponse.json({
          data: {
            path: filePath,
            content: markdown,
            size: markdown.length,
            encoding: "utf-8",
            hash: "file-v1",
            source: "committed",
            repoOid: "oid-base",
            snapshotAt: null,
            vmId: null,
          },
        }),
      ),
    );
    renderRestoredTab(
      { type: "file", filePath, name: "guide.md" },
      `file:${filePath}`,
    );
    // jsdom has no scrollIntoView; record the elements it is asked of.
    const scrolled: Element[] = [];
    const scrollIntoView = Element.prototype.scrollIntoView;
    Element.prototype.scrollIntoView = function (this: Element) {
      scrolled.push(this);
    };
    try {
      await expectHtmlThreadsBesidePreview(
        replies,
        "file-view-preview-threads",
      );
      expect(screen.getByTestId("markdown-preview")).toHaveTextContent("Guide");
      // The card on line 2 is in the margin beside the document, level with
      // its paragraph; expanding it set it level again rather than scrolling.
      expect(scrolled).toEqual([]);
      expect(
        screen
          .getByTestId("file-view-preview-threads")
          .querySelector('[data-margin-item="thread:html-thread-1"]'),
      ).toHaveAttribute("data-active", "true");
      // Its line label opens plain text at that line and starts no draft.
      const panel = screen.getByTestId("file-view-preview-threads");
      await userEvent.click(
        within(panel).getByRole("button", {
          name: "Open line 2 in plain text",
        }),
      );
      await waitFor(() =>
        expect(screen.getByTestId("file-view-switch-mode")).toHaveTextContent(
          "Switch to rich text",
        ),
      );
      expect(screen.queryByTestId("file-view-preview-threads")).toBeNull();
      expect(screen.queryByLabelText("Comment")).toBeNull();
    } finally {
      Element.prototype.scrollIntoView = scrollIntoView;
    }
  });
});
