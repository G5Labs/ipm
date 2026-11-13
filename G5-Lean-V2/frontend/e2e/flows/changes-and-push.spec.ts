import { randomUUID } from "node:crypto";
import { expect, test, type Page } from "@playwright/test";
import {
  authedApiViaRequest,
  cloneWorkspaceById,
  createWorkspaceClone,
  deleteWorkspaceById,
  openWorkspacePath,
  readWorkspaceVm,
  runWorkspaceVmAction,
  type WorkspaceClone,
  waitForLiveWorkspaceChanges,
  waitForWorkspaceVmStatus,
} from "../helpers";

type Branch = {
  repoId?: string;
  repo_id?: string;
  headCommit?: string;
  head_commit?: string;
};

async function branches(page: Page, workspaceId: string): Promise<Branch[]> {
  const response = await authedApiViaRequest(
    page,
    `/api/v1/workspaces/${workspaceId}/branches`,
  );
  expect(response.status, JSON.stringify(response.body)).toBe(200);
  const body = response.body?.data ?? response.body;
  return Array.isArray(body) ? body : (body?.branches ?? []);
}

function headFor(rows: Branch[], repoId: string): string | undefined {
  const row = rows.find((candidate) =>
    [candidate.repoId, candidate.repo_id].includes(repoId),
  );
  return row?.headCommit ?? row?.head_commit;
}

