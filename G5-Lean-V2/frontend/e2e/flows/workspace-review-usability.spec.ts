import AxeBuilder from "@axe-core/playwright";
import { createHash } from "node:crypto";
import type { VectorRevision } from "../../src/lib/types";
import {
  test,
  expect,
  api,
  landing,
  contentView,
  approvals,
  submitPr,
  request,
  timeline,
  timelineEntry,
  setPhase,
  stop,
  revise,
  vector,
  requirements,
  interceptNextPost,
} from "../workspace-review";
import { expandPrimaryIssue } from "../helpers";
import { matrix } from "../matrix";

test.describe.configure({ timeout: 420_000 });

for (const submitted of matrix([false, true], [true])) {
  test(`RV1 ${submitted ? "submitted PR" : "pre-PR"}: keyboard, narrow layout, full descriptions and accessible review action discovery for all roles`, async ({
    review: f,
  }) => {
    const unsafe = `${f.task}\n<script>window.reviewInjected = true</script>\n[Unsafe](javascript:alert(1))`;
    const updated = await api(
      f.owner.page,
      `/api/v1/workspaces/${f.workspace.id}`,
      "PATCH",
      { description: unsafe },
    );
    expect(updated.status).toBe(200);
    if (submitted) {
      await submitPr(f);
      await request(f);
    }
    await stop(f);
    for (const actor of [f.owner, f.reviewer, f.visitor]) {
      // Below 1024px G5 uses its separate chat-only mobile layout. Exercise
      // the narrowest existing desktop workbench, not a different product flow.
      await actor.page.setViewportSize({ width: 1024, height: 720 });
      await landing(actor, f);
      await expect(
        actor.page.getByRole("tab", {
          name: /^(Overview|Timeline|Changes|Approvals)$/,
        }),
      ).toHaveCount(0);
      await expect(
        actor.page.getByRole("link", {
          name: /^(Open workspace timeline|View approvals)/,
        }),
      ).toHaveCount(0);
      await expect(actor.page.getByTestId("landing-task-github")).toHaveCount(
        0,
      ); // No invented issue when none is linked.
      await expandPrimaryIssue(actor.page);
      const trigger = actor.page.getByTestId("workspace-read-task-description");
      await trigger.focus();
      await actor.page.keyboard.press("Enter");
      const dialog = actor.page.getByRole("dialog", {
        name: "Workspace task",
        exact: true,
      });
      await expect(dialog.getByRole("button")).toHaveCount(1);
      await expect(dialog.locator('a[href^="javascript:"]')).toHaveCount(0);
      expect(
        await actor.page.evaluate(() => Reflect.get(window, "reviewInjected")),
      ).toBeUndefined();
      const region = dialog.getByRole("region", {
        name: "Full task description",
      });
      await region.focus();
      await actor.page.keyboard.press("End");
      await expect(dialog.getByText(/END OF TASK/)).toBeInViewport();
      for (let i = 0; i < 5; i++) {
        await actor.page.keyboard.press("Tab");
        expect(
          await dialog.evaluate((el) => el.contains(document.activeElement)),
        ).toBe(true);
      }
      const violations = await new AxeBuilder({ page: actor.page })
        .include('[role="dialog"]')
        .withTags(["wcag2a", "wcag2aa"])
        .analyze();
      expect(violations.violations).toEqual([]);
      await actor.page.keyboard.press("Escape");
      await expect(trigger).toBeFocused();
      await approvals(actor, f);
      const label =
        actor === f.owner
          ? "Request ▾"
          : actor === f.reviewer && submitted
            ? "Approve"
            : "Add workspace comment";
      const action = actor.page.getByRole("button", {
        name: label,
        exact: true,
      });
      await expect(action).toBeEnabled();
      await expect(action).toBeInViewport();
      {
        await action.focus();
        await actor.page.keyboard.press("Enter");
        const active = actor.page.getByRole("dialog");
        await expect(active).toBeVisible();
        await actor.page.keyboard.press("Escape");
        await expect(action).toBeFocused();
      }
      // The layout can scroll its content; the browser page itself must not overflow.
      expect(
        await actor.page.evaluate(
          () => document.documentElement.scrollWidth <= window.innerWidth,
        ),
      ).toBe(true);
    }
  });
}

