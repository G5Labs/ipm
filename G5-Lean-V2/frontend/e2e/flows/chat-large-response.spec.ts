// =============================================================================
// Chat: a large streamed reply reaches the browser live, exactly once
// =============================================================================
// Acceptance for the live relay (chat g5.chat.live-delivery,
// g5.chat.history-batches, g5.chat.live-tail-recovery, g5.chat.replay): the
// fake Claude turn emits 10,000 text deltas — more rows than one journal
// replay page (1,024) and the follower's live tail buffer (4,096) hold — and
// the browser must render exactly the provider's text, keep one contiguous
// copy through a reload and a WebSocket reconnect while the turn is still
// streaming, read the saved history and the live tail a bounded number of
// times, converge in a CPU-throttled second tab, and show the saved
// transcript after the turn ends. The flow also measures the latency from the
// send to the first live delta and from the last live delta to the saved
// transcript, as test annotations and console output.
//
// Prompt directive (tools/fake-provider-cli/src/workflows.rs):
//   G5_STREAM_DELTAS_E2E:<count>:<bytes_per_delta>[:<delay_ms>]
// Each delta is a distinct word, `w` + five-digit index + filler letters +
// one space, exactly <bytes_per_delta> bytes; the final text block and the
// result carry the concatenation. 24-byte deltas keep the 240,000-byte reply
// under the guest's 256 KiB inline cap for a final text block
// (g5d-agent MAX_INLINE_TEXT_BYTES), so the rendered final text can be exact.
// =============================================================================

import { expect, test, type Page } from "@playwright/test";
import {
  authedApiViaRequest,
  captureChatTurn,
  createWorkspaceClone,
  deleteWorkspaceById,
  openChatComposer,
  openWorkspacePath,
  releaseChatTurn,
  runWorkspaceVmAction,
  softVisible,
  waitForWorkspaceVmStatus,
  type ChatTurnIdentity,
  type WorkspaceClone,
} from "../helpers";

const hasLlm =
  !!process.env.ANTHROPIC_API_KEY ||
  !!process.env.OPENROUTER_API_KEY ||
  !!process.env.OPENAI_API_KEY ||
  !!process.env.LLM_ENABLED;

/** 10,000 deltas by default; a lane may size the case through the
 * environment without touching the flow. */
const DELTA_COUNT = Number(process.env.G5_LARGE_RESPONSE_DELTAS ?? "10000");
const DELTA_BYTES = 24;
/** The paced variant streams ~4 ms apart: about 45 s for 10,000 deltas, so a
 * reload and a reconnect land while the provider is still emitting. */
const PACED_DELAY_MS = 4;
/** CPU slowdown of the second tab (CDP Emulation.setCPUThrottlingRate). */
const SLOW_TAB_CPU_RATE = 2;
/** History batches close at this many events (attempt_follower
 * BATCH_MAX_EVENTS); a burst of 10,000 deltas therefore saves in about 40
 * batches, and each saved batch may move the persisted watermark once. */
const HISTORY_BATCH_MAX_EVENTS = 256;
/** Frames around the deltas: init, thinking, block starts, final, result. */
const TURN_OVERHEAD_EVENTS = 16;
const EXPECTED_BATCHES = Math.ceil(
  (DELTA_COUNT + TURN_OVERHEAD_EVENTS) / HISTORY_BATCH_MAX_EVENTS,
);
/**
 * Saved-history and live-tail reads scale with saved batches (per persisted
 * watermark: one tail read, and a refresh that reads back until it meets a
 * held saved row, two pages for a 256-row batch) plus a constant for the
 * send, the turn end, reloads, reconnects and the hole fills a dropped range
 * needs — never with the delta count.
 */
const MAX_READS_PER_KIND = 3 * EXPECTED_BATCHES + 40;
/** Heap growth in tab one across the whole turn; 10,000 rows of ~1 KiB plus
 * React state stay far below this, a duplicated or leaking window does not. */
const MAX_HEAP_GROWTH_BYTES = 512 * 1024 * 1024;

