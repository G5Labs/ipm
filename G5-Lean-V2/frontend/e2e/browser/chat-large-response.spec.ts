// Browser-only counterpart of e2e/flows/chat-large-response.spec.ts: the
// actual ChatPanel over the live-row placement path and the session-timeline
// cache receives 10,000 live delta rows one macrotask each (as the socket
// handler would), then a duplicate burst, the final text block and the saved
// rows. It proves exact, duplicate-free rendering and reload-equivalent
// convergence, and measures the browser's own cost (commits, time, heap)
// without an API, a VM or a WebSocket.
import { expect, test, type Page } from "@playwright/test";

// 2,000 rows by default: placing one live row re-renders the whole panel, so
// the cost grows with the rows already held (see the recorded checkpoints);
// G5_LARGE_RESPONSE_ROWS=10000 runs the full-size case.
const COUNT = Number(process.env.G5_LARGE_RESPONSE_ROWS ?? "2000");
// With the full stack's saved-history refreshes emulated after every 256th
// row, the held window stays bounded and the full-size case is affordable.
const BATCHED_COUNT = Number(
  process.env.G5_LARGE_RESPONSE_BATCHED_ROWS ?? "10000",
);
const BYTES = 24;
/** History batches close at this many events (attempt_follower
 * BATCH_MAX_EVENTS); the batched harness emulates one refresh per batch. */
const HISTORY_BATCH_MAX_EVENTS = 256;
/** One token is exactly `BYTES - 1` characters (`w`, five digits, filler), so
 * tokens are read by shape and adjacent block elements, whose text content
 * has no separator between them, cannot run two tokens together. */
const FILLER_LETTERS = "abcdefghijklmnopqrstuvwxyz".slice(0, BYTES - 7);
const TOKEN_PATTERN = new RegExp(
  `w\\d{5}[${FILLER_LETTERS}]{${BYTES - 7}}`,
  "g",
);

function streamDeltaText(index: number, size: number): string {
  const filler = "abcdefghijklmnopqrstuvwxyz";
  const head = `w${String(index).padStart(5, "0")}`;
  let text = head;
  while (text.length + 1 < size) {
    text += filler[(text.length - head.length) % filler.length];
  }
  return `${text} `;
}
const EXPECTED_TOKENS = Array.from(
  { length: Math.max(COUNT, BATCHED_COUNT) },
  (_, index) => streamDeltaText(index + 1, BYTES).trimEnd(),
);

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

function exactness(tokens: string[], count = COUNT): string {
  if (tokens.length !== count) return `${tokens.length} tokens`;
  for (let index = 0; index < tokens.length; index += 1) {
    if (tokens[index] !== EXPECTED_TOKENS[index])
      return `token ${index + 1} is ${tokens[index]}`;
  }
  return "exact";
}

type Stats = {
  phase: string;
  placed: number;
  checkpointsMs: Record<number, number>;
  delivered: number;
  duplicatesDelivered: number;
  refreshes: number;
  heldRows: number;
  commits: number;
  commitCost: Record<
    string,
    { commits: number; totalMs: number; maxMs: number }
  >;
  startedAt: number | null;
  deliveredAt: number | null;
  finalAt: number | null;
};

async function stats(page: Page): Promise<Stats> {
  return page.evaluate(() => ({
    ...(window as unknown as { __g5LargeResponse: Stats }).__g5LargeResponse,
  }));
}

