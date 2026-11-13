import { randomUUID } from "node:crypto";
import { expect, test, type Page } from "@playwright/test";
import {
  authedApi,
  authedApiViaRequest,
  captureChatTurn,
  createWorkspaceClone,
  deleteWorkspaceById,
  docIdForPath,
  expectSelectedAgentProvider,
  listedDoc,
  openWorkspacePath,
  readCurrentOrgId,
  readWorkspaceChanges,
  readWorkspaceVm,
  releaseChatTurn,
  runWorkspaceVmAction,
  selectAgentProviderViaSettings,
  waitForWorkspaceVmStatus,
  type ChatTurnIdentity,
  type WorkspaceClone,
  expectAssistantReply,
} from "../helpers";

// =============================================================================
// Referenced questions (`g5.story.answer-a-question-anywhere`)
// =============================================================================
// A question lives once in a set file in the docs repository and is shown by
// reference in a Project Doc card and an HTML explainer card. The owner's
// Answer is an explicit Save that commits and pushes that one file; agents
// propose through `g5d question`; a frame message alone saves nothing.
// =============================================================================

interface QuestionSet {
  path: string;
  questions: Array<{
    id: string;
    revision: number;
    status: string;
    decision: boolean;
    answer: {
      selectedOptionIds: string[];
      answeredBy: { kind: string } | null;
    };
  }>;
}

function payload<T>(body: unknown): T {
  const value = body as { data?: T };
  return (value?.data ?? value) as T;
}

async function docsRepoId(page: Page, workspaceId: string): Promise<string> {
  const response = await authedApiViaRequest(
    page,
    `/api/v1/workspaces/${workspaceId}`,
  );
  expect(response.status).toBe(200);
  return String(payload<{ docsRepoId: string }>(response.body).docsRepoId);
}

async function createSet(
  page: Page,
  workspaceId: string,
  setId: string,
): Promise<{ path: string; questionId: string }> {
  const response = await authedApi(
    page,
    `/api/v1/workspaces/${workspaceId}/questions`,
    "POST",
    {
      setId,
      title: "Question store E2E",
      questions: [
        {
          prompt: "Which storage backend should the plan use?",
          severity: "high",
          options: [
            { label: "Set file", recommended: true },
            { label: "Database" },
          ],
        },
      ],
    },
  );
  expect(response.status, JSON.stringify(response.body)).toBe(200);
  const outcome = payload<{
    set: { path: string };
    questionIds: string[];
    publication: { status: string };
  }>(response.body);
  // A person's create is an explicit Save: the set file is published.
  expect(outcome.publication.status, JSON.stringify(outcome.publication)).toBe(
    "published",
  );
  return { path: outcome.set.path, questionId: outcome.questionIds[0] };
}

async function readSet(
  page: Page,
  workspaceId: string,
  path: string,
): Promise<QuestionSet> {
  const response = await authedApiViaRequest(
    page,
    `/api/v1/workspaces/${workspaceId}/questions/set?path=${encodeURIComponent(path)}`,
  );
  expect(response.status, JSON.stringify(response.body)).toBe(200);
  return payload<QuestionSet>(response.body);
}

/** Create a Project Doc file in the owner's live checkout and return its id. */
async function createDoc(
  page: Page,
  workspaceId: string,
  name: string,
  filePath: string,
  content: string,
): Promise<string> {
  const created = await authedApi(
    page,
    `/api/v1/workspaces/${workspaceId}/docs`,
    "POST",
    { name, docType: "design", filePath, content },
  );
  expect(created.status, JSON.stringify(created.body)).toBe(201);
  return docIdForPath(page, workspaceId, filePath);
}

/** The owner's explicit Save of a Project Doc file: commit and push it, so
 * the committed cut a stopped workspace shows carries the document itself. */
async function publishDocFile(
  page: Page,
  workspaceId: string,
  filePath: string,
  content: string,
): Promise<void> {
  const repoId = await docsRepoId(page, workspaceId);
  const vm = await readWorkspaceVm(page, workspaceId);
  const saved = await authedApi(
    page,
    `/api/v1/workspaces/${workspaceId}/files/${filePath}?repoId=${repoId}`,
    "PUT",
    {
      content,
      encoding: "utf-8",
      expectedHash: null,
      expectedVmId: vm.vmId,
      commitAndPush: true,
    },
  );
  expect(saved.status, JSON.stringify(saved.body)).toBe(200);
}