/** Mirrors `stream_delta_text` in tools/fake-provider-cli/src/workflows.rs. */
function streamDeltaText(index: number, bytes: number): string {
  const filler = "abcdefghijklmnopqrstuvwxyz";
  const head = `w${String(index).padStart(5, "0")}`;
  let text = head;
  while (text.length + 1 < bytes) {
    text += filler[(text.length - head.length) % filler.length];
  }
  return `${text} `;
}

const EXPECTED_DELTAS = Array.from({ length: DELTA_COUNT }, (_, index) =>
  streamDeltaText(index + 1, DELTA_BYTES),
);
const EXPECTED_TEXT = EXPECTED_DELTAS.join("");
const EXPECTED_TOKENS = EXPECTED_DELTAS.map((delta) => delta.trimEnd());
const TOKEN_INDEX = new Map(
  EXPECTED_TOKENS.map((token, index) => [token, index]),
);
const TOKEN_PATTERN = /w\d{5}[a-z]+/g;

function streamPrompt(delayMs?: number): string {
  const base = `G5_STREAM_DELTAS_E2E:${DELTA_COUNT}:${DELTA_BYTES}`;
  return delayMs === undefined ? base : `${base}:${delayMs}`;
}

// ---------------------------------------------------------------------------
// Wire observation: saved-history and tail reads, live delta frames
// ---------------------------------------------------------------------------

type Wire = {
  liveTailReads: number;
  timelineReads: number;
  sockets: number;
  liveDeltas: number;
  duplicateLiveDeltas: number;
  firstLiveDeltaAt: number | null;
  lastLiveDeltaAt: number | null;
  liveDeltaIds: Set<string>;
};

function observeWire(page: Page): Wire {
  const wire: Wire = {
    liveTailReads: 0,
    timelineReads: 0,
    sockets: 0,
    liveDeltas: 0,
    duplicateLiveDeltas: 0,
    firstLiveDeltaAt: null,
    lastLiveDeltaAt: null,
    liveDeltaIds: new Set(),
  };
  page.on("request", (request) => {
    if (request.method() !== "GET") return;
    const pathname = new URL(request.url()).pathname;
    if (/\/api\/v1\/sessions\/[^/]+\/live-tail$/.test(pathname)) {
      wire.liveTailReads += 1;
    } else if (/\/api\/v1\/sessions\/[^/]+\/timeline$/.test(pathname)) {
      wire.timelineReads += 1;
    }
  });
  page.on("websocket", (socket) => {
    wire.sockets += 1;
    socket.on("framereceived", (frame) => {
      const payload =
        typeof frame.payload === "string"
          ? frame.payload
          : frame.payload.toString("utf8");
      if (
        !payload.includes('"chat.timeline"') ||
        !payload.includes('"message.delta"')
      )
        return;
      let event: {
        type?: string;
        entry?: { id?: string; metadata?: Record<string, unknown> };
      };
      try {
        event = JSON.parse(payload);
      } catch {
        return;
      }
      if (event.type !== "chat.timeline") return;
      const metadata = event.entry?.metadata ?? {};
      if (metadata.kind !== "message.delta" || metadata.live !== true) return;
      const now = Date.now();
      wire.firstLiveDeltaAt ??= now;
      wire.lastLiveDeltaAt = now;
      wire.liveDeltas += 1;
      const id = event.entry?.id;
      if (typeof id !== "string") return;
      if (wire.liveDeltaIds.has(id)) wire.duplicateLiveDeltas += 1;
      else wire.liveDeltaIds.add(id);
    });
  });
  return wire;
}

/** Keep every WebSocket the page opens reachable, so a test can drop the
 * live one and let the client's own backoff reconnect it. */
async function trackWebSockets(page: Page): Promise<void> {
  await page.addInitScript(() => {
    const sockets: WebSocket[] = [];
    (window as unknown as { __g5e2eSockets: WebSocket[] }).__g5e2eSockets =
      sockets;
    const Native = window.WebSocket;
    class TrackedWebSocket extends Native {
      constructor(url: string | URL, protocols?: string | string[]) {
        super(url, protocols);
        sockets.push(this);
      }
    }
    window.WebSocket = TrackedWebSocket;
  });
}