async function pageNow(page: Page): Promise<number> {
  return page.evaluate(() => performance.now());
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

function meanCommitMs(current: Stats, bucket: string): number {
  const cost = current.commitCost[bucket];
  return cost && cost.commits > 0 ? cost.totalMs / cost.commits : 0;
}

function record(label: string, value: string | number) {
  test.info().annotations.push({
    type: "large-response",
    description: `${label}: ${value}`,
  });
  console.log(`[chat-large-response browser] ${label}: ${value}`);
}

test(`ChatPanel renders ${COUNT} live delta rows exactly once, survives a duplicate burst and converges on the saved rows`, async ({
  page,
}) => {
  test.setTimeout(1_800_000);
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.route("**/api/**", (route) =>
    route.fulfill({ json: { data: [] } }),
  );
  await page.goto(
    `/e2e/browser/chat-large-response.html?count=${COUNT}&bytes=${BYTES}`,
  );
  const heapBefore = await heapUsedBytes(page);

  await page.getByRole("button", { name: "Stream live rows" }).click();
  let lastProgress = -1;
  await expect
    .poll(
      async () => {
        const current = await stats(page);
        if (current.placed !== lastProgress) {
          lastProgress = current.placed;
          console.log(
            `[chat-large-response browser] placed ${current.placed}/${COUNT}`,
          );
        }
        return current.delivered;
      },
      { timeout: 1_500_000, intervals: [2_000] },
    )
    .toBe(COUNT);
  const delivered = await stats(page);
  record(
    "deliver all rows (ms)",
    Math.round(delivered.deliveredAt! - delivered.startedAt!),
  );
  let previous = delivered.startedAt!;
  for (const [rows, at] of Object.entries(delivered.checkpointsMs)
    .map(([rows, at]) => [Number(rows), at] as const)
    .sort((left, right) => left[0] - right[0])) {
    record(
      `rows ${rows - 1000 + 1}..${rows} placed in (ms)`,
      Math.round(at - previous),
    );
    previous = at;
  }
  await expect
    .poll(async () => exactness(await renderedTokens(page)), {
      timeout: 120_000,
      intervals: [500, 1_000],
    })
    .toBe("exact");
  const renderedAt = await pageNow(page);
  const rendered = await stats(page);
  record(
    "stream start → exact rendering (ms)",
    Math.round(renderedAt - rendered.startedAt!),
  );
  record("React commits while streaming", rendered.commits);
  // One placement per row is the current ceiling; the constant covers the
  // initial renders and the completion re-render.
  expect(rendered.commits).toBeLessThanOrEqual(COUNT + 50);

  // Offscreen rows skip layout and paint (`content-visibility: auto`) but
  // stay reachable: automation can still find the first row's text, bring
  // it into view and click it, and the reading position holds.
  const firstRow = page.getByTestId("user-message").first();
  await expect(firstRow).toContainText("G5_STREAM_DELTAS_E2E", {
    timeout: 10_000,
  });
  await firstRow.click({ timeout: 10_000 });
  await expect(firstRow).toBeInViewport();
  await page.getByTestId("assistant-message").last().scrollIntoViewIfNeeded();

  await page.getByRole("button", { name: "Deliver duplicate burst" }).click();
  await expect
    .poll(async () => (await stats(page)).duplicatesDelivered, {
      timeout: 120_000,
    })
    .toBeGreaterThan(0);
  const afterDuplicates = await stats(page);
  expect(exactness(await renderedTokens(page))).toBe("exact");
  record(
    "React commits for the duplicate burst",
    afterDuplicates.commits - rendered.commits,
  );
  // Rows already held change nothing: the window is returned unchanged.
  expect(afterDuplicates.commits - rendered.commits).toBeLessThanOrEqual(5);

  await page.getByRole("button", { name: "Deliver final block" }).click();
  await expect
    .poll(async () => exactness(await renderedTokens(page)), {
      timeout: 60_000,
    })
    .toBe("exact");

  const heapAfter = await heapUsedBytes(page);
  record("JS heap growth (bytes)", heapAfter - heapBefore);

  await page.getByRole("button", { name: "Saved rows only" }).click();
  await expect(page.getByTestId("large-response-phase")).toHaveText("saved");
  await expect
    .poll(async () => exactness(await renderedTokens(page)), {
      timeout: 60_000,
    })
    .toBe("exact");
  await page.reload();
  await expect(page.getByTestId("large-response-phase")).toHaveText("saved");
  await expect
    .poll(async () => exactness(await renderedTokens(page)), {
      timeout: 120_000,
      intervals: [500, 1_000],
    })
    .toBe("exact");
  await expect(page.getByLabel("Generating response")).toHaveCount(0);
  expect(errors).toEqual([]);
});

test(`ChatPanel renders ${BATCHED_COUNT} live delta rows with saved-history refreshes emulated after every 256th row`, async ({
  page,
}) => {
  test.setTimeout(1_800_000);
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.route("**/api/**", (route) =>
    route.fulfill({ json: { data: [] } }),
  );
  await page.goto(
    `/e2e/browser/chat-large-response.html?count=${BATCHED_COUNT}&bytes=${BYTES}&batches=1`,
  );
  const heapBefore = await heapUsedBytes(page);
  await page.getByRole("button", { name: "Stream live rows" }).click();
  let lastProgress = -1;
  await expect
    .poll(
      async () => {
        const current = await stats(page);
        if (current.placed !== lastProgress) {
          lastProgress = current.placed;
          console.log(
            `[chat-large-response browser, batched] placed ${current.placed}/${BATCHED_COUNT}, held ${current.heldRows}`,
          );
        }
        return current.delivered;
      },
      { timeout: 1_500_000, intervals: [2_000] },
    )
    .toBe(BATCHED_COUNT);
  const delivered = await stats(page);
  record(
    "batched: deliver all rows (ms)",
    Math.round(delivered.deliveredAt! - delivered.startedAt!),
  );
  let previous = delivered.startedAt!;
  for (const [rows, at] of Object.entries(delivered.checkpointsMs)
    .map(([rows, at]) => [Number(rows), at] as const)
    .sort((left, right) => left[0] - right[0])) {
    record(
      `batched: rows ${rows - 1000 + 1}..${rows} placed in (ms)`,
      Math.round(at - previous),
    );
    previous = at;
  }
  record("batched: emulated refreshes", delivered.refreshes);
  record("batched: rows held after the last refresh", delivered.heldRows);
  record("batched: React commits while streaming", delivered.commits);
  // Every saved row replaced its live copy and nothing was held twice: at
  // the last refresh the window held the rows saved so far, each once.
  expect(delivered.heldRows).toBe(
    delivered.refreshes * HISTORY_BATCH_MAX_EVENTS,
  );
  // The rendered reply is already exact from the held rows; the final block
  // then carries the whole text.
  expect(exactness(await renderedTokens(page), BATCHED_COUNT)).toBe("exact");
  await page.getByRole("button", { name: "Deliver final block" }).click();
  await expect
    .poll(async () => exactness(await renderedTokens(page), BATCHED_COUNT), {
      timeout: 120_000,
      intervals: [500, 1_000],
    })
    .toBe("exact");
  const heapAfter = await heapUsedBytes(page);
  record("batched: JS heap growth (bytes)", heapAfter - heapBefore);
  expect(errors).toEqual([]);
});

/** Rows placed per animation frame in the paced run. */
const PACE_ROWS_PER_FRAME = 10;

test(`ChatPanel's commit cost does not grow with the rows it holds (${COUNT} rows, ${PACE_ROWS_PER_FRAME} per frame)`, async ({
  page,
}) => {
  test.setTimeout(1_800_000);
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.route("**/api/**", (route) =>
    route.fulfill({ json: { data: [] } }),
  );
  // One macrotask places a frame's rows, then the next frame is awaited, so
  // the per-frame placement commits once per frame.
  await page.goto(
    `/e2e/browser/chat-large-response.html?count=${COUNT}&bytes=${BYTES}&chunk=${PACE_ROWS_PER_FRAME}&pace=${PACE_ROWS_PER_FRAME}`,
  );
  await page.getByRole("button", { name: "Stream live rows" }).click();
  await expect
    .poll(async () => (await stats(page)).delivered, {
      timeout: 1_500_000,
      intervals: [2_000],
    })
    .toBe(COUNT);
  await expect
    .poll(async () => exactness(await renderedTokens(page)), {
      timeout: 120_000,
      intervals: [500, 1_000],
    })
    .toBe("exact");
  const paced = await stats(page);
  record(
    "paced: deliver all rows (ms)",
    Math.round(paced.deliveredAt! - paced.startedAt!),
  );
  record("paced: React commits while streaming", paced.commits);
  const costAt200 = meanCommitMs(paced, "200");
  const costAt2000 = meanCommitMs(paced, "2000");
  record(
    "paced: commits at ~200 held rows",
    paced.commitCost["200"]?.commits ?? 0,
  );
  record("paced: mean commit at ~200 held rows (ms)", costAt200.toFixed(2));
  record(
    "paced: commits at ~2000 held rows",
    paced.commitCost["2000"]?.commits ?? 0,
  );
  record("paced: mean commit at ~2000 held rows (ms)", costAt2000.toFixed(2));
  record(
    "paced: max commit at ~2000 held rows (ms)",
    (paced.commitCost["2000"]?.maxMs ?? 0).toFixed(2),
  );
  // One commit per frame of rows: the frames are the ceiling.
  expect(paced.commits).toBeLessThanOrEqual(COUNT / PACE_ROWS_PER_FRAME + 50);
  // A commit re-projects and re-renders only the turn whose rows changed,
  // so its cost does not grow with the rows the window already holds: the
  // mean commit at about 2,000 held rows stays within a small constant
  // factor of the mean at about 200 (a few milliseconds of slack keep the
  // smallest commits from making the ratio meaningless).
  expect(paced.commitCost["200"]?.commits ?? 0).toBeGreaterThan(0);
  expect(paced.commitCost["2000"]?.commits ?? 0).toBeGreaterThan(0);
  expect(costAt2000).toBeLessThanOrEqual(
    Math.max(2 * costAt200, costAt200 + 4),
  );
  if (COUNT >= 10_000) {
    const costAt10000 = meanCommitMs(paced, "10000");
    record(
      "paced: commits at ~10000 held rows",
      paced.commitCost["10000"]?.commits ?? 0,
    );
    record(
      "paced: mean commit at ~10000 held rows (ms)",
      costAt10000.toFixed(2),
    );
    // At ten thousand held rows the commit still pays a dozen whole-window
    // passes (projection, ordering checks, activity label, approval
    // classification), each well under a microsecond per row; that residual
    // bounds the growth to a few times the small-window cost, not the
    // thirty-fold a full re-render cost before.
    expect(paced.commitCost["10000"]?.commits ?? 0).toBeGreaterThan(0);
    expect(costAt10000).toBeLessThanOrEqual(
      Math.max(4 * costAt200, costAt200 + 8),
    );
  }
  expect(errors).toEqual([]);
});
