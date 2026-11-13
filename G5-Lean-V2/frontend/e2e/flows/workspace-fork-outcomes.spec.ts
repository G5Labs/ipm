import { randomUUID } from "node:crypto";
import { expect, test, type Page } from "@playwright/test";
import {
  activateSidebarSection,
  authedApiViaRequest,
  captureChatTurn,
  cloneWorkspaceById,
  createWorkspaceClone,
  deleteWorkspaceById,
  expectSelectedAgentProvider,
  openRailTerminal,
  openWorkspacePath,
  readWorkspaceVm,
  releaseChatTurn,
  runWorkspaceVmAction,
  selectAgentProviderViaSettings,
  type ChatTurnIdentity,
  type WorkspaceClone,
  waitForLiveWorkspaceChanges,
  waitForWorkspaceVmStatus,
} from "../helpers";

type Branch = {
  repoId: string;
  repoName: string;
  branchName: string;
  headCommit: string;
};

type WorkspaceDetail = {
  id: string;
  ownerUserId: string;
  clonedFromId: string | null;
  docsRepoId: string;
  branches: Branch[];
};

async function detail(
  page: Page,
  workspaceId: string,
): Promise<WorkspaceDetail> {
  const response = await authedApiViaRequest(
    page,
    `/api/v1/workspaces/${workspaceId}`,
  );
  expect(response.status, JSON.stringify(response.body)).toBe(200);
  return (response.body?.data ?? response.body) as WorkspaceDetail;
}

async function file(
  page: Page,
  workspaceId: string,
  repoId: string,
  path: string,
) {
  const response = await authedApiViaRequest(
    page,
    `/api/v1/workspaces/${workspaceId}/files/${path}?repoId=${repoId}`,
  );
  expect(response.status, JSON.stringify(response.body)).toBe(200);
  return response.body?.data ?? response.body;
}

async function saveFile(
  page: Page,
  workspaceId: string,
  repoId: string,
  path: string,
  content: string,
) {
  const response = await authedApiViaRequest(
    page,
    `/api/v1/workspaces/${workspaceId}/files/${path}?repoId=${repoId}`,
    "PUT",
    { content, encoding: "utf-8", expectedHash: null },
  );
  expect([200, 201], JSON.stringify(response.body)).toContain(response.status);
}

function quoted(value: string): string {
  return `'${value.replaceAll("'", "'\\''")}'`;
}

async function terminalCommand(page: Page, command: string) {
  await openRailTerminal(page);
  const terminal = page.getByTestId("workspace-terminal");
  await expect(terminal).toBeVisible({ timeout: 30_000 });
  await expect(page.getByText("Workspace terminal · Connected")).toBeVisible({
    timeout: 30_000,
  });
  const input = terminal.locator(".xterm-helper-textarea");
  await input.focus();
  await input.pressSequentially("stty -echo");
  await input.press("Enter");
  const suffix = randomUUID().replaceAll("-", "");
  const marker = `G5_FORK_DONE_${suffix}`;
  await input.pressSequentially(
    `${command} && printf '%s_%s\\n' G5_FORK_DONE ${suffix}`,
  );
  await input.press("Enter");
  await expect(terminal.locator(".xterm-accessibility-tree")).toContainText(
    marker,
    { timeout: 30_000 },
  );
}

function memberVector(workspace: WorkspaceDetail) {
  // The fixture's Base has one code repo and the org docs repo; seed a
  // different file in each so the assertions cover its complete member set.
  expect(workspace.branches.length).toBe(2);
  expect(workspace.docsRepoId).toBeTruthy();
  expect(new Set(workspace.branches.map((branch) => branch.repoId)).size).toBe(
    workspace.branches.length,
  );
  const docs = workspace.branches.find(
    (branch) => branch.repoId === workspace.docsRepoId,
  );
  const code = workspace.branches.find(
    (branch) => branch.repoId !== workspace.docsRepoId,
  );
  expect(docs, JSON.stringify(workspace.branches)).toBeTruthy();
  expect(code, JSON.stringify(workspace.branches)).toBeTruthy();
  return { docs: docs!, code: code! };
}