async function dropOpenWebSockets(page: Page): Promise<number> {
  return page.evaluate(() => {
    const sockets =
      (window as unknown as { __g5e2eSockets?: WebSocket[] }).__g5e2eSockets ??
      [];
    const open = sockets.filter(
      (socket) => socket.readyState === WebSocket.OPEN,
    );
    for (const socket of open) socket.close(4000, "e2e forced reconnect");
    return open.length;
  });
}

// ---------------------------------------------------------------------------
// Rendered output
// ---------------------------------------------------------------------------

/**
 * The delta words the last assistant message shows. A plain evaluate with no
 * deadline of its own: while the page drains a burst of live rows its main
 * thread is busy for long stretches, and the read must wait its turn rather
 * than report an empty reply.
 */
async function renderedTokens(page: Page): Promise<string[]> {
  const text = await page
    .evaluate(() => {
      const nodes = document.querySelectorAll(
        '[data-testid="assistant-message"]',
      );
      return nodes[nodes.length - 1]?.textContent ?? "";
    })
    .catch(() => "");
  return text.match(TOKEN_PATTERN) ?? [];
}

type Sample = {
  count: number;
  first: number;
  last: number;
  /** Places where the next rendered delta is not the one after its
   * predecessor: a hole (rows missing) or a repeat. */
  holes: number;
  duplicates: number;
  detail: string | null;
};

/** How the rendered deltas relate to the expected ones, 1-based. */
function sampleTokens(tokens: string[]): Sample {
  const indexes = tokens.map((token) => (TOKEN_INDEX.get(token) ?? -2) + 1);
  let holes = 0;
  let duplicates = 0;
  let detail: string | null = null;
  for (let offset = 1; offset < indexes.length; offset += 1) {
    const previous = indexes[offset - 1];
    const current = indexes[offset];
    if (current === previous + 1) continue;
    if (current <= previous) duplicates += 1;
    else holes += 1;
    detail ??= `delta ${previous} is followed by ${current} (${tokens[offset]})`;
  }
  if (indexes.some((index) => index <= 0)) detail ??= "unknown token rendered";
  return {
    count: tokens.length,
    first: indexes[0] ?? 0,
    last: indexes[indexes.length - 1] ?? 0,
    holes,
    duplicates,
    detail,
  };
}

/**
 * A hole may show for the moment a skipped row waits for its tail read, or
 * for the saved-history read that closes a hole persistence already covers
 * (chat g5.chat.live-tail-recovery); one that lasts longer than this is
 * missing output. The bound is a test choice: it leaves room for those reads
 * and their placement, not for a page that drains a backlog of live rows for
 * longer than this.
 */
const HOLE_TOLERANCE_MS = 10_000;

type Growth = Sample & { growths: number; maxHoleMs: number };

/**
 * Sample the rendered reply until it has grown twice, or is complete, while
 * measuring how long any hole or repeat persists. Nothing rendered within the
 * budget is a failure; a lasting hole is reported for the caller to judge, so
 * one run still measures everything after it.
 */
