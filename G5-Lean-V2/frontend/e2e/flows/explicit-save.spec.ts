import { randomUUID } from "node:crypto";
import { execFileSync, spawnSync } from "node:child_process";
import { expect, test, type Locator, type Page } from "@playwright/test";
import {
  authedApi,
  authedApiViaRequest,
  cloneWorkspaceById,
  createWorkspaceClone,
  deleteWorkspaceById,
  expandProjectDocs,
  expandRepositoryFileRoot,
  openWorkspacePath,
  openFileSource,
  openFilePreview,
  readWorkspaceVm,
  runWorkspaceVmAction,
  softVisible,
  waitForWorkspaceVmStatus,
  watchWorkspaceSubscription,
  type WorkspaceClone,
} from "../helpers";

async function openRawCodeEditor(
  page: Page,
  repoId?: string,
): Promise<Locator> {
  await expandRepositoryFileRoot(page, repoId);
  const rows = page
    .getByTestId("sidebar-files-tab")
    .getByTestId("file-tree-entry");
  // The expanded root lists its children only once the live tree read
  // returns; counting before that sees an empty tree.
  await expect(rows.first()).toBeVisible({ timeout: 30_000 });
  const count = await rows.count();
  for (let index = 0; index < Math.min(count, 30); index += 1) {
    await rows.nth(index).click();
    const editor = page.getByTestId("center-panel").getByTestId("code-editor");
    if (await softVisible(editor, 1_500)) return editor;
  }
  throw new Error("No raw repository file opened in CodeEditor");
}

async function appendInEditor(
  page: Page,
  editor: Locator,
  text: string,
): Promise<void> {
  // FILES opens files read-first; the owner edits after Edit
  // (`g5.documents.files-view`).
  const edit = page.getByTestId("center-panel").getByTestId("file-view-edit");
  if (await softVisible(edit, 2_000)) await edit.click();
  // Markdown read rendered opens Rich text; this flow types plain text.
  const toggle = page
    .getByTestId("center-panel")
    .getByTestId("file-view-switch-mode");
  if (
    (await toggle.count()) &&
    ((await toggle.textContent()) ?? "").includes("plain text")
  )
    await toggle.click();
  // Monaco in Chromium takes input through its EditContext element, which
  // Playwright reports as hidden, and the only `<textarea>` it renders is the
  // read-only IME helper. Clicking the text area gives the editor focus; the
  // keystrokes then reach the model through whichever input path it uses.
  const lines = editor.locator(".monaco-editor .view-lines").first();
  await expect(lines).toBeVisible({ timeout: 15_000 });
  await lines.click();
  await page.keyboard.press("ControlOrMeta+End");
  await page.keyboard.insertText(text);
  await expect(editor).toContainText(text.trim());
}

async function openProjectDocEditor(page: Page): Promise<Locator> {
  await expandProjectDocs(page);
  const rows = page.getByTestId("sidebar-docs-tab").getByTestId("doc-item");
  await expect(rows.first()).toBeVisible({ timeout: 30_000 });
  const count = await rows.count();
  for (let index = 0; index < count; index += 1) {
    await rows.nth(index).click();
    const editor = page.getByTestId("center-panel").getByTestId("file-view");
    if (await softVisible(editor, 2_000)) return editor;
  }
  throw new Error("No existing project document opened in Monaco");
}

async function branchHead(page: Page, workspaceId: string, repoId: string) {
  const response = await authedApiViaRequest(
    page,
    `/api/v1/workspaces/${workspaceId}/branches`,
  );
  expect(response.status, JSON.stringify(response.body)).toBe(200);
  const body = response.body?.data ?? response.body;
  const branches = Array.isArray(body) ? body : (body?.branches ?? []);
  const branch = branches.find(
    (row: { repoId?: string; repo_id?: string }) =>
      row.repoId === repoId || row.repo_id === repoId,
  );
  return String(branch?.headCommit ?? branch?.head_commit ?? "");
}

