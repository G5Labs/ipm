import { expect, test, type Page } from "@playwright/test";

import {
  TEST_ORG,
  USING_OAUTH_STUB,
  authedApi,
  authedApiViaRequest,
  cloneWorkspaceById,
  createWorkspaceClone,
  deleteWorkspaceById,
  openRailTerminal,
  openRibbonPreview,
  openWorkspacePath,
  patchStoredUserSettings,
  railTerminalButton,
  readCurrentOrgContext,
  readWorkspaceVm,
  recordExpectedSubject,
  runWorkspaceVmAction,
  type WorkspaceClone,
  waitForLiveWorkspaceChanges,
  waitForWorkspaceVmStatus,
  selectWorkspaceDefaultPreview,
} from "../helpers";
import { loginWithOAuthStub, SEEDED_FORK_MEMBER_IDENTITY } from "../oauth";

/**
 * Design v9 §4.3 invariant 12 and §5.1/§9.1/§9.2: one human writer.
 *
 * The workspace owner alone mutates it; read access never implies write
 * access. A non-owner (org member, and an org admin alike, until transfer)
 * reads durable Git only, never the owner's live VM, and cannot Save, start,
 * stop, restart, preview, chat or open a terminal through any API or UI path.
 * Their calls must not change the owner's VM state either.
 */