async function localCutOids(page: Page, workspaceId: string) {
  // Capture the owner's local committed cut while running. Stop receipts
  // preserve it for fork/restore, but are not a stopped browsing surface.
  const changes = await waitForLiveWorkspaceChanges(page, workspaceId);
  const heads = new Map<string, string>(
    changes.members.map((member) => [member.repo_id, member.head_oid!]),
  );
  expect(heads.size).toBe(2);
  for (const head of heads.values()) {
    expect(head).toMatch(/^([0-9a-f]{40}|[0-9a-f]{64})$/);
  }
  return heads;
}

function assertPinnedVector(
  source: WorkspaceDetail,
  child: WorkspaceDetail,
  expectedCut: Map<string, string>,
) {
  expect(child.clonedFromId).toBe(source.id);
  expect(child.ownerUserId).toBe(source.ownerUserId);
  expect(child.branches.length).toBe(source.branches.length);
  for (const parentMember of source.branches) {
    const childMember = child.branches.find(
      (candidate) => candidate.repoId === parentMember.repoId,
    );
    expect(
      childMember,
      `missing child member ${parentMember.repoId}`,
    ).toBeTruthy();
    expect(childMember!.headCommit, `repo ${parentMember.repoId}`).toBe(
      expectedCut.get(parentMember.repoId),
    );
    expect(childMember!.branchName).toBe(child.branches[0].branchName);
    expect(childMember!.headCommit).toMatch(/^([0-9a-f]{40}|[0-9a-f]{64})$/);
  }
}