test.describe("Explicit Save", () => {
  test.describe.configure({ timeout: 360_000 });
  let workspace: WorkspaceClone;
  let forks: WorkspaceClone[] = [];

  test.beforeEach(async ({ page }, testInfo) => {
    forks = [];
    workspace = await createWorkspaceClone(page, {
      namePrefix: `explicit-save-${testInfo.title}`,
    });
    await runWorkspaceVmAction(page, workspace.id, "resume");
    await waitForWorkspaceVmStatus(page, workspace.id, "running");
    await openWorkspacePath(page, workspace.path);
  });

  test.afterEach(async ({ page }) => {
    for (const fork of forks) {
      await deleteWorkspaceById(page, fork.id).catch(() => {});
    }
    await deleteWorkspaceById(page, workspace?.id).catch(() => {});
  });

  async function forkRead(
    page: Page,
    repoId: string,
    path: string,
    namePrefix: string,
    source: "live" | "durable" = "live",
  ): Promise<string> {
    const fork = await cloneWorkspaceById(page, workspace.id, {
      namePrefix,
      source,
      interrupt: false,
      start: false,
    });
    forks.push(fork);
    expect((await readWorkspaceVm(page, fork.id)).status).toBe("stopped");
    // A live fork retains unpublished work for its next attachment. Stopped
    // browsing deliberately exposes only published Git, so inspect that
    // retained work as the running fork owner.
    if (source === "live") {
      await runWorkspaceVmAction(page, fork.id, "resume");
      await waitForWorkspaceVmStatus(page, fork.id, "running");
    }
    const read = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${fork.id}/files/${path}?repoId=${repoId}`,
    );
    expect(read.status, JSON.stringify(read.body)).toBe(200);
    if (source === "live") {
      await runWorkspaceVmAction(page, fork.id, "stop");
      await waitForWorkspaceVmStatus(page, fork.id, "stopped");
    }
    return String((read.body?.data ?? read.body)?.content ?? "");
  }

  test("selected Save commits a file with Git clean filters", async ({
    page,
  }) => {
    const detailResponse = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${workspace.id}`,
    );
    expect(detailResponse.status).toBe(200);
    const detail = detailResponse.body?.data ?? detailResponse.body;
    const repoId = detail.branches.find(
      (branch: { repoId: string }) => branch.repoId !== detail.docsRepoId,
    )?.repoId;
    if (!repoId) throw new Error("No code repository in workspace");
    const vm = await waitForWorkspaceVmStatus(page, workspace.id, "running");
    const path = `e2e-filter-${randomUUID()}.txt`;
    const fileUrl = `/api/v1/workspaces/${workspace.id}/files/${path}?repoId=${repoId}`;
    const attributes = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${workspace.id}/files/.gitattributes?repoId=${repoId}`,
      "PUT",
      {
        content: "e2e-filter-*.txt text eol=lf\n",
        encoding: "utf-8",
        expectedHash: null,
        expectedVmId: vm.vmId,
        commitAndPush: true,
      },
    );
    expect(attributes.status, JSON.stringify(attributes.body)).toBe(200);

    const before = await branchHead(page, workspace.id, repoId);
    const content = "first\r\nsecond\r\n";
    const saved = await authedApiViaRequest(page, fileUrl, "PUT", {
      content,
      encoding: "utf-8",
      expectedHash: null,
      expectedVmId: vm.vmId,
      commitAndPush: true,
    });
    expect(saved.status, JSON.stringify(saved.body)).toBe(200);
    const after = await branchHead(page, workspace.id, repoId);
    expect(after).not.toBe(before);
    expect((await authedApiViaRequest(page, fileUrl)).body?.data?.content).toBe(
      content,
    );
    expect(
      await forkRead(page, repoId, path, "filtered-save-durable", "durable"),
    ).toBe("first\nsecond\n");
  });

  test("CodeEditor saves with a hash and exposes reload-or-overwrite on conflict", async ({
    page,
  }) => {
    const workspaceDetailResponse = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${workspace.id}`,
    );
    expect(workspaceDetailResponse.status).toBe(200);
    const workspaceDetail =
      workspaceDetailResponse.body?.data ?? workspaceDetailResponse.body;
    const codeBranch = workspaceDetail.branches.find(
      (branch: { repoId: string }) =>
        branch.repoId !== workspaceDetail.docsRepoId,
    );
    expect(codeBranch?.repoId).toBeTruthy();
    const editor = await openRawCodeEditor(page, codeBranch.repoId);
    const filePath = await editor.getAttribute("data-file-path");
    expect(filePath).toBeTruthy();
    const fileUrl = `/api/v1/workspaces/${workspace.id}/files/${filePath}?repoId=${codeBranch.repoId}`;
    const initial = await authedApiViaRequest(page, fileUrl);
    expect(initial.status, JSON.stringify(initial.body)).toBe(200);
    const initialFile = initial.body?.data ?? initial.body;
    const headBeforeSave = await branchHead(
      page,
      workspace.id,
      codeBranch.repoId,
    );
    const added = `\nexplicit save ${Date.now()}\n`;
    await appendInEditor(page, editor, added);
    await expect(
      editor.getByTestId("code-editor-dirty-indicator"),
    ).toContainText("browser only");
    expect((await authedApiViaRequest(page, fileUrl)).body?.data?.content).toBe(
      initialFile.content,
    );
    expect(
      await forkRead(page, codeBranch.repoId, filePath!, "f4-code-unsaved"),
    ).toBe(initialFile.content);

    const unrelatedPath = `e2e-unrelated-${Date.now()}.txt`;
    const unrelated = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${workspace.id}/files/${unrelatedPath}?repoId=${codeBranch.repoId}`,
      "PUT",
      {
        content: "keep this draft out of Save\n",
        encoding: "utf-8",
        expectedHash: null,
      },
    );
    expect([200, 201], JSON.stringify(unrelated.body)).toContain(
      unrelated.status,
    );

    const saveRequest = page.waitForRequest(
      (request) =>
        request.method() === "PUT" &&
        request.url().includes(`/workspaces/${workspace.id}/files/`),
    );
    const saveResponse = page.waitForResponse(
      (response) =>
        response.request().method() === "PUT" &&
        response.url().includes(`/workspaces/${workspace.id}/files/`),
    );
    await editor.getByTestId("code-editor-save-button").click();
    const request = await saveRequest;
    const requestBody = request.postDataJSON();
    // The file version is the repository's blob id (SHA-1 or SHA-256).
    expect(requestBody.expectedHash).toBe(initialFile.hash);
    expect(requestBody.commitAndPush).toBe(true);
    expect(requestBody.content).toContain(added.trim());
    expect(requestBody.content).not.toBe(initialFile.content);
    const response = await saveResponse;
    expect(response.status()).toBe(200);
    await expect(editor.getByTestId("code-editor-dirty-indicator")).toHaveCount(
      0,
    );
    const savedBody = await response.json();
    const savedHash = String(savedBody?.data?.hash ?? savedBody?.hash ?? "");
    expect(savedHash).toMatch(/^([0-9a-f]{40}|[0-9a-f]{64})$/);
    expect((await authedApiViaRequest(page, fileUrl)).body?.data?.content).toBe(
      requestBody.content,
    );
    expect(
      await forkRead(page, codeBranch.repoId, filePath!, "f4-code-saved"),
    ).toBe(requestBody.content);
    const headAfterSave = await branchHead(
      page,
      workspace.id,
      codeBranch.repoId,
    );
    expect(headAfterSave).not.toBe(headBeforeSave);
    expect(
      await forkRead(
        page,
        codeBranch.repoId,
        filePath!,
        "f4-code-pushed",
        "durable",
      ),
    ).toBe(requestBody.content);
    const changes = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${workspace.id}/changes`,
    );
    expect(changes.status, JSON.stringify(changes.body)).toBe(200);
    const member = (changes.body?.data ?? changes.body).members.find(
      (row: { repo_id: string }) => row.repo_id === codeBranch.repoId,
    );
    expect(member.against_internal?.ahead).toBe(0);
    expect(
      member.dirty?.some(
        (file: { path: string }) => file.path === unrelatedPath,
      ),
    ).toBe(true);
    expect(
      member.dirty?.some((file: { path: string }) => file.path === filePath),
    ).toBe(false);

    const sameSave = await authedApiViaRequest(page, fileUrl, "PUT", {
      content: requestBody.content,
      encoding: "utf-8",
      expectedHash: savedHash,
      expectedVmId: initialFile.vmId,
      commitAndPush: true,
    });
    expect(sameSave.status, JSON.stringify(sameSave.body)).toBe(200);
    expect(await branchHead(page, workspace.id, codeBranch.repoId)).toBe(
      headAfterSave,
    );
    await page.reload({ waitUntil: "domcontentloaded" });
    const reloadedEditor = await openRawCodeEditor(page, codeBranch.repoId);
    expect(await reloadedEditor.getAttribute("data-file-path")).toBe(filePath);
    await expect(reloadedEditor).toContainText(added.trim());

    // Change the file on disk behind the editor through the same VM write
    // boundary (design v9 §9.2), so the editor's next Save carries a stale
    // hash and the server, not a mocked route, answers with the real conflict
    // and the current bytes.
    const savedUrl = new URL(request.url());
    await appendInEditor(
      page,
      reloadedEditor,
      `\nconflicting save ${Date.now()}\n`,
    );
    const outOfBand = await authedApiViaRequest(
      page,
      `${savedUrl.pathname}${savedUrl.search}`,
      "PUT",
      {
        content: "current bytes\n",
        encoding: "utf-8",
        expectedHash: savedHash,
      },
    );
    expect([200, 201], JSON.stringify(outOfBand.body)).toContain(
      outOfBand.status,
    );

    const conflictResponse = page.waitForResponse(
      (candidate) =>
        candidate.request().method() === "PUT" &&
        candidate.url().includes(`/workspaces/${workspace.id}/files/`),
    );
    await reloadedEditor.getByTestId("code-editor-save-button").click();
    const conflict = await conflictResponse;
    expect(conflict.status()).toBe(409);
    const conflictBody = await conflict.json();
    expect(conflictBody?.error?.code).toBe("FILE_VERSION_CONFLICT");
    expect(conflictBody?.error?.details?.content).toBe("current bytes\n");
    const dialog = reloadedEditor.getByTestId("code-editor-conflict-dialog");
    await expect(dialog).toBeVisible();
    await expect(
      dialog.getByTestId("code-editor-conflict-overwrite"),
    ).toBeVisible();
    await dialog.getByTestId("code-editor-conflict-reload").click();
    await expect(reloadedEditor).toContainText("current bytes", {
      timeout: 15_000,
    });
  });

  // This scenario injects a hook into the local g5-dev-api container. The CI
  // receipt requires every collected Explicit Save case to run without skip.
  const localDevTest = process.env.CI ? null : test;
  localDevTest?.(
    "Save reports a rejected remote push and retries without another commit",
    async ({ page }) => {
      const probe = spawnSync(
        "docker",
        ["inspect", "-f", "{{.State.Running}}", "g5-dev-api"],
        { encoding: "utf8" },
      );
      test.skip(
        probe.status !== 0 || probe.stdout.trim() !== "true",
        "requires the local dev-stack Git remote",
      );
      const detailResponse = await authedApiViaRequest(
        page,
        `/api/v1/workspaces/${workspace.id}`,
      );
      expect(detailResponse.status).toBe(200);
      const detail = detailResponse.body?.data ?? detailResponse.body;
      const codeRepoId = String(
        detail.branches.find(
          (branch: { repoId: string }) => branch.repoId !== detail.docsRepoId,
        )?.repoId ?? "",
      );
      expect(codeRepoId).toMatch(/^[0-9a-f-]{36}$/);
      const orgId = String(detail.orgId ?? "");
      expect(orgId).toMatch(/^[0-9a-f-]{36}$/);
      const editor = await openRawCodeEditor(page, codeRepoId);
      const path = await editor.getAttribute("data-file-path");
      expect(path).toBeTruthy();
      const initialResponse = await authedApiViaRequest(
        page,
        `/api/v1/workspaces/${workspace.id}/files/${path}?repoId=${codeRepoId}`,
      );
      expect(initialResponse.status).toBe(200);
      const initial = initialResponse.body?.data ?? initialResponse.body;
      const remoteBefore = await branchHead(page, workspace.id, codeRepoId);
      const marker = `\nretry rejected push ${Date.now()}\n`;
      await appendInEditor(page, editor, marker);
      const setHook = (enabled: boolean) => {
        const script = enabled
          ? 'repo="/mnt/g5-repos/repos/$1/$2"; if [ -d "$repo/.git" ]; then repo="$repo/.git"; fi; printf "#!/bin/sh\\nexit 1\\n" > "$repo/hooks/pre-receive"; chmod 755 "$repo/hooks/pre-receive"'
          : 'repo="/mnt/g5-repos/repos/$1/$2"; if [ -d "$repo/.git" ]; then repo="$repo/.git"; fi; rm -f "$repo/hooks/pre-receive"';
        execFileSync("docker", [
          "exec",
          "g5-dev-api",
          "sh",
          "-c",
          script,
          "sh",
          orgId,
          codeRepoId,
        ]);
      };
      try {
        setHook(true);
        const failedResponse = page.waitForResponse(
          (candidate) =>
            candidate.request().method() === "PUT" &&
            candidate
              .url()
              .includes(`/workspaces/${workspace.id}/files/${path}`),
        );
        await editor.getByTestId("code-editor-save-button").click();
        const failed = await failedResponse;
        expect(failed.status()).toBe(409);
        await expect(
          editor.getByTestId("code-editor-dirty-indicator"),
        ).toBeVisible();
        expect(await branchHead(page, workspace.id, codeRepoId)).toBe(
          remoteBefore,
        );
        const changes = await authedApiViaRequest(
          page,
          `/api/v1/workspaces/${workspace.id}/changes`,
        );
        expect(changes.status).toBe(200);
        const localHead = (changes.body?.data ?? changes.body).members.find(
          (member: { repo_id: string }) => member.repo_id === codeRepoId,
        )?.head_oid;
        expect(localHead).toMatch(/^([0-9a-f]{40}|[0-9a-f]{64})$/);
        setHook(false);
        const successResponse = page.waitForResponse(
          (candidate) =>
            candidate.request().method() === "PUT" &&
            candidate
              .url()
              .includes(`/workspaces/${workspace.id}/files/${path}`),
        );
        await editor.getByTestId("code-editor-save-button").click();
        const succeeded = await successResponse;
        expect(succeeded.status()).toBe(200);
        const retryRequest = succeeded.request().postDataJSON();
        expect(retryRequest.expectedHash).toBe(initial.hash);
        expect(retryRequest.commitAndPush).toBe(true);
        const saved = (await succeeded.json())?.data;
        expect(await branchHead(page, workspace.id, codeRepoId)).toBe(
          localHead,
        );
        expect(
          await forkRead(
            page,
            codeRepoId,
            path!,
            "push-retry-durable",
            "durable",
          ),
        ).toBe(retryRequest.content);
        expect(saved.hash).toMatch(/^([0-9a-f]{40}|[0-9a-f]{64})$/);
      } finally {
        setHook(false);
      }
    },
  );

  test("Monaco saves exact Project Doc bytes and rejects a stale hash", async ({
    page,
  }) => {
    // Project Docs are per workspace: the sidebar lists only the current
    // workspace's `docs/<id>/` folder, so a fresh fork inherits none of its
    // parent's docs. Create the doc this test edits in the fork itself.
    const docName = `explicit-save-${Date.now()}.md`;
    const docPath = `docs/${workspace.id}/${docName}`;
    const initialContent =
      "# Explicit save\n\nSeeded by the explicit-save spec.\n";
    const created = await authedApi(
      page,
      `/api/v1/workspaces/${workspace.id}/docs`,
      "POST",
      {
        name: docName,
        docType: "design",
        filePath: docPath,
        content: initialContent,
      },
    );
    expect(created.status, JSON.stringify(created.body)).toBe(201);
    const workspaceDetailResponse = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${workspace.id}`,
    );
    expect(workspaceDetailResponse.status).toBe(200);
    const docsRepoId = String(
      (workspaceDetailResponse.body?.data ?? workspaceDetailResponse.body)
        ?.docsRepoId ?? "",
    );
    expect(docsRepoId).toBeTruthy();
    const docUrl = `/api/v1/workspaces/${workspace.id}/docs/by-path?path=${encodeURIComponent(docPath)}`;
    const initial = await authedApiViaRequest(page, docUrl);
    expect(initial.status, JSON.stringify(initial.body)).toBe(200);
    const initialDoc = initial.body?.data ?? initial.body;
    expect(initialDoc.content).toBe(initialContent);
    expect(initialDoc.contentHash).toMatch(/^([0-9a-f]{40}|[0-9a-f]{64})$/);
    await openWorkspacePath(page, workspace.path);

    const editor = await openProjectDocEditor(page);
    const marker = ` explicit document save ${Date.now()}`;
    await appendInEditor(page, await openFileSource(page), marker);

    const save = editor.getByTestId("code-editor-save-button");
    await expect(save).toBeEnabled();
    await expect(editor.getByText("Unsaved · browser only")).toBeVisible();
    expect((await authedApiViaRequest(page, docUrl)).body?.data?.content).toBe(
      initialContent,
    );
    expect(await forkRead(page, docsRepoId, docPath, "f4-doc-unsaved")).toBe(
      initialContent,
    );
    const saveRequest = page.waitForRequest(
      (request) =>
        request.method() === "PUT" &&
        request.url().includes(`/workspaces/${workspace.id}/files/`),
    );
    const saveResponse = page.waitForResponse(
      (candidate) =>
        candidate.request().method() === "PUT" &&
        candidate.url().includes(`/workspaces/${workspace.id}/files/`),
    );
    await save.click();
    const request = await saveRequest;
    const requestBody = request.postDataJSON();
    expect(requestBody.expectedHash).toBe(initialDoc.contentHash);
    expect(requestBody.commitAndPush).toBe(true);
    expect(requestBody.content).toContain(marker.trim());
    const response = await saveResponse;
    expect(response.status()).toBe(200);
    await expect(save).toHaveText(/Saved|Save/, { timeout: 15_000 });
    const saved = await response.json();
    const savedHash = String(saved?.data?.hash ?? "");
    expect(savedHash).toMatch(/^([0-9a-f]{40}|[0-9a-f]{64})$/);
    expect((await authedApiViaRequest(page, docUrl)).body?.data?.content).toBe(
      requestBody.content,
    );
    expect(await forkRead(page, docsRepoId, docPath, "f4-doc-saved")).toBe(
      requestBody.content,
    );
    expect(
      await forkRead(page, docsRepoId, docPath, "f4-doc-pushed", "durable"),
    ).toBe(requestBody.content);

    const subscription = watchWorkspaceSubscription(page, workspace.id);
    const refreshEvents: string[] = [];
    const docReads: Array<{ status: number; body: unknown }> = [];
    page.on("websocket", (socket) => {
      socket.on("framereceived", ({ payload }) => {
        const frame = payload.toString();
        if (frame.includes(workspace.id) && frame.includes("file_changed")) {
          refreshEvents.push(frame);
        }
      });
    });
    page.on("response", async (response) => {
      if (
        response.request().method() === "GET" &&
        response.url().includes(`/files/${docPath}`)
      ) {
        docReads.push({
          status: response.status(),
          body: await response.json().catch(() => null),
        });
      }
    });
    await page.reload({ waitUntil: "domcontentloaded" });
    const reloadedEditor = await openProjectDocEditor(page);
    await expect(reloadedEditor).toContainText(marker.trim());
    expect((await authedApiViaRequest(page, docUrl)).body?.data?.content).toBe(
      requestBody.content,
    );

    // Rendering the document does not prove the event socket is subscribed.
    // Gate the external write so its live-refresh event has a recipient.
    await expect
      .poll(() => subscription.acknowledged(), {
        timeout: 15_000,
        message: `server never acknowledged the workspace:${workspace.id} subscription`,
      })
      .toBe(true);

    const outOfBand = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${workspace.id}/files/${docPath}?repoId=${docsRepoId}`,
      "PUT",
      {
        content: "# Updated outside the editor\n",
        encoding: "utf-8",
        expectedHash: savedHash,
      },
    );
    expect([200, 201], JSON.stringify(outOfBand.body)).toContain(
      outOfBand.status,
    );
    const saveUrl = new URL(request.url());
    const conflict = await authedApiViaRequest(
      page,
      `${saveUrl.pathname}${saveUrl.search}`,
      "PUT",
      {
        content: `${requestBody.content}\nstale document edit\n`,
        expectedHash: savedHash,
        expectedVmId: initialDoc.vmId,
        commitAndPush: true,
      },
    );
    expect(conflict.status).toBe(409);
    const conflictBody = conflict.body;
    expect(conflictBody?.error?.code).toBe("FILE_VERSION_CONFLICT");
    try {
      await expect(reloadedEditor).toContainText("Updated outside the editor");
    } catch (error) {
      // The CI GitHub reporter prints failure messages, but retains console
      // output only in the downloadable report. Keep the evidence inline.
      const diagnostics = JSON.stringify({
        subscribed: subscription.acknowledged(),
        refreshEvents,
        docReads,
        saveEnabled: await reloadedEditor
          .getByTestId("code-editor-save-button")
          .isEnabled()
          .catch(() => null),
        current: await authedApiViaRequest(page, docUrl).catch(
          (diagnosticError) => ({ error: String(diagnosticError) }),
        ),
      });
      throw new Error(
        `${String(error)}\nProject Doc refresh diagnostics: ${diagnostics}`,
        { cause: error },
      );
    }
  });
  test("JSONL Project Doc source Save checkpoints its exact bytes and survives a durable fork", async ({
    page,
  }) => {
    const docName = `explicit-jsonl-${Date.now()}.jsonl`;
    const docPath = `docs/${workspace.id}/${docName}`;
    const initialContent = '{"type":"message","text":"before"}\n';
    const created = await authedApi(
      page,
      `/api/v1/workspaces/${workspace.id}/docs`,
      "POST",
      {
        name: docName,
        docType: "design",
        filePath: docPath,
        content: initialContent,
      },
    );
    expect(created.status, JSON.stringify(created.body)).toBe(201);
    const detail = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${workspace.id}`,
    );
    expect(detail.status).toBe(200);
    const docsRepoId = String((detail.body?.data ?? detail.body).docsRepoId);
    const docUrl = `/api/v1/workspaces/${workspace.id}/docs/by-path?path=${encodeURIComponent(docPath)}`;
    const initial = await authedApiViaRequest(page, docUrl);
    const initialDoc = initial.body?.data ?? initial.body;
    expect(initial.status).toBe(200);
    const oldHead = await branchHead(page, workspace.id, docsRepoId);

    await openWorkspacePath(page, workspace.path);
    await expandProjectDocs(page);
    await page
      .getByTestId("sidebar-docs-tab")
      .getByTestId("doc-item")
      .filter({ hasText: docName })
      .click();
    const transcript = page
      .getByTestId("center-panel")
      .getByTestId("jsonl-transcript-view");
    await expect(transcript).toBeVisible();
    const editor = await openFileSource(page);
    await expect(editor.getByTestId("code-editor-save-button")).toBeVisible();
    const added = `\n{"type":"message","text":"saved-${Date.now()}"}\n`;
    await appendInEditor(page, editor, added);
    expect((await authedApiViaRequest(page, docUrl)).body?.data?.content).toBe(
      initialContent,
    );
    const saveRequest = page.waitForRequest(
      (request) =>
        request.method() === "PUT" &&
        request.url().includes(`/workspaces/${workspace.id}/files/`),
    );
    const saveResponse = page.waitForResponse(
      (response) =>
        response.request().method() === "PUT" &&
        response.url().includes(`/workspaces/${workspace.id}/files/`),
    );
    await editor.getByTestId("code-editor-save-button").click();
    const request = await saveRequest;
    const body = request.postDataJSON();
    expect(body.expectedHash).toBe(initialDoc.contentHash);
    expect(body.expectedVmId).toBe(initialDoc.vmId);
    expect(body.commitAndPush).toBe(true);
    expect(body.content).toContain(added.trim());
    expect((await saveResponse).status()).toBe(200);
    const savedHead = await branchHead(page, workspace.id, docsRepoId);
    expect(savedHead).not.toBe(oldHead);
    expect((await authedApiViaRequest(page, docUrl)).body?.data?.content).toBe(
      body.content,
    );
    expect(
      await forkRead(page, docsRepoId, docPath, "jsonl-durable", "durable"),
    ).toBe(body.content);
    const saveUrl = new URL(request.url());
    const noCheckpointContent = `${body.content}{"type":"message","text":"no-checkpoint-${Date.now()}"}\n`;
    const noCheckpoint = await authedApiViaRequest(
      page,
      `${saveUrl.pathname}${saveUrl.search}`,
      "PUT",
      {
        content: noCheckpointContent,
        expectedHash: (await authedApiViaRequest(page, docUrl)).body?.data
          ?.contentHash,
        commitAndPush: false,
      },
    );
    expect(noCheckpoint.status, JSON.stringify(noCheckpoint.body)).toBe(200);
    expect((noCheckpoint.body?.data ?? noCheckpoint.body).vmId).toBe(
      initialDoc.vmId,
    );
    expect((await authedApiViaRequest(page, docUrl)).body?.data?.content).toBe(
      noCheckpointContent,
    );
    expect(await branchHead(page, workspace.id, docsRepoId)).toBe(savedHead);
    await openFilePreview(page);
    const rows = transcript.getByTestId(
      "jsonl-transcript-collapsible-row-toggle",
    );
    // The out-of-band API write reaches this open view asynchronously. Wait
    // for its row before expanding the last entry, or a click can expand the
    // prior record just before the new one appears.
    await expect(rows).toHaveCount(3);
    await rows.last().click();
    await expect(transcript).toContainText(
      noCheckpointContent.match(/no-checkpoint-\d+/)![0],
    );
    await openFileSource(page);
    await expect(editor).toContainText(added.trim());
  });

  test("interactive HTML answer Apply saves without an explicit-Save checkpoint", async ({
    page,
  }) => {
    const docName = `explicit-html-answer-${randomUUID()}.html`;
    const docPath = `docs/${workspace.id}/${docName}`;
    const initialContent = [
      "<!doctype html><html><body>",
      '<script type="application/json" id="g5-doc-answers">{"choice":"before"}</script>',
      '<button id="propose-answer" type="button">Propose answer</button>',
      "<script>",
      "let generation = null;",
      "window.addEventListener('message', (event) => {",
      "  if (event.data?.source === 'g5-html-doc-host' && event.data?.type === 'bridge-ready') {",
      "    generation = event.data.generation;",
      "    document.getElementById('propose-answer').dataset.ready = String(event.data.canSave);",
      "  }",
      "});",
      "document.getElementById('propose-answer').addEventListener('click', () => {",
      "  window.parent.postMessage({ source: 'g5-html-doc', version: 1, type: 'propose-answers',",
      "    generation, requestId: 'answer-1', answersJson: '{\"choice\":\"after\"}' }, '*');",
      "});",
      "</script></body></html>",
    ].join("\n");
    const created = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${workspace.id}/docs`,
      "POST",
      {
        name: docName,
        docType: "design",
        filePath: docPath,
        content: initialContent,
      },
    );
    expect(created.status, JSON.stringify(created.body)).toBe(201);
    const detail = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${workspace.id}`,
    );
    expect(detail.status).toBe(200);
    const docsRepoId = String((detail.body?.data ?? detail.body).docsRepoId);
    const oldHead = await branchHead(page, workspace.id, docsRepoId);

    await openWorkspacePath(page, workspace.path);
    await expandProjectDocs(page);
    await page
      .getByTestId("sidebar-docs-tab")
      .getByTestId("doc-item")
      .filter({ hasText: docName })
      .click();
    const frame = page.frameLocator('[data-testid="center-panel"] iframe');
    const propose = frame.locator("#propose-answer");
    await expect(propose).toHaveAttribute("data-ready", "true");
    await propose.click();
    await expect(page.getByTestId("html-doc-answer-apply")).toBeVisible();
    const saveRequest = page.waitForRequest(
      (request) =>
        request.method() === "PUT" &&
        request.url().includes(`/workspaces/${workspace.id}/files/`),
    );
    const saveResponse = page.waitForResponse(
      (response) =>
        response.request().method() === "PUT" &&
        response.url().includes(`/workspaces/${workspace.id}/files/`),
    );
    await page.getByTestId("html-doc-answer-apply").click();
    const body = (await saveRequest).postDataJSON();
    expect(body.commitAndPush).toBe(false);
    expect(body.content).toContain('{"choice":"after"}');
    expect((await saveResponse).status()).toBe(200);
    // Saving refreshes srcDoc and the bridge clears its bar on frame load.
    // Verify the refreshed preview and persisted bytes, not that transient bar.
    await expect(frame.locator("#g5-doc-answers")).toHaveJSProperty(
      "textContent",
      '{"choice":"after"}',
    );
    await expect(propose).toHaveAttribute("data-ready", "true");
    expect(await branchHead(page, workspace.id, docsRepoId)).toBe(oldHead);
    const read = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${workspace.id}/docs/by-path?path=${encodeURIComponent(docPath)}`,
    );
    expect(read.status).toBe(200);
    expect(read.body?.data?.content).toContain('{"choice":"after"}');
  });
});
