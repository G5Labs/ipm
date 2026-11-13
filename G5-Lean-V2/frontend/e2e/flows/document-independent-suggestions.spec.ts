import { randomUUID } from "node:crypto";
import { expect, test, type Locator } from "@playwright/test";
import {
  APP_BASE_URL,
  authedApi,
  authedApiViaRequest,
  cloneWorkspaceById,
  createWorkspaceClone,
  deleteWorkspaceById,
  expandProjectDocs,
  expandRepositoryFileRoot,
  openWorkspacePath,
  openFilePreview,
  readCurrentOrgContext,
  recordExpectedSubject,
  runWorkspaceVmAction,
  TEST_ORG,
  USING_OAUTH_STUB,
  waitForWorkspaceVmStatus,
} from "../helpers";
import {
  loginWithOAuthStub,
  SEEDED_FORK_MEMBER_IDENTITY,
  SEEDED_ORG_ADMIN_IDENTITY,
} from "../oauth";

type Edit = {
  id: string;
  beforeText: string;
  afterText: string;
  status: string;
};
type Parent = { id: string; path: string; status: string; edits: Edit[] };

test.describe.configure({ timeout: 360_000 });

test("Push review gives each PROJECT DOCS edit an independent durable bubble and Source keeps Git diff", async ({
  browser,
  page,
}) => {
  test.skip(!USING_OAUTH_STUB, "review needs a second OAuth stub identity");
  const source = await createWorkspaceClone(page, {
    namePrefix: "independent-doc-suggestions",
  });
  const reviewerContext = await browser.newContext({
    baseURL: APP_BASE_URL,
    storageState: { cookies: [], origins: [] },
  });
  const reviewerPage = await reviewerContext.newPage();
  let reviewWorkspaceId = "";
  let durableCloneId = "";
  const docName = `suggestions-${randomUUID()}.md`;
  const docPath = `docs/${source.id}/${docName}`;
  const before = "old\n\nQuiet.\n";
  const after =
    "replacement\n\nQuiet. added\n\nSecond added block.\n\nQuiet.\n";
  const afterReplacement = "replacement\n\nQuiet.\n";
  const afterSelectedAddition = "replacement\n\nQuiet. added\n\nQuiet.\n";
  try {
    await runWorkspaceVmAction(page, source.id, "resume");
    const sourceVm = await waitForWorkspaceVmStatus(page, source.id, "running");
    expect(sourceVm.vmId).toBeTruthy();
    const created = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${source.id}/docs`,
      "POST",
      { name: docName, docType: "design", filePath: docPath, content: before },
    );
    expect(created.status, JSON.stringify(created.body)).toBe(201);
    const detailResponse = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${source.id}`,
    );
    expect(detailResponse.status).toBe(200);
    const repoId = String(
      (detailResponse.body?.data ?? detailResponse.body)?.docsRepoId ?? "",
    );
    expect(repoId).toBeTruthy();
    const fileUrl = (workspaceId: string) =>
      `/api/v1/workspaces/${workspaceId}/files/${docPath}?repoId=${repoId}`;
    const original = await authedApiViaRequest(page, fileUrl(source.id));
    expect(original.status, JSON.stringify(original.body)).toBe(200);
    const seeded = await authedApiViaRequest(page, fileUrl(source.id), "PUT", {
      content: before,
      encoding: "utf-8",
      expectedHash: (original.body?.data ?? original.body).hash,
      expectedVmId: sourceVm.vmId,
      commitAndPush: true,
    });
    expect(seeded.status, JSON.stringify(seeded.body)).toBe(200);

    const { orgId } = await readCurrentOrgContext(page);
    const invited = await authedApi(
      page,
      `/api/v1/orgs/${orgId}/members`,
      "POST",
      {
        email: SEEDED_FORK_MEMBER_IDENTITY.email,
        role: "member",
      },
    );
    expect([200, 201], JSON.stringify(invited.body)).toContain(invited.status);
    await loginWithOAuthStub(reviewerPage, SEEDED_FORK_MEMBER_IDENTITY, {
      targetOrgSlug: TEST_ORG,
    });
    await recordExpectedSubject(reviewerPage);
    const started = await authedApiViaRequest(
      reviewerPage,
      `/api/v1/workspaces/${source.id}/review/start`,
      "POST",
      {},
    );
    expect([200, 201], JSON.stringify(started.body)).toContain(started.status);
    reviewWorkspaceId = String(
      (started.body?.data ?? started.body)?.review?.reviewWorkspaceId ?? "",
    );
    expect(reviewWorkspaceId).toBeTruthy();
    const reviewVm = await waitForWorkspaceVmStatus(
      reviewerPage,
      reviewWorkspaceId,
      "running",
    );
    const reviewOriginal = await authedApiViaRequest(
      reviewerPage,
      fileUrl(reviewWorkspaceId),
    );
    expect(reviewOriginal.status, JSON.stringify(reviewOriginal.body)).toBe(
      200,
    );
    const edited = await authedApiViaRequest(
      reviewerPage,
      fileUrl(reviewWorkspaceId),
      "PUT",
      {
        content: after,
        encoding: "utf-8",
        expectedHash: (reviewOriginal.body?.data ?? reviewOriginal.body).hash,
        expectedVmId: reviewVm.vmId,
      },
    );
    expect(edited.status, JSON.stringify(edited.body)).toBe(200);

    const reviewContextResponse = await authedApiViaRequest(
      reviewerPage,
      `/api/v1/workspaces/${reviewWorkspaceId}`,
    );
    expect(reviewContextResponse.status).toBe(200);
    const reviewDetail =
      reviewContextResponse.body?.data ?? reviewContextResponse.body;
    const reviewPath = `/${TEST_ORG}/${reviewDetail.name}`;
    await openWorkspacePath(reviewerPage, `${reviewPath}?view=reviews`);
    await expect(reviewerPage.getByTestId("my-review-panel")).toBeVisible({
      timeout: 30_000,
    });
    await reviewerPage.getByTestId("my-review-push").click();
    const push = reviewerPage.getByTestId("push-review-confirm");
    await expect(push).toBeEnabled({ timeout: 30_000 });
    const publishedResponse = reviewerPage.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        response
          .url()
          .includes(`/workspaces/${reviewWorkspaceId}/review/publish`),
    );
    await push.click();
    expect((await publishedResponse).status()).toBe(201);
    await expect(
      reviewerPage.getByTestId("push-review-dialog"),
    ).not.toBeVisible({ timeout: 30_000 });

    const list = async () => {
      const response = await authedApiViaRequest(
        page,
        `/api/v1/workspaces/${source.id}/review/suggestions?currentOnly=true`,
      );
      expect(response.status, JSON.stringify(response.body)).toBe(200);
      return (
        (response.body?.data ?? response.body).suggestions as Parent[]
      ).find((parent) => parent.path === docPath)!;
    };
    const parent = await list();
    expect(parent).toBeTruthy();
    expect(parent.status).toBe("open");
    expect(parent.edits).toHaveLength(3);
    expect(
      parent.edits.map((edit) => [
        edit.beforeText.trim(),
        edit.afterText.trim(),
      ]),
    ).toEqual([
      ["old", "replacement"],
      ["", "Quiet. added"],
      ["", "Second added block."],
    ]);
    const replacement = parent.edits.find(
      (edit) => edit.beforeText.trim() === "old",
    )!;
    const addition = parent.edits.find(
      (edit) => edit.afterText.trim() === "Quiet. added",
    )!;
    const remainder = parent.edits.find(
      (edit) => edit.afterText.trim() === "Second added block.",
    )!;
    expect(replacement).toBeTruthy();
    expect(addition).toBeTruthy();
    expect(remainder).toBeTruthy();
    expect(addition.beforeText).toBe("");
    expect(remainder.beforeText).toBe("");
    const editIds = parent.edits.map((edit) => edit.id);
    expect(new Set(editIds).size).toBe(3);
    for (const id of editIds)
      expect(id).toMatch(
        /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i,
      );
    const replacementCardId = `${parent.id}:${replacement.id}`;
    const additionCardId = `${parent.id}:${addition.id}`;
    const remainderCardId = `${parent.id}:${remainder.id}`;
    const assertLedger = async (statuses: string[]) => {
      const current = await list();
      expect(current.id).toBe(parent.id);
      expect(current.edits.map((edit) => [edit.id, edit.status])).toEqual(
        editIds.map((id, index) => [id, statuses[index]]),
      );
      return current;
    };
    const canonicalContent = async () => {
      const response = await authedApiViaRequest(page, fileUrl(source.id));
      expect(response.status, JSON.stringify(response.body)).toBe(200);
      return (response.body?.data ?? response.body).content;
    };

    const assertReadableAdditions = async (
      editor: Locator,
      texts: string[],
      canonicalQuietIndex: number,
    ) => {
      // Attention cards are actionable too; require the full proposed blocks in
      // the readable document preview, in source order, with one original row.
      const previews = editor.getByTestId(/^spec-suggestion-preview-/);
      const addedBlocks = previews.locator('[data-spec-review-change="add"]');
      await expect(addedBlocks).toHaveText(texts);
      for (const text of texts) {
        const block = previews.getByRole("button", {
          name: `Suggested addition: ${text}`,
          exact: true,
        });
        await expect(block).toBeVisible();
        await expect(block).toHaveAttribute("data-spec-review-change", "add");
        await expect(block.locator("p")).toHaveText(text);
      }
      await expect(
        editor.getByText("Needs attention:", { exact: false }),
      ).toHaveCount(0);
      await expect(
        editor.getByTestId(/^review-suggestion-competing-/),
      ).toHaveCount(0);
      const quiet = editor
        .locator("p:not(aside p)")
        .filter({ hasText: /^Quiet\.$/ });
      await expect(quiet).toHaveCount(1);
      await expect(quiet).toBeVisible();
      await expect(
        quiet.locator("xpath=ancestor-or-self::*[@data-block-index][1]"),
      ).toHaveAttribute("data-block-index", String(canonicalQuietIndex));
      await expect(
        quiet.locator("xpath=ancestor::*[@data-comment-connector-ignore]"),
      ).toHaveCount(0);
    };

    const openDoc = async () => {
      await expandProjectDocs(page);
      await page
        .getByTestId("sidebar-docs-tab")
        .getByTestId("doc-item")
        .filter({ hasText: docName })
        .click();
      await openFilePreview(page);
      const editor = page
        .getByTestId("center-panel")
        .getByTestId("document-preview");
      await expect(editor).toBeVisible({ timeout: 30_000 });
      return editor;
    };
    await openWorkspacePath(page, source.path);
    const editor = await openDoc();
    await expect(
      editor.getByTestId(`spec-suggestion-card-${replacementCardId}`),
    ).toBeVisible({ timeout: 30_000 });
    await expect(
      editor.getByTestId(`spec-suggestion-card-${additionCardId}`),
    ).toBeVisible();
    await expect(
      editor.getByTestId(`spec-suggestion-card-${remainderCardId}`),
    ).toBeVisible();
    await expect(editor.getByTestId(/^spec-suggestion-card-/)).toHaveCount(3);
    const previews = editor.getByTestId(/^spec-suggestion-preview-/);
    await expect(
      previews.getByRole("button", {
        name: "Suggested removal: old",
        exact: true,
      }),
    ).toBeVisible();
    await expect(
      previews.getByRole("button", {
        name: "Suggested addition: replacement",
        exact: true,
      }),
    ).toBeVisible();
    await assertReadableAdditions(
      editor,
      ["Quiet. added", "Second added block."],
      1,
    );
    await expect(
      editor
        .getByTestId("document-preview-content")
        .locator(".markdown-body p:not(aside p)"),
    ).toHaveText([
      "oldreplacement",
      "Quiet. added",
      "Second added block.",
      "Quiet.",
    ]);
    expect(await canonicalContent()).toBe(before);
    await expect(
      editor.getByText("Accept or reject them together", { exact: false }),
    ).toHaveCount(0);
    const acceptedResponse = page.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        response.url().includes(`/review/suggestions/${parent.id}/accept`),
    );
    await editor
      .getByTestId(`review-suggestion-accept-${replacementCardId}`)
      .click();
    const accepted = await acceptedResponse;
    expect(accepted.status()).toBe(200);
    expect(accepted.request().postDataJSON().editId).toBe(replacement.id);
    await expect.poll(canonicalContent).toBe(afterReplacement);
    await expect(
      editor.getByTestId(`review-suggestion-accept-${additionCardId}`),
    ).toBeVisible({ timeout: 30_000 });
    await expect(
      editor.getByTestId(`review-suggestion-accept-${remainderCardId}`),
    ).toBeVisible();
    // Preserve the screenshot criterion: choosing the replacement leaves all
    // separately added blocks open, and cannot write either one canonically.
    expect((await assertLedger(["accepted", "open", "open"])).status).toBe(
      "open",
    );
    await assertReadableAdditions(
      editor,
      ["Quiet. added", "Second added block."],
      1,
    );

    const addedResponse = page.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        response.url().includes(`/review/suggestions/${parent.id}/accept`),
    );
    await editor
      .getByTestId(`review-suggestion-accept-${additionCardId}`)
      .click();
    const added = await addedResponse;
    expect(added.status()).toBe(200);
    expect(added.request().postDataJSON().editId).toBe(addition.id);
    await expect.poll(canonicalContent).toBe(afterSelectedAddition);
    await expect(
      editor.getByTestId(`review-suggestion-accept-${remainderCardId}`),
    ).toBeVisible({ timeout: 30_000 });
    const partiallyDecided = await assertLedger([
      "accepted",
      "accepted",
      "open",
    ]);
    expect(partiallyDecided.status).toBe("open");
    await assertReadableAdditions(editor, ["Second added block."], 2);

    await page.reload({ waitUntil: "domcontentloaded" });
    const reopened = await openDoc();
    await expect(
      reopened.getByTestId(`review-suggestion-accept-${remainderCardId}`),
    ).toBeVisible({ timeout: 30_000 });
    await assertReadableAdditions(reopened, ["Second added block."], 2);
    expect(await canonicalContent()).toBe(afterSelectedAddition);
    expect((await assertLedger(["accepted", "accepted", "open"])).status).toBe(
      partiallyDecided.status,
    );

    // Cross the actual FILES section and repository/folder disclosure boundaries.
    const repositoryRoot = await expandRepositoryFileRoot(page, repoId);
    const files = repositoryRoot.locator("..").getByRole("group");
    const docsFolder = files
      .getByRole("treeitem")
      .filter({ has: page.getByText("docs", { exact: true }) });
    if ((await docsFolder.getAttribute("aria-expanded")) !== "true")
      await docsFolder.click();
    const workspaceFolder = files
      .getByRole("treeitem")
      .filter({ has: page.getByText(source.name, { exact: true }) });
    if ((await workspaceFolder.getAttribute("aria-expanded")) !== "true")
      await workspaceFolder.click();
    await files
      .getByTestId("file-tree-entry")
      .filter({ hasText: docName })
      .click();
    // FILES opens Markdown rendered; its Git diff-style cards are in plain text.
    await page
      .getByTestId("center-panel")
      .getByTestId("file-view-switch-mode")
      .click();
    const raw = page.getByTestId("center-panel").getByTestId("code-editor");
    await expect(raw).toBeVisible({ timeout: 30_000 });
    const rawSuggestion = page
      .getByTestId("center-panel")
      .getByTestId(`review-suggestion-${remainderCardId}`);
    await expect(rawSuggestion.getByTestId("review-hunk-diff")).toBeVisible({
      timeout: 30_000,
    });
    await expect(
      rawSuggestion
        .getByTestId("review-hunk-diff")
        .locator('[data-line-type="addition"]')
        .getByTestId("diff-line-code")
        .filter({ hasText: "Second added block." }),
    ).toHaveText("Second added block.");
    await expect(
      page.getByTestId("center-panel").getByTestId("document-preview"),
    ).toHaveCount(0);

    const again = await openDoc();
    await assertReadableAdditions(again, ["Second added block."], 2);
    await again
      .getByTestId(`review-suggestion-reject-${remainderCardId}`)
      .click();
    const rejectedResponse = page.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        response.url().includes(`/review/suggestions/${parent.id}/reject`),
    );
    await again
      .getByTestId(`review-suggestion-reject-confirm-${remainderCardId}`)
      .click();
    const rejected = await rejectedResponse;
    expect(rejected.status()).toBe(200);
    expect(rejected.request().postDataJSON().editId).toBe(remainder.id);
    const settled = await assertLedger(["accepted", "accepted", "rejected"]);
    expect(settled.status).toBe("partial");
    expect(await canonicalContent()).toBe(afterSelectedAddition);
    await page.reload({ waitUntil: "domcontentloaded" });
    const settledEditor = await openDoc();
    await expect(
      settledEditor.getByTestId(/^spec-suggestion-card-/),
    ).toHaveCount(0);
    expect(
      (await assertLedger(["accepted", "accepted", "rejected"])).status,
    ).toBe(settled.status);
    expect(await canonicalContent()).toBe(afterSelectedAddition);

    const durable = await cloneWorkspaceById(page, source.id, {
      namePrefix: "mixed-suggestions-durable",
      source: "durable",
      start: false,
    });
    durableCloneId = durable.id;
    const durableFile = await authedApiViaRequest(page, fileUrl(durable.id));
    expect(durableFile.status, JSON.stringify(durableFile.body)).toBe(200);
    expect((durableFile.body?.data ?? durableFile.body).content).toBe(
      afterSelectedAddition,
    );
  } finally {
    if (durableCloneId)
      await deleteWorkspaceById(page, durableCloneId).catch(() => {});
    if (reviewWorkspaceId)
      await deleteWorkspaceById(reviewerPage, reviewWorkspaceId).catch(
        () => {},
      );
    await reviewerContext.close();
    await deleteWorkspaceById(page, source.id).catch(() => {});
  }
});

