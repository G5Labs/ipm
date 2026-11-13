import { test, expect, type Page } from "@playwright/test";
import { randomUUID } from "node:crypto";
import {
  activateSidebarSection,
  TEST_ORG,
  authedApi,
  readCurrentOrgContext,
  readWorkspaceVm,
  runWorkspaceVmAction,
  waitForWorkspaceVmStatus,
  waitForLiveWorkspaceChanges,
} from "../helpers";
import { shell, createReviewActor, api as reviewApi } from "../workspace-review";
import { gitHead, quote, readAppliedDiff } from "../source-control";
import type {
  SourceMenu,
  SourceSnapshot,
} from "../../src/lib/sourceControlSelection";

// Personal workspace on Home (personal_workspace g5.personal.home,
// g5.personal.start-and-send): with the personal VM stopped, the Home side
// panel's Chat offers Start and send; one request records Start consent and
// queues the message, and the panel's status shows the Start's progress.

const personalChangesTest = test.extend({
  page: async ({ page, browser }, provide) => {
    // Other VMs can sync the shared user's personal memory, and failed
    // attempts retain their commits. Give this exact-diff case its own owner.
    const { orgId } = await readCurrentOrgContext(page);
    const { actor, context } = await createReviewActor(
      page,
      browser,
      orgId,
      "Personal Changes",
    );
    try {
      await provide(actor.page);
    } finally {
      try {
        await context.close();
      } finally {
        const removed = await reviewApi(
          page,
          `/api/v1/orgs/${orgId}/members/${actor.id}`,
          "DELETE",
        );
        expect([200, 204], JSON.stringify(removed.body)).toContain(removed.status);
      }
    }
  },
});