async function observeContiguousGrowth(
  page: Page,
  label: string,
  timeoutMs: number,
): Promise<Growth> {
  const started = Date.now();
  let previousLast = -1;
  let growths = 0;
  let holeSince: number | null = null;
  let maxHoleMs = 0;
  let holeDetail: string | null = null;
  let latest: Sample | null = null;
  while (Date.now() - started < timeoutMs) {
    const sample = sampleTokens(await renderedTokens(page));
    if (sample.count) {
      latest = sample;
      if (sample.holes || sample.duplicates) {
        holeSince ??= Date.now();
        const lasted = Date.now() - holeSince;
        if (lasted >= maxHoleMs) {
          maxHoleMs = lasted;
          holeDetail = sample.detail;
        }
      } else {
        holeSince = null;
      }
      if (sample.last > previousLast) {
        if (previousLast >= 0) growths += 1;
        previousLast = sample.last;
      }
      // Keep sampling while a hole is open, so its closing time is measured.
      const broken = sample.holes > 0 || sample.duplicates > 0;
      if (sample.last >= DELTA_COUNT) break;
      if (growths >= 2 && !broken) break;
    }
    await page.waitForTimeout(250);
  }
  if (!latest) {
    throw new Error(
      `${label}: nothing of the reply rendered within ${timeoutMs} ms`,
    );
  }
  record(
    `${label}: rendered`,
    `${latest.first}..${latest.last} (${latest.count} deltas, growths ${growths}, holes ${latest.holes}, repeats ${latest.duplicates}, longest hole ${maxHoleMs} ms${holeDetail ? `: ${holeDetail}` : ""})`,
  );
  // A repeat is never legitimate; a hole must close within the bound.
  expect(latest.duplicates, `${label}: no delta is rendered twice`).toBe(0);
  expect(
    maxHoleMs,
    `${label}: a hole in the rendered reply closes within ${HOLE_TOLERANCE_MS} ms`,
  ).toBeLessThanOrEqual(HOLE_TOLERANCE_MS);
  return { ...latest, growths, maxHoleMs };
}

async function expectExactRenderedReply(
  page: Page,
  label: string,
  timeoutMs: number,
): Promise<void> {
  await expect
    .poll(
      async () => {
        const sample = sampleTokens(await renderedTokens(page));
        if (sample.count !== DELTA_COUNT) return `${sample.count} deltas`;
        if (sample.holes || sample.duplicates) return sample.detail;
        return sample.first === 1 && sample.last === DELTA_COUNT
          ? "exact"
          : `run ${sample.first}..${sample.last}`;
      },
      {
        message: `${label}: the rendered reply is the provider's text exactly`,
        timeout: timeoutMs,
        intervals: [500, 1_000, 2_000],
      },
    )
    .toBe("exact");
}

// ---------------------------------------------------------------------------
// Server-side reads
// ---------------------------------------------------------------------------

type TaskStatusSample = { status: string; at: number };

/** The turn's task by id; the session's current task answers null once the
 * turn has ended, so it only stands in while the id is unknown. */
async function waitForTurnEnd(
  page: Page,
  turn: ChatTurnIdentity,
  timeoutMs: number,
): Promise<TaskStatusSample> {
  let sample: TaskStatusSample = { status: "", at: 0 };
  await expect
    .poll(
      async () => {
        const result = await authedApiViaRequest(
          page,
          turn.taskId
            ? `/api/v1/tasks/${turn.taskId}`
            : `/api/v1/sessions/${turn.sessionId}/task`,
        );
        if (result.status === 404) return "missing";
        const task = result.body?.data ?? result.body;
        if (!turn.taskId && task === null) return "ended";
        sample = { status: String(task?.status ?? ""), at: Date.now() };
        return sample.status;
      },
      { timeout: timeoutMs, intervals: [500, 1_000] },
    )
    .toMatch(/^(succeeded|failed|cancelled|ended)$/);
  if (sample.status === "") sample = { status: "ended", at: Date.now() };
  return sample;
}

type TranscriptMessage = {
  id: string;
  role: string;
  content: string;
  status: string;
  messageIndex: number;
  taskId?: string | null;
};

/** The turn's assistant transcript row, read like the UI's history. */
async function readAssistantTranscript(
  page: Page,
  turn: ChatTurnIdentity,
): Promise<TranscriptMessage | null> {
  const messages: TranscriptMessage[] = [];
  let cursor: number | undefined;
  for (let pages = 0; pages < 50; pages += 1) {
    const query = new URLSearchParams({ limit: "500" });
    if (cursor !== undefined) query.set("cursor", String(cursor));
    const result = await authedApiViaRequest(
      page,
      `/api/v1/sessions/${turn.sessionId}/messages?${query}`,
    );
    expect(result.status, "chat transcript must be readable").toBe(200);
    const batch = result.body?.data ?? result.body;
    messages.push(...((batch?.messages ?? []) as TranscriptMessage[]));
    if (!batch?.hasMore || batch.cursor === null || batch.cursor === undefined)
      break;
    cursor = Number(batch.cursor);
  }
  const input = messages.find(
    (row) => row.id === turn.inputId && row.role !== "assistant",
  );
  if (!input) return null;
  return (
    messages
      .filter(
        (row) =>
          row.role === "assistant" &&
          row.messageIndex > input.messageIndex &&
          (!turn.taskId || !row.taskId || row.taskId === turn.taskId),
      )
      .sort((left, right) => left.messageIndex - right.messageIndex)[0] ?? null
  );
}

