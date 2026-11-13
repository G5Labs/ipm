import { randomUUID } from "node:crypto";
import { expect, test, type Page } from "@playwright/test";
import {
  APP_BASE_URL,
  activateSidebarSection,
  authedApi,
  authedApiViaRequest,
  cloneWorkspaceById,
  createWorkspaceClone,
  deleteWorkspaceById,
  openWorkspacePath,
  readCurrentOrgContext,
  recordExpectedSubject,
  runWorkspaceVmAction,
  TEST_ORG,
  USING_OAUTH_STUB,
  waitForLiveWorkspaceChanges,
  waitForWorkspaceVmStatus,
} from "../helpers";
import { loginWithOAuthStub, SEEDED_FORK_MEMBER_IDENTITY } from "../oauth";
import type { SourceMenu } from "../../src/lib/sourceControlSelection";

async function branchHead(page: Page, workspaceId: string, repoId: string) {
  const response = await authedApiViaRequest(
    page,
    `/api/v1/workspaces/${workspaceId}/branches`,
  );
  expect(response.status).toBe(200);
  const body = response.body?.data ?? response.body;
  const rows = Array.isArray(body) ? body : (body?.branches ?? []);
  const branch = rows.find(
    (row: { repoId?: string; repo_id?: string }) =>
      row.repoId === repoId || row.repo_id === repoId,
  );
  return String(branch?.headCommit ?? branch?.head_commit ?? "");
}

test.describe.configure({ timeout: 360_000 });