test.describe("Sharing matrix", () => {
  test.skip(
    !USING_OAUTH_STUB,
    "Needs the OAuth stub to log a second, non-owner identity in",
  );

  let workspace: WorkspaceClone | undefined;
  let orgId = "";
  let memberUserId = "";

  test.beforeEach(async ({ page }, testInfo) => {
    orgId = (await readCurrentOrgContext(page)).orgId;
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
    workspace = await createWorkspaceClone(page, {
      namePrefix: `sharing-${testInfo.title}`,
    });
    await runWorkspaceVmAction(page, workspace.id, "resume");
    await waitForWorkspaceVmStatus(page, workspace.id, "running");
  });

  test.afterEach(async ({ page }) => {
    if (memberUserId) {
      // Leave the fixed identity as a plain member for the next run.
      await authedApi(
        page,
        `/api/v1/orgs/${orgId}/members/${memberUserId}`,
        "PATCH",
        {
          role: "member",
        },
      ).catch(() => {});
    }
    if (workspace) {
      await deleteWorkspaceById(page, workspace.id).catch(() => {});
      workspace = undefined;
    }
  });

  async function loginMember(memberPage: Page): Promise<string> {
    await loginWithOAuthStub(memberPage, SEEDED_FORK_MEMBER_IDENTITY, {
      targetOrgSlug: TEST_ORG,
    });
    return recordExpectedSubject(memberPage);
  }

  async function assertReadOnlyForNonOwner(
    ownerPage: Page,
    memberPage: Page,
    who: string,
  ) {
    const id = workspace!.id;
    const live = await waitForLiveWorkspaceChanges(ownerPage, id);
    const workspaceDetail = await authedApiViaRequest(
      ownerPage,
      `/api/v1/workspaces/${id}`,
    );
    expect(workspaceDetail.status).toBe(200);
    const docsRepoId = String(
      (workspaceDetail.body?.data ?? workspaceDetail.body).docsRepoId,
    );
    const codeMember = live.members.find(
      (member) => member.repo_id !== docsRepoId,
    );
    expect(codeMember, "code repo member").toBeTruthy();
    const repoId = codeMember!.repo_id;

    // Owner: live source, writes admitted.
    const livePath = `e2e-sharing-${Date.now()}.txt`;
    const ownerWrite = await authedApiViaRequest(
      ownerPage,
      `/api/v1/workspaces/${id}/files/${livePath}?repoId=${repoId}`,
      "PUT",
      { content: "owner\n", encoding: "utf-8", expectedHash: null },
    );
    expect([200, 201], JSON.stringify(ownerWrite.body)).toContain(
      ownerWrite.status,
    );
    const ownerLiveRead = await authedApiViaRequest(
      ownerPage,
      `/api/v1/workspaces/${id}/files/${livePath}?repoId=${repoId}`,
    );
    expect(ownerLiveRead.status, JSON.stringify(ownerLiveRead.body)).toBe(200);
    expect(ownerLiveRead.body?.data?.content).toBe("owner\n");
    expect(ownerLiveRead.body?.data?.source).toBe("live");
    const ownerSeededRead = await authedApiViaRequest(
      ownerPage,
      `/api/v1/workspaces/${id}/files/semlift.yaml?repoId=${repoId}`,
    );
    expect(ownerSeededRead.status, JSON.stringify(ownerSeededRead.body)).toBe(
      200,
    );
    const vmBefore = await readWorkspaceVm(ownerPage, id);
    expect(vmBefore.status).toBe("running");

    // Non-owner reads resolve durable Git, never the owner's VM.
    const changes = await authedApiViaRequest(
      memberPage,
      `/api/v1/workspaces/${id}/changes`,
    );
    expect(
      changes.status,
      `${who} changes: ${JSON.stringify(changes.body)}`,
    ).toBe(200);
    const report = changes.body?.data ?? changes.body ?? {};
    expect(report.source, `${who} must read the durable Changes report`).toBe(
      "durable",
    );
    expect(report.attached).toBe(false);
    const hiddenLiveRead = await authedApiViaRequest(
      memberPage,
      `/api/v1/workspaces/${id}/files/${livePath}?repoId=${repoId}`,
    );
    expect(hiddenLiveRead.status, `${who} must not see live bytes`).toBe(404);

    const seededRead = await authedApiViaRequest(
      memberPage,
      `/api/v1/workspaces/${id}/files/semlift.yaml?repoId=${repoId}`,
    );
    expect(
      seededRead.status,
      `${who} file read: ${JSON.stringify(seededRead.body)}`,
    ).toBe(200);
    expect(["committed", "flush"]).toContain(
      seededRead.body?.data?.source ?? seededRead.body?.source,
    );
    const durableFile = seededRead.body?.data ?? seededRead.body;
    expect(durableFile.content).toBe(ownerSeededRead.body?.data?.content);
    const durableMember = report.members.find(
      (member: { repo_id: string }) => member.repo_id === repoId,
    );
    // A running owner's durable report has no flush receipt yet. Its
    // internal ref still names the same committed cut used by file reads.
    const memberHead =
      durableMember?.internal_ref ?? durableMember?.local_head_oid;
    expect(memberHead, `${who} durable member HEAD`).toMatch(
      /^([0-9a-f]{40}|[0-9a-f]{64})$/,
    );
    expect(durableFile.repoOid, `${who} verified file OID`).toBe(memberHead);
    const snapshotAt = Date.parse(String(durableFile.snapshotAt));
    expect(Number.isFinite(snapshotAt), `${who} snapshot age`).toBe(true);
    expect(snapshotAt).toBeGreaterThan(Date.now() - 24 * 60 * 60 * 1_000);
    expect(snapshotAt).toBeLessThanOrEqual(Date.now() + 5_000);

    const docs = await authedApiViaRequest(
      memberPage,
      `/api/v1/workspaces/${id}/docs?surface=workspace`,
    );
    expect(docs.status, `${who} docs list: ${JSON.stringify(docs.body)}`).toBe(
      200,
    );

    // Non-owner mutations and compute controls are refused, without booting or
    // touching the owner's VM.
    const denied: Array<[string, "PUT" | "POST", Record<string, unknown>]> = [
      [
        `/api/v1/workspaces/${id}/files/e2e-${who}-write.txt?repoId=${repoId}`,
        "PUT",
        { content: "x\n", encoding: "utf-8", expectedHash: null },
      ],
      [
        `/api/v1/workspaces/${id}/docs`,
        "POST",
        {
          name: `${who}-doc.md`,
          docType: "design",
          filePath: `docs/${id}/${who}-doc.md`,
          content: "# no\n",
        },
      ],
      [`/api/v1/workspaces/${id}/stop`, "POST", {}],
      [`/api/v1/workspaces/${id}/resume`, "POST", {}],
      [
        `/api/v1/workspaces/${id}/preview`,
        "POST",
        { repoId, idempotencyKey: `${who}-${Date.now()}` },
      ],
      [
        `/api/v1/workspaces/${id}/sessions/start`,
        "POST",
        { content: "hello from a non-owner" },
      ],
      [`/api/v1/workspaces/${id}/changes/push`, "POST", {}],
    ];
    for (const [path, method, body] of denied) {
      const result = await authedApiViaRequest(memberPage, path, method, body);
      expect(
        result.status,
        `${who} ${method} ${path} must be forbidden: ${JSON.stringify(result.body)}`,
      ).toBe(403);
    }

    const vmAfter = await readWorkspaceVm(ownerPage, id);
    expect(vmAfter.status).toBe("running");
    expect(vmAfter.vmId).toBe(vmBefore.vmId);
    expect(vmAfter.epoch).toBe(vmBefore.epoch);

    // UI: no owner controls, but a way to fork. A fresh context misses
    // auth.setup's pin of the full sidebar (g5.ui.sidebar-default); the rail
    // check below reads the Files section header.
    await patchStoredUserSettings(memberPage, {
      sidebarDisplayMode: "expanded",
    });
    await openWorkspacePath(memberPage, workspace!.path);
    await expect(memberPage.getByTestId("workspace-tab-add")).toBeVisible({
      timeout: 30_000,
    });
    for (const control of [
      "workspace-vm-start",
      "workspace-vm-stop",
      "workspace-vm-restart",
    ]) {
      await expect(memberPage.getByTestId(control)).toHaveCount(0);
    }
    await expect(await railTerminalButton(memberPage)).toHaveCount(0);
    // Design v9 §5.1: transfer is the one workspace action an org admin who
    // is not the owner may take; a plain member has no such control. The
    // ribbon's owner control opens the transfer menu only for the admin.
    const ownerTrigger = memberPage.getByTestId("workspace-owner-trigger");
    await expect(ownerTrigger).toBeVisible({ timeout: 30_000 });
    if (who === "admin") {
      await expect(ownerTrigger).toBeEnabled();
      await expect(ownerTrigger).toHaveAttribute("aria-haspopup", /.+/);
    } else {
      await expect(ownerTrigger).toBeDisabled();
    }

    // The owner is unaffected.
    const ownerAgain = await authedApiViaRequest(
      ownerPage,
      `/api/v1/workspaces/${id}/files/e2e-sharing-again-${Date.now()}.txt?repoId=${repoId}`,
      "PUT",
      { content: "owner again\n", encoding: "utf-8", expectedHash: null },
    );
    expect([200, 201], JSON.stringify(ownerAgain.body)).toContain(
      ownerAgain.status,
    );
    return repoId;
  }

  test("M10 member browsing reads a verified Git cut while the owner reads live bytes", async ({
    page,
    browser,
  }) => {
    const memberContext = await browser.newContext({
      // The chromium project applies the seeded owner's storageState to every
      // context by default; the member must start signed out.
      storageState: { cookies: [], origins: [] },
    });
    try {
      const memberPage = await memberContext.newPage();
      memberUserId = await loginMember(memberPage);
      const repoId = await assertReadOnlyForNonOwner(
        page,
        memberPage,
        "member",
      );

      const filePath = `/api/v1/workspaces/${workspace!.id}/files/semlift.yaml?repoId=${repoId}`;
      const beforeStop = await authedApiViaRequest(page, filePath);
      expect(beforeStop.status, JSON.stringify(beforeStop.body)).toBe(200);
      expect(beforeStop.body?.data?.source).toBe("live");
      await runWorkspaceVmAction(page, workspace!.id, "stop");
      await waitForWorkspaceVmStatus(page, workspace!.id, "stopped");
      const stoppedRead = await authedApiViaRequest(page, filePath);
      expect(stoppedRead.status, JSON.stringify(stoppedRead.body)).toBe(200);
      const stoppedFile = stoppedRead.body?.data ?? stoppedRead.body;
      expect(stoppedFile.content).toBe(beforeStop.body?.data?.content);
      expect(stoppedFile.source).toBe("committed");
      const pushedBranches = await authedApiViaRequest(
        page,
        `/api/v1/workspaces/${workspace!.id}/branches`,
      );
      expect(pushedBranches.status).toBe(200);
      const pushedMember = pushedBranches.body?.data?.find(
        (member: { repoId: string }) => member.repoId === repoId,
      );
      // Stopping preserves current work for resume without publishing it.
      // Both owner and reader browse the same pushed repository revision.
      expect(stoppedFile.repoOid).toBe(pushedMember?.headCommit);
      const memberStopped = await authedApiViaRequest(memberPage, filePath);
      expect(memberStopped.status).toBe(200);
      expect(memberStopped.body?.data?.repoOid).toBe(stoppedFile.repoOid);
      expect(memberStopped.body?.data?.content).toBe(stoppedFile.content);
      const stoppedSnapshotAt = Date.parse(String(stoppedFile.snapshotAt));
      expect(Number.isFinite(stoppedSnapshotAt)).toBe(true);
      expect(stoppedSnapshotAt).toBeGreaterThan(
        Date.now() - 24 * 60 * 60 * 1_000,
      );
      expect(stoppedSnapshotAt).toBeLessThanOrEqual(Date.now() + 5_000);
      await openWorkspacePath(page, workspace!.path);
      expect((await readWorkspaceVm(page, workspace!.id)).status).toBe(
        "stopped",
      );
    } finally {
      await memberContext.close();
    }
  });

  test.describe(() => {
    test.describe.configure({ timeout: 360_000 });
    test("F5 reader durable fork takes a verified Git cut while owner edits with terminal and preview open", async ({
      page,
      browser,
    }) => {
      const memberContext = await browser.newContext({
        storageState: { cookies: [], origins: [] },
      });
      let child: WorkspaceClone | undefined;
      try {
        const memberPage = await memberContext.newPage();
        memberUserId = await loginMember(memberPage);
        const id = workspace!.id;
        const sourceDetailResponse = await authedApiViaRequest(
          page,
          `/api/v1/workspaces/${id}`,
        );
        expect(sourceDetailResponse.status).toBe(200);
        const sourceDetail =
          sourceDetailResponse.body?.data ?? sourceDetailResponse.body;
        const docsRepoId = String(sourceDetail.docsRepoId);
        const codeBranch = sourceDetail.branches.find(
          (branch: { repoId: string }) => branch.repoId !== docsRepoId,
        );
        expect(codeBranch?.repoId).toBeTruthy();
        const baseline = new Map(
          sourceDetail.branches.map(
            (branch: { repoId: string; headCommit: string }) => [
              branch.repoId,
              branch.headCommit,
            ],
          ),
        );

        const codePath = `e2e-reader-excluded-${Date.now()}.txt`;
        const docPath = `docs/${id}/e2e-reader-excluded-${Date.now()}.md`;
        const codeWrite = await authedApiViaRequest(
          page,
          `/api/v1/workspaces/${id}/files/${codePath}?repoId=${codeBranch.repoId}`,
          "PUT",
          {
            content: "owner live edit\n",
            encoding: "utf-8",
            expectedHash: null,
          },
        );
        expect([200, 201], JSON.stringify(codeWrite.body)).toContain(
          codeWrite.status,
        );
        const docWrite = await authedApiViaRequest(
          page,
          `/api/v1/workspaces/${id}/docs`,
          "POST",
          {
            name: docPath.split("/").at(-1),
            docType: "design",
            filePath: docPath,
            content: "# Owner live Project Doc\n",
          },
        );
        expect(docWrite.status, JSON.stringify(docWrite.body)).toBe(201);
        const live = await waitForLiveWorkspaceChanges(page, id);
        expect(
          live.members
            .find((member) => member.repo_id === codeBranch.repoId)
            ?.dirty?.some((entry) => entry.path === codePath),
        ).toBe(true);
        expect(
          live.members
            .find((member) => member.repo_id === docsRepoId)
            ?.dirty?.some((entry) => entry.path === docPath),
        ).toBe(true);

        await openWorkspacePath(page, workspace!.path);
        await openRailTerminal(page);
        const terminal = page.getByTestId("workspace-terminal");
        await expect(terminal).toBeVisible({ timeout: 30_000 });
        await expect(
          page.getByText("Workspace terminal · Connected"),
        ).toBeVisible({ timeout: 30_000 });
        await openRibbonPreview(page);
        await selectWorkspaceDefaultPreview(page);
        const previewStart = page.getByTestId("workspace-preview-start");
        await expect(previewStart).toBeEnabled({ timeout: 30_000 });
        await previewStart.click();
        await expect
          .poll(
            async () => {
              const response = await authedApiViaRequest(
                page,
                `/api/v1/workspaces/${id}/preview`,
              );
              expect(response.status, JSON.stringify(response.body)).toBe(200);
              const preview = response.body?.data ?? response.body;
              if (
                ["failed", "cancelled", "succeeded"].includes(preview.status)
              ) {
                throw new Error(
                  `preview ended before reader fork: ${JSON.stringify(preview)}`,
                );
              }
              return preview.ready === true;
            },
            { timeout: 120_000, intervals: [1_000, 2_000] },
          )
          .toBe(true);
        const previewBeforeResponse = await authedApiViaRequest(
          page,
          `/api/v1/workspaces/${id}/preview`,
        );
        const previewBefore =
          previewBeforeResponse.body?.data ?? previewBeforeResponse.body;
        const vmBefore = await readWorkspaceVm(page, id);
        expect(vmBefore.status).toBe("running");

        child = await cloneWorkspaceById(memberPage, id, {
          namePrefix: "f5-reader-durable",
          source: "durable",
          start: false,
        });
        expect((await readWorkspaceVm(memberPage, child.id)).status).toBe(
          "stopped",
        );
        const childDetailResponse = await authedApiViaRequest(
          memberPage,
          `/api/v1/workspaces/${child.id}`,
        );
        expect(childDetailResponse.status).toBe(200);
        const childDetail =
          childDetailResponse.body?.data ?? childDetailResponse.body;
        expect(childDetail.clonedFromId).toBe(id);
        expect(childDetail.ownerUserId).toBe(memberUserId);
        expect(childDetail.branches.length).toBe(baseline.size);
        for (const branch of childDetail.branches) {
          expect(branch.headCommit, `repo ${branch.repoId}`).toBe(
            baseline.get(branch.repoId),
          );
        }
        expect(
          new Set(
            childDetail.branches.map(
              (branch: { branchName: string }) => branch.branchName,
            ),
          ).size,
        ).toBe(1);
        for (const [repoId, path] of [
          [codeBranch.repoId, codePath],
          [docsRepoId, docPath],
        ]) {
          const excluded = await authedApiViaRequest(
            memberPage,
            `/api/v1/workspaces/${child.id}/files/${path}?repoId=${repoId}`,
          );
          expect(
            excluded.status,
            `${repoId}:${path} ${JSON.stringify(excluded.body)}`,
          ).toBe(404);
        }
        const sourceVmAfter = await readWorkspaceVm(page, id);
        expect(sourceVmAfter.status).toBe("running");
        expect(sourceVmAfter.vmId).toBe(vmBefore.vmId);
        expect(sourceVmAfter.epoch).toBe(vmBefore.epoch);
        expect(sourceVmAfter.lastFlushId).toBe(vmBefore.lastFlushId);
        const previewAfterResponse = await authedApiViaRequest(
          page,
          `/api/v1/workspaces/${id}/preview`,
        );
        const previewAfter =
          previewAfterResponse.body?.data ?? previewAfterResponse.body;
        expect(previewAfter.taskId).toBe(previewBefore.taskId);
        expect(previewAfter.ready).toBe(true);

        const suffix = `${Date.now()}`;
        const marker = `G5_F5_OWNER_STILL_ATTACHED_${suffix}`;
        const input = terminal.locator(".xterm-helper-textarea");
        await input.focus();
        await input.pressSequentially(
          `printf '%s_%s\\n' G5_F5_OWNER_STILL_ATTACHED ${suffix}`,
        );
        await input.press("Enter");
        await expect(
          terminal.locator(".xterm-accessibility-tree"),
        ).toContainText(marker, { timeout: 30_000 });
        const ownerWrite = await authedApiViaRequest(
          page,
          `/api/v1/workspaces/${id}/files/e2e-after-reader-fork-${Date.now()}.txt?repoId=${codeBranch.repoId}`,
          "PUT",
          {
            content: "owner still admitted\n",
            encoding: "utf-8",
            expectedHash: null,
          },
        );
        expect([200, 201], JSON.stringify(ownerWrite.body)).toContain(
          ownerWrite.status,
        );
      } finally {
        if (child)
          await deleteWorkspaceById(memberContext.pages()[0]!, child.id).catch(
            () => {},
          );
        await memberContext.close();
      }
    });
  });

  test("an org admin has no residual write authority until ownership is transferred", async ({
    page,
    browser,
  }) => {
    const memberContext = await browser.newContext({
      // The chromium project applies the seeded owner's storageState to every
      // context by default; the member must start signed out.
      storageState: { cookies: [], origins: [] },
    });
    try {
      const memberPage = await memberContext.newPage();
      memberUserId = await loginMember(memberPage);
      const promoted = await authedApi(
        page,
        `/api/v1/orgs/${orgId}/members/${memberUserId}`,
        "PATCH",
        { role: "admin" },
      );
      expect(promoted.status, JSON.stringify(promoted.body)).toBe(200);
      await assertReadOnlyForNonOwner(page, memberPage, "admin");
    } finally {
      await memberContext.close();
    }
  });
});
