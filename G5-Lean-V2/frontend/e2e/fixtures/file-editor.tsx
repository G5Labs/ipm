// Browser fixture uses production FileView, identity, draft store and bundled Monaco.
// The server is a controllable double: assertions observe its submitted bytes.
import React, { useState } from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { FileView } from "../../src/components/files/FileView";
import { currentFileTab } from "../../src/lib/currentFileTab";
import {
  fileDraftKey,
  getFileDraft,
  retainFileDraftsFor,
} from "../../src/components/files/fileDrafts";
import type { WorkspaceFileContent } from "../../src/hooks/useFiles";
import {
  ApiError,
  setApiToken,
  subscribeApiTokenChange,
} from "../../src/lib/api";
import { AuthProvider } from "../../src/hooks/useAuth";
import { installFetchInterceptor } from "../../src/mocks/fetch-interceptor";
import { monaco } from "../../src/components/editor/monacoRuntime";
import { ComposerOwner } from "../../src/components/comments/ComposerOwner";
import { HtmlPreviewShell } from "../../src/components/files/HtmlPreviewShell";
import { OntologyChangesView } from "../../src/components/editor/OntologyChangesView";
import type { ReviewSuggestion } from "../../src/lib/types";
import "../../src/index.css";

installFetchInterceptor();
const params = new URLSearchParams(window.location.search);
const htmlMode = params.has("html");
const ontologyMode = params.has("ontology");
// `changed`: a baseline with Modified, Deleted and Added statements.
// `all-changes`: the same comparison on the All changes page instead of a file.
const allChangesMode = ontologyMode && params.has("all-changes");
const changedMode = ontologyMode && (params.has("changed") || allChangesMode);
// `sentences`: browser.a changes inside a statement of several sentences
// (g5.documents.ontology.change-highlights).
const sentencesMode = changedMode && params.has("sentences");
const sentences = (text: string, current: boolean) =>
  sentencesMode
    ? current
      ? text.replace(
          "Modified statement with a [link](https://example.com/a).",
          "Agents push to the workspace branch. Only the owner may **force-push** to the [branch](https://example.com/a). The server rejects stale leases with a clear error.",
        )
      : text.replace(
          "Original statement.",
          "Agents push to the workspace branch. Each push needs a fresh lease from the server. The server rejects stale leases.",
        )
    : text;
const path = htmlMode
  ? "docs/shared.html"
  : ontologyMode
    ? ".g5/ontology/shared.g5.md"
    : "docs/shared.md";
let submitted: {
  content: string;
  hash: string | null;
  vmId?: string | null;
  commitAndPush?: boolean;
}[] = [];
let release: (() => void) | undefined;
let hold = false;
let fail = false;
let refresh: () => void;
let showComments = false;
let commentsEnabled = false;
const comments: unknown[] = [];
let suggestions: ReviewSuggestion[] = [];
// A statement or heading navigation request, as the page sends one.
let navigation: { label: string; seq: number } | null = null;
const initial = htmlMode
  ? `<!doctype html><html><body>
<script type="application/json" id="g5-doc-answers">{"choice":"before"}</script>
<button id="propose-answer" type="button">Propose answer</button>
<script>
let generation = null;
window.addEventListener('message', (event) => {
  if (event.data?.source === 'g5-html-doc-host' && event.data?.type === 'bridge-ready') {
    generation = event.data.generation;
    document.getElementById('propose-answer').dataset.ready = String(event.data.canSave);
  }
});
document.getElementById('propose-answer').addEventListener('click', () => {
  window.parent.postMessage({ source: 'g5-html-doc', version: 1, type: 'propose-answers',
    generation, requestId: 'answer-1', answersJson: '{"choice":"after"}' }, '*');
});
</script></body></html>`
  : changedMode
    ? sentences(
        '# Comparison browser\n\nReading context remains visible.\n\n<a id="browser.a"></a>\n[I · browser.a]\n\nModified statement with a [link](https://example.com/a).\n\n<a id="browser.b"></a>\n[D · browser.b]\n\nUnchanged statement.\n\nIts second paragraph.\n\n<a id="browser.c"></a>\n[I · browser.c]\n\nAdded statement.\n\n## Metadata\n\n### browser.a\n\n- human-provided\n- Original evidence.\n\n### browser.b\n\n- Other evidence.\n\n### browser.c\n\n- New evidence.\n',
        true,
      )
    : ontologyMode
      ? '# Comparison browser\n\nReading context remains visible.\n\n<a id="browser.a"></a>\n[I · browser.a]\n\nOriginal statement.\n\n<a id="browser.b"></a>\n[D · browser.b]\n\nUnchanged statement.\n\n## Metadata\n\n### browser.a\n\n- Original evidence.\n\n### browser.b\n\n- Other evidence.\n'
      : "# Shared café 中文 😀\r\n\r\nOriginal.\r\n";