async function waitForSavedReply(
  page: Page,
  turn: ChatTurnIdentity,
  timeoutMs: number,
): Promise<number> {
  let savedAt = 0;
  await expect
    .poll(
      async () => {
        const reply = await readAssistantTranscript(page, turn);
        if (!reply) return "no assistant row";
        if (reply.content.trimEnd() !== EXPECTED_TEXT.trimEnd())
          return `content ${reply.content.length} bytes, status ${reply.status}`;
        if (reply.status === "streaming" || reply.status === "pending")
          return `status ${reply.status}`;
        savedAt ||= Date.now();
        return "saved";
      },
      {
        message: "the saved transcript carries the provider's exact text",
        timeout: timeoutMs,
        intervals: [500, 1_000],
      },
    )
    .toBe("saved");
  return savedAt;
}

type StoredRow = {
  id: string;
  metadata?: {
    kind?: string;
    seq?: number;
    attempt?: number;
    payload?: { text_delta?: string; content?: string };
  };
};

/** Every stored row of the task, oldest first. */
async function readStoredRows(
  page: Page,
  taskId: string,
): Promise<StoredRow[]> {
  const rows: StoredRow[] = [];
  let cursor: string | undefined;
  for (let pages = 0; pages < 200; pages += 1) {
    const query = new URLSearchParams({ limit: "200" });
    if (cursor) query.set("cursor", cursor);
    const result = await authedApiViaRequest(
      page,
      `/api/v1/tasks/${taskId}/timeline?${query}`,
    );
    expect(result.status, "task timeline must be readable").toBe(200);
    const body = result.body?.data ?? result.body;
    rows.push(...((body?.entries ?? []) as StoredRow[]));
    if (!body?.hasMore || !body.cursor) break;
    cursor = String(body.cursor);
  }
  return rows;
}

/** Saved history holds each delta exactly once, in order, with the exact
 * text, and one final text block carrying the whole reply. */
async function expectStoredRowsExact(page: Page, taskId: string) {
  const rows = await readStoredRows(page, taskId);
  const byId = new Set<string>();
  for (const row of rows) {
    expect(byId.has(row.id), `stored row ${row.id} appears once`).toBe(false);
    byId.add(row.id);
  }
  const deltas = rows
    .filter((row) => row.metadata?.kind === "message.delta")
    .map((row) => ({
      seq: Number(row.metadata?.seq),
      attempt: Number(row.metadata?.attempt),
      text: String(row.metadata?.payload?.text_delta ?? ""),
    }))
    .sort((left, right) => left.seq - right.seq);
  expect(deltas.length, "every delta is saved exactly once").toBe(DELTA_COUNT);
  expect(new Set(deltas.map((delta) => delta.attempt)).size).toBe(1);
  for (let index = 1; index < deltas.length; index += 1) {
    expect(
      deltas[index].seq,
      `delta sequences are strictly increasing at ${index}`,
    ).toBeGreaterThan(deltas[index - 1].seq);
  }
  expect(deltas.map((delta) => delta.text).join("")).toBe(EXPECTED_TEXT);
  const finals = rows.filter((row) => row.metadata?.kind === "message.final");
  expect(finals.length, "one final text block").toBe(1);
  expect(String(finals[0].metadata?.payload?.content ?? "")).toBe(
    EXPECTED_TEXT,
  );
  return { rows: rows.length, lastSeq: deltas[deltas.length - 1].seq };
}