test.describe("Personal workspace — Home Start and send", () => {
  let personalWorkspaceId = "";

  test.beforeEach(async ({ page }) => {
    const { orgId } = await readCurrentOrgContext(page);
    // `/me` ensures the caller's personal workspace exists.
    const me = await authedApi(page, `/api/v1/orgs/${orgId}/me`);
    expect(me.status, JSON.stringify(me.body)).toBe(200);
    personalWorkspaceId = (me.body?.data ?? me.body).workspaceId;
    expect(personalWorkspaceId).toBeTruthy();
    const vm = await authedApi(
      page,
      `/api/v1/workspaces/${personalWorkspaceId}/vm`,
    );
    const status = (vm.body?.data ?? vm.body)?.status;
    if (status && status !== "stopped") {
      if (status === "running" || status === "starting") {
        await runWorkspaceVmAction(page, personalWorkspaceId, "stop");
      }
      await waitForWorkspaceVmStatus(page, personalWorkspaceId, "stopped");
    }
  });

  test.afterEach(async ({ page }) => {
    if (!personalWorkspaceId) return;
    // Free the VM slot before the shard's next spec: a stop sent while the
    // Start is still settling can be refused, so repeat it until the VM
    // reports stopped (or failed, which holds no slot).
    await expect
      .poll(
        async () => {
          const { status } = await readWorkspaceVm(page, personalWorkspaceId);
          if (status !== "stopped" && status !== "stopping") {
            await runWorkspaceVmAction(page, personalWorkspaceId, "stop").catch(
              () => {},
            );
          }
          return status === "failed" ? "stopped" : status;
        },
        { timeout: 120_000, intervals: [1_000, 2_000, 5_000] },
      )
      .toBe("stopped");
  });

  test.describe("Personal Changes", () => {
    test.describe.configure({ timeout: 300_000 });
    personalChangesTest("personal unpushed Changes excludes reference edits and Save preserves the applied diff", async ({
      page,
    }) => {
      const detail = await authedApi(
        page,
        `/api/v1/workspaces/${personalWorkspaceId}`,
      );
      expect(detail.status, JSON.stringify(detail.body)).toBe(200);
      const ws = {
        id: personalWorkspaceId,
        name: (detail.body?.data ?? detail.body).name as string,
        path: `/${TEST_ORG}/me`,
      };
      await runWorkspaceVmAction(page, ws.id, "resume");
      await waitForWorkspaceVmStatus(page, ws.id, "running");
      const live = await waitForLiveWorkspaceChanges(page, ws.id);
      expect(live.members).toHaveLength(1);
      const root = live.members[0].root_path;
      const note = `personal-source-${randomUUID()}.txt`;
      const reference = `reference-source-${randomUUID()}.txt`;
      const referenceHeads = `/tmp/${reference}.heads`;
      const bytes = `PERSONAL SOURCE ${note}`;
      await shell(
        page,
        ws,
        `ref_count=0; ref_failure=0; private_git=$(git -C ${quote(root)} rev-parse --absolute-git-dir) && : > ${quote(referenceHeads)} && printf '%s\\n' ${quote(bytes)} > ${quote(`${root}/${note}`)} && for candidate_root in /work/*; do reference_git=$(git -C "$candidate_root" rev-parse --absolute-git-dir 2>/dev/null) || continue; [ "$reference_git" = "$private_git" ] && continue; reference_root=$(git -C "$candidate_root" rev-parse --show-toplevel 2>/dev/null) || continue; if printf '%s\\n' REFERENCE_ONLY > "$reference_root/${reference}" && printf '%s %s\\n' "$(git -C "$reference_root" rev-parse HEAD)" "$reference_root" >> ${quote(referenceHeads)}; then ref_count=$((ref_count+1)); else ref_failure=1; fi; done && test "$ref_count" -gt 0 && test "$ref_failure" -eq 0`,
      );
      const loaded = page.waitForResponse(
        (r) =>
          r.url().endsWith("/source-control/menu") &&
          r.request().method() === "GET",
      );
      await activateSidebarSection(page, "sidebar-source-control-tab");
      const response = await loaded;
      expect(response.status(), await response.text()).toBe(200);
      const menu = (await response.json()).data as SourceMenu;
      expect(menu.repos).toHaveLength(1);
      const repo = menu.repos[0];
      expect(repo).toMatchObject({
        base: null,
        unpushedOnly: true,
        error: null,
        commits: [],
      });
      expect(repo.live).toEqual({ selectable: true });
      const control = page.getByTestId("source-control-menu");
      await expect(
        control.getByLabel(`Not yet pushed in ${repo.name}`),
      ).toBeChecked();
      const applied = page.waitForResponse((r) =>
        r.url().endsWith("/source-control/apply"),
      );
      await control.getByRole("button", { name: "Show", exact: true }).click();
      const appliedResponse = await applied;
      expect(appliedResponse.status(), await appliedResponse.text()).toBe(200);
      const snapshot = (await appliedResponse.json()).data as SourceSnapshot;
      expect(snapshot).toMatchObject({ fileCount: 1, private: true });
      expect(snapshot.patches).toHaveLength(1);
      expect(snapshot.patches[0]).toMatchObject({
        repoId: repo.repoId,
        before: repo.pushed,
        after: null,
        file: { path: note },
      });
      expect(
        snapshot.patches[0].file.diff.hunks
          .flatMap((h) => h.lines)
          .filter((l) => l.type === "addition")
          .map((l) => l.content),
      ).toEqual([bytes]);
      await expect(
        page.getByRole("heading", { name: /^Not yet pushed/ }),
      ).toBeVisible();
      const stream = page.getByTestId("source-control-diff-stream");
      const displayed = await readAppliedDiff(page);
      const agent = await page.context().newPage();
      try {
        const checked = page.waitForResponse(
          (r) =>
            r.url().endsWith("/source-control/check") && r.status() === 200,
        );
        await shell(agent, ws, "g5d workspace save");
        const current = (await (await checked).json()).data.menu as SourceMenu;
        expect(current.repos).toHaveLength(1);
        expect(current.repos[0]).toMatchObject({
          base: null,
          unpushedOnly: true,
          commits: [],
          live: { selectable: true },
        });
        expect(current.repos[0].pushed).not.toBe(repo.pushed);
        expect(current.repos[0].pushed).toBe(await gitHead(agent, ws, root));
        await shell(
          agent,
          ws,
          `ref_status=0; checked_refs=0; while read -r expected_head reference_root; do checked_refs=$((checked_refs+1)); [ "$(git -C "$reference_root" rev-parse HEAD)" = "$expected_head" ] && [ "$(git -C "$reference_root" status --porcelain --untracked-files=all -- ${quote(reference)})" = ${quote(`?? ${reference}`)} ] && [ "$(cat "$reference_root/${reference}")" = REFERENCE_ONLY ] || ref_status=1; done < ${quote(referenceHeads)}; test "$checked_refs" -gt 0 && test "$ref_status" -eq 0`,
        );
        await expect(
          control.getByRole("button", { name: "Show", exact: true }),
        ).toBeEnabled();
        expect(await readAppliedDiff(page)).toBe(displayed);
        const cleanApply = page.waitForResponse((r) =>
          r.url().endsWith("/source-control/apply"),
        );
        await control.getByRole("button", { name: "Show", exact: true }).click();
        expect((await (await cleanApply).json()).data.fileCount).toBe(0);
        await expect(stream).toContainText(
          "No net file changes in the selected comparison.",
        );
      } finally {
        await agent.close();
      }
      await runWorkspaceVmAction(page, ws.id, "stop");
      await waitForWorkspaceVmStatus(page, ws.id, "stopped");
      await expect(stream).toBeHidden();
      const durable = await authedApi(
        page,
        `/api/v1/workspaces/${ws.id}/files/${note}?repoId=${repo.repoId}`,
      );
      expect(durable.status, JSON.stringify(durable.body)).toBe(200);
      expect((durable.body?.data ?? durable.body).content).toBe(`${bytes}\n`);
    });
  });

  const startAndSendFromHome = async ({ page }: { page: Page }) => {
    await page.goto(`/${TEST_ORG}/home`, { waitUntil: "domcontentloaded" });
    // At desktop widths the personal workspace is the side panel's workspace
    // Chat with its status and notice; files stay in the workbench.
    const chat = page.getByTestId("home-personal-chat");
    await expect(chat).toBeVisible({ timeout: 20_000 });
    await expect(
      chat.getByTestId("home-personal-chat-admin-notice"),
    ).toContainText("Organization administrators can see your personal work.");
    await expect(page.getByTestId("home-personal-panel")).toBeHidden();

    const composer = chat.getByTestId("chat-input");
    await expect(composer).toBeEnabled({ timeout: 20_000 });
    const message = `E2E: say hello from my personal workspace ${randomUUID()}`;
    await composer.fill(message);
    const startAndSend = chat.getByTestId("chat-panel-no-session-send");
    await expect(startAndSend).toHaveText(/Start and send/);
    await expect(startAndSend).toBeEnabled({ timeout: 20_000 });

    const startRequest = page.waitForRequest(
      (request) =>
        request.method() === "POST" &&
        request
          .url()
          .includes(`/api/v1/workspaces/${personalWorkspaceId}/sessions/start`),
    );
    await startAndSend.click();
    const body = (await startRequest).postDataJSON() as Record<string, unknown>;
    expect(body.startCompute).toBe(true);

    // Transcript publication follows native acceptance after cold VM startup.
    // Give startup its normal readiness wait before the message-render deadline.
    await waitForWorkspaceVmStatus(page, personalWorkspaceId, "running");
    await expect(chat.getByTestId("chat-messages")).toContainText(message, {
      timeout: 20_000,
    });
    await expect(
      chat.getByTestId("home-personal-chat-vm-status"),
    ).not.toHaveText(/Stopped/, { timeout: 30_000 });
  };

  // Exercise fresh cold starts while the other browser shard and API replicas
  // are active. The mandatory selector receipt requires every run to pass on
  // its first attempt; unique content prevents old history satisfying it.
  for (let run = 1; run <= 3; run++) {
    test(
      "Start and send from Home starts the personal VM and queues the message" +
        (run === 1 ? "" : ` (repeat ${run})`),
      startAndSendFromHome,
    );
  }
});