const baselineContent = changedMode
  ? sentences(
      '# Comparison browser\n\n<a id="browser.a"></a>\n[I · browser.a]\n\nOriginal statement.\n\n<a id="browser.gone"></a>\n[D · browser.gone]\n\nDeleted statement.\n\n<a id="browser.b"></a>\n[D · browser.b]\n\nUnchanged statement.\n\nIts second paragraph.\n\n## Metadata\n\n### browser.a\n\n- human-provided\n- Original evidence.\n\n### browser.gone\n\n- Old evidence.\n\n### browser.b\n\n- Other evidence.\n',
      false,
    )
  : initial;
// Mock auth installs an opaque token, but a request made for a principal is
// refused under a token that does not name it (`refuseForAnotherPrincipal`).
// Name the signed-in person in the token so a reader's Push review is sent.
let currentPerson = "person";
const tokenFor = (person: string) =>
  `e30.${btoa(JSON.stringify({ sub: person, org: "org", exp: 9999999999 })).replace(/=+$/, "")}.sig`;
subscribeApiTokenChange((_previous, next) => {
  if (next === "mock-token")
    queueMicrotask(() => setApiToken(tokenFor(currentPerson)));
});
setApiToken(tokenFor(currentPerson));
// A reader's Push review lands here; the body is what the assertions read.
const pushed: unknown[] = [];
{
  const fixtureFetch = window.fetch;
  window.fetch = async (input, init) => {
    const url = new URL(
      input instanceof Request ? input.url : String(input),
      location.href,
    );
    if (url.pathname === "/api/v1/workspaces/ws/review/file-suggestions") {
      pushed.push(JSON.parse(String(init?.body ?? "{}")));
      return Response.json(
        {
          data: {
            publication: { id: "pub" },
            suggestions: [{ id: `s${pushed.length}` }],
          },
        },
        { status: 201 },
      );
    }
    return fixtureFetch(input, init);
  };
}
if (ontologyMode) {
  const fixtureFetch = window.fetch;
  window.fetch = (input, init) => {
    const url = new URL(
      input instanceof Request ? input.url : String(input),
      location.href,
    );
    if (
      url.pathname === "/api/v1/workspaces/ws/source-control/ontology-baseline"
    ) {
      return Promise.resolve(
        Response.json({
          data: {
            revision: "baseline",
            workspaceId: "ws",
            repoId: "repo",
            path,
            baselineCommit: "1234567890abcdef",
            sourceCommit: "head",
            parentCommit: "parent",
            content: baselineContent,
          },
        }),
      );
    }
    if (
      url.pathname === "/api/v1/workspaces/ws/source-control/ontology-changes"
    ) {
      return Promise.resolve(
        Response.json({
          data: {
            revision: "changes",
            repositories: [
              {
                repoId: "repo",
                repoName: "repo",
                sourceCommit: "head",
                parentCommit: "parent",
                baselineCommit: "1234567890abcdef",
              },
            ],
            files: [
              {
                repoId: "repo",
                path,
                source: "live",
                current: initial,
                baseline: baselineContent,
              },
            ],
          },
        }),
      );
    }
    return fixtureFetch(input, init);
  };
}
let file: WorkspaceFileContent = {
  path,
  content: initial,
  size: initial.length,
  encoding: "utf-8",
  hash: "v1",
  source: "live",
  repoOid: null,
  snapshotAt: null,
  vmId: "vm1",
};
(window as any).editorTest = {
  initial,
  monaco,
  submitted: () => submitted,
  pushed: () => pushed,
  enableComments: () => {
    commentsEnabled = true;
    refresh();
  },
  comments: () => comments,
  navigate: (label: string) => {
    navigation = { label, seq: (navigation?.seq ?? 0) + 1 };
    refresh();
  },
  suggestions: (next: ReviewSuggestion[]) => {
    suggestions = next;
    refresh();
  },
  hold: () => {
    hold = true;
  },
  release: () => {
    hold = false;
    release?.();
  },
  fail: (value: boolean) => {
    fail = value;
  },
  external: (content: string, vmId = "vm1") => {
    file = { ...file, content, hash: `external-${Date.now()}`, vmId };
    refresh();
  },
  large: (bytes: number) => {
    const row = "one paragraph of representative text.\n\n";
    file = {
      ...file,
      content: (
        "# Large\n\n" + row.repeat(Math.ceil(bytes / row.length))
      ).slice(0, bytes),
      hash: "large",
      size: bytes,
    };
    refresh();
  },
  draft: (person = "person") =>
    getFileDraft(
      fileDraftKey({ person, workspaceId: "ws", repoId: "repo", path }),
    ),
};
function App() {
  const [active, setActive] = useState(path);
  const [person, setPerson] = useState("person");
  const [owner, setOwner] = useState(true);
  const [writable, setWritable] = useState(true);
  const [revision, setRevision] = useState(0);
  refresh = () => setRevision((n) => n + 1);
  const openDoc = () => {
    const tab = currentFileTab({ type: "doc", docId: "doc", name: "Shared" }, [
      { id: "doc", name: "Shared", filePath: path, repoId: "repo" },
    ] as any);
    if (tab.type === "file") setActive(tab.filePath);
  };
  return (
    <div style={{ height: "100vh", display: "flex", flexDirection: "column" }}>
      <nav style={{ display: "flex", gap: 12 }}>
        <button onClick={openDoc}>Docs</button>
        <button onClick={() => setActive(path)}>Files</button>
        <button onClick={() => setActive("away")}>Away</button>
        <button onClick={() => setOwner((v) => !v)}>Change role</button>
        <button onClick={() => setWritable((v) => !v)}>
          Change permission
        </button>
        <button
          onClick={() => {
            currentPerson = "other";
            setApiToken(tokenFor("other"));
            retainFileDraftsFor("other");
            setPerson("other");
          }}
        >
          Change person
        </button>
      </nav>
      <div data-testid="center-panel" style={{ flex: 1, minHeight: 0 }}>
        {allChangesMode && (
          <OntologyChangesView
            workspaceId="ws"
            onNavigate={() => {}}
            onOpenFile={() => {}}
          />
        )}
        {!allChangesMode && active === path && (
          <FileView
            key={`${person}:${active}`}
            workspaceId="ws"
            repoId="repo"
            filePath={path}
            language={htmlMode ? "html" : "markdown"}
            renderPreview={
              htmlMode
                ? (html, actions) => (
                    <HtmlPreviewShell
                      displayPath={path}
                      title="Shared HTML preview"
                      html={html}
                      answerBridge={{
                        workspaceId: "ws",
                        filePath: path,
                        displayed: html,
                        canSave: actions.canSave,
                        onSave: actions.save,
                      }}
                    />
                  )
                : undefined
            }
            file={{ ...file }}
            principal={`${person}:org`}
            lineTarget={null}
            viewerOwnsWorkspace={owner}
            ownerVmRunning
            workspaceWritable={writable}
            onSave={async (content, hash, vmId, options) => {
              submitted.push({ content, hash, vmId, ...options });
              if (hold)
                await new Promise<void>((resolve) => {
                  release = resolve;
                });
              if (fail) throw new Error("Controlled save failure");
              if (vmId !== file.vmId) throw new Error("Workspace was replaced");
              if (hash !== file.hash)
                throw new ApiError({
                  status: 409,
                  code: "FILE_VERSION_CONFLICT",
                  message: "File changed",
                  details: { hash: file.hash, content: file.content },
                });
              file = { ...file, content, hash: `saved-${submitted.length}` };
              refresh();
              return { hash: file.hash };
            }}
            onReload={async () => ({ ...file })}
            onFetchCurrent={async () => ({ ...file })}
            onStartWorkspace={() => {}}
            previewProps={{
              currentUserId: "human",
              currentUserEmail: "human@example.com",
            }}
            editorProps={{
              showComments,
              comments: [],
              scrollToIntentRecord: navigation?.label ?? null,
              scrollToIntentRecordSeq: navigation?.seq,
              onToggleComments: () => {
                showComments = !showComments;
                refresh();
              },
              onAddComment: commentsEnabled
                ? async (comment) => {
                    comments.push(comment);
                  }
                : undefined,
              suggestionOverlay: suggestions.length
                ? {
                    workspaceId: "ws",
                    suggestions,
                    viewerOwnsWorkspace: owner,
                    viewerUserId: "human",
                    ownerVmRunning: true,
                    sourceWritable: writable,
                  }
                : undefined,
            }}
          />
        )}
      </div>
      <output hidden>{revision}</output>
    </div>
  );
}
createRoot(document.getElementById("root")!).render(
  <QueryClientProvider
    client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
  >
    <MemoryRouter>
      <AuthProvider>
        <ComposerOwner scope="file-editor-browser">
          <App />
        </ComposerOwner>
      </AuthProvider>
    </MemoryRouter>
  </QueryClientProvider>,
);