async function heapUsedBytes(page: Page): Promise<number> {
  const cdp = await page.context().newCDPSession(page);
  try {
    await cdp.send("Performance.enable");
    const { metrics } = await cdp.send("Performance.getMetrics");
    return (
      metrics.find((metric) => metric.name === "JSHeapUsedSize")?.value ?? 0
    );
  } finally {
    await cdp.detach().catch(() => {});
  }
}

function record(label: string, value: string | number) {
  const line = `${label}: ${value}`;
  test.info().annotations.push({ type: "large-response", description: line });
  console.log(`[chat-large-response] ${line}`);
}

function ms(from: number | null, to: number | null): string {
  return from && to ? `${to - from} ms` : "n/a";
}

// ---------------------------------------------------------------------------
// Chat driving
// ---------------------------------------------------------------------------

async function openWorkspaceChat(page: Page, path: string): Promise<void> {
  const resolved = page
    .waitForResponse(
      (response) =>
        response.url().includes("/api/v1/resolve") && response.status() < 400,
      { timeout: 20_000 },
    )
    .catch(() => null);
  await openWorkspacePath(page, path);
  await resolved;
  if (
    (await page.locator('dialog:visible, [role="dialog"]:visible').count()) > 0
  ) {
    await page.keyboard.press("Escape");
  }
  await openChatComposer(page);
}

/** Send `prompt` from the composer and return the turn and the send time. */
async function sendPrompt(
  page: Page,
  prompt: string,
  onTurn: (turn: ChatTurnIdentity) => void,
): Promise<{ turn: ChatTurnIdentity; sentAt: number }> {
  const newSessionButton = page
    .getByTestId("new-session-empty")
    .or(page.getByTestId("new-session-header"))
    .first();
  if (await softVisible(newSessionButton, 3_000)) {
    await newSessionButton.click();
    await page
      .getByTestId("new-session-empty")
      .waitFor({ state: "hidden", timeout: 10_000 })
      .catch(() => {});
  }
  const input = page.getByTestId("chat-input").first();
  await expect(input).toBeVisible({ timeout: 10_000 });
  await expect(input).toBeEnabled({ timeout: 60_000 });
  await input.fill(prompt);
  const [sendResponse] = await Promise.all([
    page.waitForResponse(
      (response) => {
        if (response.request().method() !== "POST") return false;
        const pathname = new URL(response.url()).pathname;
        return (
          /\/api\/v1\/sessions\/[^/]+\/messages$/.test(pathname) ||
          /\/api\/v1\/workspaces\/[^/]+\/sessions\/start$/.test(pathname)
        );
      },
      { timeout: 15_000 },
    ),
    input.press("Enter"),
  ]);
  const sentAt = Date.now();
  const turn = await captureChatTurn(page, sendResponse, onTurn);
  return { turn, sentAt };
}

/** Show the turn's chat in another tab of the same person. */
async function showChatIn(page: Page, path: string, prompt: string) {
  await openWorkspaceChat(page, path);
  const messages = page.getByTestId("chat-messages");
  if (await softVisible(messages.filter({ hasText: prompt }), 10_000)) return;
  const search = page.getByPlaceholder("Search sessions…");
  if (!(await search.isVisible().catch(() => false))) {
    // A tab that opened on the no-chat landing has the center picker trigger;
    // one showing a chat has the active-chat trigger.
    await page
      .getByTestId("chat-center-history")
      .or(page.getByTestId("chat-active-history"))
      .first()
      .click();
  }
  await expect(search).toBeVisible();
  await search.fill("G5_STREAM_DELTAS_E2E");
  const row = page.getByTestId("chat-session-select").first();
  await expect(row).toBeVisible({ timeout: 15_000 });
  await row.click();
  await expect(messages).toContainText("G5_STREAM_DELTAS_E2E", {
    timeout: 30_000,
  });
}

// ---------------------------------------------------------------------------
// Flows
// ---------------------------------------------------------------------------