/** The workspace docs list entry for `filePath`, when listed. */
/** The document's id once the committed cut carries it: a live-only file is
 * listed with source `overlay`, and only a committed file is listed after a
 * stop. The id itself is derived from the path, so it does not change. */
async function committedDocIdForPath(
  page: Page,
  workspaceId: string,
  filePath: string,
): Promise<string> {
  let docId = "";
  await expect
    .poll(
      async () => {
        const doc = await listedDoc(page, workspaceId, filePath);
        docId = doc?.id ?? "";
        return doc?.source ?? "";
      },
      { timeout: 30_000 },
    )
    .toBe("committed");
  return docId;
}

/** What the page shows, for a failure message: its URL, test ids and text. */
async function pageSnapshot(page: Page): Promise<string> {
  const testIds = await page
    .locator("[data-testid]")
    .evaluateAll((nodes) =>
      Array.from(
        new Set(nodes.map((node) => node.getAttribute("data-testid") ?? "")),
      ).slice(0, 120),
    )
    .catch(() => [] as string[]);
  const text = await page
    .locator("body")
    .innerText()
    .catch(() => "");
  return `url=${page.url()}\ntestids=${testIds.join(",")}\ntext=${text.replace(/\s+/g, " ").slice(0, 1500)}`;
}

function referenceBlock(label: string, path: string, id: string): string {
  return [
    `[${label}. Storage backend]`,
    "",
    "```g5-question-ref",
    JSON.stringify({ path, id }),
    "```",
    "",
  ].join("\n");
}

const LEGACY_BLOCK = [
  "[Q2. Legacy embedded question]",
  "",
  "```g5-question",
  "schema: 1",
  "id: legacy-embedded",
  "status: open",
  "prompt: Keep the legacy fence?",
  "multi_select: false",
  "allow_free_text: true",
  "options:",
  "  - label: Keep",
  "    description: Keep it.",
  "answer:",
  "  selected: []",
  '  text: ""',
  '  rationale: ""',
  "  answered_by: null",
  "  answered_at: null",
  "```",
  "",
].join("\n");

async function expectPublished(page: Page, workspaceId: string, path: string) {
  const changes = await readWorkspaceChanges(page, workspaceId);
  const dirty = changes.members.flatMap((member) => member.dirty ?? []);
  expect(
    dirty.some((entry) => entry.path.endsWith(path)),
    `${path} must be committed by the answer`,
  ).toBe(false);
  for (const member of changes.members) {
    expect(member.against_internal?.ahead ?? 0, member.repo_id).toBe(0);
  }
}