test.describe("Fork product outcomes", () => {
  test.describe.configure({ timeout: 360_000 });
  let source: WorkspaceClone;
  let child: WorkspaceClone | undefined;
  let turn: ChatTurnIdentity | undefined;

  test.beforeEach(async ({ page }, testInfo) => {
    source = await createWorkspaceClone(page, {
      namePrefix: `fork-outcome-${testInfo.title}`,
      start: true,
    });
    await openWorkspacePath(page, source.path);
  });

  test.afterEach(async ({ page }) => {
    if (turn) await releaseChatTurn(page, turn).catch(() => {});
    if (child) await deleteWorkspaceById(page, child.id).catch(() => {});
    await deleteWorkspaceById(page, source?.id).catch(() => {});
    child = undefined;
    turn = undefined;
  });

  async function seedCommittedCodeAndDoc(page: Page) {
    const sourceDetail = await detail(page, source.id);
    const { code, docs } = memberVector(sourceDetail);
    const changes = await waitForLiveWorkspaceChanges(page, source.id);
    const codeRoot = changes.members.find(
      (member) => member.repo_id === code.repoId,
    );
    const docsRoot = changes.members.find(
      (member) => member.repo_id === docs.repoId,
    );
    expect(codeRoot?.root_path).toBeTruthy();
    expect(docsRoot?.root_path).toBeTruthy();
    const codePath = `e2e-fork-code-${randomUUID()}.txt`;
    const docsPath = `docs/${source.id}/e2e-fork-doc-${randomUUID()}.md`;
    const codeBytes = `code from ${source.id}\n`;
    const docsBytes = `# Project Doc from ${source.id}\n`;
    await saveFile(page, source.id, code.repoId, codePath, codeBytes);
    const created = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${source.id}/docs`,
      "POST",
      {
        name: docsPath.split("/").at(-1),
        docType: "design",
        filePath: docsPath,
        content: docsBytes,
      },
    );
    expect(created.status, JSON.stringify(created.body)).toBe(201);
    await terminalCommand(
      page,
      `git -C ${quoted(codeRoot!.root_path)} add -- ${quoted(codePath)} && git -C ${quoted(codeRoot!.root_path)} -c user.email=e2e@g5.dev -c user.name=e2e commit -qm 'fork code cut' -- ${quoted(codePath)} && git -C ${quoted(docsRoot!.root_path)} add -- ${quoted(docsPath)} && git -C ${quoted(docsRoot!.root_path)} -c user.email=e2e@g5.dev -c user.name=e2e commit -qm 'fork docs cut' -- ${quoted(docsPath)}`,
    );
    return { code, docs, codePath, docsPath, codeBytes, docsBytes };
  }

  async function assertChildFiles(
    page: Page,
    seeded: Awaited<ReturnType<typeof seedCommittedCodeAndDoc>>,
    expectedSource: "live" | "committed",
    pinned?: WorkspaceDetail,
  ) {
    for (const { repoId, path, content } of [
      {
        repoId: seeded.code.repoId,
        path: seeded.codePath,
        content: seeded.codeBytes,
      },
      {
        repoId: seeded.docs.repoId,
        path: seeded.docsPath,
        content: seeded.docsBytes,
      },
    ]) {
      const actual = await file(page, child!.id, repoId, path);
      expect(actual.content, `${repoId}:${path}`).toBe(content);
      if (expectedSource === "committed") {
        expect(actual.source, `${repoId}:${path}`).toBe("committed");
      } else {
        expect(actual.source, `${repoId}:${path}`).toBe("live");
      }
      if (pinned && expectedSource === "committed") {
        expect(actual.repoOid, `${repoId}:${path}`).toBe(
          pinned.branches.find((branch) => branch.repoId === repoId)
            ?.headCommit,
        );
      }
    }
  }

  test("F1/F10 stopped source forks exact bytes while inherited Project Docs stay in All Docs", async ({
    page,
  }) => {
    const seeded = await seedCommittedCodeAndDoc(page);
    const sourceCutOids = await localCutOids(page, source.id);
    await runWorkspaceVmAction(page, source.id, "stop");
    const stopped = await waitForWorkspaceVmStatus(page, source.id, "stopped");
    expect(stopped.lastFlushId).toMatch(/^[0-9a-f-]{36}$/);
    const sourceCut = await detail(page, source.id);
    child = await cloneWorkspaceById(page, source.id, {
      namePrefix: "f1-stopped-source",
      source: "durable",
      start: false,
    });
    const childBeforeStart = await readWorkspaceVm(page, child.id);
    expect(childBeforeStart.status).toBe("stopped");
    const childCut = await detail(page, child.id);
    assertPinnedVector(sourceCut, childCut, sourceCutOids);
    await assertChildFiles(page, seeded, "committed", childCut);
    expect((await readWorkspaceVm(page, child.id)).status).toBe("stopped");

    // The fork pins the inherited docs Git path. Project Docs is scoped to
    // docs/<child-id>; the parent's path remains browsable in All Docs as a
    // read-only document, without starting the child VM.
    const docsList = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${child.id}/docs?surface=workspace`,
    );
    expect(docsList.status, JSON.stringify(docsList.body)).toBe(200);
    const docsData = docsList.body?.data ?? docsList.body;
    expect(docsData.repoOid).toBe(
      childCut.branches.find((branch) => branch.repoId === seeded.docs.repoId)
        ?.headCommit,
    );
    expect(docsData.docs).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          filePath: seeded.docsPath,
          source: "committed",
        }),
      ]),
    );

    await openWorkspacePath(page, child.path);
    await expect(page.getByTestId("workspace-vm-start")).toHaveAttribute(
      "title",
      /^VM: Stopped/,
    );
    await activateSidebarSection(page, "sidebar-docs-tab");
    const projectDocs = page.getByTestId("sidebar-docs-tab");
    await expect(projectDocs).toContainText("No project docs yet");
    await expect(projectDocs).not.toContainText(
      seeded.docsPath.split("/").at(-1)!,
    );
    await activateSidebarSection(page, "sidebar-all-docs-tab");
    const allDocs = page.getByTestId("sidebar-all-docs-tab");
    const inheritedFolder = allDocs
      .getByTestId("doc-folder")
      .filter({ hasText: source.name })
      .or(allDocs.getByTestId("doc-folder").filter({ hasText: source.id }))
      .first();
    await expect(inheritedFolder).toBeVisible();
    await inheritedFolder.click();
    await allDocs
      .getByText(seeded.docsPath.split("/").at(-1)!, { exact: true })
      .click();
    await expect(page.getByTestId("file-view-edit")).toHaveCount(0);
    await expect(page.getByTestId("file-view-start-to-edit")).toBeVisible();
    expect((await readWorkspaceVm(page, child.id)).status).toBe("stopped");
    await page.getByTestId("workspace-vm-start").click();
    const started = await waitForWorkspaceVmStatus(page, child.id, "running");
    expect(started.vmId).toBeTruthy();
    await assertChildFiles(page, seeded, "live");
  });

  test("F2 idle running source keeps its VM while durable child restores the pinned vector", async ({
    page,
  }) => {
    const seeded = await seedCommittedCodeAndDoc(page);
    const sourceCutOids = await localCutOids(page, source.id);
    // Retain the committed member heads through a normal lifecycle receipt,
    // then measure the fork while the source is attached and idle.
    await runWorkspaceVmAction(page, source.id, "stop");
    const stopped = await waitForWorkspaceVmStatus(page, source.id, "stopped");
    expect(stopped.lastFlushId).toMatch(/^[0-9a-f-]{36}$/);
    await runWorkspaceVmAction(page, source.id, "resume");
    await waitForWorkspaceVmStatus(page, source.id, "running");
    const parentBefore = await readWorkspaceVm(page, source.id);
    const sourceCut = await detail(page, source.id);
    child = await cloneWorkspaceById(page, source.id, {
      namePrefix: "f2-running-source",
      source: "durable",
      start: false,
    });
    const childCut = await detail(page, child.id);
    assertPinnedVector(sourceCut, childCut, sourceCutOids);
    expect((await readWorkspaceVm(page, child.id)).status).toBe("stopped");
    await assertChildFiles(page, seeded, "committed", childCut);
    const parentAfter = await readWorkspaceVm(page, source.id);
    expect(parentAfter.status).toBe("running");
    expect(parentAfter.vmId).toBe(parentBefore.vmId);
    expect(parentAfter.epoch).toBe(parentBefore.epoch);

    await runWorkspaceVmAction(page, child.id, "resume");
    await waitForWorkspaceVmStatus(page, child.id, "running");
    await assertChildFiles(page, seeded, "live");
  });

  test("F3 live fork carries dirty code, Project Doc and unpushed commit while parent stays usable", async ({
    page,
  }) => {
    const sourceDetail = await detail(page, source.id);
    const { code, docs } = memberVector(sourceDetail);
    const changes = await waitForLiveWorkspaceChanges(page, source.id);
    const codeRoot = changes.members.find(
      (member) => member.repo_id === code.repoId,
    );
    expect(codeRoot?.root_path).toBeTruthy();
    const committedPath = `e2e-live-commit-${randomUUID()}.txt`;
    const codePath = `e2e-live-dirty-${randomUUID()}.txt`;
    const docsPath = `docs/${source.id}/e2e-live-doc-${randomUUID()}.md`;
    const committedBytes = `unpushed ${source.id}\n`;
    const codeBytes = `dirty code ${source.id}\n`;
    const docsBytes = `# Dirty Project Doc ${source.id}\n`;
    await saveFile(page, source.id, code.repoId, committedPath, committedBytes);
    await terminalCommand(
      page,
      `cd -- ${quoted(codeRoot!.root_path)} && git add -- ${quoted(committedPath)} && git -c user.email=e2e@g5.dev -c user.name=e2e commit -qm 'fork unpushed commit' -- ${quoted(committedPath)}`,
    );
    await saveFile(page, source.id, code.repoId, codePath, codeBytes);
    const created = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${source.id}/docs`,
      "POST",
      {
        name: docsPath.split("/").at(-1),
        docType: "design",
        filePath: docsPath,
        content: docsBytes,
      },
    );
    expect(created.status, JSON.stringify(created.body)).toBe(201);
    const dirty = await waitForLiveWorkspaceChanges(page, source.id);
    expect(
      dirty.members
        .find((member) => member.repo_id === code.repoId)
        ?.dirty?.some((entry) => entry.path === codePath),
    ).toBe(true);
    expect(
      dirty.members
        .find((member) => member.repo_id === docs.repoId)
        ?.dirty?.some((entry) => entry.path === docsPath),
    ).toBe(true);
    expect(
      dirty.members.find((member) => member.repo_id === code.repoId)
        ?.against_internal?.ahead,
    ).toBeGreaterThanOrEqual(1);
    const parentBefore = await readWorkspaceVm(page, source.id);

    child = await cloneWorkspaceById(page, source.id, {
      namePrefix: "f3-live-source",
      source: "live",
      interrupt: false,
      start: false,
    });
    const childBeforeStart = await readWorkspaceVm(page, child.id);
    expect(childBeforeStart.status).toBe("stopped");
    expect(childBeforeStart.lastFlushId).toMatch(/^[0-9a-f-]{36}$/);
    const childCut = await detail(page, child.id);
    expect(childCut.clonedFromId).toBe(source.id);
    expect(childCut.ownerUserId).toBe(sourceDetail.ownerUserId);
    expect(childCut.branches.map((branch) => branch.repoId).sort()).toEqual(
      sourceDetail.branches.map((branch) => branch.repoId).sort(),
    );
    expect(
      new Set(childCut.branches.map((branch) => branch.branchName)).size,
    ).toBe(1);
    await runWorkspaceVmAction(page, child.id, "resume");
    await waitForWorkspaceVmStatus(page, child.id, "running");
    for (const { repoId, path, content } of [
      { repoId: code.repoId, path: committedPath, content: committedBytes },
      { repoId: code.repoId, path: codePath, content: codeBytes },
      { repoId: docs.repoId, path: docsPath, content: docsBytes },
    ]) {
      const actual = await file(page, child.id, repoId, path);
      expect(actual.source, `${repoId}:${path}`).toBe("live");
      expect(actual.content, `${repoId}:${path}`).toBe(content);
    }
    const childChanges = await waitForLiveWorkspaceChanges(page, child.id);
    expect(
      childChanges.members.find((member) => member.repo_id === code.repoId)
        ?.head_oid,
      "the child's restored Git head must include the source's unpushed commit",
    ).toBe(
      dirty.members.find((member) => member.repo_id === code.repoId)?.head_oid,
    );
    expect(
      childChanges.members.find((member) => member.repo_id === code.repoId)
        ?.head_oid,
    ).toMatch(/^([0-9a-f]{40}|[0-9a-f]{64})$/);
    const parentAfter = await readWorkspaceVm(page, source.id);
    expect(parentAfter.status).toBe("running");
    expect(parentAfter.vmId).toBe(parentBefore.vmId);
    expect(parentAfter.epoch).toBe(parentBefore.epoch);
    expect((await file(page, source.id, code.repoId, codePath)).content).toBe(
      codeBytes,
    );
    const parentOnlyPath = `e2e-parent-only-${randomUUID()}.txt`;
    await saveFile(
      page,
      source.id,
      code.repoId,
      parentOnlyPath,
      "parent remains writable\n",
    );
    expect(
      (await file(page, source.id, code.repoId, parentOnlyPath)).content,
    ).toBe("parent remains writable\n");
    const absentFromChild = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${child.id}/files/${parentOnlyPath}?repoId=${code.repoId}`,
    );
    expect(absentFromChild.status, JSON.stringify(absentFromChild.body)).toBe(
      404,
    );
  });

  test("F7 admitted delete, rename and writes become one complete live child cut", async ({
    page,
  }) => {
    const seeded = await seedCommittedCodeAndDoc(page);
    const sourceBefore = await readWorkspaceVm(page, source.id);
    const codeHash = (
      await file(page, source.id, seeded.code.repoId, seeded.codePath)
    ).hash;
    const docsHash = (
      await file(page, source.id, seeded.docs.repoId, seeded.docsPath)
    ).hash;
    expect(codeHash).toBeTruthy();
    expect(docsHash).toBeTruthy();
    const deleted = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${source.id}/files/${seeded.codePath}?repoId=${seeded.code.repoId}`,
      "DELETE",
      { expectedHash: codeHash },
    );
    expect(deleted.status, JSON.stringify(deleted.body)).toBe(200);
    const renamedDocsPath = `docs/${source.id}/e2e-renamed-${randomUUID()}.md`;
    const renamed = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${source.id}/files/rename`,
      "POST",
      {
        repoId: seeded.docs.repoId,
        from: seeded.docsPath,
        to: renamedDocsPath,
        expectedHash: docsHash,
      },
    );
    expect(renamed.status, JSON.stringify(renamed.body)).toBe(200);
    const newCodePath = `e2e-f7-new-code-${randomUUID()}.txt`;
    const newDocsPath = `docs/${source.id}/e2e-f7-new-doc-${randomUUID()}.md`;
    const newCodeBytes = `complete code after admitted mutations ${source.id}\n`;
    const newDocsBytes = `# Complete docs after admitted mutations ${source.id}\n`;
    await saveFile(
      page,
      source.id,
      seeded.code.repoId,
      newCodePath,
      newCodeBytes,
    );
    await saveFile(
      page,
      source.id,
      seeded.docs.repoId,
      newDocsPath,
      newDocsBytes,
    );

    child = await cloneWorkspaceById(page, source.id, {
      namePrefix: "f7-complete-cut",
      source: "live",
      interrupt: false,
      start: false,
    });
    const childVm = await readWorkspaceVm(page, child.id);
    expect(childVm.status).toBe("stopped");
    expect(childVm.lastFlushId).toMatch(/^[0-9a-f-]{36}$/);
    // Unpublished changes remain in the retained live-fork snapshot. Only
    // its owner after Start may inspect them.
    for (const [repoId, path] of [
      [seeded.docs.repoId, renamedDocsPath],
      [seeded.code.repoId, newCodePath],
      [seeded.docs.repoId, newDocsPath],
    ]) {
      const hidden = await authedApiViaRequest(
        page,
        `/api/v1/workspaces/${child.id}/files/${path}?repoId=${repoId}`,
      );
      expect(hidden.status, JSON.stringify(hidden.body)).toBe(404);
    }
    await runWorkspaceVmAction(page, child.id, "resume");
    await waitForWorkspaceVmStatus(page, child.id, "running");
    for (const { repoId, path, content } of [
      {
        repoId: seeded.docs.repoId,
        path: renamedDocsPath,
        content: seeded.docsBytes,
      },
      { repoId: seeded.code.repoId, path: newCodePath, content: newCodeBytes },
      { repoId: seeded.docs.repoId, path: newDocsPath, content: newDocsBytes },
    ]) {
      const actual = await file(page, child.id, repoId, path);
      expect(actual.content, `${repoId}:${path}`).toBe(content);
      expect(actual.source, `${repoId}:${path}`).toBe("live");
    }
    for (const { repoId, path } of [
      { repoId: seeded.code.repoId, path: seeded.codePath },
      { repoId: seeded.docs.repoId, path: seeded.docsPath },
    ]) {
      const missing = await authedApiViaRequest(
        page,
        `/api/v1/workspaces/${child.id}/files/${path}?repoId=${repoId}`,
      );
      expect(
        missing.status,
        `${repoId}:${path} ${JSON.stringify(missing.body)}`,
      ).toBe(404);
    }
    const sourceAfter = await readWorkspaceVm(page, source.id);
    expect(sourceAfter.vmId).toBe(sourceBefore.vmId);
    expect(sourceAfter.status).toBe("running");
  });

  test("F6 busy live fork refuses without interrupt, then the same authorized operation completes and reopens admission", async ({
    page,
  }) => {
    const sourceDetail = await detail(page, source.id);
    const { code, docs } = memberVector(sourceDetail);
    const live = await waitForLiveWorkspaceChanges(page, source.id);
    const codeRoot = live.members.find(
      (member) => member.repo_id === code.repoId,
    );
    const docsRoot = live.members.find(
      (member) => member.repo_id === docs.repoId,
    );
    expect(codeRoot?.root_path).toBeTruthy();
    expect(docsRoot?.root_path).toBeTruthy();
    const token = randomUUID();
    const gatePath = `e2e-f6-release-${token}.txt`;
    const codePath = `e2e-f6-agent-${token}.txt`;
    const docsPath = `docs/${source.id}/e2e-f6-agent-${token}.md`;
    const codeBytes = `busy owner code ${token}\n`;
    const docsBytes = `# Busy owner Project Doc ${token}\n`;
    const writes = [
      {
        path: `${codeRoot!.root_path}/${codePath}`,
        contentBase64: Buffer.from(codeBytes).toString("base64"),
      },
      {
        path: `${docsRoot!.root_path}/${docsPath}`,
        contentBase64: Buffer.from(docsBytes).toString("base64"),
      },
    ];
    const writeSet = Buffer.from(JSON.stringify(writes)).toString("base64");
    await selectAgentProviderViaSettings(page, "claude_code");
    await expectSelectedAgentProvider(page, "claude_code");
    const input = page.getByTestId("chat-input").first();
    await input.fill(
      `Write both workspace members and wait for release. G5_FS_WRITE_SET_E2E:${writeSet} G5_WAIT_FOR_FILE_E2E:${codeRoot!.root_path}/${gatePath}`,
    );
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
    expect(turn.workspaceId).toBe(source.id);
    for (const { repoId, path, bytes } of [
      { repoId: code.repoId, path: codePath, bytes: codeBytes },
      { repoId: docs.repoId, path: docsPath, bytes: docsBytes },
    ]) {
      await expect
        .poll(
          async () => {
            const response = await authedApiViaRequest(
              page,
              `/api/v1/workspaces/${source.id}/files/${path}?repoId=${repoId}`,
            );
            expect([200, 404], JSON.stringify(response.body)).toContain(
              response.status,
            );
            return response.status === 200
              ? (response.body?.data ?? response.body).content
              : null;
          },
          { timeout: 90_000 },
        )
        .toBe(bytes);
    }
    await expect
      .poll(
        async () => {
          const response = await authedApiViaRequest(
            page,
            `/api/v1/tasks/${turn!.taskId}`,
          );
          expect(response.status, JSON.stringify(response.body)).toBe(200);
          return (response.body?.data ?? response.body).status;
        },
        { timeout: 30_000 },
      )
      .toMatch(/^(running|waiting)$/);

    const parentBefore = await readWorkspaceVm(page, source.id);
    const name = `f6-busy-${token}`;
    const idempotencyKey = randomUUID();
    const payload = (interrupt: boolean) => ({
      name,
      idempotencyKey,
      source: "live",
      interrupt,
    });
    const refused = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${source.id}/clone`,
      "POST",
      payload(false),
    );
    expect(refused.status, JSON.stringify(refused.body)).toBe(409);
    expect(JSON.stringify(refused.body)).toContain("WORKSPACE_BUSY");
    const parentAfterRefusal = await readWorkspaceVm(page, source.id);
    expect(parentAfterRefusal.status).toBe("running");
    expect(parentAfterRefusal.vmId).toBe(parentBefore.vmId);
    expect(parentAfterRefusal.epoch).toBe(parentBefore.epoch);
    expect(parentAfterRefusal.lastFlushId).toBe(parentBefore.lastFlushId);
    const refusalPath = `e2e-f6-refusal-${token}.txt`;
    const refusalBytes = `admission remained open ${token}\n`;
    await saveFile(page, source.id, code.repoId, refusalPath, refusalBytes);
    expect(
      (await file(page, source.id, code.repoId, refusalPath)).content,
    ).toBe(refusalBytes);

    // The provider remains inside its wait gate through the interrupting fork.
    // Its write-set cannot turn into a completed reply before the busy check.
    const sourceVector = await waitForLiveWorkspaceChanges(page, source.id);
    const sourceHeads = new Map(
      sourceVector.members.map((member) => [member.repo_id, member.head_oid]),
    );
    expect(sourceHeads.size).toBe(2);
    for (const head of sourceHeads.values()) {
      expect(head).toMatch(/^([0-9a-f]{40}|[0-9a-f]{64})$/);
    }
    const completed = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${source.id}/clone`,
      "POST",
      payload(true),
    );
    expect(completed.status, JSON.stringify(completed.body)).toBe(201);
    const result = completed.body?.data ?? completed.body;
    const childId = String(result.workspace?.id ?? "");
    expect(childId).toMatch(/^[0-9a-f-]{36}$/);
    child = { id: childId, name, path: "" };
    expect(result.workspace.clonedFromId).toBe(source.id);
    expect(result.workspace.ownerUserId).toBe(sourceDetail.ownerUserId);
    const replay = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${source.id}/clone`,
      "POST",
      payload(true),
    );
    expect([200, 201], JSON.stringify(replay.body)).toContain(replay.status);
    expect((replay.body?.data ?? replay.body).workspace?.id).toBe(childId);
    const childStopped = await readWorkspaceVm(page, childId);
    expect(childStopped.status).toBe("stopped");
    expect(childStopped.lastFlushId).toMatch(/^[0-9a-f-]{36}$/);
    const childCut = await detail(page, childId);
    for (const member of childCut.branches) {
      expect(member.headCommit, `forked repo ${member.repoId}`).toBe(
        sourceHeads.get(member.repoId),
      );
    }
    const parentAfterFork = await readWorkspaceVm(page, source.id);
    expect(parentAfterFork.status).toBe("running");
    expect(parentAfterFork.vmId).toBe(parentBefore.vmId);
    expect(parentAfterFork.epoch).toBe(parentBefore.epoch);
    const afterPath = `e2e-f6-after-${token}.txt`;
    const afterBytes = `admission reopened ${token}\n`;
    await saveFile(page, source.id, code.repoId, afterPath, afterBytes);
    expect((await file(page, source.id, code.repoId, afterPath)).content).toBe(
      afterBytes,
    );

    await runWorkspaceVmAction(page, childId, "resume");
    await waitForWorkspaceVmStatus(page, childId, "running");
    const childVector = await waitForLiveWorkspaceChanges(page, childId);
    for (const member of childVector.members) {
      expect(member.head_oid, `restored repo ${member.repo_id}`).toBe(
        sourceHeads.get(member.repo_id),
      );
    }
    for (const { repoId, path, bytes } of [
      { repoId: code.repoId, path: codePath, bytes: codeBytes },
      { repoId: code.repoId, path: refusalPath, bytes: refusalBytes },
      { repoId: docs.repoId, path: docsPath, bytes: docsBytes },
    ]) {
      expect((await file(page, childId, repoId, path)).content).toBe(bytes);
    }
    const absent = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${childId}/files/${afterPath}?repoId=${code.repoId}`,
    );
    expect(absent.status, JSON.stringify(absent.body)).toBe(404);
  });
});