test.describe("Chat large streamed response", () => {
  // Provisioning the clone's VM, the stream, and the browser's drain of
  // thousands of live rows (tens of milliseconds each) all fit in this budget;
  // the recorded timings say how much of it a run used.
  test.describe.configure({ timeout: 1_800_000 });
  test.skip(!hasLlm, "Requires the fake providers (set LLM_ENABLED)");

  let workspace: WorkspaceClone | undefined;
  let turn: ChatTurnIdentity | undefined;

  test.beforeEach(async ({ page }, testInfo) => {
    workspace = await createWorkspaceClone(page, {
      namePrefix: `chat-large-${testInfo.title.slice(0, 24)}`,
    });
    await runWorkspaceVmAction(page, workspace.id, "resume");
    await waitForWorkspaceVmStatus(page, workspace.id, "running");
  });

  test.afterEach(async ({ page }) => {
    try {
      if (turn) await releaseChatTurn(page, turn).catch(() => {});
    } finally {
      turn = undefined;
      if (workspace) {
        await deleteWorkspaceById(page, workspace.id).catch(() => {});
        workspace = undefined;
      }
    }
  });

  test("burst: 10,000 deltas render exactly once with bounded history and tail reads", async ({
    page,
  }) => {
    const wire = observeWire(page);
    await openWorkspaceChat(page, workspace!.path);
    const heapBefore = await heapUsedBytes(page);

    const sent = await sendPrompt(page, streamPrompt(), (created) => {
      turn = created;
    });
    turn = sent.turn;

    // Live delivery: the first delta reaches the page before anything is saved.
    await expect
      .poll(() => wire.firstLiveDeltaAt, { timeout: 120_000 })
      .not.toBeNull();
    record(
      "send → first live delta (wire)",
      ms(sent.sentAt, wire.firstLiveDeltaAt),
    );
    await expect
      .poll(async () => (await renderedTokens(page)).length, {
        timeout: 120_000,
      })
      .toBeGreaterThan(0);
    record("send → first delta rendered", ms(sent.sentAt, Date.now()));

    const ended = await waitForTurnEnd(page, turn, 600_000);
    expect(ended.status, "the streamed turn succeeds").toMatch(
      /^(succeeded|ended)$/,
    );
    record(
      "last live delta (wire) → task succeeded",
      ms(wire.lastLiveDeltaAt, ended.at),
    );
    const savedAt = await waitForSavedReply(page, turn, 180_000);
    record(
      "last live delta (wire) → saved transcript",
      ms(wire.lastLiveDeltaAt, savedAt),
    );
    record("send → saved transcript", ms(sent.sentAt, savedAt));

    await expectExactRenderedReply(page, "after the turn", 900_000);
    record("send → exact reply rendered", ms(sent.sentAt, Date.now()));
    record("task succeeded → exact reply rendered", ms(ended.at, Date.now()));
    const stored = await expectStoredRowsExact(page, turn.taskId);
    record("stored rows", stored.rows);
    record("live delta frames received", wire.liveDeltas);
    record("duplicate live delta frames", wire.duplicateLiveDeltas);
    record("live-tail reads", wire.liveTailReads);
    record("saved-history reads", wire.timelineReads);
    record("sockets", wire.sockets);
    const heapAfter = await heapUsedBytes(page);
    record("JS heap growth (bytes)", heapAfter - heapBefore);

    expect(
      wire.liveTailReads,
      `live-tail reads stay bounded by saved batches (≤ ${MAX_READS_PER_KIND})`,
    ).toBeLessThanOrEqual(MAX_READS_PER_KIND);
    expect(
      wire.timelineReads,
      `saved-history reads stay bounded by saved batches (≤ ${MAX_READS_PER_KIND})`,
    ).toBeLessThanOrEqual(MAX_READS_PER_KIND);
    expect(heapAfter - heapBefore).toBeLessThan(MAX_HEAP_GROWTH_BYTES);

    // The saved transcript renders the same text after a reload.
    const reloadedAt = Date.now();
    await page.reload({ waitUntil: "domcontentloaded" });
    await openChatComposer(page);
    await expectExactRenderedReply(page, "after the final reload", 180_000);
    record("final reload → exact reply rendered", ms(reloadedAt, Date.now()));
  });

  test("paced: a reload, a reconnect and a throttled second tab keep one contiguous copy", async ({
    page,
    context,
  }) => {
    await trackWebSockets(page);
    const wire = observeWire(page);
    await openWorkspaceChat(page, workspace!.path);

    const prompt = streamPrompt(PACED_DELAY_MS);
    const sent = await sendPrompt(page, prompt, (created) => {
      turn = created;
    });
    turn = sent.turn;
    await expect
      .poll(() => wire.firstLiveDeltaAt, { timeout: 120_000 })
      .not.toBeNull();
    record(
      "send → first live delta (wire)",
      ms(sent.sentAt, wire.firstLiveDeltaAt),
    );

    // A slow consumer: the same chat in a second tab whose CPU is throttled,
    // opened while the turn streams and left to catch up on its own while the
    // first tab is reloaded and reconnected.
    const slow = await context.newPage();
    const throttle = await context.newCDPSession(slow);
    await throttle.send("Emulation.setCPUThrottlingRate", {
      rate: SLOW_TAB_CPU_RATE,
    });
    const slowWire = observeWire(slow);
    const slowShown = showChatIn(slow, workspace!.path, prompt).then(
      () => null,
      (error: unknown) => error,
    );

    // Reload while streaming: the shown copy stays one contiguous run of the
    // provider's deltas and keeps growing from the live relay.
    const beforeReload = await observeContiguousGrowth(
      page,
      "before the reload",
      120_000,
    );
    void beforeReload;
    await page.reload({ waitUntil: "domcontentloaded" });
    await openChatComposer(page);
    const afterReload = await observeContiguousGrowth(
      page,
      "after the reload",
      180_000,
    );
    void afterReload;

    // Drop the live socket: the client reconnects on its own backoff and the
    // copy continues without a duplicate or a hole.
    const reconnected = page.waitForEvent("websocket", { timeout: 60_000 });
    const dropped = await dropOpenWebSockets(page);
    expect(dropped, "an open WebSocket was dropped").toBeGreaterThan(0);
    await reconnected;
    const socketsBefore = wire.sockets;
    const afterReconnect = await observeContiguousGrowth(
      page,
      "after the reconnect",
      180_000,
    );
    record("sockets after the forced reconnect", socketsBefore);
    void afterReconnect;

    const ended = await waitForTurnEnd(page, turn, 600_000);
    expect(ended.status, "the streamed turn succeeds").toMatch(
      /^(succeeded|ended)$/,
    );
    const savedAt = await waitForSavedReply(page, turn, 180_000);
    record(
      "last live delta (wire) → saved transcript",
      ms(wire.lastLiveDeltaAt, savedAt),
    );
    record("send → saved transcript", ms(sent.sentAt, savedAt));

    await expectExactRenderedReply(page, "tab one after the turn", 900_000);
    record("task succeeded → exact reply rendered", ms(ended.at, Date.now()));
    expect(await slowShown, "the throttled tab shows the chat").toBeNull();
    await expectExactRenderedReply(
      slow,
      "throttled tab after the turn",
      900_000,
    );
    record("task succeeded → throttled tab exact", ms(ended.at, Date.now()));
    await throttle.send("Emulation.setCPUThrottlingRate", { rate: 1 });
    await throttle.detach().catch(() => {});

    const stored = await expectStoredRowsExact(page, turn.taskId);
    record("stored rows", stored.rows);
    record("live delta frames received", wire.liveDeltas);
    record("duplicate live delta frames", wire.duplicateLiveDeltas);
    record("live-tail reads", wire.liveTailReads);
    record("saved-history reads", wire.timelineReads);
    record("throttled tab live-tail reads", slowWire.liveTailReads);
    record("throttled tab saved-history reads", slowWire.timelineReads);
    expect(wire.liveTailReads).toBeLessThanOrEqual(MAX_READS_PER_KIND);
    expect(wire.timelineReads).toBeLessThanOrEqual(MAX_READS_PER_KIND);
    expect(slowWire.liveTailReads).toBeLessThanOrEqual(MAX_READS_PER_KIND);
    expect(slowWire.timelineReads).toBeLessThanOrEqual(MAX_READS_PER_KIND);
    await slow.close();
  });
});
