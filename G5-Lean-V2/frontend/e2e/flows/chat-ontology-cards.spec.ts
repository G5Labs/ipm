import { randomUUID } from "node:crypto";
import { expect, test, type Page } from "@playwright/test";
import {
  authedApiViaRequest,
  captureChatTurn,
  createWorkspaceClone,
  deleteWorkspaceById,
  expectAssistantReply,
  expectSelectedAgentProvider,
  openWorkspacePath,
  readWorkspaceChanges,
  releaseChatTurn,
  runWorkspaceVmAction,
  seedWorkspaceClaudeCredential,
  seedWorkspaceCodexCredential,
  selectAgentProviderViaSettings,
  waitForWorkspaceVmStatus,
  type ChatTurnIdentity,
  type WorkspaceClone,
} from "../helpers";

// =============================================================================
// Ontology changes cards in workspace chats (chat g5.chat.ontology-card)
// =============================================================================
// Real API, dispatch, runner and Git with the deterministic fake Claude: a
// turn that changes ontology statements ends its reply with a card of the
// changes between the turn's start and end, whoever made them; the owner edits
// a statement in place on the newest card and the next turn is told.
// =============================================================================

const FILE = ".g5/ontology/cards-e2e.g5.md";
const EDITOR_FILE = ".g5/ontology/cards-editor-e2e.g5.md";

function ontologyPage(statements: Array<[string, string, string]>): string {
  let text = "# Cards E2E\n\n";
  for (const [id, kind, body] of statements)
    text += `<a id="${id}"></a>\n[${kind} · ${id}]\n\n${body}\n\n`;
  text += "## Metadata\n\n";
  for (const [id] of statements) text += `### ${id}\n\n- agent\n\n`;
  return text;
}

const b64 = (text: string) => Buffer.from(text).toString("base64");

function payload<T>(body: unknown): T {
  const value = body as { data?: T };
  return (value?.data ?? value) as T;
}

interface Member {
  repoId: string;
  root: string;
}

async function codeMember(page: Page, workspaceId: string): Promise<Member> {
  const detail = await authedApiViaRequest(
    page,
    `/api/v1/workspaces/${workspaceId}`,
  );
  const docsRepoId = payload<{ docsRepoId?: string }>(detail.body).docsRepoId;
  const changes = await readWorkspaceChanges(page, workspaceId);
  const member = changes.members.find((m) => m.repo_id !== docsRepoId)!;
  expect(member, "the workspace has a code member").toBeTruthy();
  return { repoId: member.repo_id, root: member.root_path };
}

async function readFile(
  page: Page,
  workspaceId: string,
  member: Member,
  path: string,
): Promise<string | null> {
  const result = await authedApiViaRequest(
    page,
    `/api/v1/workspaces/${workspaceId}/files/${path}?repoId=${member.repoId}`,
  );
  if (result.status === 404) return null;
  expect(result.status, JSON.stringify(result.body)).toBe(200);
  return payload<{ content: string }>(result.body).content;
}

async function writeFile(
  page: Page,
  workspaceId: string,
  member: Member,
  path: string,
  content: string,
) {
  const result = await authedApiViaRequest(
    page,
    `/api/v1/workspaces/${workspaceId}/files/${path}?repoId=${member.repoId}`,
    "PUT",
    { content, commitAndPush: false },
  );
  expect(result.status, JSON.stringify(result.body)).toBe(200);
}

async function send(page: Page, text: string): Promise<ChatTurnIdentity> {
  const input = page.getByTestId("chat-input").first();
  await expect(input).toBeVisible({ timeout: 30_000 });
  await expect(input).toBeEnabled({ timeout: 60_000 });
  await input.fill(text);
  const sendResponse = page.waitForResponse((response) => {
    if (response.request().method() !== "POST") return false;
    const path = new URL(response.url()).pathname;
    return (
      /\/api\/v1\/sessions\/[^/]+\/messages$/.test(path) ||
      /\/api\/v1\/workspaces\/[^/]+\/sessions\/start$/.test(path)
    );
  });
  await input.press("Enter");
  return captureChatTurn(page, await sendResponse);
}

const cards = (p: Page) => p.getByTestId("ontology-changes-card");
const statementIn = (card: ReturnType<typeof cards>, id: string) =>
  card.locator(
    `[data-testid="ontology-card-statement"][data-statement-id="${id}"]`,
  );

