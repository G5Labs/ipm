import { useState } from "react";
import {
  act,
  fireEvent,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { server } from "../test/msw/server";
import { renderApp } from "../test/render";
import { CreateProjectDocModal } from "./views/CreateProjectDocModal";
import { createMockDocNode } from "../test/fixtures";
import { WorkspaceNamesProvider } from "../hooks/useWorkspaceNames";

const DOC_BRANCH_REPO_ID = "22222222-2222-4222-8222-222222222222";

function defaultModalProps(
  overrides: Partial<React.ComponentProps<typeof CreateProjectDocModal>> = {},
) {
  return {
    open: true,
    workspaceId: "test-workspace",
    onClose: vi.fn(),
    ...overrides,
  };
}

function fileDataTransfer(files: File[], types: string[] = ["Files"]) {
  return { files, types, dropEffect: "none" };
}

describe("CreateProjectDocModal", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("defaults to plain doc mode", () => {
    const props = defaultModalProps();
    renderApp(<CreateProjectDocModal {...props} />, {
      route: "/org-g5labs/test-workspace",
    });

    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: "Create Doc" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Plain doc")).toBeInTheDocument();
    expect(screen.getByText("Upload a doc")).toBeInTheDocument();
    expect(screen.queryByText("Linked doc")).not.toBeInTheDocument();
    expect(screen.queryByText("Plain Doc Type")).not.toBeInTheDocument();
    expect(screen.queryByText("Persona")).not.toBeInTheDocument();
    expect(
      screen.queryByPlaceholderText("Search ontology nodes"),
    ).not.toBeInTheDocument();
  });

  it("does not expose linked or typed project-doc controls", () => {
    const props = defaultModalProps();

    renderApp(<CreateProjectDocModal {...props} />, {
      route: "/org-g5labs/test-workspace",
    });

    expect(screen.getByText("Plain doc")).toBeInTheDocument();
    expect(screen.getByText("Upload a doc")).toBeInTheDocument();
    expect(screen.queryByText("Linked doc")).not.toBeInTheDocument();
    expect(screen.queryByText("Plain Doc Type")).not.toBeInTheDocument();
    expect(screen.queryByText("Persona")).not.toBeInTheDocument();
  });

  it("creates a local-mode plain doc with the default design type", async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    const props = defaultModalProps({
      onClose,
    });

    server.use(
      http.post("/api/v1/workspaces/:wid/docs", async ({ request }) => {
        const body = (await request.json()) as Record<string, unknown>;
        expect(body).toMatchObject({
          name: "local-notes.md",
          docType: "design",
          filePath: "docs/local-notes.md",
          content: "",
        });
        return HttpResponse.json({
          data: {
            id: "doc-local-1",
            name: "local-notes.md",
            docType: "design",
            filePath: "docs/local-notes.md",
            workspaceId: "test-workspace",
            repoId: DOC_BRANCH_REPO_ID,
            linkedNodeCount: 0,
            lastUpliftedAt: null,
            lastDownliftedAt: null,
            contentUpdatedAt: null,
            createdAt: "2026-04-13T00:00:00Z",
            updatedAt: "2026-04-13T00:00:00Z",
          },
        });
      }),
    );

    renderApp(<CreateProjectDocModal {...props} />, {
      route: "/org-g5labs/test-workspace",
    });

    expect(screen.queryByText("Plain Doc Type")).not.toBeInTheDocument();
    fireEvent.change(screen.getByPlaceholderText("e.g., auth-review"), {
      target: { value: "Local Notes" },
    });
    await user.click(screen.getByRole("button", { name: "Create Doc" }));

    await waitFor(() => {
      expect(onClose).toHaveBeenCalledTimes(1);
    });
  });

  it("creates a plain doc through the docs endpoint", async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    const onCreated = vi.fn();
    const props = defaultModalProps({ onClose, onCreated });

    server.use(
      http.post("/api/v1/workspaces/:wid/docs", async ({ request }) => {
        const body = (await request.json()) as Record<string, unknown>;
        expect(body).toMatchObject({
          name: "architecture-notes.md",
          docType: "design",
          filePath: "docs/architecture-notes.md",
          content: "",
        });
        return HttpResponse.json({
          data: {
            id: "doc-1",
            name: "architecture-notes.md",
            docType: "design",
            filePath: "docs/architecture-notes.md",
            workspaceId: "test-workspace",
            repoId: DOC_BRANCH_REPO_ID,
            linkedNodeCount: 0,
            lastUpliftedAt: null,
            lastDownliftedAt: null,
            contentUpdatedAt: null,
            createdAt: "2026-04-13T00:00:00Z",
            updatedAt: "2026-04-13T00:00:00Z",
          },
        });
      }),
    );

    renderApp(<CreateProjectDocModal {...props} />, {
      route: "/org-g5labs/test-workspace",
    });

    fireEvent.change(screen.getByPlaceholderText("e.g., auth-review"), {
      target: { value: "Architecture Notes.md" },
    });
    expect(screen.getByText("docs/architecture-notes.md")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Create Doc" }));

    await waitFor(() => {
      expect(onClose).toHaveBeenCalledTimes(1);
    });
    expect(onCreated).toHaveBeenCalledWith(
      expect.objectContaining({
        id: expect.stringMatching(/^overlay:/),
        name: "architecture-notes.md",
        filePath: "docs/architecture-notes.md",
        folder: "docs",
      }),
    );
  });

  it("defaults plain docs into the workspace's dedicated folder when defaultFolderPath is set", async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    const wsFolder = "docs/11111111-1111-4111-8111-111111111111";
    const props = defaultModalProps({
      onClose,
      defaultFolderPath: wsFolder,
    });

    server.use(
      http.post("/api/v1/workspaces/:wid/docs", async ({ request }) => {
        const body = (await request.json()) as Record<string, unknown>;
        expect(body).toMatchObject({
          name: "roadmap-notes.md",
          docType: "design",
          filePath: `${wsFolder}/roadmap-notes.md`,
          content: "",
        });
        return HttpResponse.json({
          data: {
            id: "doc-ws-1",
            name: "roadmap-notes.md",
            docType: "design",
            filePath: `${wsFolder}/roadmap-notes.md`,
            workspaceId: "test-workspace",
            repoId: DOC_BRANCH_REPO_ID,
            linkedNodeCount: 0,
            lastUpliftedAt: null,
            lastDownliftedAt: null,
            contentUpdatedAt: null,
            createdAt: "2026-06-12T00:00:00Z",
            updatedAt: "2026-06-12T00:00:00Z",
          },
        });
      }),
    );

    renderApp(<CreateProjectDocModal {...props} />, {
      route: "/org-g5labs/test-workspace",
    });

    fireEvent.change(screen.getByPlaceholderText("e.g., auth-review"), {
      target: { value: "Roadmap Notes" },
    });
    // The preview shows the workspace-folder destination path.
    expect(
      screen.getByText(`${wsFolder}/roadmap-notes.md`),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Create Doc" }));

    await waitFor(() => {
      expect(onClose).toHaveBeenCalledTimes(1);
    });
  });

  it("preview shows the workspace name but submits the docs/<id> UUID path (D8/D9)", async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    const wsId = "11111111-1111-4111-8111-111111111111";
    const wsFolder = `docs/${wsId}`;
    const props = defaultModalProps({ onClose, defaultFolderPath: wsFolder });

    let submittedFilePath: unknown = null;
    server.use(
      http.post("/api/v1/workspaces/:wid/docs", async ({ request }) => {
        const body = (await request.json()) as Record<string, unknown>;
        submittedFilePath = body.filePath;
        return HttpResponse.json({
          data: {
            id: "doc-ws-2",
            name: "roadmap-notes.md",
            docType: "design",
            filePath: `${wsFolder}/roadmap-notes.md`,
            workspaceId: "test-workspace",
            repoId: DOC_BRANCH_REPO_ID,
            linkedNodeCount: 0,
            lastUpliftedAt: null,
            lastDownliftedAt: null,
            contentUpdatedAt: null,
            createdAt: "2026-06-12T00:00:00Z",
            updatedAt: "2026-06-12T00:00:00Z",
          },
        });
      }),
    );

    renderApp(
      <WorkspaceNamesProvider value={{ [wsId]: "compliance-service" }}>
        <CreateProjectDocModal {...props} />
      </WorkspaceNamesProvider>,
      { route: "/org-g5labs/test-workspace" },
    );

    fireEvent.change(screen.getByPlaceholderText("e.g., auth-review"), {
      target: { value: "Roadmap Notes" },
    });
    // Preview substitutes the workspace name for the UUID segment (D9)...
    expect(
      screen.getByText("docs/compliance-service/roadmap-notes.md"),
    ).toBeInTheDocument();
    expect(
      screen.queryByText(`${wsFolder}/roadmap-notes.md`),
    ).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Create Doc" }));

    // ...but the submitted filePath keeps the docs/<workspace_id> convention (D2).
    await waitFor(() => {
      expect(submittedFilePath).toBe(`${wsFolder}/roadmap-notes.md`);
    });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("uploads a markdown project doc through the docs endpoint", async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    const onCreated = vi.fn();
    const props = defaultModalProps({ onClose, onCreated });

    server.use(
      http.post("/api/v1/workspaces/:wid/docs", async ({ request }) => {
        const body = (await request.json()) as Record<string, unknown>;
        expect(body).toMatchObject({
          name: "uploaded-notes.md",
          docType: "design",
          filePath: "docs/uploaded-notes.md",
          content: "# Uploaded notes\n\nImported content.",
        });
        return HttpResponse.json({
          data: {
            id: "doc-upload-1",
            name: "uploaded-notes.md",
            docType: "design",
            filePath: "docs/uploaded-notes.md",
            workspaceId: "test-workspace",
            repoId: DOC_BRANCH_REPO_ID,
            linkedNodeCount: 0,
            lastUpliftedAt: null,
            lastDownliftedAt: null,
            contentUpdatedAt: null,
            createdAt: "2026-04-13T00:00:00Z",
            updatedAt: "2026-04-13T00:00:00Z",
          },
        });
      }),
    );

    renderApp(<CreateProjectDocModal {...props} />, {
      route: "/org-g5labs/test-workspace",
    });

    await user.click(screen.getByRole("button", { name: /Upload a doc/ }));
    expect(screen.getByLabelText("Upload a doc")).toHaveAttribute(
      "accept",
      ".md,.html,.htm,.jsonl,text/markdown,text/html",
    );
    await user.upload(
      screen.getByLabelText("Upload a doc"),
      new File(["# Uploaded notes\n\nImported content."], "Uploaded Notes.md", {
        type: "text/markdown",
      }),
    );
    expect(screen.getByText("docs/uploaded-notes.md")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Upload Doc" }));

    await waitFor(() => {
      expect(onClose).toHaveBeenCalledTimes(1);
    });
    expect(onCreated).toHaveBeenCalledWith(
      expect.objectContaining({
        id: expect.stringMatching(/^overlay:/),
        filePath: "docs/uploaded-notes.md",
        content: "# Uploaded notes\n\nImported content.",
      }),
    );
  });

  // Workbench g5.workbench.meeting: transcript mode stores a meeting
  // transcript as a Markdown Project Doc under `meetings/`.
  it("transcript mode accepts text and subtitle files and stores them as Markdown under meetings/", async () => {
    // `applyAccept: false` so the component's own extension check is what
    // rejects the HTML file, not user-event's `accept` filter.
    const user = userEvent.setup({ applyAccept: false });
    const onClose = vi.fn();
    const onCreated = vi.fn();
    const created: Record<string, unknown>[] = [];
    const props = defaultModalProps({
      onClose,
      onCreated,
      initialMode: "transcript",
      defaultFolderPath: "docs/ws-1",
    });

    server.use(
      http.post("/api/v1/workspaces/:wid/docs", async ({ request }) => {
        const body = (await request.json()) as Record<string, unknown>;
        created.push(body);
        return HttpResponse.json({
          data: createMockDocNode({
            id: "doc-transcript-1",
            name: String(body.name),
            docType: "design",
            filePath: String(body.filePath),
            workspaceId: "test-workspace",
            repoId: DOC_BRANCH_REPO_ID,
          }),
        });
      }),
    );

    renderApp(<CreateProjectDocModal {...props} />, {
      route: "/org-g5labs/test-workspace",
    });

    // A dedicated flow: no Doc Kind picker, its own copy and extensions.
    expect(screen.queryByText("Plain doc")).not.toBeInTheDocument();
    expect(screen.getByText(/Upload a meeting transcript/)).toBeInTheDocument();
    const input = screen.getByLabelText("Upload a transcript");
    expect(input).toHaveAttribute(
      "accept",
      ".md,.txt,.vtt,.srt,text/plain,text/markdown,text/vtt",
    );

    // Project-doc extensions that are not transcripts are refused here.
    await user.upload(
      input,
      new File(["<html></html>"], "slides.html", { type: "text/html" }),
    );
    expect(
      screen.getByText(/Unsupported files: slides.html/),
    ).toBeInTheDocument();

    // A subtitle file lands as Markdown with the cues in one fenced block.
    await user.upload(
      input,
      new File(
        ["WEBVTT\n\n00:01.000 --> 00:02.000\nWe ship Friday"],
        "UI Discussion 2026-10-06.vtt",
        {
          type: "text/vtt",
        },
      ),
    );
    expect(
      screen.getByText("docs/ws-1/meetings/ui-discussion-2026-10-06.md"),
    ).toBeInTheDocument();
    await user.click(
      screen.getByRole("button", { name: "Upload and extract decisions" }),
    );

    await waitFor(() => expect(onClose).toHaveBeenCalledTimes(1));
    expect(created).toEqual([
      {
        name: "ui-discussion-2026-10-06.md",
        docType: "design",
        filePath: "docs/ws-1/meetings/ui-discussion-2026-10-06.md",
        content:
          "```text\nWEBVTT\n\n00:01.000 --> 00:02.000\nWe ship Friday\n```\n",
      },
    ]);
    expect(onCreated).toHaveBeenCalledWith(
      expect.objectContaining({
        filePath: "docs/ws-1/meetings/ui-discussion-2026-10-06.md",
      }),
    );
  });

  it("transcript mode takes one transcript at a time", async () => {
    const user = userEvent.setup();
    const props = defaultModalProps({
      initialMode: "transcript",
      defaultFolderPath: "docs/ws-1",
    });
    renderApp(<CreateProjectDocModal {...props} />, {
      route: "/org-g5labs/test-workspace",
    });
    const input = screen.getByLabelText("Upload a transcript");
    expect(input).not.toHaveAttribute("multiple");
    // Two dropped files are refused as a unit; one is queued.
    fireEvent.drop(screen.getByTestId("upload-file-drop-zone"), {
      dataTransfer: fileDataTransfer([
        new File(["a"], "one.txt", { type: "text/plain" }),
        new File(["b"], "two.txt", { type: "text/plain" }),
      ]),
    });
    expect(
      screen.getByText("Upload one transcript at a time."),
    ).toBeInTheDocument();
    expect(screen.queryAllByTestId("upload-queue-item")).toHaveLength(0);
    await user.upload(
      input,
      new File(["a"], "one.txt", { type: "text/plain" }),
    );
    expect(screen.getByText("docs/ws-1/meetings/one.md")).toBeInTheDocument();
    fireEvent.drop(screen.getByTestId("upload-file-drop-zone"), {
      dataTransfer: fileDataTransfer([
        new File(["b"], "two.txt", { type: "text/plain" }),
      ]),
    });
    expect(screen.getAllByTestId("upload-queue-item")).toHaveLength(1);
    expect(
      screen.getByText("Upload one transcript at a time."),
    ).toBeInTheDocument();
  });

  it("transcript mode fences a subtitle file longer than any backtick run it holds", async () => {
    const user = userEvent.setup();
    const created: Record<string, unknown>[] = [];
    const props = defaultModalProps({
      initialMode: "transcript",
      defaultFolderPath: "docs/ws-1",
    });
    server.use(
      http.post("/api/v1/workspaces/:wid/docs", async ({ request }) => {
        const body = (await request.json()) as Record<string, unknown>;
        created.push(body);
        return HttpResponse.json({
          data: createMockDocNode({
            id: "doc-transcript-3",
            name: String(body.name),
            docType: "design",
            filePath: String(body.filePath),
            workspaceId: "test-workspace",
            repoId: DOC_BRANCH_REPO_ID,
          }),
        });
      }),
    );
    renderApp(<CreateProjectDocModal {...props} />, {
      route: "/org-g5labs/test-workspace",
    });
    await user.upload(
      screen.getByLabelText("Upload a transcript"),
      new File(
        ["1\n00:00:01,000 --> 00:00:02,000\n```\nfence inside\n"],
        "cues.srt",
        { type: "text/plain" },
      ),
    );
    await user.click(
      screen.getByRole("button", { name: "Upload and extract decisions" }),
    );
    await waitFor(() => expect(created).toHaveLength(1));
    expect(created[0]).toMatchObject({
      filePath: "docs/ws-1/meetings/cues.md",
      content:
        "````text\n1\n00:00:01,000 --> 00:00:02,000\n```\nfence inside\n````\n",
    });
  });

  it("transcript fencing handles a subtitle file with very many backtick runs", async () => {
    const { transcriptMarkdown } =
      await import("./views/CreateProjectDocModal");
    const body = "`x".repeat(200_000);
    const markdown = transcriptMarkdown("cues.vtt", body);
    expect(markdown.startsWith("```text\n")).toBe(true);
    expect(markdown.endsWith("\n```\n")).toBe(true);
    expect(transcriptMarkdown("cues.vtt", "````\n")).toBe(
      "`````text\n````\n`````\n",
    );
  });

  it("a sidebar launch after a transcript upload seeds ordinary upload rules", async () => {
    function Harness() {
      const [launch, setLaunch] = useState<{
        open: boolean;
        mode?: "transcript";
        files: readonly File[];
      }>({ open: true, mode: "transcript", files: [] });
      return (
        <>
          <button
            type="button"
            onClick={() => setLaunch({ open: false, files: [] })}
          >
            Close launch
          </button>
          <button
            type="button"
            onClick={() =>
              setLaunch({
                open: true,
                files: [
                  new File(["# One"], "ordinary.md", { type: "text/markdown" }),
                  new File(["<html></html>"], "page.html", {
                    type: "text/html",
                  }),
                ],
              })
            }
          >
            Drop into Project Docs
          </button>
          <CreateProjectDocModal
            {...defaultModalProps({
              open: launch.open,
              defaultFolderPath: "docs/ws-1",
              initialMode: launch.mode,
              initialUploadFiles: launch.files,
            })}
          />
        </>
      );
    }
    const user = userEvent.setup();
    renderApp(<Harness />, { route: "/org-g5labs/test-workspace" });
    expect(screen.getByLabelText("Upload a transcript")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Close launch" }));
    await user.click(
      screen.getByRole("button", { name: "Drop into Project Docs" }),
    );
    // Ordinary rules: both files, at the docs folder, HTML accepted.
    expect(
      await screen.findByText("docs/ws-1/ordinary.md"),
    ).toBeInTheDocument();
    expect(screen.getByText("docs/ws-1/page.html")).toBeInTheDocument();
    expect(screen.queryByText(/Unsupported files/)).not.toBeInTheDocument();
  });

  it("transcript mode keeps a plain-text transcript verbatim", async () => {
    const user = userEvent.setup();
    const created: Record<string, unknown>[] = [];
    const props = defaultModalProps({
      initialMode: "transcript",
      defaultFolderPath: "docs/ws-1",
    });
    server.use(
      http.post("/api/v1/workspaces/:wid/docs", async ({ request }) => {
        const body = (await request.json()) as Record<string, unknown>;
        created.push(body);
        return HttpResponse.json({
          data: createMockDocNode({
            id: "doc-transcript-2",
            name: String(body.name),
            docType: "design",
            filePath: String(body.filePath),
            workspaceId: "test-workspace",
            repoId: DOC_BRANCH_REPO_ID,
          }),
        });
      }),
    );
    renderApp(<CreateProjectDocModal {...props} />, {
      route: "/org-g5labs/test-workspace",
    });
    await user.upload(
      screen.getByLabelText("Upload a transcript"),
      new File(["Tim: timeline is the landing page.\n"], "notes.txt", {
        type: "text/plain",
      }),
    );
    await user.click(
      screen.getByRole("button", { name: "Upload and extract decisions" }),
    );
    await waitFor(() => expect(created).toHaveLength(1));
    expect(created[0]).toMatchObject({
      name: "notes.md",
      filePath: "docs/ws-1/meetings/notes.md",
      content: "Tim: timeline is the landing page.\n",
    });
  });

  it("uploads an HTML project doc through the docs endpoint", async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    const props = defaultModalProps({ onClose });

    server.use(
      http.post("/api/v1/workspaces/:wid/docs", async ({ request }) => {
        const body = (await request.json()) as Record<string, unknown>;
        expect(body).toMatchObject({
          name: "compliance-report.html",
          docType: "design",
          filePath: "docs/compliance-report.html",
          content:
            "<!doctype html><html><body><h1>Compliance</h1></body></html>",
        });
        return HttpResponse.json({
          data: {
            id: "doc-upload-html-1",
            name: "compliance-report.html",
            docType: "design",
            filePath: "docs/compliance-report.html",
            workspaceId: "test-workspace",
            repoId: DOC_BRANCH_REPO_ID,
            linkedNodeCount: 0,
            lastUpliftedAt: null,
            lastDownliftedAt: null,
            contentUpdatedAt: null,
            createdAt: "2026-04-13T00:00:00Z",
            updatedAt: "2026-04-13T00:00:00Z",
          },
        });
      }),
    );

    renderApp(<CreateProjectDocModal {...props} />, {
      route: "/org-g5labs/test-workspace",
    });

    await user.click(screen.getByRole("button", { name: /Upload a doc/ }));
    await user.upload(
      screen.getByLabelText("Upload a doc"),
      new File(
        ["<!doctype html><html><body><h1>Compliance</h1></body></html>"],
        "Compliance Report.html",
        { type: "text/html" },
      ),
    );
    expect(screen.getByText("docs/compliance-report.html")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Upload Doc" }));

    await waitFor(() => {
      expect(onClose).toHaveBeenCalledTimes(1);
    });
  });

  it("uploads a JSONL transcript project doc through the docs endpoint", async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    const props = defaultModalProps({ onClose });

    const transcript =
      '{"type":"user","message":{"role":"user","content":"hello"}}\n' +
      '{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"hi"}]}}\n';

    server.use(
      http.post("/api/v1/workspaces/:wid/docs", async ({ request }) => {
        const body = (await request.json()) as Record<string, unknown>;
        expect(body).toMatchObject({
          name: "claude-session.jsonl",
          docType: "design",
          filePath: "docs/claude-session.jsonl",
          content: transcript,
        });
        return HttpResponse.json({
          data: {
            id: "doc-upload-jsonl-1",
            name: "claude-session.jsonl",
            docType: "design",
            filePath: "docs/claude-session.jsonl",
            workspaceId: "test-workspace",
            repoId: DOC_BRANCH_REPO_ID,
            linkedNodeCount: 0,
            lastUpliftedAt: null,
            lastDownliftedAt: null,
            contentUpdatedAt: null,
            createdAt: "2026-04-13T00:00:00Z",
            updatedAt: "2026-04-13T00:00:00Z",
          },
        });
      }),
    );

    renderApp(<CreateProjectDocModal {...props} />, {
      route: "/org-g5labs/test-workspace",
    });

    await user.click(screen.getByRole("button", { name: /Upload a doc/ }));
    await user.upload(
      await screen.findByLabelText("Upload a doc"),
      new File([transcript], "Claude Session.jsonl", {
        type: "application/jsonl",
      }),
    );
    expect(
      await screen.findByText("docs/claude-session.jsonl"),
    ).toBeInTheDocument();
    await user.click(await screen.findByRole("button", { name: "Upload Doc" }));

    await waitFor(() => {
      expect(onClose).toHaveBeenCalledTimes(1);
    });
  });

  it("rejects an oversized upload with a clear error and issues no request", async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    const props = defaultModalProps({ onClose });

    let requestMade = false;
    server.use(
      http.post("/api/v1/workspaces/:wid/docs", async () => {
        requestMade = true;
        return HttpResponse.json({ data: {} });
      }),
    );

    renderApp(<CreateProjectDocModal {...props} />, {
      route: "/org-g5labs/test-workspace",
    });

    await user.click(screen.getByRole("button", { name: /Upload a doc/ }));

    // A 26 MiB HTML file is over the 25 MiB client ceiling. Override `size`
    // rather than allocating the bytes so the test stays fast.
    const oversized = new File(
      ["<!doctype html><html><body>big</body></html>"],
      "huge-report.html",
      { type: "text/html" },
    );
    Object.defineProperty(oversized, "size", { value: 26 * 1024 * 1024 });

    await user.upload(screen.getByLabelText("Upload a doc"), oversized);
    await user.click(screen.getByRole("button", { name: "Upload Doc" }));

    // The user sees a clear size error instead of an indefinite spinner...
    expect(
      await screen.findByText(/maximum upload size is 25\.0 MB/),
    ).toBeInTheDocument();
    // ...and no network request was ever issued, and the modal stays open.
    expect(requestMade).toBe(false);
    expect(onClose).not.toHaveBeenCalled();
  });

  it("appends, normalizes, deduplicates, preflights known paths, and removes queued files", async () => {
    const user = userEvent.setup();
    const props = defaultModalProps();
    const rendered = renderApp(<CreateProjectDocModal {...props} />, {
      route: "/org-g5labs/test-workspace",
    });
    rendered.queryClient.setQueryData(
      ["docs", "test-workspace"],
      [{ id: "existing-doc", filePath: "docs/existing.md" }],
    );

    await user.click(screen.getByRole("button", { name: /Upload a doc/ }));
    const picker = screen.getByLabelText("Upload a doc");
    expect(picker).toHaveAttribute("multiple");
    fireEvent.change(picker, {
      target: {
        files: [
          new File(["# A"], "A Notes.md", { type: "text/markdown" }),
          new File(["<html>B</html>"], "Report.HTML", {
            type: "text/html",
          }),
        ],
      },
    });

    expect(screen.getByText("docs/a-notes.md")).toBeInTheDocument();
    expect(screen.getByText("docs/report.html")).toBeInTheDocument();

    fireEvent.change(picker, {
      target: {
        files: [
          new File(["duplicate"], "a-notes.md", {
            type: "text/markdown",
          }),
          new File(["invalid"], "draft.txt", { type: "text/plain" }),
          new File(["known"], "Existing.md", { type: "text/markdown" }),
          new File(["# C"], "Later.md", { type: "text/markdown" }),
        ],
      },
    });

    expect(screen.getAllByTestId("upload-queue-item")).toHaveLength(3);
    expect(
      screen.getByText(/Duplicate destinations: a-notes\.md/),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/Unsupported files: draft\.txt/),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/Already in Project Docs: Existing\.md/),
    ).toBeInTheDocument();
    expect(screen.getByText("docs/later.md")).toBeInTheDocument();

    await user.click(
      screen.getByRole("button", { name: "Remove Report.HTML" }),
    );
    expect(screen.queryByText("docs/report.html")).not.toBeInTheDocument();
    expect(screen.getAllByTestId("upload-queue-item")).toHaveLength(2);
  });

  it("drops onto the existing picker surface, appends across picker/drop selections, and clears drag styling", async () => {
    const user = userEvent.setup();
    let requestMade = false;
    server.use(
      http.post("/api/v1/workspaces/:wid/docs", () => {
        requestMade = true;
        return HttpResponse.json({ data: {} });
      }),
    );
    renderApp(<CreateProjectDocModal {...defaultModalProps()} />, {
      route: "/org-g5labs/test-workspace",
    });

    await user.click(screen.getByRole("button", { name: /Upload a doc/ }));
    const dropZone = screen.getByTestId("upload-file-drop-zone");
    expect(
      screen.getByText(
        "Choose .md, .html, or .jsonl files, or drag and drop them here",
      ),
    ).toBeInTheDocument();
    const droppedFiles = [
      new File(["# Dropped"], "Dropped Notes.md", {
        type: "text/markdown",
      }),
      new File(["<html>Dropped</html>"], "Dropped Report.HTML", {
        type: "text/html",
      }),
    ];

    fireEvent.dragEnter(dropZone, {
      dataTransfer: fileDataTransfer(droppedFiles),
    });
    expect(screen.getByText("Drop files here")).toBeInTheDocument();
    expect(dropZone).toHaveClass("ring-[var(--g5-active)]");

    fireEvent.drop(dropZone, {
      dataTransfer: fileDataTransfer(droppedFiles),
    });
    expect(screen.queryByText("Drop files here")).not.toBeInTheDocument();
    expect(screen.getByText("docs/dropped-notes.md")).toBeInTheDocument();
    expect(screen.getByText("docs/dropped-report.html")).toBeInTheDocument();

    await user.upload(
      screen.getByLabelText("Upload a doc"),
      new File(["# Picked"], "Picked.md", { type: "text/markdown" }),
    );
    fireEvent.drop(dropZone, {
      dataTransfer: fileDataTransfer([
        new File(["duplicate"], "dropped-notes.md", {
          type: "text/markdown",
        }),
        new File(["unsupported"], "ignored.txt", { type: "text/plain" }),
        new File(["# Later"], "Later.md", { type: "text/markdown" }),
      ]),
    });

    expect(screen.getAllByTestId("upload-queue-item")).toHaveLength(4);
    expect(screen.getByText("docs/picked.md")).toBeInTheDocument();
    expect(screen.getByText("docs/later.md")).toBeInTheDocument();
    expect(
      screen.getByText(/Duplicate destinations: dropped-notes\.md/),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/Unsupported files: ignored\.txt/),
    ).toBeInTheDocument();
    expect(requestMade).toBe(false);

    fireEvent.dragEnter(dropZone, {
      dataTransfer: fileDataTransfer(droppedFiles),
    });
    fireEvent.dragLeave(dropZone, {
      dataTransfer: fileDataTransfer(droppedFiles),
    });
    expect(screen.queryByText("Drop files here")).not.toBeInTheDocument();
  });

  it("ignores empty and non-file drops on the picker surface", async () => {
    const user = userEvent.setup();
    renderApp(<CreateProjectDocModal {...defaultModalProps()} />, {
      route: "/org-g5labs/test-workspace",
    });
    await user.click(screen.getByRole("button", { name: /Upload a doc/ }));
    const dropZone = screen.getByTestId("upload-file-drop-zone");

    fireEvent.dragEnter(dropZone, {
      dataTransfer: fileDataTransfer([], ["text/plain"]),
    });
    fireEvent.drop(dropZone, {
      dataTransfer: fileDataTransfer([], ["text/plain"]),
    });
    fireEvent.drop(dropZone, {
      dataTransfer: fileDataTransfer([]),
    });

    expect(screen.queryAllByTestId("upload-queue-item")).toHaveLength(0);
    expect(screen.queryByText("Drop files here")).not.toBeInTheDocument();
  });

  it("applies the existing per-file, path, and aggregate constraints to dropped files", async () => {
    const user = userEvent.setup();
    const rendered = renderApp(
      <CreateProjectDocModal {...defaultModalProps()} />,
      { route: "/org-g5labs/test-workspace" },
    );
    rendered.queryClient.setQueryData(
      ["docs", "test-workspace"],
      [{ id: "known-doc", filePath: "docs/known.md" }],
    );
    await user.click(screen.getByRole("button", { name: /Upload a doc/ }));
    const dropZone = screen.getByTestId("upload-file-drop-zone");
    const oversized = new File(["oversized"], "oversized.md", {
      type: "text/markdown",
    });
    Object.defineProperty(oversized, "size", { value: 26 * 1024 * 1024 });

    fireEvent.drop(dropZone, {
      dataTransfer: fileDataTransfer([
        new File(["known"], "Known.md", { type: "text/markdown" }),
        new File(["unsupported"], "unsupported.txt", {
          type: "text/plain",
        }),
        oversized,
        new File(["# Valid"], "valid.md", { type: "text/markdown" }),
      ]),
    });

    expect(screen.getByText("docs/valid.md")).toBeInTheDocument();
    expect(
      screen.getByText(/Already in Project Docs: Known\.md/),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/Unsupported files: unsupported\.txt/),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/maximum upload size is 25\.0 MB: oversized\.md/),
    ).toBeInTheDocument();

    const aggregateOverflow = Array.from({ length: 5 }, (_, index) => {
      const file = new File(["aggregate"], `aggregate-${index}.md`, {
        type: "text/markdown",
      });
      Object.defineProperty(file, "size", { value: 21 * 1024 * 1024 });
      return file;
    });
    fireEvent.drop(dropZone, {
      dataTransfer: fileDataTransfer(aggregateOverflow),
    });
    expect(
      screen.getByText(
        /A batch can contain at most 20 files and 100\.0 MB total/,
      ),
    ).toBeInTheDocument();
    expect(screen.getAllByTestId("upload-queue-item")).toHaveLength(1);
  });

  it("cancels a seeded sidebar batch without issuing a request", async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    let requestMade = false;
    server.use(
      http.post("/api/v1/workspaces/:wid/docs", () => {
        requestMade = true;
        return HttpResponse.json({ data: {} });
      }),
    );
    renderApp(
      <CreateProjectDocModal
        {...defaultModalProps({
          onClose,
          initialUploadFiles: [
            new File(["# Cancelled"], "Cancelled.md", {
              type: "text/markdown",
            }),
          ],
        })}
      />,
      { route: "/org-g5labs/test-workspace" },
    );

    await screen.findByText("docs/cancelled.md");
    await user.click(screen.getByRole("button", { name: "Cancel" }));

    expect(onClose).toHaveBeenCalledTimes(1);
    expect(requestMade).toBe(false);
  });

  it("seeds an initial sidebar batch once per open cycle and replaces it on a later launch", async () => {
    const firstBatch = [
      new File(["# One"], "One.md", { type: "text/markdown" }),
      new File(["<html>Two</html>"], "Two.html", { type: "text/html" }),
    ];
    const secondBatch = [
      new File(["# Three"], "Three.md", { type: "text/markdown" }),
    ];

    function SeedHarness() {
      const [launch, setLaunch] = useState({
        open: true,
        files: firstBatch as readonly File[],
      });
      return (
        <>
          <button
            type="button"
            onClick={() =>
              setLaunch((current) => ({
                ...current,
                files: [...current.files],
              }))
            }
          >
            Rerender seed
          </button>
          <button
            type="button"
            onClick={() => setLaunch({ open: false, files: [] })}
          >
            Close seed
          </button>
          <button
            type="button"
            onClick={() => setLaunch({ open: true, files: secondBatch })}
          >
            Reopen seed
          </button>
          <CreateProjectDocModal
            {...defaultModalProps({
              open: launch.open,
              defaultFolderPath: "docs/test-workspace/nested",
              initialUploadFiles: launch.files,
            })}
          />
        </>
      );
    }

    const user = userEvent.setup();
    renderApp(<SeedHarness />, {
      route: "/org-g5labs/test-workspace",
    });

    expect(
      await screen.findByText("docs/test-workspace/nested/one.md"),
    ).toBeInTheDocument();
    expect(
      screen.getByText("docs/test-workspace/nested/two.html"),
    ).toBeInTheDocument();
    expect(screen.getAllByTestId("upload-queue-item")).toHaveLength(2);

    await user.click(screen.getByRole("button", { name: "Rerender seed" }));
    expect(screen.getAllByTestId("upload-queue-item")).toHaveLength(2);

    await user.click(screen.getByRole("button", { name: "Close seed" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Reopen seed" }));

    expect(
      await screen.findByText("docs/test-workspace/nested/three.md"),
    ).toBeInTheDocument();
    expect(screen.queryByText(/\/one\.md$/)).not.toBeInTheDocument();
    expect(screen.getAllByTestId("upload-queue-item")).toHaveLength(1);
  });

  it("consumes but ignores picker-surface drops while an upload is running", async () => {
    const releaseRequest: { current?: () => void } = {};
    const onClose = vi.fn();
    server.use(
      http.post("/api/v1/workspaces/:wid/docs", async () => {
        await new Promise<void>((resolve) => {
          releaseRequest.current = resolve;
        });
        return HttpResponse.json({
          data: {
            id: "uploaded-running",
            name: "running.md",
            docType: "design",
            filePath: "docs/running.md",
            workspaceId: "test-workspace",
            repoId: DOC_BRANCH_REPO_ID,
          },
        });
      }),
    );
    const user = userEvent.setup();
    renderApp(
      <CreateProjectDocModal
        {...defaultModalProps({
          onClose,
          initialUploadFiles: [
            new File(["# Running"], "Running.md", {
              type: "text/markdown",
            }),
          ],
        })}
      />,
      { route: "/org-g5labs/test-workspace" },
    );

    await screen.findByText("docs/running.md");
    await user.click(screen.getByRole("button", { name: "Upload Doc" }));
    await screen.findByText("Uploading 1 of 1…");

    const dropZone = screen.getByTestId("upload-file-drop-zone");
    fireEvent.drop(dropZone, {
      dataTransfer: fileDataTransfer([
        new File(["# Ignored"], "Ignored.md", { type: "text/markdown" }),
      ]),
    });
    expect(screen.queryByText("docs/ignored.md")).not.toBeInTheDocument();
    expect(screen.getAllByTestId("upload-queue-item")).toHaveLength(1);

    await waitFor(() => {
      expect(releaseRequest.current).toBeTypeOf("function");
    });
    releaseRequest.current?.();
    await waitFor(() => {
      expect(onClose).toHaveBeenCalledTimes(1);
    });
  });

  it("accepts the exact 20-file and 100 MiB batch boundary and rejects the next picker addition as a unit", async () => {
    const user = userEvent.setup();
    renderApp(<CreateProjectDocModal {...defaultModalProps()} />, {
      route: "/org-g5labs/test-workspace",
    });
    await user.click(screen.getByRole("button", { name: /Upload a doc/ }));
    const picker = screen.getByLabelText("Upload a doc");
    const boundaryFiles = Array.from({ length: 20 }, (_, index) => {
      const file = new File([`# ${index}`], `boundary-${index}.md`, {
        type: "text/markdown",
      });
      Object.defineProperty(file, "size", { value: 5 * 1024 * 1024 });
      return file;
    });

    fireEvent.change(picker, { target: { files: boundaryFiles } });
    expect(screen.getAllByTestId("upload-queue-item")).toHaveLength(20);

    fireEvent.change(picker, {
      target: {
        files: [
          new File(["# rejected"], "rejected-as-a-unit.md", {
            type: "text/markdown",
          }),
        ],
      },
    });
    expect(
      screen.getByText(/Selection rejected.*rejected-as-a-unit\.md/),
    ).toBeInTheDocument();
    expect(screen.getAllByTestId("upload-queue-item")).toHaveLength(20);
    expect(
      screen.queryByText("docs/rejected-as-a-unit.md"),
    ).not.toBeInTheDocument();
  });

  it("uploads strictly sequentially, announces progress, refreshes once, and closes multi-file success without opening a doc", async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    const onCreated = vi.fn();
    const releases: Array<() => void> = [];
    const requestNames: string[] = [];
    let activeRequests = 0;
    let maxActiveRequests = 0;
    server.use(
      http.post("/api/v1/workspaces/:wid/docs", async ({ request }) => {
        const body = (await request.json()) as Record<string, string>;
        requestNames.push(body.name);
        activeRequests += 1;
        maxActiveRequests = Math.max(maxActiveRequests, activeRequests);
        await new Promise<void>((resolve) => releases.push(resolve));
        activeRequests -= 1;
        return HttpResponse.json({
          data: {
            id: `doc-${body.name}`,
            name: body.name,
            docType: "design",
            filePath: body.filePath,
            workspaceId: "test-workspace",
            repoId: DOC_BRANCH_REPO_ID,
            linkedNodeCount: 0,
            createdAt: "2026-08-03T00:00:00Z",
            updatedAt: "2026-08-03T00:00:00Z",
          },
        });
      }),
    );

    const rendered = renderApp(
      <CreateProjectDocModal {...defaultModalProps({ onClose, onCreated })} />,
      { route: "/org-g5labs/test-workspace" },
    );
    const invalidateSpy = vi.spyOn(rendered.queryClient, "invalidateQueries");
    await user.click(screen.getByRole("button", { name: /Upload a doc/ }));
    fireEvent.change(screen.getByLabelText("Upload a doc"), {
      target: {
        files: [
          new File(["# One"], "one.md", { type: "text/markdown" }),
          new File(["<html>Two</html>"], "two.html", {
            type: "text/html",
          }),
        ],
      },
    });
    await user.click(screen.getByRole("button", { name: "Upload Docs" }));

    expect(await screen.findByRole("status")).toHaveTextContent(
      "Uploading 1 of 2…",
    );
    await waitFor(() => expect(releases).toHaveLength(1));
    expect(screen.getByRole("button", { name: "Close" })).toBeDisabled();
    expect(screen.getByTestId("create-doc-mode-plain")).toBeDisabled();
    await user.keyboard("{Escape}");
    fireEvent.click(screen.getByRole("dialog").parentElement!);
    expect(onClose).not.toHaveBeenCalled();

    releases[0]();
    await waitFor(() => expect(releases).toHaveLength(2));
    expect(screen.getByRole("status")).toHaveTextContent("Uploading 2 of 2…");
    releases[1]();

    await waitFor(() => expect(onClose).toHaveBeenCalledTimes(1));
    expect(requestNames).toEqual(["one.md", "two.html"]);
    expect(maxActiveRequests).toBe(1);
    expect(onCreated).not.toHaveBeenCalled();
    // One refresh per upload run: the docs list and the Changes report.
    expect(invalidateSpy).toHaveBeenCalledTimes(2);
    expect(
      rendered.queryClient.getQueryData<unknown[]>(["docs", "test-workspace"]),
    ).toHaveLength(2);
  });

  it("continues after 409, validation, network, and server failures, then retries only failed files", async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    const onCreated = vi.fn();
    const calls: string[] = [];
    const attempts = new Map<string, number>();
    server.use(
      http.post("/api/v1/workspaces/:wid/docs", async ({ request }) => {
        const body = (await request.json()) as Record<string, string>;
        const attempt = (attempts.get(body.name) ?? 0) + 1;
        attempts.set(body.name, attempt);
        calls.push(body.name);
        if (attempt === 1) {
          if (body.name === "conflict.md") {
            return HttpResponse.json(
              {
                error: {
                  code: "CONFLICT",
                  message: "Destination already exists",
                  details: {},
                  requestId: "req_conflict",
                },
              },
              { status: 409 },
            );
          }
          if (body.name === "validation.md") {
            return HttpResponse.json(
              {
                error: {
                  code: "VALIDATION_ERROR",
                  message: "Validation failed",
                  details: {},
                  requestId: "req_validation",
                },
              },
              { status: 422 },
            );
          }
          if (body.name === "network.md") return HttpResponse.error();
          if (body.name === "server.md") {
            return HttpResponse.json(
              {
                error: {
                  code: "INTERNAL_ERROR",
                  message: "Server failed",
                  details: {},
                  requestId: "req_server",
                },
              },
              { status: 500 },
            );
          }
        }
        return HttpResponse.json({
          data: {
            id: `doc-${body.name}`,
            name: body.name,
            docType: "design",
            filePath: body.filePath,
            workspaceId: "test-workspace",
            repoId: DOC_BRANCH_REPO_ID,
            linkedNodeCount: 0,
            createdAt: "2026-08-03T00:00:00Z",
            updatedAt: "2026-08-03T00:00:00Z",
          },
        });
      }),
    );

    const rendered = renderApp(
      <CreateProjectDocModal {...defaultModalProps({ onClose, onCreated })} />,
      { route: "/org-g5labs/test-workspace" },
    );
    const invalidateSpy = vi.spyOn(rendered.queryClient, "invalidateQueries");
    await user.click(screen.getByRole("button", { name: /Upload a doc/ }));
    const names = [
      "conflict.md",
      "validation.md",
      "network.md",
      "server.md",
      "success.html",
    ];
    fireEvent.change(screen.getByLabelText("Upload a doc"), {
      target: {
        files: names.map(
          (name) =>
            new File([name], name, {
              type: name.endsWith(".html") ? "text/html" : "text/markdown",
            }),
        ),
      },
    });
    await user.click(screen.getByRole("button", { name: "Upload Docs" }));

    expect(
      await screen.findByRole("button", { name: "Retry failed" }),
    ).toBeInTheDocument();
    expect(screen.getAllByText(/· failed$/)).toHaveLength(4);
    expect(screen.getAllByText(/· succeeded$/)).toHaveLength(1);
    expect(calls).toEqual(names);
    expect(onClose).not.toHaveBeenCalled();
    expect(onCreated).not.toHaveBeenCalled();
    expect(invalidateSpy).toHaveBeenCalledTimes(2);
    expect(
      screen.queryByRole("button", { name: "Remove success.html" }),
    ).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Retry failed" }));
    await waitFor(() => expect(onClose).toHaveBeenCalledTimes(1));
    expect(calls).toEqual([...names, ...names.slice(0, 4)]);
    expect(attempts.get("success.html")).toBe(1);
    expect(invalidateSpy).toHaveBeenCalledTimes(4);
    expect(onCreated).not.toHaveBeenCalled();
  });

  it("starts one retry run when Retry failed is clicked twice in the same tick", async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    const calls: string[] = [];
    const attempts = new Map<string, number>();
    let activeRequests = 0;
    let maxActiveRequests = 0;
    const releaseFirstRetry: { current?: () => void } = {};

    server.use(
      http.post("/api/v1/workspaces/:wid/docs", async ({ request }) => {
        const body = (await request.json()) as Record<string, string>;
        const attempt = (attempts.get(body.name) ?? 0) + 1;
        attempts.set(body.name, attempt);
        calls.push(body.name);

        if (attempt === 1) {
          return HttpResponse.json(
            {
              error: {
                code: "CONFLICT",
                message: "Destination already exists",
                details: {},
                requestId: `req_conflict_${body.name}`,
              },
            },
            { status: 409 },
          );
        }

        activeRequests += 1;
        maxActiveRequests = Math.max(maxActiveRequests, activeRequests);
        if (body.name === "one.md") {
          await new Promise<void>((resolve) => {
            releaseFirstRetry.current = resolve;
          });
        }
        activeRequests -= 1;

        return HttpResponse.json({
          data: {
            id: `doc-${body.name}-${attempt}`,
            name: body.name,
            docType: "design",
            filePath: body.filePath,
            workspaceId: "test-workspace",
            repoId: DOC_BRANCH_REPO_ID,
            linkedNodeCount: 0,
            createdAt: "2026-08-04T00:00:00Z",
            updatedAt: "2026-08-04T00:00:00Z",
          },
        });
      }),
    );

    renderApp(<CreateProjectDocModal {...defaultModalProps({ onClose })} />, {
      route: "/org-g5labs/test-workspace",
    });
    await user.click(screen.getByRole("button", { name: /Upload a doc/ }));
    fireEvent.change(screen.getByLabelText("Upload a doc"), {
      target: {
        files: [
          new File(["# One"], "one.md", { type: "text/markdown" }),
          new File(["# Two"], "two.md", { type: "text/markdown" }),
        ],
      },
    });
    await user.click(screen.getByRole("button", { name: "Upload Docs" }));

    const retryButton = await screen.findByRole("button", {
      name: "Retry failed",
    });
    expect(calls).toEqual(["one.md", "two.md"]);

    // Both native events enter the same rendered retry closure before React
    // can apply the disabled/running state. The synchronous queue lock, not a
    // render, must reject the second invocation (spec D14 / TC-WDF-014).
    await act(async () => {
      retryButton.dispatchEvent(
        new MouseEvent("click", { bubbles: true, cancelable: true }),
      );
      retryButton.dispatchEvent(
        new MouseEvent("click", { bubbles: true, cancelable: true }),
      );
    });

    await waitFor(() =>
      expect(releaseFirstRetry.current).toBeTypeOf("function"),
    );
    releaseFirstRetry.current?.();

    await waitFor(() => expect(onClose).toHaveBeenCalledTimes(1));
    expect(calls).toEqual(["one.md", "two.md", "one.md", "two.md"]);
    expect(attempts.get("one.md")).toBe(2);
    expect(attempts.get("two.md")).toBe(2);
    expect(maxActiveRequests).toBe(1);
  });

  it("stops after the in-flight request settles and resumes only the remaining queued files", async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    const calls: string[] = [];
    let releaseFirst: (() => void) | null = null;
    server.use(
      http.post("/api/v1/workspaces/:wid/docs", async ({ request }) => {
        const body = (await request.json()) as Record<string, string>;
        calls.push(body.name);
        if (calls.length === 1) {
          await new Promise<void>((resolve) => {
            releaseFirst = resolve;
          });
        }
        return HttpResponse.json({
          data: {
            id: `doc-${body.name}`,
            name: body.name,
            docType: "design",
            filePath: body.filePath,
            workspaceId: "test-workspace",
            repoId: DOC_BRANCH_REPO_ID,
            linkedNodeCount: 0,
            createdAt: "2026-08-03T00:00:00Z",
            updatedAt: "2026-08-03T00:00:00Z",
          },
        });
      }),
    );

    renderApp(<CreateProjectDocModal {...defaultModalProps({ onClose })} />, {
      route: "/org-g5labs/test-workspace",
    });
    await user.click(screen.getByRole("button", { name: /Upload a doc/ }));
    fireEvent.change(screen.getByLabelText("Upload a doc"), {
      target: {
        files: ["first.md", "second.md", "third.md"].map(
          (name) => new File([name], name, { type: "text/markdown" }),
        ),
      },
    });
    await user.click(screen.getByRole("button", { name: "Upload Docs" }));
    await waitFor(() => expect(releaseFirst).not.toBeNull());
    await user.click(screen.getByRole("button", { name: "Stop" }));
    expect(screen.getByRole("button", { name: "Stopping…" })).toBeDisabled();
    releaseFirst!();

    expect(
      await screen.findByText(/Upload stopped after the current file/),
    ).toBeInTheDocument();
    expect(calls).toEqual(["first.md"]);
    const queue = screen.getByRole("list", { name: "Upload queue" });
    expect(within(queue).getAllByText(/· queued$/)).toHaveLength(2);
    expect(within(queue).getAllByText(/· succeeded$/)).toHaveLength(1);
    expect(onClose).not.toHaveBeenCalled();

    await user.click(screen.getByRole("button", { name: "Upload Docs" }));
    await waitFor(() => expect(onClose).toHaveBeenCalledTimes(1));
    expect(calls).toEqual(["first.md", "second.md", "third.md"]);
  });

  it("rejects an upload whose serialized JSON body exceeds the request cap and issues no request", async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    const props = defaultModalProps({ onClose });

    let requestMade = false;
    server.use(
      http.post("/api/v1/workspaces/:wid/docs", async () => {
        requestMade = true;
        return HttpResponse.json({ data: {} });
      }),
    );

    renderApp(<CreateProjectDocModal {...props} />, {
      route: "/org-g5labs/test-workspace",
    });

    await user.click(screen.getByRole("button", { name: /Upload a doc/ }));

    // 17 MiB of newlines passes the 25 MiB raw File.size gate but each "\n"
    // serializes to the two-byte "\\n" escape — ~34 MiB of JSON body, past
    // the 31 MiB request cap (spec D10 serialized-request gate). This is the
    // escape-heavy shape a raw-size cap alone cannot bound.
    const escapeHeavy = new File(
      ["\n".repeat(17 * 1024 * 1024)],
      "escape-heavy.jsonl",
      { type: "application/jsonl" },
    );

    await user.upload(
      await screen.findByLabelText("Upload a doc"),
      escapeHeavy,
    );
    await user.click(await screen.findByRole("button", { name: "Upload Doc" }));

    expect(
      await screen.findAllByText(/maximum upload request size is 31\.0 MB/),
    ).toHaveLength(2);
    expect(requestMade).toBe(false);
    expect(onClose).not.toHaveBeenCalled();
  }, 30_000);

  it("issues exactly one create request when the submit button is double-clicked", async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    const props = defaultModalProps({ onClose });

    let requestCount = 0;
    server.use(
      http.post("/api/v1/workspaces/:wid/docs", async () => {
        requestCount += 1;
        return HttpResponse.json({
          data: {
            id: "doc-upload-once-1",
            name: "double-click.jsonl",
            docType: "design",
            filePath: "docs/double-click.jsonl",
            workspaceId: "test-workspace",
            repoId: DOC_BRANCH_REPO_ID,
            linkedNodeCount: 0,
            lastUpliftedAt: null,
            lastDownliftedAt: null,
            contentUpdatedAt: null,
            createdAt: "2026-04-13T00:00:00Z",
            updatedAt: "2026-04-13T00:00:00Z",
          },
        });
      }),
    );

    renderApp(<CreateProjectDocModal {...props} />, {
      route: "/org-g5labs/test-workspace",
    });

    await user.click(screen.getByRole("button", { name: /Upload a doc/ }));
    await user.upload(
      await screen.findByLabelText("Upload a doc"),
      new File(['{"type":"mode","mode":"normal"}'], "double-click.jsonl", {
        type: "application/jsonl",
      }),
    );

    // Two NATIVE clicks in one act() so React flushes only once at the end:
    // both events enter the handler closure before any re-render can disable
    // the button — exercising the synchronous ref lock (spec D11; experience
    // note react_query_ispending_not_a_double_submit_guard). `user.click`
    // twice would flush a disabling re-render in between and mask a missing
    // lock.
    const submitButton = await screen.findByRole("button", {
      name: "Upload Doc",
    });
    await act(async () => {
      submitButton.dispatchEvent(
        new MouseEvent("click", { bubbles: true, cancelable: true }),
      );
      submitButton.dispatchEvent(
        new MouseEvent("click", { bubbles: true, cancelable: true }),
      );
    });

    await waitFor(() => {
      expect(onClose).toHaveBeenCalledTimes(1);
    });
    expect(requestCount).toBe(1);
  });

  it("closes on escape", async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    const props = defaultModalProps({ onClose });

    renderApp(<CreateProjectDocModal {...props} />, {
      route: "/org-g5labs/test-workspace",
    });

    await user.keyboard("{Escape}");

    expect(onClose).toHaveBeenCalledTimes(1);
  });
  // Workbench g5.workbench.meeting: the dialog's second source extracts a
  // transcript already under meetings/ without an upload.
  it("transcript mode offers the transcripts already in Project Docs and extracts the chosen one", async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    const onCreated = vi.fn();
    const onUseExisting = vi.fn();
    const stored = createMockDocNode({
      id: "doc-standup",
      name: "standup.md",
      filePath: "docs/ws-1/meetings/standup.md",
    });
    const onOpenDecisions = vi.fn();
    const decisions = createMockDocNode({
      id: "doc-standup-decisions",
      name: "standup.decisions.md",
      filePath: "docs/ws-1/meetings/standup.decisions.md",
    });
    const props = defaultModalProps({
      onClose,
      onCreated,
      initialMode: "transcript",
      defaultFolderPath: "docs/ws-1",
    });
    renderApp(
      <CreateProjectDocModal
        {...props}
        existingTranscripts={[stored]}
        onUseExisting={onUseExisting}
        decisionDocuments={[decisions]}
        onOpenDecisions={onOpenDecisions}
      />,
      { route: "/org-g5labs/test-workspace" },
    );

    // Earlier extractions are one press away.
    expect(screen.getByTestId("transcript-decisions-list")).toHaveTextContent(
      "standup.decisions.md",
    );

    const submit = screen.getByTestId("create-doc-submit");
    expect(submit).toHaveTextContent("Upload and extract decisions");
    await user.click(screen.getByTestId("transcript-source-existing"));
    expect(
      screen.queryByLabelText("Upload a transcript"),
    ).not.toBeInTheDocument();
    expect(submit).toHaveTextContent("Extract decisions");
    expect(submit).toBeDisabled();
    await user.click(screen.getByTestId("transcript-existing-doc-standup"));
    expect(submit).toBeEnabled();
    await user.click(submit);
    expect(onUseExisting).toHaveBeenCalledWith(stored);
    expect(onCreated).not.toHaveBeenCalled();
    expect(onClose).toHaveBeenCalled();
  });

  it("transcript mode opens an earlier decisions document from its list", async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    const onOpenDecisions = vi.fn();
    const decisions = createMockDocNode({
      id: "doc-standup-decisions",
      name: "standup.decisions.md",
      filePath: "docs/ws-1/meetings/standup.decisions.md",
    });
    const props = defaultModalProps({
      onClose,
      initialMode: "transcript",
      defaultFolderPath: "docs/ws-1",
    });
    renderApp(
      <CreateProjectDocModal
        {...props}
        decisionDocuments={[decisions]}
        onOpenDecisions={onOpenDecisions}
      />,
      { route: "/org-g5labs/test-workspace" },
    );
    await user.click(
      screen.getByTestId("transcript-decisions-doc-standup-decisions"),
    );
    expect(onOpenDecisions).toHaveBeenCalledWith(decisions);
    expect(onClose).toHaveBeenCalled();
  });
});
