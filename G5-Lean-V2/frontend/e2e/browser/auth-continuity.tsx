import React, { useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { AuthProvider, RequireAuth, useAuth } from "../../src/hooks/useAuth";
import { api } from "../../src/lib/api";
import {
  ChatPanel,
  createChatComposerDraft,
} from "../../src/components/chat/ChatPanel";
import { ComposerOwner } from "../../src/components/comments/ComposerOwner";
import { useComposerField } from "../../src/hooks/useComposerStore";
import {
  fileDraftKey,
  newFileDraftId,
  setFileDraft,
  useFileDraft,
} from "../../src/components/files/fileDrafts";
import "../../src/index.css";

function ScopedComment() {
  const [words, setWords] = useComposerField(
    { kind: "thread", id: "one" },
    "words",
    "",
  );
  return (
    <textarea
      aria-label="Organization comment"
      value={words}
      onChange={(e) => setWords(e.target.value)}
    />
  );
}

function Workspace() {
  const { user, principalKey, switchOrg, logout, isLoading } = useAuth();
  // The real WorkspacePage owns these objects across ChatPanel mounts.
  const chatDraft = useRef(createChatComposerDraft());
  const key = fileDraftKey({
    person: user!.id,
    workspaceId: "workspace-a",
    repoId: "repo",
    path: "draft.md",
  });
  const draft = useFileDraft(key);
  const [probe, setProbe] = useState("");
  return (
    <>
      <output aria-label="Principal">{principalKey}</output>
      <output aria-label="Recovery">{isLoading ? "retrying" : "ready"}</output>
      <button onClick={() => void switchOrg("org-a")}>Organization A</button>
      <button onClick={() => void switchOrg("org-b")}>Organization B</button>
      <button onClick={() => void logout()}>Sign out</button>
      <button
        onClick={() =>
          void api.get("/api/v1/continuity-probe").then(
            () => setProbe("success"),
            () => setProbe("refused"),
          )
        }
      >
        Refresh through API
      </button>
      <output aria-label="Probe">{probe}</output>
      <label>
        File draft
        <textarea
          aria-label="File draft"
          value={draft?.content ?? ""}
          onChange={(e) =>
            setFileDraft(key, {
              id: draft?.id ?? newFileDraftId(),
              role: "owner",
              content: e.target.value,
              baseline: { content: "", hash: null, vmId: null },
            })
          }
        />
      </label>
      <ComposerOwner scope="workspace-a">
        <ScopedComment />
      </ComposerOwner>
      <div style={{ height: 500 }}>
        <ChatPanel
          session={null}
          messages={[]}
          sessions={[]}
          isStreaming={false}
          scopeType="workspace"
          scopeRef="workspace-a"
          scopeLabel="Workspace A"
          draft={chatDraft.current}
          onSendMessage={() => false}
          onSummarize={() => {}}
          onSelectSession={() => {}}
        />
      </div>
    </>
  );
}
createRoot(document.getElementById("root")!).render(
  <QueryClientProvider
    client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
  >
    <MemoryRouter>
      <AuthProvider>
        <Routes>
          <Route
            path="/"
            element={
              <RequireAuth>
                <Workspace />
              </RequireAuth>
            }
          />
          <Route path="/login" element={<p>Sign in</p>} />
        </Routes>
      </AuthProvider>
    </MemoryRouter>
  </QueryClientProvider>,
);