test.describe("Ontology changes cards", () => {
  test.describe.configure({ timeout: 900_000 });
  let workspace: WorkspaceClone | null = null;
  let turn: ChatTurnIdentity | undefined;

  test.beforeEach(async ({ page }) => {
    test.skip(
      !process.env.LLM_ENABLED && !process.env.ANTHROPIC_API_KEY,
      "needs the dev stack's fake Claude (LLM_ENABLED)",
    );
    workspace = await createWorkspaceClone(page, {
      namePrefix: "ontology-cards",
    });
    await seedWorkspaceClaudeCredential(page, workspace.id);
    await runWorkspaceVmAction(page, workspace.id, "resume");
    await waitForWorkspaceVmStatus(page, workspace.id, "running");
    await openWorkspacePath(page, workspace.path);
    await selectAgentProviderViaSettings(page, "claude_code");
    await expectSelectedAgentProvider(page, "claude_code");
  });

  test.afterEach(async ({ page }) => {
    if (turn) await releaseChatTurn(page, turn).catch(() => {});
    turn = undefined;
    if (workspace)
      await deleteWorkspaceById(page, workspace.id).catch(() => {});
    workspace = null;
  });

  test("turns end with cards of their ontology changes, and the owner edits the newest", async ({
    page,
  }) => {
    const ws = workspace!;
    const member = await codeMember(page, ws.id);
    const target = `${member.root}/${FILE}`;

    // 1. A turn that adds statements ends with a card of them.
    const first = ontologyPage([
      ["e2e.cards.a", "D", "Cards show each turn's ontology changes."],
      ["e2e.cards.b", "I", "The card stays fixed after later turns."],
    ]);
    turn = await send(
      page,
      `Add the card statements. G5_FS_WRITE_E2E:${target}|${b64(first)}`,
    );
    await expectAssistantReply(page, turn, "Fake workspace write completed", {
      timeout: 240_000,
    });
    await expect(cards(page)).toHaveCount(1, { timeout: 60_000 });
    const firstCard = cards(page).first();
    await expect(firstCard).toHaveAttribute("open", "");
    await expect(
      firstCard.getByLabel("2 added, 0 modified, 0 deleted"),
    ).toBeVisible();
    await expect(statementIn(firstCard, "e2e.cards.a")).toContainText(
      "Cards show each turn's ontology changes.",
    );
    turn = undefined;

    // 2. A code-only turn has no card; the first stays the newest.
    turn = await send(
      page,
      `Write a note. G5_FS_WRITE_E2E:${member.root}/notes/cards-e2e.txt|${b64("code only\n")}`,
    );
    await expectAssistantReply(page, turn, "Fake workspace write completed", {
      timeout: 240_000,
    });
    turn = undefined;
    await expect(cards(page)).toHaveCount(1);
    await expect(firstCard).toHaveAttribute("data-newest", "true");

    // 3. A turn that modifies a statement and names a governing one, while
    //    the owner saves another file in the editor: the card shows both.
    const gate = `${member.root}/.cards-e2e-gate-${randomUUID().slice(0, 8)}`;
    const second = ontologyPage([
      ["e2e.cards.a", "D", "Cards show every ontology change of a turn."],
      ["e2e.cards.b", "I", "The card stays fixed after later turns."],
    ]);
    const governing = b64(
      JSON.stringify({
        id: "e2e.cards.b",
        reason: "The changed decision relies on fixed cards.",
      }),
    );
    turn = await send(
      page,
      `Refine the decision. G5_FS_WRITE_E2E:${target}|${b64(second)} G5_CHAT_GOVERNING_E2E:${governing} G5_WAIT_FOR_FILE_E2E:${gate}`,
    );
    await expect
      .poll(() => readFile(page, ws.id, member, FILE), { timeout: 240_000 })
      .toBe(second);
    await writeFile(
      page,
      ws.id,
      member,
      EDITOR_FILE,
      ontologyPage([
        ["e2e.cards.editor", "I", "Saved in the editor during the turn."],
      ]),
    );
    await writeFile(
      page,
      ws.id,
      member,
      gate.slice(member.root.length + 1),
      "go\n",
    );
    await expectAssistantReply(
      page,
      turn,
      "governing statement recorded: e2e.cards.b",
      {
        timeout: 240_000,
      },
    );
    turn = undefined;
    await expect(cards(page)).toHaveCount(2, { timeout: 60_000 });
    const secondCard = cards(page).nth(1);
    await expect(secondCard).toHaveAttribute("open", "");
    await expect(firstCard).not.toHaveAttribute("open", "");
    await expect(
      secondCard.getByLabel("1 added, 1 modified, 0 deleted"),
    ).toBeVisible();
    const modified = statementIn(secondCard, "e2e.cards.a");
    await expect(modified).toContainText(
      "Cards show every ontology change of a turn.",
    );
    await expect(
      modified.getByRole("region", { name: "Before statement e2e.cards.a" }),
    ).toContainText("Cards show each turn's ontology changes.");
    await expect(statementIn(secondCard, "e2e.cards.editor")).toContainText(
      "Saved in the editor during the turn.",
    );
    const governingList = secondCard.getByTestId("ontology-card-governing");
    await governingList.locator("summary").click();
    await expect(governingList).toContainText("e2e.cards.b");
    await expect(governingList).toContainText(
      "The card stays fixed after later turns.",
    );
    await expect(governingList).toContainText(
      "The changed decision relies on fixed cards.",
    );
    // Earlier cards are not editable.
    await expect(firstCard.getByTestId("ontology-card-edit")).toHaveCount(0);

    // 4. The owner edits the modified statement in place on the newest card.
    await modified.getByTestId("ontology-card-edit").click();
    const text = secondCard.getByTestId("ontology-card-edit-text");
    await expect(text).toHaveValue(
      "Cards show every ontology change of a turn.",
    );
    await text.fill(
      "Cards show every ontology change of a turn, whoever made it.",
    );
    await secondCard.getByTestId("ontology-card-edit-save").click();
    await expect(modified.getByTestId("ontology-card-edited")).toHaveAttribute(
      "title",
      /^You edited this after the agent's turn/,
      { timeout: 60_000 },
    );
    await expect(modified).toContainText(
      "Cards show every ontology change of a turn, whoever made it.",
    );
    await expect
      .poll(() => readFile(page, ws.id, member, FILE), { timeout: 30_000 })
      .toBe(
        ontologyPage([
          [
            "e2e.cards.a",
            "D",
            "Cards show every ontology change of a turn, whoever made it.",
          ],
          ["e2e.cards.b", "I", "The card stays fixed after later turns."],
        ]),
      );

    // 5. The next turn's input names the edit (the fake echoes its prompt),
    //    and a turn without ontology changes adds no card.
    turn = await send(page, "Continue with the design.");
    await expectAssistantReply(page, turn, "edited statement e2e.cards.a", {
      timeout: 240_000,
    });
    turn = undefined;
    await expect(cards(page)).toHaveCount(2);

    // 6. A reload, and a stopped VM, keep the cards; Edit says why it waits.
    await page.reload();
    await expect(cards(page)).toHaveCount(2, { timeout: 60_000 });
    await runWorkspaceVmAction(page, ws.id, "stop");
    await waitForWorkspaceVmStatus(page, ws.id, "stopped");
    await page.reload();
    await expect(cards(page)).toHaveCount(2, { timeout: 60_000 });
    const stoppedEdit = statementIn(
      cards(page).nth(1),
      "e2e.cards.a",
    ).getByTestId("ontology-card-edit");
    await expect(stoppedEdit).toBeDisabled();
    await expect(stoppedEdit).toHaveAttribute(
      "title",
      /Start the workspace to edit the card\./,
    );
    await expect(
      statementIn(cards(page).nth(1), "e2e.cards.a").getByTestId(
        "ontology-card-edited",
      ),
    ).toBeVisible();
  });

  test("a Codex turn's g5d chat governing records on its turn", async ({
    page,
  }) => {
    const ws = workspace!;
    await seedWorkspaceCodexCredential(page, ws.id);
    await selectAgentProviderViaSettings(page, "codex");
    await expectSelectedAgentProvider(page, "codex");
    const member = await codeMember(page, ws.id);
    // The governing statement is in place before the turn: a statement the
    // turn changes is on the card already, so its governing list leaves it out.
    const rule: [string, string, string] = [
      "e2e.cards.codex-rule",
      "I",
      "Every workspace chat has cards.",
    ];
    await writeFile(page, ws.id, member, FILE, ontologyPage([rule]));
    const text = ontologyPage([
      ["e2e.cards.codex", "D", "Codex turns have cards too."],
      rule,
    ]);
    const governing = b64(
      JSON.stringify({
        id: "e2e.cards.codex-rule",
        reason: "Cards do not depend on the agent.",
      }),
    );
    turn = await send(
      page,
      `Add a statement. G5_FS_WRITE_E2E:${member.root}/${FILE}|${b64(text)} G5_CHAT_GOVERNING_E2E:${governing}`,
    );
    await expectAssistantReply(
      page,
      turn,
      "governing statement recorded: e2e.cards.codex-rule",
      { timeout: 240_000 },
    );
    turn = undefined;
    await expect(cards(page)).toHaveCount(1, { timeout: 60_000 });
    const governingList = cards(page)
      .first()
      .getByTestId("ontology-card-governing");
    await governingList.locator("summary").click();
    await expect(governingList).toContainText(
      "Cards do not depend on the agent.",
    );
    await expect(governingList).toContainText(
      "Every workspace chat has cards.",
    );
  });

  test("a turn the person stops has no card, though its changes stay", async ({
    page,
  }) => {
    const ws = workspace!;
    const member = await codeMember(page, ws.id);
    const gate = `${member.root}/.cards-e2e-gate-${randomUUID().slice(0, 8)}`;
    const text = ontologyPage([
      ["e2e.cards.stopped", "D", "Written by a stopped turn."],
    ]);
    turn = await send(
      page,
      `Write and wait. G5_FS_WRITE_E2E:${member.root}/${FILE}|${b64(text)} G5_WAIT_FOR_FILE_E2E:${gate}`,
    );
    await expect
      .poll(() => readFile(page, ws.id, member, FILE), { timeout: 240_000 })
      .toBe(text);
    await releaseChatTurn(page, turn);
    turn = undefined;
    await page.reload();
    await expect(page.getByTestId("assistant-message").first()).toBeVisible({
      timeout: 60_000,
    });
    await expect(cards(page)).toHaveCount(0);
    expect(await readFile(page, ws.id, member, FILE)).toBe(text);
  });
});