test.describe("Changes and native push", () => {
  test.describe.configure({ timeout: 360_000 });
  let workspace: WorkspaceClone;
  let child: WorkspaceClone | undefined;

  test.beforeEach(async ({ page }, testInfo) => {
    workspace = await createWorkspaceClone(page, {
      namePrefix: `changes-push-${testInfo.title}`,
    });
    await runWorkspaceVmAction(page, workspace.id, "resume");
    await waitForWorkspaceVmStatus(page, workspace.id, "running");
  });

  test.afterEach(async ({ page }) => {
    if (child) await deleteWorkspaceById(page, child.id).catch(() => {});
    await deleteWorkspaceById(page, workspace?.id).catch(() => {});
    child = undefined;
  });

  test("Save stays uncommitted and Push sends only existing commits", async ({
    page,
  }) => {
    const live = await waitForLiveWorkspaceChanges(page, workspace.id);
    const workspaceDetailResponse = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${workspace.id}`,
    );
    expect(workspaceDetailResponse.status).toBe(200);
    const workspaceDetail =
      workspaceDetailResponse.body?.data ?? workspaceDetailResponse.body;
    const docsRepoId = String(workspaceDetail.docsRepoId ?? "");
    expect(docsRepoId).toBeTruthy();
    const member = live.members.find(
      (candidate) => candidate.repo_id !== docsRepoId,
    );
    const docsMember = live.members.find(
      (candidate) => candidate.repo_id === docsRepoId,
    );
    expect(member).toBeTruthy();
    expect(docsMember).toBeTruthy();
    const headBeforeSave = headFor(
      await branches(page, workspace.id),
      member!.repo_id,
    );
    const path = `e2e-uncommitted-${Date.now()}.txt`;

    const save = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${workspace.id}/files/${path}?repoId=${member!.repo_id}`,
      "PUT",
      {
        content: "saved without a platform commit\n",
        encoding: "utf-8",
        expectedHash: null,
      },
    );
    expect([200, 201], JSON.stringify(save.body)).toContain(save.status);
    expect(headFor(await branches(page, workspace.id), member!.repo_id)).toBe(
      headBeforeSave,
    );

    await openWorkspacePath(page, workspace.path);
    // Native publication controls remain on the canonical current-changes
    // surface; repository selection now belongs to its separate diff tab.
    await openWorkspacePath(page, `${workspace.path}?view=changes`);
    const view = page.getByTestId("workspace-changes-view");
    await expect(view).toBeVisible();
    await expect(view.getByTestId("workspace-change-row")).toContainText(path);
    await expect(view.getByRole("button", { name: /^Commit$/i })).toHaveCount(
      0,
    );
    await expect(view.getByRole("button", { name: /flush/i })).toHaveCount(0);

    const pushResponse = page.waitForResponse((response) =>
      response.url().includes(`/workspaces/${workspace.id}/changes/push`),
    );
    await view.getByTestId("workspace-changes-push").click();
    expect((await pushResponse).status()).toBe(200);

    expect(headFor(await branches(page, workspace.id), member!.repo_id)).toBe(
      headBeforeSave,
    );
    await expect(view.getByTestId("workspace-change-row")).toContainText(path);

    // Commit one separate file in each checkout through the owner's shell.
    // The earlier Save stays dirty and must remain absent from the pushed cut.
    const committedPath = `e2e-push-code-${randomUUID()}.txt`;
    const committedBytes = `committed code ${workspace.id}\n`;
    const docPath = `docs/${workspace.id}/e2e-push-doc-${randomUUID()}.md`;
    const docBytes = `# Committed Project Doc ${workspace.id}\n`;
    const codeWrite = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${workspace.id}/files/${committedPath}?repoId=${member!.repo_id}`,
      "PUT",
      { content: committedBytes, encoding: "utf-8", expectedHash: null },
    );
    expect([200, 201], JSON.stringify(codeWrite.body)).toContain(
      codeWrite.status,
    );
    const docWrite = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${workspace.id}/docs`,
      "POST",
      {
        name: docPath.split("/").at(-1),
        docType: "design",
        filePath: docPath,
        content: docBytes,
      },
    );
    expect(docWrite.status, JSON.stringify(docWrite.body)).toBe(201);
    await openWorkspacePath(page, `${workspace.path}?view=terminal`);
    const terminal = page.getByTestId("workspace-terminal");
    await expect(terminal).toBeVisible({ timeout: 30_000 });
    await expect(page.getByText("Workspace terminal · Connected")).toBeVisible({
      timeout: 30_000,
    });
    const input = terminal.locator(".xterm-helper-textarea");
    await input.focus();
    await input.pressSequentially("stty -echo");
    await input.press("Enter");
    const quote = (value: string) => `'${value.replaceAll("'", "'\\''")}'`;
    const suffix = randomUUID().replaceAll("-", "");
    const marker = `G5_M2_COMMITTED_${suffix}`;
    await input.pressSequentially(
      `git -C ${quote(member!.root_path)} add -- ${quote(committedPath)} && git -C ${quote(member!.root_path)} -c user.email=e2e@g5.dev -c user.name=e2e commit -qm 'M2 code commit' -- ${quote(committedPath)} && git -C ${quote(docsMember!.root_path)} add -- ${quote(docPath)} && git -C ${quote(docsMember!.root_path)} -c user.email=e2e@g5.dev -c user.name=e2e commit -qm 'M2 docs commit' -- ${quote(docPath)} && printf '%s_%s\\n' G5_M2_COMMITTED ${suffix}`,
    );
    await input.press("Enter");
    await expect(terminal.locator(".xterm-accessibility-tree")).toContainText(
      marker,
      { timeout: 30_000 },
    );

    const beforeCommitPush = await waitForLiveWorkspaceChanges(
      page,
      workspace.id,
    );
    const committedHeads = new Map(
      beforeCommitPush.members.map((candidate) => [
        candidate.repo_id,
        candidate.head_oid,
      ]),
    );
    for (const repoId of [member!.repo_id, docsRepoId]) {
      expect(committedHeads.get(repoId), `repo ${repoId} local HEAD`).toMatch(
        /^([0-9a-f]{40}|[0-9a-f]{64})$/,
      );
    }
    for (const repoId of [member!.repo_id, docsRepoId]) {
      expect(
        beforeCommitPush.members.find(
          (candidate) => candidate.repo_id === repoId,
        )?.against_internal?.ahead,
        `repo ${repoId} must have a local commit before Push`,
      ).toBeGreaterThanOrEqual(1);
    }
    await openWorkspacePath(page, `${workspace.path}?view=changes`);
    await expect(view.getByTestId("workspace-changes-push")).toBeVisible();
    const committedPush = page.waitForResponse(
      (response) =>
        response.url().includes(`/workspaces/${workspace.id}/changes/push`) &&
        response.request().method() === "POST",
    );
    await view.getByTestId("workspace-changes-push").click();
    const committedPushResponse = await committedPush;
    const pushOutcome = await committedPushResponse.json();
    expect(committedPushResponse.status(), JSON.stringify(pushOutcome)).toBe(
      200,
    );
    expect(pushOutcome?.data?.status, JSON.stringify(pushOutcome)).toBe(
      "succeeded",
    );
    const pushedMembers = pushOutcome?.data?.members;
    expect(Array.isArray(pushedMembers), JSON.stringify(pushOutcome)).toBe(
      true,
    );
    for (const repoId of [member!.repo_id, docsRepoId]) {
      expect(
        pushedMembers?.find(
          (candidate: { repo_id?: string }) => candidate.repo_id === repoId,
        )?.pushed,
        JSON.stringify(pushOutcome),
      ).toBe(true);
    }
    let pushed = beforeCommitPush;
    await expect
      .poll(
        async () => {
          const response = await authedApiViaRequest(
            page,
            `/api/v1/workspaces/${workspace.id}/changes`,
          );
          if (
            response.status === 409 &&
            response.body?.error?.message?.includes(
              "MODULE_CONCURRENCY_EXHAUSTED",
            )
          ) {
            return [-1, -1];
          }
          expect(response.status, JSON.stringify(response.body)).toBe(200);
          const changes = (response.body?.data ??
            response.body) as typeof pushed;
          expect(changes.source).toBe("live");
          expect(changes.attached).toBe(true);
          pushed = changes;
          return [member!.repo_id, docsRepoId].map(
            (repoId) =>
              changes.members.find((candidate) => candidate.repo_id === repoId)
                ?.against_internal?.ahead,
          );
        },
        { timeout: 30_000 },
      )
      .toEqual([0, 0]);
    for (const repoId of [member!.repo_id, docsRepoId]) {
      const remoteHead = pushed.members.find(
        (candidate) => candidate.repo_id === repoId,
      )?.internal_ref;
      expect(remoteHead, `repo ${repoId} remote ref`).toBe(
        committedHeads.get(repoId),
      );
    }
    expect(
      pushed.members
        .find((candidate) => candidate.repo_id === member!.repo_id)
        ?.dirty?.some((entry) => entry.path === path),
    ).toBe(true);

    child = await cloneWorkspaceById(page, workspace.id, {
      namePrefix: "m2-pushed-commits",
      source: "durable",
      start: false,
    });
    expect((await readWorkspaceVm(page, child.id)).status).toBe("stopped");
    const childBranches = await branches(page, child.id);
    for (const { repoId, filePath, bytes } of [
      {
        repoId: member!.repo_id,
        filePath: committedPath,
        bytes: committedBytes,
      },
      { repoId: docsRepoId, filePath: docPath, bytes: docBytes },
    ]) {
      const read = await authedApiViaRequest(
        page,
        `/api/v1/workspaces/${child.id}/files/${filePath}?repoId=${repoId}`,
      );
      expect(read.status, JSON.stringify(read.body)).toBe(200);
      const file = read.body?.data ?? read.body;
      expect(file.content, `${repoId}:${filePath}`).toBe(bytes);
      expect(file.repoOid, `${repoId}:${filePath}`).toBe(
        headFor(childBranches, repoId),
      );
      expect(headFor(childBranches, repoId)).toBe(committedHeads.get(repoId));
    }
    const uncommitted = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${child.id}/files/${path}?repoId=${member!.repo_id}`,
    );
    expect(uncommitted.status, JSON.stringify(uncommitted.body)).toBe(404);
  });
});