test("RV1/RV4 timeline pagination preserves all durable comments exactly once newest first; phase filters survive reload", async ({
  review: f,
}) => {
  await submitPr(f);
  const ids: string[] = [];
  for (let i = 0; i < 65; i++) {
    if (i === 0 || i === 33) await setPhase(f, i === 0 ? "be" : "ie");
    const r = await api(
      f.owner.page,
      `/api/v1/workspaces/${f.workspace.id}/comments`,
      "POST",
      {
        scope: "line",
        repoId: f.codeRepo,
        filePath: f.codePath,
        lineNumber: 2,
        content: `Paged review discussion ${String(i).padStart(2, "0")}`,
        lineHash: createHash("sha256").update("after review").digest("hex"),
        contentHash: createHash("sha256").update(f.after).digest("hex"),
        commitSha: f.pushed[f.codeRepo],
        anchorContext: f.after,
      },
    );
    expect(r.status, JSON.stringify(r.body)).toBe(201);
    ids.push(r.body.data.id);
  }
  await stop(f);
  for (const actor of [f.owner, f.reviewer, f.visitor]) {
    await landing(actor, f);
    await expect(
      actor.page.getByRole("button", {
        name: "Load earlier activity",
        exact: true,
      }),
    ).toBeVisible();
    for (let page = 0; page < 5; page++) {
      const earlier = actor.page.getByRole("button", {
        name: "Load earlier activity",
        exact: true,
      });
      if (!(await earlier.isVisible())) break;
      const before = await timeline(actor.page).getByRole("article").count();
      await earlier.click();
      // Wait for the requested page, not for a transient loading label that
      // may not have rendered when click() returns.
      await expect
        .poll(() => timeline(actor.page).getByRole("article").count())
        .toBeGreaterThan(before);
    }
    for (const id of ids)
      await expect(
        timeline(actor.page).locator(`[data-comment-id="${id}"]`),
      ).toHaveCount(1);
    const times = await timeline(actor.page)
      .getByRole("article")
      .evaluateAll((entries) =>
        entries.map((e) =>
          Date.parse(e.getAttribute("data-occurred-at") ?? ""),
        ),
      );
    expect(times.every(Number.isFinite)).toBe(true);
    // Newest first (g5.workbench.landing); earlier pages load below.
    expect(times).toEqual([...times].sort((a, b) => b - a));
    await actor.page
      .getByLabel("Timeline phase filter")
      .selectOption({ label: "Business Engineering" });
    await expect(timeline(actor.page)).toContainText(
      "Paged review discussion 00",
    );
    await expect(timeline(actor.page)).not.toContainText(
      "Paged review discussion 64",
    );
    await actor.page.reload();
    await expect(actor.page.getByLabel("Timeline phase filter")).toHaveValue(
      "be",
    );
  }
});

test.describe(() => {
  test.describe.configure({ timeout: 900_000 });
  test("RV9 retained history pagination carries unchanged repository members across every recorded push @long", async ({
    review: f,
  }) => {
    await submitPr(f);
    const revisions = [f.pushed];
    // Cross the first page with actual coordinated pushes, not fabricated rows.
    for (let index = 0; index < 26; index++)
      revisions.push(
        await revise(f, index % 2 ? "code" : "docs", `History page ${index}`),
      );
    await stop(f);
    for (const actor of [f.owner, f.visitor]) {
      const rows: VectorRevision[] = [];
      let cursor: string | null = null;
      for (let page = 0; page < 5; page++) {
        const history = await api(
          actor.page,
          `/api/v1/workspaces/${f.workspace.id}/review-flow/history${cursor ? `?before=${encodeURIComponent(cursor)}` : ""}`,
        );
        expect(history.status).toBe(200);
        rows.push(...history.body.data.rows);
        cursor = history.body.data.nextCursor;
        if (page === 0) expect(cursor).toBeTruthy();
        if (!cursor) break;
      }
      expect(cursor).toBeNull();
      // Ancestor snapshots retain their original membership. These pushes
      // belong to the child and must carry both of its repository members.
      const recorded = rows.filter((row) => row.workspaceId === f.workspace.id);
      for (const revision of revisions) {
        const matches = recorded.filter((row) =>
          Object.entries(revision).every(([repoId, sha]) =>
            row.heads.some(
              (head) => head.repoId === repoId && head.headCommit === sha,
            ),
          ),
        );
        expect(matches).toHaveLength(1);
        expect(vector(matches[0].heads)).toEqual(revision);
      }
    }
  });
});