test("accepting a review edit and new file commits and pushes only those paths", async ({
  browser,
  page,
}) => {
  test.skip(!USING_OAUTH_STUB, "review needs a second OAuth stub identity");
  const source = await createWorkspaceClone(page, {
    namePrefix: "auto-commit-review",
  });
  let reviewWorkspaceId = "";
  let durableCloneId = "";
  const reviewerContext = await browser.newContext({
    baseURL: APP_BASE_URL,
    storageState: { cookies: [], origins: [] },
  });
  const reviewerPage = await reviewerContext.newPage();
  try {
    await runWorkspaceVmAction(page, source.id, "resume");
    const sourceVm = await waitForWorkspaceVmStatus(page, source.id, "running");
    expect(sourceVm.vmId).toBeTruthy();
    const live = await waitForLiveWorkspaceChanges(page, source.id);
    const detailResponse = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${source.id}`,
    );
    expect(detailResponse.status).toBe(200);
    const detail = detailResponse.body?.data ?? detailResponse.body;
    const repoId = live.members.find(
      (member) => member.repo_id !== detail.docsRepoId,
    )?.repo_id;
    expect(repoId).toBeTruthy();
    const token = randomUUID().replaceAll("-", "");
    const editPath = `e2e-review-${token}.txt`;
    const savePath = `e2e-review-save-${token}.txt`;
    const newPath = `e2e-review-new-${token}.txt`;
    const unrelatedPath = `e2e-review-draft-${token}.txt`;
    const fileUrl = (workspaceId: string, path: string) =>
      `/api/v1/workspaces/${workspaceId}/files/${path}?repoId=${repoId}`;
    const seed = await authedApiViaRequest(
      page,
      fileUrl(source.id, editPath),
      "PUT",
      {
        content: "before review\n",
        encoding: "utf-8",
        expectedHash: null,
        expectedVmId: sourceVm.vmId,
        commitAndPush: true,
      },
    );
    expect(seed.status, JSON.stringify(seed.body)).toBe(200);
    const saveSeed = await authedApiViaRequest(
      page,
      fileUrl(source.id, savePath),
      "PUT",
      {
        content: "owner file before review\n",
        encoding: "utf-8",
        expectedHash: null,
        expectedVmId: sourceVm.vmId,
        commitAndPush: true,
      },
    );
    expect(saveSeed.status, JSON.stringify(saveSeed.body)).toBe(200);

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
    await waitForWorkspaceVmStatus(reviewerPage, reviewWorkspaceId, "running");
    const original = await authedApiViaRequest(
      reviewerPage,
      fileUrl(reviewWorkspaceId, editPath),
    );
    expect(original.status).toBe(200);
    const edited = await authedApiViaRequest(
      reviewerPage,
      fileUrl(reviewWorkspaceId, editPath),
      "PUT",
      {
        content: "after review\n",
        encoding: "utf-8",
        expectedHash: (original.body?.data ?? original.body).hash,
      },
    );
    expect(edited.status, JSON.stringify(edited.body)).toBe(200);
    const saveOriginal = await authedApiViaRequest(
      reviewerPage,
      fileUrl(reviewWorkspaceId, savePath),
    );
    expect(saveOriginal.status).toBe(200);
    const saveProposal = await authedApiViaRequest(
      reviewerPage,
      fileUrl(reviewWorkspaceId, savePath),
      "PUT",
      {
        content: "reviewer proposal on saved file\n",
        encoding: "utf-8",
        expectedHash: (saveOriginal.body?.data ?? saveOriginal.body).hash,
      },
    );
    expect(saveProposal.status, JSON.stringify(saveProposal.body)).toBe(200);
    const created = await authedApiViaRequest(
      reviewerPage,
      fileUrl(reviewWorkspaceId, newPath),
      "PUT",
      { content: "new reviewer file\n", encoding: "utf-8", expectedHash: null },
    );
    expect(created.status, JSON.stringify(created.body)).toBe(200);
    const published = await authedApiViaRequest(
      reviewerPage,
      `/api/v1/workspaces/${reviewWorkspaceId}/review/publish`,
      "POST",
      { summary: "Review commit and push", verdict: "request_changes" },
    );
    expect(published.status, JSON.stringify(published.body)).toBe(201);

    const suggestions = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${source.id}/review/suggestions`,
    );
    expect(suggestions.status).toBe(200);
    const rows = (suggestions.body?.data ?? suggestions.body)
      .suggestions as Array<{
      id: string;
      path: string;
      status: string;
      edits?: Array<{ id: string; status: string }>;
    }>;
    const edit = rows.find(
      (row) => row.path === editPath && row.status === "open",
    );
    const added = rows.find(
      (row) => row.path === newPath && row.status === "open",
    );
    const savedSuggestion = rows.find(
      (row) => row.path === savePath && row.status === "open",
    );
    expect(edit).toBeTruthy();
    expect(added).toBeTruthy();
    expect(savedSuggestion).toBeTruthy();
    const cardId = (row: (typeof rows)[number]) =>
      row.edits?.find((child) => child.status === "open")
        ? `${row.id}:${row.edits.find((child) => child.status === "open")!.id}`
        : row.id;

    const draft = await authedApiViaRequest(
      page,
      fileUrl(source.id, unrelatedPath),
      "PUT",
      {
        content: "unrelated owner draft\n",
        encoding: "utf-8",
        expectedHash: null,
      },
    );
    expect(draft.status).toBe(200);
    await openWorkspacePath(page, `${source.path}?view=reviews`);
    await expect(page.getByTestId("workspace-commit")).toHaveCount(0);
    await expect(page.getByTestId("reviews-panel")).toBeVisible({
      timeout: 30_000,
    });

    // Save from the editor while a suggestion for this file is still open.
    const beforeEditorSave = await branchHead(page, source.id, repoId!);
    await page
      .getByTestId(`review-suggestion-open-file-${cardId(savedSuggestion!)}`)
      .click();
    const editor = page.getByTestId("center-panel").getByTestId("code-editor");
    await expect(editor).toBeVisible();
    // FILES opens files read-first; the owner edits after Edit.
    await page
      .getByTestId("center-panel")
      .getByTestId("file-view-edit")
      .click();
    // Click the first line itself. The open suggestion is drawn inline below
    // it, and a click inside that box leaves the editor without a cursor.
    await editor.locator(".monaco-editor .view-line").first().click();
    await page.keyboard.press("ControlOrMeta+End");
    const ownerEdit = `owner edit ${token}`;
    await page.keyboard.insertText(ownerEdit);
    const saveButton = editor.getByTestId("code-editor-save-button");
    await expect(saveButton).toBeEnabled();
    const saveResponse = page.waitForResponse(
      (candidate) =>
        candidate.request().method() === "PUT" &&
        candidate.url().includes(`/workspaces/${source.id}/files/${savePath}`),
    );
    await saveButton.click();
    expect((await saveResponse).status()).toBe(200);
    expect(await branchHead(page, source.id, repoId!)).not.toBe(
      beforeEditorSave,
    );
    await openWorkspacePath(page, `${source.path}?view=reviews`);
    await expect(page.getByTestId("workspace-commit")).toHaveCount(0);
    await expect(page.getByTestId("reviews-panel")).toBeVisible({
      timeout: 30_000,
    });
    const outdatedAccept = page.getByTestId(
      `review-suggestion-accept-${cardId(edit!)}`,
    );
    if (!(await outdatedAccept.isVisible())) {
      await page
        .locator('[data-testid^="reviews-commit-toggle-"]')
        .first()
        .click();
    }
    await expect(outdatedAccept).toBeVisible();

    let previousHead = await branchHead(page, source.id, repoId!);
    for (const suggestion of [edit!, added!]) {
      const response = page.waitForResponse(
        (candidate) =>
          candidate.request().method() === "POST" &&
          candidate
            .url()
            .includes(`/review/suggestions/${suggestion.id}/accept`),
      );
      await page
        .getByTestId(`review-suggestion-accept-${cardId(suggestion)}`)
        .click();
      expect((await response).status()).toBe(200);
      const nextHead = await branchHead(page, source.id, repoId!);
      expect(nextHead).not.toBe(previousHead);
      previousHead = nextHead;
    }

    const clone = await cloneWorkspaceById(page, source.id, {
      namePrefix: "accepted-review-durable",
      source: "durable",
      start: false,
    });
    durableCloneId = clone.id;
    for (const [path, content] of [
      [editPath, "after review\n"],
      [newPath, "new reviewer file\n"],
    ]) {
      const file = await authedApiViaRequest(page, fileUrl(clone.id, path));
      expect(file.status, JSON.stringify(file.body)).toBe(200);
      expect((file.body?.data ?? file.body).content).toBe(content);
    }
    const savedFile = await authedApiViaRequest(
      page,
      fileUrl(clone.id, savePath),
    );
    expect(savedFile.status).toBe(200);
    expect((savedFile.body?.data ?? savedFile.body).content).toContain(
      ownerEdit,
    );
    const omitted = await authedApiViaRequest(
      page,
      fileUrl(clone.id, unrelatedPath),
    );
    expect(omitted.status).toBe(404);
    const remaining = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${source.id}/changes`,
    );
    expect(remaining.status).toBe(200);
    const member = (remaining.body?.data ?? remaining.body).members.find(
      (row: { repo_id: string }) => row.repo_id === repoId,
    );
    expect(member.against_internal?.ahead).toBe(0);
    expect(
      member.dirty?.some(
        (file: { path: string }) => file.path === unrelatedPath,
      ),
    ).toBe(true);
    const duplicate = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${source.id}/review/suggestions/${added!.id}/accept`,
      "POST",
      {},
    );
    expect(duplicate.status).toBe(409);

    // The unrelated draft survives proposal acceptance until the explicit
    // Source Control Save checkpoint commits and pushes it.
    await openWorkspacePath(page, `${source.path}?view=activity`);
    const checklistResponse = page.waitForResponse(
      (response) =>
        response.request().method() === "GET" &&
        response.url().endsWith(`/workspaces/${source.id}/source-control/menu`),
    );
    await activateSidebarSection(page, "sidebar-source-control-tab");
    const checklistReceipt = await checklistResponse;
    expect(checklistReceipt.status(), await checklistReceipt.text()).toBe(200);
    const checklist = (await checklistReceipt.json()).data as SourceMenu;
    const code = checklist.repos.find((repo) => repo.repoId === repoId);
    expect(code?.live).toEqual({ selectable: true });
    const menu = page.getByTestId("source-control-menu");
    await expect(
      menu.getByRole("checkbox", {
        name: `Not yet pushed in ${code!.name}`,
        exact: true,
      }),
    ).toBeChecked();
    const checkpoint = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${source.id}/source-control/checkpoint`,
    );
    expect(checkpoint.status).toBe(200);
    const receipt = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${source.id}/source-control/save`,
      "POST",
      {},
    );
    expect(receipt.status, JSON.stringify(receipt.body)).toBe(200);
    const saved = receipt.body.data;
    expect(saved.saved).toBe(true);
    expect(saved.generation).toBe((checkpoint.body.data.generation ?? 0) + 1);
    await expect(menu).toContainText(
      "checklist has been updated after Save changes",
    );
    expect(await branchHead(page, source.id, repoId!)).not.toBe(previousHead);
    await runWorkspaceVmAction(page, source.id, "stop");
    await waitForWorkspaceVmStatus(page, source.id, "stopped");
    const durableDraft = await authedApiViaRequest(
      page,
      fileUrl(source.id, unrelatedPath),
    );
    expect(durableDraft.status, JSON.stringify(durableDraft.body)).toBe(200);
    expect((durableDraft.body?.data ?? durableDraft.body).content).toBe(
      "unrelated owner draft\n",
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