test("reader file suggestions from multiple authors preview separately and recheck peers after acceptance", async ({
  browser,
  page,
}) => {
  test.skip(!USING_OAUTH_STUB, "review needs separate reader identities");
  const source = await createWorkspaceClone(page, {
    namePrefix: "overlapping-doc-suggestions",
  });
  const contexts = [];
  const name = `overlapping-${randomUUID()}.md`;
  const path = `docs/${source.id}/${name}`;
  const before = "Intro.\n\nhuman-provided\n\nEnd.\n";
  const replacements = [
    " an inplace edit with leading space",
    "kaihui-provided",
    "another-provided",
  ];
  const parents: Parent[] = [];
  try {
    await runWorkspaceVmAction(page, source.id, "resume");
    const vm = await waitForWorkspaceVmStatus(page, source.id, "running");
    const created = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${source.id}/docs`,
      "POST",
      { name, docType: "design", filePath: path, content: before },
    );
    expect(created.status, JSON.stringify(created.body)).toBe(201);
    const detail = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${source.id}`,
    );
    expect(detail.status).toBe(200);
    const repoId = String((detail.body?.data ?? detail.body).docsRepoId);
    const fileUrl = `/api/v1/workspaces/${source.id}/files/${path}?repoId=${repoId}`;
    const original = await authedApiViaRequest(page, fileUrl);
    expect(original.status).toBe(200);
    const seed = await authedApiViaRequest(page, fileUrl, "PUT", {
      content: before,
      encoding: "utf-8",
      expectedHash: (original.body?.data ?? original.body).hash,
      expectedVmId: vm.vmId,
      commitAndPush: true,
    });
    expect(seed.status, JSON.stringify(seed.body)).toBe(200);
    const { orgId } = await readCurrentOrgContext(page);
    const readers = [];
    for (const [identity, role] of [
      [SEEDED_FORK_MEMBER_IDENTITY, "member"],
      [SEEDED_ORG_ADMIN_IDENTITY, "admin"],
    ] as const) {
      const invited = await authedApi(
        page,
        `/api/v1/orgs/${orgId}/members`,
        "POST",
        { email: identity.email, role },
      );
      expect([200, 201], JSON.stringify(invited.body)).toContain(
        invited.status,
      );
      const context = await browser.newContext({
        baseURL: APP_BASE_URL,
        storageState: { cookies: [], origins: [] },
      });
      contexts.push(context);
      const reader = await context.newPage();
      await loginWithOAuthStub(reader, identity, { targetOrgSlug: TEST_ORG });
      await recordExpectedSubject(reader);
      readers.push(reader);
    }
    // The Source editor's publication API is additive, including another proposal
    // from a prior author; only the owner's later decision may write the source.
    for (let i = 0; i < replacements.length; i++) {
      const published = await authedApiViaRequest(
        readers[i % readers.length],
        `/api/v1/workspaces/${source.id}/review/file-suggestions`,
        "POST",
        {
          repoId,
          path,
          baseContent: before,
          content: before.replace("human-provided", replacements[i]),
          note: `Alternative ${i}`,
        },
      );
      expect(published.status, JSON.stringify(published.body)).toBe(201);
      const proposals = (published.body?.data ?? published.body)
        .suggestions as Parent[];
      expect(proposals).toHaveLength(1);
      expect(proposals[0].edits.length).toBeGreaterThan(0);
      parents.push(proposals[0]);
    }
    const readSource = async () => {
      const response = await authedApiViaRequest(page, fileUrl);
      expect(response.status).toBe(200);
      return (response.body?.data ?? response.body).content as string;
    };
    expect(await readSource()).toBe(before);
    await openWorkspacePath(page, source.path);
    const openDoc = async () => {
      await expandProjectDocs(page);
      await page
        .getByTestId("sidebar-docs-tab")
        .getByTestId("doc-item")
        .filter({ hasText: name })
        .click();
      await openFilePreview(page);
      const editor = page
        .getByTestId("center-panel")
        .getByTestId("document-preview");
      await expect(editor).toBeVisible({ timeout: 30_000 });
      return editor;
    };
    const editor = await openDoc();
    const cards = editor.getByTestId(/^spec-suggestion-card-/);
    // Publication splits the leading space into an independent insertion.
    // Discover durable child identities instead of assuming one edit per author.
    const alternatives = parents.map((parent) => {
      const replacements = parent.edits.filter((edit) => edit.beforeText);
      expect(replacements).toHaveLength(1);
      return { parent, edit: replacements[0] };
    });
    const whitespaceEdits = parents[0].edits.filter(
      (edit) => edit.beforeText === "" && edit.afterText === " ",
    );
    expect(whitespaceEdits).toHaveLength(1);
    const whitespace = whitespaceEdits[0];
    await expect(cards).toHaveCount(
      parents.reduce((total, parent) => total + parent.edits.length, 0),
      { timeout: 30_000 },
    );
    const previews = editor.getByTestId(/^spec-suggestion-preview-/);
    await expect(previews).toHaveCount(1);
    for (const { parent, edit } of alternatives) {
      const card = editor.getByTestId(
        `spec-suggestion-card-${parent.id}:${edit.id}`,
      );
      if ((await card.getAttribute("data-selected")) !== "true")
        await card.getByRole("radio", { name: "Preview in document" }).click();
      await expect(card).toHaveAttribute("data-selected", "true");
      await expect(previews.locator("ins")).toHaveText([
        "·",
        edit.afterText.replaceAll(" ", "·"),
      ]);
    }
    const { parent: chosen, edit: chosenEdit } = alternatives[1];
    const chosenCard = editor.getByTestId(
      `spec-suggestion-card-${chosen.id}:${chosenEdit.id}`,
    );
    const accepted = page.waitForResponse(
      (r) =>
        r.request().method() === "POST" &&
        r.url().endsWith(`/review/suggestions/${chosen.id}/accept`),
    );
    await chosenCard
      .getByRole("button", { name: "Accept", exact: true })
      .click();
    expect((await accepted).status()).toBe(200);
    await expect
      .poll(readSource, { timeout: 30_000 })
      .toBe(before.replace("human-provided", replacements[1]));
    // The server retires incompatible same-source alternatives. A reload must
    // honor those authoritative statuses instead of reviving inline previews.
    await page.reload();
    const reloaded = await openDoc();
    await expect(reloaded).toContainText("kaihui-provided");
    const remainingCard = reloaded.getByTestId(
      `spec-suggestion-card-${parents[0].id}:${whitespace.id}`,
    );
    await expect(remainingCard).toBeVisible();
    await expect(reloaded.getByTestId(/^spec-suggestion-card-/)).toHaveCount(1);
    for (const { parent, edit } of [alternatives[0], alternatives[2]]) {
      await expect(
        reloaded.getByTestId(`spec-suggestion-card-${parent.id}:${edit.id}`),
      ).toHaveCount(0);
      const refused = await authedApiViaRequest(
        page,
        `/api/v1/workspaces/${source.id}/review/suggestions/${parent.id}/accept`,
        "POST",
        { editId: edit.id },
      );
      expect(refused.status, JSON.stringify(refused.body)).toBe(409);
      expect(JSON.stringify(refused.body)).toContain("obsolete");
    }
    expect(await readSource()).toBe(
      before.replace("human-provided", replacements[1]),
    );
    const listed = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${source.id}/review/suggestions`,
    );
    expect(listed.status).toBe(200);
    const rows = (listed.body?.data ?? listed.body).suggestions as Parent[];
    for (const [i, { parent, edit }] of alternatives.entries())
      expect(
        rows
          .find((p) => p.id === parent.id)
          ?.edits.find((e) => e.id === edit.id)?.status,
      ).toBe(i === 1 ? "accepted" : "obsolete");
    expect(
      rows
        .find((p) => p.id === parents[0].id)
        ?.edits.find((e) => e.id === whitespace.id)?.status,
    ).toBe("open");
    // This boundary insertion never competed with the replacement. It retains
    // its own decision and may be rejected without changing accepted source.
    await remainingCard
      .getByRole("button", { name: "Reject", exact: true })
      .click();
    const rejected = page.waitForResponse(
      (r) =>
        r.request().method() === "POST" &&
        r.url().endsWith(`/review/suggestions/${parents[0].id}/reject`),
    );
    await remainingCard
      .getByTestId(
        `review-suggestion-reject-confirm-${parents[0].id}:${whitespace.id}`,
      )
      .click();
    expect((await rejected).status()).toBe(200);
    await expect(reloaded.getByTestId(/^spec-suggestion-card-/)).toHaveCount(0);
    await expect(reloaded.getByTestId(/^spec-suggestion-preview-/)).toHaveCount(
      0,
    );
    expect(await readSource()).toBe(
      before.replace("human-provided", replacements[1]),
    );
  } finally {
    await Promise.all(contexts.map((context) => context.close()));
    await deleteWorkspaceById(page, source.id);
  }
});