test("RV5/RV10 a lost feedback response retains text and retry creates one canonical thread, with no formal checkpoint", async ({
  review: f,
}) => {
  await submitPr(f);
  await request(f);
  const before = await requirements(f);
  await approvals(f.visitor, f);
  await f.visitor.page
    .getByRole("button", { name: "Add workspace comment", exact: true })
    .click();
  const dialog = f.visitor.page.getByRole("dialog", {
    name: "Add workspace comment",
    exact: true,
  });
  const text = "Retry visitor feedback without duplicating it";
  await dialog.getByLabel("Comment", { exact: true }).fill(text);
  let savedId = "";
  await interceptNextPost(
    f.visitor.page,
    `**/api/v1/workspaces/${f.workspace.id}/comments`,
    async (route) => {
      const saved = await route.fetch();
      expect(saved.status()).toBe(201);
      savedId = String((await saved.json()).data.id);
      await route.abort("failed");
    },
  );
  await dialog.getByRole("button", { name: "Post comment" }).click();
  await expect(dialog.getByRole("alert")).toBeVisible();
  await expect(dialog.getByLabel("Comment", { exact: true })).toHaveValue(text);
  expect(savedId).toBeTruthy();
  await dialog.getByRole("button", { name: "Post comment" }).click();
  await expect(dialog).toHaveCount(0);
  for (const actor of [f.owner, f.visitor]) {
    await actor.page.reload();
    const entry = await timelineEntry(actor, f, text);
    await expect(entry).toHaveAttribute("data-comment-id", savedId);
  }
  const comments = await api(
    f.owner.page,
    `/api/v1/workspaces/${f.workspace.id}/comments`,
  );
  expect(comments.status).toBe(200);
  expect(JSON.stringify(comments.body).split(text)).toHaveLength(2);
  expect(await requirements(f)).toEqual(before);
});

test("RV1/RV10 failed timeline and history reads show retry, not a misleading empty or successful state", async ({
  review: f,
}) => {
  await submitPr(f);
  await request(f);
  await f.visitor.page.route(
    `**/api/v1/workspaces/${f.workspace.id}/activity*`,
    (route) => route.abort("failed"),
  );
  await landing(f.visitor, f);
  await f.visitor.page.reload();
  await expect(
    f.visitor.page.getByTestId("workspace-activity-error"),
  ).toBeVisible();
  await expect(
    f.visitor.page.getByTestId("workspace-activity-empty"),
  ).toHaveCount(0);
  await f.visitor.page.unroute(
    `**/api/v1/workspaces/${f.workspace.id}/activity*`,
  );
  await f.visitor.page
    .getByTestId("workspace-activity-error")
    .getByRole("button", { name: /retry/i })
    .click();
  await expect(timeline(f.visitor.page)).toContainText(f.security.name);
  await f.visitor.page.route(
    `**/api/v1/workspaces/${f.workspace.id}/source-control/history`,
    (route) => route.abort("failed"),
  );
  await contentView(f.visitor, f, "source-control", "All files");
  const history = f.visitor.page
    .getByTestId("source-control-menu")
    .locator("details.source-control-history")
    .first();
  await history.locator("summary").click();
  await expect(history.getByRole("alert")).toBeVisible();
  await f.visitor.page.unroute(
    `**/api/v1/workspaces/${f.workspace.id}/source-control/history`,
  );
  await history.getByRole("button", { name: /retry/i }).click();
  await expect(history.getByRole("alert")).toHaveCount(0);
  await expect(history.locator(".source-control-option").first()).toBeVisible();
});
