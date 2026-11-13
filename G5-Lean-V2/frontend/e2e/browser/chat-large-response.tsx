// The actual ChatPanel over the live-row placement path (`applyLiveTimelineRow`,
// the `chat.timeline` WebSocket handler's call) and the session-timeline query
// cache, fed thousands of deterministic live delta rows without an API or a
// socket. The full-stack counterpart is e2e/flows/chat-large-response.spec.ts;
// this page measures the browser's own cost and proves exact, duplicate-free
// rendering and reload-equivalent convergence from the saved rows.
import React, { useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import {
  QueryClient,
  QueryClientProvider,
  useQuery,
} from "@tanstack/react-query";
import { ChatPanel } from "../../src/components/chat/ChatPanel";
import { createMockChatSession } from "../../src/test/fixtures";
import { applyLiveTimelineRow } from "../../src/lib/ws";
import {
  retainLiveRows,
  type TimelineWindow,
} from "../../src/lib/liveTimeline";
import type { ChatMessageDTO, TimelineEntry } from "../../src/lib/types";
import "../../src/index.css";

const query = new URLSearchParams(location.search);
const count = Number(query.get("count") ?? "10000");
const bytes = Number(query.get("bytes") ?? "24");
/** Rows delivered per macrotask; 1 places each row like one socket frame. */
const chunk = Math.max(1, Number(query.get("chunk") ?? "1"));
/** With `pace=N`, each animation frame places N rows and commits once, the
 * way a steady stream does; 0 delivers as fast as the browser takes rows. */
const pace = Math.max(0, Number(query.get("pace") ?? "0"));
/** With `batches=1`, every 256th row also lands a saved-history refresh the
 * way the full stack does after each history batch: the refresh reads the
 * newest page and back until it meets a held saved row, so every row the
 * batch saved arrives as its saved copy and replaces its live row
 * (`retainLiveRows`); the window then holds the saved reply so far plus the
 * unsaved tail, every row once. */
const batches = query.get("batches") === "1";
const HISTORY_BATCH_MAX_EVENTS = 256;
document.documentElement.dataset.theme = "alpine-graphite";
document.documentElement.dataset.mode = query.get("mode") ?? "light";

const SESSION_ID = "session-large";
const ASSISTANT_ID = "assistant-large";
const TASK_ID = "task-large";
const BLOCK_ID = "block-large";
const QUERY_KEY = ["session-timeline", SESSION_ID];
const storageKey = `chat-large-response:${location.search}`;

/** Mirrors `stream_delta_text` in tools/fake-provider-cli/src/workflows.rs,
 * except that every `PARAGRAPH_EVERY`th delta ends a paragraph: prose has
 * paragraph breaks, and the transcript renders a streaming answer in stable
 * pieces split only there, so the browser's cost is measured on prose, not
 * on one unbroken paragraph. The token check reads words, not spacing. */
const PARAGRAPH_EVERY = 40;
function streamDeltaText(index: number, size: number): string {
  const filler = "abcdefghijklmnopqrstuvwxyz";
  const head = `w${String(index).padStart(5, "0")}`;
  let text = head;
  while (text.length + 1 < size) {
    text += filler[(text.length - head.length) % filler.length];
  }
  return index % PARAGRAPH_EVERY === 0 ? `${text}\n\n` : `${text} `;
}

const deltas = Array.from({ length: count }, (_, index) =>
  streamDeltaText(index + 1, bytes),
);
const fullText = deltas.join("");

function deltaRow(seq: number, live: boolean): TimelineEntry {
  const text = deltas[seq - 1];
  return {
    id: `row-${seq}`,
    entryType: "output",
    content: text,
    createdAt: new Date(Date.UTC(2026, 9, 30, 0, 0, 0, seq)).toISOString(),
    metadata: {
      schema_version: "g5.task-event.v1",
      exec_id: "exec-large",
      kind: "message.delta",
      payload: {
        message_id: "msg_large",
        block_id: BLOCK_ID,
        text_delta: text,
      },
      terminal: false,
      taskId: TASK_ID,
      attempt: 1,
      seq,
      messageId: ASSISTANT_ID,
      sessionId: SESSION_ID,
      projection_error: null,
      ...(live ? { live: true } : {}),
    },
  };
}

function finalRow(live: boolean): TimelineEntry {
  const seq = count + 1;
  return {
    id: `row-${seq}`,
    entryType: "agent_message",
    content: fullText,
    createdAt: new Date(Date.UTC(2026, 9, 30, 0, 0, 0, seq)).toISOString(),
    metadata: {
      schema_version: "g5.task-event.v1",
      exec_id: "exec-large",
      kind: "message.final",
      payload: {
        message_id: "msg_large",
        block_id: BLOCK_ID,
        content: fullText,
        content_truncated: false,
        in_reply_to_input_ids: ["input-large"],
      },
      terminal: false,
      taskId: TASK_ID,
      attempt: 1,
      seq,
      messageId: ASSISTANT_ID,
      sessionId: SESSION_ID,
      projection_error: null,
      ...(live ? { live: true } : {}),
    },
  };
}

type Phase = "empty" | "streaming" | "saved";
type Stats = {
  phase: Phase;
  /** Rows placed so far by the current delivery. */
  placed: number;
  /** Time (performance.now) after every thousandth placed row. */
  checkpointsMs: Record<number, number>;
  delivered: number;
  duplicatesDelivered: number;
  /** Emulated saved-history refreshes (`batches=1`) and the window size
   * after the latest one. */
  refreshes: number;
  heldRows: number;
  commits: number;
  /** React render+commit work (Profiler `actualDuration`) of the panel per
   * commit, binned by the rows held when the commit happened: `200` covers
   * commits at 100..300 held rows, `2000` those at 1,800..2,200, `10000`
   * those at 9,000..11,000. Per-commit cost must not grow with the rows
   * held (chat g5.chat.live-tail-recovery). */
  commitCost: Record<
    string,
    { commits: number; totalMs: number; maxMs: number }
  >;
  startedAt: number | null;
  deliveredAt: number | null;
  finalAt: number | null;
  busy: boolean;
};
const stats: Stats = {
  phase: (sessionStorage.getItem(storageKey) as Phase | null) ?? "empty",
  placed: 0,
  checkpointsMs: {},
  delivered: 0,
  duplicatesDelivered: 0,
  refreshes: 0,
  heldRows: 0,
  commits: 0,
  commitCost: {},
  startedAt: null,
  deliveredAt: null,
  finalAt: null,
  busy: false,
};

/** The held-row bucket a commit belongs to, if it is one the spec reads. */
function commitBucket(heldRows: number): string | null {
  if (heldRows >= 100 && heldRows <= 300) return "200";
  if (heldRows >= 1800 && heldRows <= 2200) return "2000";
  if (heldRows >= 9000 && heldRows <= 11000) return "10000";
  return null;
}

function recordCommitCost(actualDuration: number): void {
  const held =
    client.getQueryData<TimelineWindow>(QUERY_KEY)?.items.length ?? 0;
  const bucket = commitBucket(held);
  if (!bucket) return;
  const current = stats.commitCost[bucket] ?? {
    commits: 0,
    totalMs: 0,
    maxMs: 0,
  };
  current.commits += 1;
  current.totalMs += actualDuration;
  current.maxMs = Math.max(current.maxMs, actualDuration);
  stats.commitCost[bucket] = current;
}
(window as unknown as { __g5LargeResponse: Stats }).__g5LargeResponse = stats;

const client = new QueryClient({
  defaultOptions: { queries: { retry: false, staleTime: Infinity } },
});

/** Saved history as a reload reads it: every row persisted, no live marker. */
function savedWindow(): TimelineWindow {
  const items = deltas.map((_, index) => deltaRow(index + 1, false));
  items.push(finalRow(false));
  return { items, cursor: null, hasMore: false };
}

function initialWindow(): TimelineWindow {
  return stats.phase === "saved"
    ? savedWindow()
    : { items: [], cursor: null, hasMore: false };
}

/** Unclamped macrotasks, one row (or `chunk` rows) each, like socket frames. */
function deliver(seqs: number[], onDone: () => void): void {
  const channel = new MessageChannel();
  let next = 0;
  channel.port1.onmessage = () => {
    const end = Math.min(next + chunk, seqs.length);
    for (; next < end; next += 1) {
      const seq = seqs[next];
      applyLiveTimelineRow(client, SESSION_ID, deltaRow(seq, true));
      if (batches && seq % HISTORY_BATCH_MAX_EVENTS === 0) {
        const saved: TimelineWindow = {
          items: Array.from({ length: seq }, (_, index) =>
            deltaRow(index + 1, false),
          ),
          cursor: null,
          hasMore: false,
        };
        client.setQueryData<TimelineWindow>(QUERY_KEY, (old) =>
          retainLiveRows(old, saved),
        );
        stats.refreshes += 1;
        stats.heldRows =
          client.getQueryData<TimelineWindow>(QUERY_KEY)?.items.length ?? 0;
      }
    }
    stats.placed = next;
    if (next % 1000 === 0) stats.checkpointsMs[next] = performance.now();
    if (next >= seqs.length) onDone();
    else if (pace > 0 && next % pace === 0)
      requestAnimationFrame(() => channel.port2.postMessage(null));
    else channel.port2.postMessage(null);
  };
  channel.port2.postMessage(null);
}

function CommitCounter() {
  useEffect(() => {
    stats.commits += 1;
  });
  return null;
}

function App() {
  const [phase, setPhaseState] = useState<Phase>(stats.phase);
  const [, rerender] = useState(0);
  const setPhase = (next: Phase) => {
    stats.phase = next;
    sessionStorage.setItem(storageKey, next);
    setPhaseState(next);
  };
  const timeline = useQuery<TimelineWindow>({
    queryKey: QUERY_KEY,
    queryFn: initialWindow,
    initialData: initialWindow,
  });
  const entries = timeline.data?.items ?? [];
  const startedRef = useRef(false);

  const stream = () => {
    if (startedRef.current) return;
    startedRef.current = true;
    stats.busy = true;
    stats.placed = 0;
    stats.checkpointsMs = {};
    stats.startedAt = performance.now();
    setPhase("streaming");
    deliver(
      Array.from({ length: count }, (_, index) => index + 1),
      () => {
        stats.delivered = count;
        stats.deliveredAt = performance.now();
        stats.busy = false;
        rerender((n) => n + 1);
      },
    );
  };
  const duplicateBurst = () => {
    stats.busy = true;
    const burst = Array.from(
      { length: Math.min(1000, count) },
      (_, index) => index + 1,
    );
    deliver(burst, () => {
      stats.duplicatesDelivered += burst.length;
      stats.busy = false;
      rerender((n) => n + 1);
    });
  };
  const finish = () => {
    applyLiveTimelineRow(client, SESSION_ID, finalRow(true));
    stats.finalAt = performance.now();
    rerender((n) => n + 1);
  };
  const save = () => {
    client.setQueryData<TimelineWindow>(QUERY_KEY, savedWindow());
    setPhase("saved");
  };

  const session = createMockChatSession({
    id: SESSION_ID,
    agentProvider: "claude_code",
    model: null,
  });
  const live = phase === "streaming";
  const messages: ChatMessageDTO[] =
    phase === "empty"
      ? []
      : [
          {
            id: "user-large",
            sessionId: SESSION_ID,
            role: "user",
            content: `G5_STREAM_DELTAS_E2E:${count}:${bytes}`,
            artifacts: null,
            citations: null,
            inputTokens: null,
            outputTokens: null,
            status: "complete",
            messageIndex: 0,
            createdAt: "2026-10-30T00:00:00Z",
          },
          {
            id: ASSISTANT_ID,
            sessionId: SESSION_ID,
            role: "assistant",
            content: phase === "saved" ? fullText : "",
            artifacts: null,
            citations: null,
            inputTokens: null,
            outputTokens: null,
            status: phase === "saved" ? "complete" : "streaming",
            messageIndex: 1,
            createdAt: "2026-10-30T00:00:01Z",
            taskId: TASK_ID,
            attempt: 1,
          },
        ];
  return (
    <main
      style={{
        height: "100vh",
        background: "var(--g5-bg0)",
        color: "var(--g5-text0)",
        display: "flex",
        flexDirection: "column",
      }}
    >
      <CommitCounter />
      <nav
        aria-label="Fixture transport"
        style={{ display: "flex", gap: 12, padding: 8, flexWrap: "wrap" }}
      >
        <button onClick={stream}>Stream live rows</button>
        <button onClick={duplicateBurst}>Deliver duplicate burst</button>
        <button onClick={finish}>Deliver final block</button>
        <button onClick={save}>Saved rows only</button>
        <span data-testid="large-response-phase">{phase}</span>
      </nav>
      <div
        style={{
          flex: 1,
          minHeight: 0,
          width: 1050,
          maxWidth: "100%",
          margin: "0 auto",
        }}
      >
        <React.Profiler
          id="chat-panel"
          onRender={(_id, _phase, actualDuration) =>
            recordCommitCost(actualDuration)
          }
        >
          <ChatPanel
            layout="center"
            session={session}
            messages={messages}
            timelineEntries={entries}
            isStreaming={live}
            scopeType="workspace"
            scopeRef="workspace-fixture"
            scopeLabel="Fixture"
            sessions={[session]}
            selectedAgentProvider="claude_code"
            availableAgentProviders={["claude_code"]}
            setupError={null}
            composerDisabledReason={null}
            hasEarlierHistory={false}
            onSendMessage={() => {}}
            onStopStreaming={() => {}}
            onCreateSession={() => {}}
            onSummarize={() => {}}
            onSelectSession={() => {}}
          />
        </React.Profiler>
      </div>
    </main>
  );
}
createRoot(document.getElementById("root")!).render(
  <QueryClientProvider client={client}>
    <App />
  </QueryClientProvider>,
);