test.describe("Referenced question store", () => {
  test.describe.configure({ timeout: 480_000 });
  let workspace: WorkspaceClone | null = null;
  let turn: ChatTurnIdentity | undefined;

  test.beforeEach(async ({ page }, testInfo) => {
    workspace = await createWorkspaceClone(page, {
      namePrefix: `question-store-${testInfo.title.slice(0, 24)}`,
    });
    await runWorkspaceVmAction(page, workspace.id, "resume");
    await waitForWorkspaceVmStatus(page, workspace.id, "running");
    await openWorkspacePath(page, workspace.path);
  });

  test.afterEach(async ({ page }) => {
    if (turn) await releaseChatTurn(page, turn).catch(() => {});
    turn = undefined;
    if (workspace)
      await deleteWorkspaceById(page, workspace.id).catch(() => {});
    workspace = null;
  });

  test("the owner answers a Project Doc card; it publishes, refuses a stale tab and stays read-only when stopped", async ({
    page,
  }) => {
    const ws = workspace!;
    const set = await createSet(
      page,
      ws.id,
      `plan-${randomUUID().slice(0, 8)}`,
    );
    const docPath = `docs/${ws.id}/question-store-${randomUUID().slice(0, 8)}.md`;
    const docContent = `# Question store plan\n\n${referenceBlock("Q1", set.path, set.questionId)}\n${LEGACY_BLOCK}`;
    await createDoc(page, ws.id, "Question store plan", docPath, docContent);
    // The stopped workspace below shows the committed cut, so the document
    // that carries the card is committed like the set file.
    await publishDocFile(page, ws.id, docPath, docContent);
    const docId = await committedDocIdForPath(page, ws.id, docPath);
    const baseline = await readSet(page, ws.id, set.path);
    expect(baseline.questions[0].status).toBe("open");

    await page.goto(`${ws.path}/doc/${encodeURIComponent(docId)}`);
    const card = page.getByTestId("referenced-question-card");
    await expect(card).toBeVisible({ timeout: 60_000 });
    // The embedded schema-1 card renders exactly as before.
    await expect(page.getByTestId("structured-question-editor")).toBeVisible();

    // Answer: saved only after the set file is committed and pushed.
    await card
      .getByTestId("referenced-question-option")
      .filter({ hasText: "Set file" })
      .click();
    await card.getByTestId("referenced-question-answer").click();
    await expect(card.getByTestId("referenced-question-saved")).toBeVisible({
      timeout: 60_000,
    });
    await expect(card).toHaveAttribute("data-question-status", "answered");
    const answered = await readSet(page, ws.id, set.path);
    expect(answered.questions[0].decision).toBe(true);
    expect(answered.questions[0].answer.answeredBy?.kind).toBe("user");
    await expectPublished(page, ws.id, set.path);

    // A stale tab: this card keeps showing revision 2 while another tab's
    // answer moves the entry to revision 3. The other answer changes the set
    // file, which makes this tab re-read the set, so that read is pinned to
    // the state an unrefreshed tab holds.
    const staleBody = JSON.stringify({ data: { ...answered } });
    const setRead = (url: URL) => url.pathname.endsWith("/questions/set");
    let pinStaleReads = true;
    await page.route(setRead, (route) =>
      pinStaleReads
        ? route.fulfill({ contentType: "application/json", body: staleBody })
        : route.continue(),
    );
    const other = await authedApi(
      page,
      `/api/v1/workspaces/${ws.id}/questions/answer`,
      "POST",
      {
        path: set.path,
        questionId: set.questionId,
        expectedRevision: 2,
        idempotencyKey: `other-tab-${randomUUID()}`,
        selectedOptionIds: ["database"],
      },
    );
    expect(other.status, JSON.stringify(other.body)).toBe(200);
    const staleCard = page.getByTestId("referenced-question-card");
    await expect(staleCard).toHaveAttribute("data-question-revision", "2");
    // Keep the stale entry only until its revision has been submitted. Later
    // file/cut refreshes must read the real set, not overwrite the refusal's
    // current entry with our deliberately stale mock response.
    const answerWrite = (url: URL) =>
      url.pathname === `/api/v1/workspaces/${ws.id}/questions/answer`;
    let submittedRevision: number | undefined;
    await page.route(
      answerWrite,
      async (route) => {
        submittedRevision = route.request().postDataJSON().expectedRevision;
        pinStaleReads = false;
        await route.continue();
      },
      { times: 1 },
    );
    const staleReply = page.waitForResponse(
      (response) =>
        answerWrite(new URL(response.url())) &&
        response.request().method() === "POST",
    );
    // A changed rationale makes the stale tab's answer a new request.
    await staleCard.getByTestId("referenced-question-rationale").fill("stale");
    await staleCard.getByTestId("referenced-question-answer").click();
    const refusal = await staleReply;
    expect(submittedRevision).toBe(2);
    expect(refusal.status()).toBe(409);
    expect((await refusal.json()).error.details.current.revision).toBe(3);
    await expect(
      staleCard.getByTestId("referenced-question-notice"),
    ).toContainText("changed since you opened it", { timeout: 30_000 });
    await expect(staleCard).toHaveAttribute("data-question-revision", "3");
    await page.unroute(setRead);

    // Metrics: the referenced entry is answered; only the legacy fence is open.
    const metrics = await authedApiViaRequest(
      page,
      `/api/v1/orgs/${await readCurrentOrgId(page)}/workspaces/open-question-metrics?workspaceIds=${ws.id}`,
    );
    expect(metrics.status, JSON.stringify(metrics.body)).toBe(200);
    const metric = payload<
      Array<{ documentOpenQuestions: number; questionSetsIncomplete: boolean }>
    >(metrics.body)[0];
    expect(metric.documentOpenQuestions).toBe(1);
    expect(metric.questionSetsIncomplete).toBe(false);

    // A stopped workspace shows the committed answer read-only and never
    // starts compute.
    await runWorkspaceVmAction(page, ws.id, "stop");
    await waitForWorkspaceVmStatus(page, ws.id, "stopped");
    // Without the live overlay the list is the committed cut's.
    expect(await committedDocIdForPath(page, ws.id, docPath)).toBe(docId);
    // The document's own URL again, with its tab persisted as the active one.
    await page.goto(`${ws.path}/doc/${encodeURIComponent(docId)}`);
    const stoppedCard = page.getByTestId("referenced-question-card");
    try {
      await expect(stoppedCard).toBeVisible({ timeout: 60_000 });
    } catch (error) {
      throw new Error(
        `stopped workspace did not show the card\n${await pageSnapshot(page)}\n${String(error)}`,
      );
    }
    await expect(
      stoppedCard.getByTestId("referenced-question-read-only"),
    ).toBeVisible();
    await expect(
      stoppedCard.getByTestId("referenced-question-answer"),
    ).toHaveCount(0);
    await expect(stoppedCard).toHaveAttribute("data-question-revision", "3");
    await page.waitForTimeout(3_000);
    expect((await readWorkspaceVm(page, ws.id)).status).toBe("stopped");
  });

  test("an explainer card saves only after host confirmation; a forged frame message saves nothing", async ({
    page,
  }) => {
    const ws = workspace!;
    const set = await createSet(
      page,
      ws.id,
      `explain-${randomUUID().slice(0, 8)}`,
    );
    const key = `${set.path}#${set.questionId}`;
    const repoId = await docsRepoId(page, ws.id);
    const htmlPath = `docs/${ws.id}/explainers/question-store-${randomUUID().slice(0, 8)}.html`;
    // The authored page also forges proposals: one with a made-up
    // generation, one naming an unlisted reference, and one with the real
    // generation sniffed from the host's own message.
    const html = `<!doctype html><html><body>
<h1>Storage explainer</h1>
<div data-g5-question="${key}"></div>
<script>
window.addEventListener("message", function (event) {
  var data = event.data || {};
  if (data.type !== "question-data" || window.__forged) return;
  // The initial payload can still be loading. Forge against the answerable
  // card, not an arbitrary delay that races the set read on a busy worker.
  var card = data.cards && data.cards[${JSON.stringify(key)}];
  if (!card || card.state !== "ok" || !card.answerable) return;
  window.__forged = true;
  var base = { source: "g5-html-doc", version: 1, type: "propose-question-answer",
    expectedRevision: 1, selectedOptionIds: ["database"], text: "", rationale: "" };
  parent.postMessage(Object.assign({}, base, { generation: "forged", requestId: "f1", ref: ${JSON.stringify(key)} }), "*");
  parent.postMessage(Object.assign({}, base, { generation: data.generation, requestId: "f2", ref: "docs/other/questions/x.g5q.json#q" }), "*");
  parent.postMessage(Object.assign({}, base, { generation: data.generation, requestId: "f3", ref: ${JSON.stringify(key)} }), "*");
});
</script>
</body></html>`;
    const written = await authedApi(
      page,
      `/api/v1/workspaces/${ws.id}/files/${htmlPath}?repoId=${repoId}`,
      "PUT",
      { content: html, encoding: "utf-8", expectedHash: null },
    );
    expect(written.status, JSON.stringify(written.body)).toBe(200);
    const docId = await docIdForPath(page, ws.id, htmlPath);

    await page.goto(`${ws.path}/doc/${encodeURIComponent(docId)}`);
    const frame = page.frameLocator("iframe[sandbox]").first();
    const frameCard = frame.locator(`[data-g5-question-card="${key}"]`);
    await expect(frameCard).toHaveAttribute("data-g5-question-status", "open", {
      timeout: 60_000,
    });

    // The forged message with the sniffed generation reaches the host bar,
    // which saves nothing until a person confirms; dismissing it keeps the
    // question open.
    const bar = page.getByTestId("question-answer-bar");
    await expect(bar).toBeVisible({ timeout: 30_000 });
    await expect(bar).toContainText("Database");
    await page.getByTestId("question-answer-dismiss").click();
    await expect(bar).toHaveCount(0);
    await page.waitForTimeout(1_000);
    expect((await readSet(page, ws.id, set.path)).questions[0].status).toBe(
      "open",
    );

    // The person answers in the card and confirms in the host bar.
    await frameCard.locator('input[data-g5-option="set-file"]').check();
    await frameCard.locator("button[data-g5-question-answer]").click();
    await expect(bar).toContainText("Set file", { timeout: 30_000 });
    await page.getByTestId("question-answer-confirm").click();
    await expect(bar).toContainText("Answer saved and published", {
      timeout: 60_000,
    });
    // The card refreshes in place: the frame was not reloaded.
    await expect(frameCard).toHaveAttribute(
      "data-g5-question-status",
      "answered",
      {
        timeout: 30_000,
      },
    );
    const answered = await readSet(page, ws.id, set.path);
    expect(answered.questions[0].decision).toBe(true);
    expect(answered.questions[0].answer.selectedOptionIds).toEqual([
      "set-file",
    ]);
    await expectPublished(page, ws.id, set.path);
  });

  test("a vote is recorded with one press in the frame, shows on every card, and needs no running VM", async ({
    page,
  }) => {
    const ws = workspace!;
    const set = await createSet(
      page,
      ws.id,
      `vote-${randomUUID().slice(0, 8)}`,
    );
    const key = `${set.path}#${set.questionId}`;
    const repoId = await docsRepoId(page, ws.id);
    const htmlPath = `docs/${ws.id}/explainers/vote-${randomUUID().slice(0, 8)}.html`;
    const html = `<!doctype html><html><body><h1>Vote</h1><div data-g5-question="${key}"></div></body></html>`;
    const written = await authedApi(
      page,
      `/api/v1/workspaces/${ws.id}/files/${htmlPath}?repoId=${repoId}`,
      "PUT",
      { content: html, encoding: "utf-8", expectedHash: null },
    );
    expect(written.status, JSON.stringify(written.body)).toBe(200);
    const explainerId = await docIdForPath(page, ws.id, htmlPath);
    const docPath = `docs/${ws.id}/vote-doc-${randomUUID().slice(0, 8)}.md`;
    const docContent = `# Vote\n\n${referenceBlock("Q1", set.path, set.questionId)}`;
    await createDoc(page, ws.id, "Vote doc", docPath, docContent);
    await publishDocFile(page, ws.id, docPath, docContent);
    const docId = await committedDocIdForPath(page, ws.id, docPath);

    // The explainer card: the one Vote press records the vote; no host bar.
    await page.goto(`${ws.path}/doc/${encodeURIComponent(explainerId)}`);
    const frame = page.frameLocator("iframe[sandbox]").first();
    const frameCard = frame.locator(`[data-g5-question-card="${key}"]`);
    await expect(frameCard).toHaveAttribute("data-g5-question-votes", "0", {
      timeout: 60_000,
    });
    await frameCard.locator('input[data-g5-option="database"]').check();
    await frameCard.locator("button[data-g5-question-vote]").click();
    await expect(frameCard).toHaveAttribute("data-g5-question-votes", "1", {
      timeout: 30_000,
    });
    await expect(frameCard.locator('[data-g5-option-votes="1"]')).toContainText(
      "(you)",
    );
    await expect(frameCard).toContainText("Vote recorded.");
    await expect(page.getByTestId("question-answer-bar")).toHaveCount(0);
    const recorded = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${ws.id}/questions/votes?path=${encodeURIComponent(set.path)}`,
    );
    expect(
      payload<{ votes: { selectedOptionIds: string[] }[] }>(recorded.body)
        .votes,
    ).toMatchObject([{ selectedOptionIds: ["database"] }]);
    // The vote is a platform record: the set file and its answer are untouched.
    const afterVote = await readSet(page, ws.id, set.path);
    expect(afterVote.questions[0].status).toBe("open");
    await expectPublished(page, ws.id, set.path);

    // The Project Doc card shows the same tally and changes the vote from a
    // stopped workspace: no VM is needed and none is started.
    await runWorkspaceVmAction(page, ws.id, "stop");
    await waitForWorkspaceVmStatus(page, ws.id, "stopped");
    await page.goto(`${ws.path}/doc/${encodeURIComponent(docId)}`);
    const card = page.getByTestId("referenced-question-card");
    await expect(card).toBeVisible({ timeout: 60_000 });
    await expect(
      card.getByTestId("referenced-question-read-only"),
    ).toBeVisible();
    await expect(
      card.getByTestId("referenced-question-vote-count"),
    ).toContainText("1 vote", { timeout: 30_000 });
    await card
      .getByTestId("referenced-question-option")
      .filter({ hasText: "Set file" })
      .click();
    await card.getByTestId("referenced-question-vote").click();
    await expect(card.getByTestId("referenced-question-voted")).toBeVisible({
      timeout: 30_000,
    });
    await expect(
      card.getByTestId("referenced-question-option-votes").first(),
    ).toContainText("1 vote");
    await card.getByTestId("referenced-question-withdraw-vote").click();
    await expect(
      card.getByTestId("referenced-question-vote-withdrawn"),
    ).toBeVisible({ timeout: 30_000 });
    await expect(card.getByTestId("referenced-question-vote-count")).toHaveText(
      "No votes yet",
    );
    expect((await readWorkspaceVm(page, ws.id)).status).toBe("stopped");
  });

  test("an agent proposes through g5d question and the owner confirms it", async ({
    page,
  }) => {
    const ws = workspace!;
    const set = await createSet(
      page,
      ws.id,
      `agent-${randomUUID().slice(0, 8)}`,
    );
    const docPath = `docs/${ws.id}/agent-question-${randomUUID().slice(0, 8)}.md`;
    const docId = await createDoc(
      page,
      ws.id,
      "Agent question",
      docPath,
      `# Agent question\n\n${referenceBlock("Q1", set.path, set.questionId)}`,
    );
    const directive = Buffer.from(
      JSON.stringify({
        path: set.path,
        id: set.questionId,
        expectedRevision: 1,
        options: ["database"],
      }),
    ).toString("base64");
    await selectAgentProviderViaSettings(page, "claude_code");
    await expectSelectedAgentProvider(page, "claude_code");
    const input = page.getByTestId("chat-input").first();
    await expect(input).toBeVisible({ timeout: 30_000 });
    await input.fill(`Propose an answer. G5_QUESTION_CLI_E2E:${directive}`);
    const sendResponse = page.waitForResponse((response) => {
      if (response.request().method() !== "POST") return false;
      const path = new URL(response.url()).pathname;
      return (
        /\/api\/v1\/sessions\/[^/]+\/messages$/.test(path) ||
        /\/api\/v1\/workspaces\/[^/]+\/sessions\/start$/.test(path)
      );
    });
    await input.press("Enter");
    turn = await captureChatTurn(page, await sendResponse, (created) => {
      turn = created;
    });
    await expectAssistantReply(page, turn!, "Fake question proposal recorded", {
      timeout: 180_000,
    });
    const proposed = await readSet(page, ws.id, set.path);
    expect(proposed.questions[0].status).toBe("proposed");
    expect(proposed.questions[0].decision).toBe(false);
    expect(proposed.questions[0].answer.answeredBy?.kind).toBe("agent");
    // The agent's write is live only: the set file is dirty until a push.
    const changes = await readWorkspaceChanges(page, ws.id);
    expect(
      changes.members
        .flatMap((member) => member.dirty ?? [])
        .some((entry) => entry.path.endsWith(set.path)),
    ).toBe(true);

    await page.goto(`${ws.path}/doc/${encodeURIComponent(docId)}`);
    const card = page.getByTestId("referenced-question-card");
    await expect(card.getByTestId("referenced-question-proposal")).toBeVisible({
      timeout: 60_000,
    });
    // The proposal's selection is preselected; confirming makes it a decision.
    await card.getByTestId("referenced-question-answer").click();
    await expect(card.getByTestId("referenced-question-saved")).toBeVisible({
      timeout: 60_000,
    });
    const confirmed = await readSet(page, ws.id, set.path);
    expect(confirmed.questions[0].decision).toBe(true);
    expect(confirmed.questions[0].answer.answeredBy?.kind).toBe("user");
    await expectPublished(page, ws.id, set.path);
  });
});
