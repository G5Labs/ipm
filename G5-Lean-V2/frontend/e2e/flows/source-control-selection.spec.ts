/** Production HTTP/dispatcher/native Git acceptance for issue #750. */
import { randomUUID } from "node:crypto";
import {
  test,
  expect,
  api,
  write,
  commit,
  push,
  shell,
  landing,
  stop,
} from "../workspace-review";
import {
  activateSidebarSection,
  waitForLiveWorkspaceChanges,
  openWorkspacePath,
  readWorkspaceVm,
} from "../helpers";
import type { SourceMenu } from "../../src/lib/sourceControlSelection";
import type { SourceSnapshot } from "../../src/lib/sourceControlSelection";
import { gitHead, readAppliedDiff, saveFromChat } from "../source-control";
import { matrix } from "../matrix";
test.describe.configure({ timeout: 420_000 });
for (const [via, choices] of matrix<
  ["agent CLI" | "chat", ("full" | "custom")[]]
>(
  [
    ["agent CLI", ["full", "custom"]],
    ["chat", ["full", "custom"]],
  ],
  [
    ["agent CLI", ["custom"]],
    ["chat", ["full"]],
  ],
))
  for (const choice of choices) {
    test(`Save via ${via} preserves ${choice} selection and the applied diff`, async ({
      review: f,
    }) => {
      const page = f.owner.page,
        ws = f.workspace;
      const agent = await page.context().newPage();
      const initialLive = await waitForLiveWorkspaceChanges(agent, ws.id);
      const root = initialLive.members.find(
        (m) => m.repo_id === f.codeRepo,
      )!.root_path;
      const pushedList: string[] = [];
      for (const marker of [
        "SELECTED PUSHED ONE",
        "EXCLUDED PUSHED TWO",
        "SELECTED PUSHED THREE",
      ]) {
        await write(page, ws, f.codeRepo, f.codePath, `${f.after}${marker}\n`);
        await commit(
          agent,
          ws,
          [{ repoId: f.codeRepo, path: f.codePath }],
          marker,
        );
        pushedList.push(await gitHead(agent, ws, root));
      }
      await push(page, ws);
      const file = `checkpoint-${randomUUID()}.txt`;
      const unrelated = `unrelated-${randomUUID()}.txt`;
      await write(
        page,
        ws,
        f.codeRepo,
        f.codePath,
        `${f.after}SELECTED LOCAL COMMIT\n`,
      );
      await commit(
        agent,
        ws,
        [{ repoId: f.codeRepo, path: f.codePath }],
        "Selected local commit",
      );
      await write(page, ws, f.codeRepo, file, "SELECTED DIRTY FIRST\n");
      await write(page, ws, f.docsRepo, f.docPath, "# SELECTED DOCS DIRTY\n");
      await landing(f.owner, f);
      let menuRequests = 0,
        checks = 0,
        applies = 0;
      page.on("request", (r) => {
        if (r.url().endsWith("/source-control/menu")) menuRequests++;
        if (r.url().endsWith("/source-control/check")) checks++;
        if (r.url().endsWith("/source-control/apply")) applies++;
      });
      const menuResponse = page.waitForResponse(
        (r) => r.status() === 200 && r.url().endsWith("/source-control/menu"),
      );
      await activateSidebarSection(page, "sidebar-source-control-tab");
      const captured = (await (await menuResponse).json()).data as SourceMenu;
      const code = captured.repos.find((r) => r.repoId === f.codeRepo)!;
      const docs = captured.repos.find((r) => r.repoId === f.docsRepo)!;
      expect(code.live).toEqual({ selectable: true });
      const menu = page.getByTestId("source-control-menu");
      await expect(
        menu.getByLabel(`Not yet pushed in ${code.name}`),
      ).toBeChecked();
      if (choice === "custom") {
        for (const c of code.commits)
          if (![pushedList[0], pushedList[2]].includes(c.sha))
            await menu
              .getByLabel(`${c.sha.slice(0, 7)} in ${code.name}`, {
                exact: true,
              })
              .uncheck();
      }
      const firstApply = page.waitForResponse(
        (r) => r.url().endsWith("/source-control/apply") && r.status() === 200,
      );
      await menu.getByRole("button", { name: "Show", exact: true }).click();
      const firstSnapshot = (await (await firstApply).json())
        .data as SourceSnapshot;
      expect(firstSnapshot.fileCount).toBe(3);
      if (choice === "custom") {
        expect(
          firstSnapshot.patches
            .filter((p) => pushedList.includes(p.after ?? ""))
            .sort(
              (a, b) =>
                pushedList.indexOf(a.after ?? "") -
                pushedList.indexOf(b.after ?? ""),
            )
            .map((p) => [p.before, p.after]),
        ).toEqual([
          [f.pushed[f.codeRepo], pushedList[0]],
          [pushedList[1], pushedList[2]],
        ]);
        expect(firstSnapshot.patches).toHaveLength(5);
      }
      const stream = page.getByTestId("source-control-diff-stream");
      const applied = await readAppliedDiff(page);
      expect(applied).toContain("SELECTED DIRTY FIRST");
      const summary = page.getByTestId("source-control-type-summary");
      const originalSummary =
        "Code 2 · Ontology 0 · Docs 1 — 3 matching of 3 changed files";
      await expect(summary).toHaveText(originalSummary);
      const initialLoads = menuRequests;
      // Ordinary edits/commits/pushes must leave the captured menu alone.
      await write(page, ws, f.codeRepo, unrelated, "UNRELATED PUSHED COMMIT\n");
      await commit(
        agent,
        ws,
        [{ repoId: f.codeRepo, path: unrelated }],
        "Unrelated after menu",
      );
      const docsUnrelated = `docs/${ws.id}/new-after-menu.md`;
      await write(
        page,
        ws,
        f.docsRepo,
        docsUnrelated,
        "DOCS FOLLOW NEW COMMIT\n",
      );
      await commit(
        agent,
        ws,
        [{ repoId: f.docsRepo, path: docsUnrelated }],
        "New docs after menu",
      );
      await push(page, ws);
      await write(page, ws, f.codeRepo, file, "SELECTED DIRTY SECOND\n");
      await shell(
        agent,
        ws,
        `git -C '${root.replaceAll("'", "'\\''")}' add -- '${file}'`,
      );
      await write(page, ws, f.codeRepo, file, "SELECTED DIRTY THIRD\n");
      await activateSidebarSection(page, "sidebar-source-control-tab");
      // Type edits are pending too: a real Save must retain this unchecked
      // choice without changing the already applied type summary or bytes.
      await menu.getByRole("checkbox", { name: "Docs", exact: true }).uncheck();
      await expect(summary).toHaveText(originalSummary);
      expect(checks).toBe(0);
      expect(menuRequests).toBe(initialLoads);
      expect(await readAppliedDiff(page)).toBe(applied);
      // Save changes is an agent action; the menu offers only Show.
      await expect(
        menu.getByRole("button", { name: "Save changes", exact: true }),
      ).toHaveCount(0);
      const checkDone = page.waitForResponse(
        (r) => r.status() === 200 && r.url().endsWith("/source-control/check"),
      );
      if (via === "chat") await saveFromChat(agent, ws);
      else await shell(agent, ws, "g5d workspace save");
      await activateSidebarSection(page, "sidebar-source-control-tab");
      const updated = (await (await checkDone).json()).data;
      expect(updated.changed).toBe(true);
      const current = updated.menu as SourceMenu;
      await expect(menu).toContainText(
        "checklist has been updated after Save changes",
      );
      expect(await readAppliedDiff(page)).toBe(applied);
      await expect(summary).toHaveText(originalSummary);
      await expect(
        menu.getByRole("checkbox", { name: "Docs", exact: true }),
      ).not.toBeChecked();
      expect(applies).toBe(1);
      const savedCode = current.repos.find((r) => r.repoId === f.codeRepo)!;
      expect(savedCode.live).toEqual({ selectable: true });
      const savedDocs = current.repos.find((r) => r.repoId === f.docsRepo)!;
      const newDocs = savedDocs.commits.find(
        (c) => c.summary === "New docs after menu",
      )!;
      await expect(
        menu.getByLabel(`${newDocs.sha.slice(0, 7)} in ${docs.name}`, {
          exact: true,
        }),
      ).toBeChecked();
      for (const sha of [pushedList[0], pushedList[2]])
        await expect(
          menu.getByLabel(`${sha.slice(0, 7)} in ${code.name}`, {
            exact: true,
          }),
        ).toBeChecked();
      if (choice === "custom")
        await expect(
          menu.getByLabel(`${pushedList[1].slice(0, 7)} in ${code.name}`, {
            exact: true,
          }),
        ).not.toBeChecked();
      const publishedLocal = savedCode.commits.find(
        (c) => c.summary === "Selected local commit",
      )!;
      await expect(
        menu.getByLabel(`${publishedLocal.sha.slice(0, 7)} in ${code.name}`, {
          exact: true,
        }),
      ).toBeChecked({ checked: choice === "full" });
      const unrelatedCommit = savedCode.commits.find(
        (c) => c.summary === "Unrelated after menu",
      )!;
      const savedCommit = savedCode.commits.find(
        (c) => c.summary === "Save workspace changes",
      )!;
      await expect(
        menu.getByLabel(`${savedCommit.sha.slice(0, 7)} in ${code.name}`, {
          exact: true,
        }),
      ).toBeChecked({ checked: choice === "full" });
      if (choice === "custom")
        await expect(
          menu.getByLabel(
            `${unrelatedCommit.sha.slice(0, 7)} in ${code.name}`,
            { exact: true },
          ),
        ).not.toBeChecked();
      else
        await expect(
          menu.getByLabel(
            `${unrelatedCommit.sha.slice(0, 7)} in ${code.name}`,
            { exact: true },
          ),
        ).toBeChecked();
      await menu.getByRole("button", { name: "Show", exact: true }).click();
      const codeFiles = choice === "full" ? 3 : 1;
      const changedFiles = codeFiles + 2;
      const reappliedSummary = `Code ${codeFiles} · Ontology 0 — ${codeFiles} matching of ${changedFiles} changed files`;
      await expect(summary).toHaveText(reappliedSummary);
      const reapplied = await readAppliedDiff(page);
      if (choice === "full")
        expect(reapplied).toContain("SELECTED DIRTY THIRD");
      else expect(reapplied).not.toContain("SELECTED DIRTY THIRD");
      expect(reapplied).not.toContain("DOCS FOLLOW NEW COMMIT");
      expect(reapplied).not.toContain("SELECTED DOCS DIRTY");
      if (choice === "custom")
        expect(reapplied).not.toContain("UNRELATED PUSHED COMMIT");
      await expect(page.getByTestId("source-control-file")).toHaveCount(
        codeFiles,
      );
      await expect(stream.locator("article")).toHaveCount(codeFiles);
      // A second Save of identical state preserves every checkbox and the
      // displayed snapshot, including repository checked/mixed states.
      const checkboxState = () =>
        menu.getByRole("checkbox").evaluateAll((elements) =>
          elements.map((element) => {
            const input = element as HTMLInputElement;
            return [
              input.getAttribute("aria-label"),
              input.checked,
              input.indeterminate,
            ];
          }),
        );
      const selectedBefore = await checkboxState();
      const cleanCheck = page.waitForResponse(
        (r) => r.status() === 200 && r.url().endsWith("/source-control/check"),
      );
      if (via === "chat") await saveFromChat(agent, ws);
      else await shell(agent, ws, "g5d workspace save");
      await activateSidebarSection(page, "sidebar-source-control-tab");
      expect((await (await cleanCheck).json()).data).toEqual({
        changed: false,
      });
      await expect(menu).not.toContainText(
        "checklist has been updated after Save changes",
      );
      expect(await checkboxState()).toEqual(selectedBefore);
      expect(await readAppliedDiff(page)).toBe(reapplied);
      await expect(summary).toHaveText(reappliedSummary);
      expect(applies).toBe(2);
      expect(menuRequests).toBe(initialLoads);
      if (choice === "custom")
        await expect(
          menu.getByLabel(
            `${unrelatedCommit.sha.slice(0, 7)} in ${code.name}`,
            { exact: true },
          ),
        ).not.toBeChecked();
    });
  }

test("live navigation retains the snapshot and stop withdraws private tabs and late responses", async ({
  review: f,
}) => {
  const page = f.owner.page;
  const privatePath = `private-${randomUUID()}.txt`;
  await write(
    page,
    f.workspace,
    f.codeRepo,
    privatePath,
    Array.from({ length: 100 }, (_, i) => `PRIVATE LIVE LINE ${i}`).join("\n") +
      "\n",
  );
  await write(page, f.workspace, f.docsRepo, f.docPath, "# PRIVATE LIVE DOC\n");
  const loadMenu = async (actor: typeof f.owner) => {
    await openWorkspacePath(actor.page, `${f.workspace.path}?view=activity`);
    await activateSidebarSection(actor.page, "sidebar-source-control-tab");
    const response = await api(
      actor.page,
      `/api/v1/workspaces/${f.workspace.id}/source-control/menu`,
    );
    expect(response.status, JSON.stringify(response.body)).toBe(200);
    return response.body.data as SourceMenu;
  };
  const collaborator = await loadMenu(f.reviewer);
  expect(collaborator.private).toBe(false);
  expect(collaborator.repos.every((r) => r.live === null)).toBe(true);
  const collaboratorUnpushed = f.reviewer.page
    .getByTestId("source-control-menu")
    .getByRole("checkbox", { name: /^Not yet pushed in / });
  await expect(collaboratorUnpushed).toHaveCount(2);
  for (const row of await collaboratorUnpushed.all()) {
    await expect(row).toBeDisabled();
    await expect(row).not.toBeChecked();
  }
  await expect(
    f.reviewer.page.getByTestId("source-control-menu"),
  ).not.toContainText(privatePath);
  const denied = await api(
    f.reviewer.page,
    `/api/v1/workspaces/${f.workspace.id}/files/${privatePath}?repoId=${f.codeRepo}`,
  );
  expect([403, 404]).toContain(denied.status);
  expect((await loadMenu(f.owner)).private).toBe(true);
  const menu = page.getByTestId("source-control-menu");
  await expect(
    menu.getByRole("checkbox", { name: /^Not yet pushed in / }),
  ).toHaveCount(2);
  for (const row of await menu
    .getByRole("checkbox", { name: /^Not yet pushed in / })
    .all())
    await expect(row).toBeChecked();
  await menu.getByRole("button", { name: "Show", exact: true }).click();
  const stream = page.getByTestId("source-control-diff-stream");
  await expect(stream).toContainText("PRIVATE LIVE LINE 99");
  await page
    .getByRole("button", { name: "Choose changes", exact: true })
    .click();
  await expect(menu).toBeVisible();
  await page.keyboard.press("Escape");
  const tree = page.getByRole("navigation", { name: "Changed files" });
  const separator = page.getByRole("separator", {
    name: "Resize changed files",
  });
  const initial = Number(await separator.getAttribute("aria-valuenow"));
  await separator.focus();
  await separator.press("ArrowRight");
  await expect(separator).toHaveAttribute(
    "aria-valuenow",
    String(initial + 10),
  );
  const bounds = await separator.boundingBox();
  await page.mouse.move(bounds!.x + 2, bounds!.y + 20);
  await page.mouse.down();
  await page.mouse.move(bounds!.x + 62, bounds!.y + 20);
  await page.mouse.up();
  expect(Number(await separator.getAttribute("aria-valuenow"))).toBeGreaterThan(
    initial + 10,
  );
  const cards = await stream.locator("article").count();
  await tree.getByRole("button", { name: privatePath, exact: true }).click();
  await expect(stream.locator("article")).toHaveCount(cards);
  const position = await stream.evaluate((el) => el.scrollTop);
  const card = stream.locator("article").filter({ hasText: privatePath });
  await card.getByLabel(`Actions for ${privatePath}`, { exact: true }).click();
  await card.getByRole("button", { name: "View file", exact: true }).click();
  await expect(
    page.getByTestId("center-panel").getByTestId("code-editor"),
  ).toHaveAttribute("data-file-path", privatePath);
  const returnedMenu = page.waitForResponse(
    (response) =>
      response.url().endsWith("/source-control/menu") &&
      response.status() === 200,
  );
  await page.getByRole("tab", { name: "Source Control", exact: true }).click();
  await returnedMenu;
  await expect(
    menu.getByRole("button", { name: "Show", exact: true }),
  ).toBeEnabled();
  await expect(stream).toContainText("PRIVATE LIVE LINE 99");
  await expect.poll(() => stream.evaluate((el) => el.scrollTop)).toBe(position);
  // Explicit apply also returns from the ordinary file reader. Content tabs
  // are optional, so the checklist must provide its own route back.
  await card.getByLabel(`Actions for ${privatePath}`, { exact: true }).click();
  await card.getByRole("button", { name: "View file", exact: true }).click();
  await expect(
    page.getByTestId("center-panel").getByTestId("code-editor"),
  ).toHaveAttribute("data-file-path", privatePath);
  await activateSidebarSection(page, "sidebar-source-control-tab");
  await menu.getByRole("button", { name: "Show", exact: true }).click();
  await expect(
    menu.getByRole("button", { name: "Show", exact: true }),
  ).toBeEnabled();
  await expect(stream).toContainText("PRIVATE LIVE LINE 99");
  expect(new URL(page.url()).pathname).toBe(f.workspace.path);
  let release!: () => void, fetched!: () => void;
  const ready = new Promise<void>((resolve) => {
    fetched = resolve;
  });
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  await page.route(
    `**/api/v1/workspaces/${f.workspace.id}/source-control/apply`,
    async (route) => {
      const response = await route.fetch();
      expect(response.status()).toBe(200);
      fetched();
      await held;
      await route.fulfill({ response });
    },
  );
  await activateSidebarSection(page, "sidebar-source-control-tab");
  try {
    await menu.getByRole("button", { name: "Show", exact: true }).click();
    await ready;
    await stop(f);
    await expect(
      page.getByRole("tab", { name: privatePath, exact: true }),
    ).toHaveCount(0);
    await expect(stream).toHaveCount(0);
  } finally {
    release();
  }
  await expect(page.getByTestId("center-panel")).not.toContainText(
    "PRIVATE LIVE LINE",
  );
  const before = await readWorkspaceVm(page, f.workspace.id);
  for (const actor of [f.owner, f.reviewer]) {
    const stopped = await loadMenu(actor);
    expect(stopped.private).toBe(false);
    expect(stopped.repos.every((r) => r.live === null)).toBe(true);
    const stoppedUnpushed = actor.page
      .getByTestId("source-control-menu")
      .getByRole("checkbox", { name: /^Not yet pushed in / });
    await expect(stoppedUnpushed).toHaveCount(2);
    for (const row of await stoppedUnpushed.all()) {
      await expect(row).toBeDisabled();
      await expect(row).not.toBeChecked();
    }
    await actor.page
      .getByTestId("source-control-menu")
      .getByRole("button", { name: "Show", exact: true })
      .click();
    await expect(
      actor.page.getByTestId("source-control-diff-stream"),
    ).toContainText("after review");
    await expect(
      actor.page.getByTestId("source-control-diff-stream"),
    ).not.toContainText("PRIVATE LIVE");
  }
  const after = await readWorkspaceVm(page, f.workspace.id);
  expect({
    status: after.status,
    vmId: after.vmId,
    epoch: after.epoch,
  }).toEqual({ status: before.status, vmId: before.vmId, epoch: before.epoch });
});

test("ownership transfer withdraws the former owner's private file tab and diff", async ({
  review: f,
}) => {
  const page = f.owner.page;
  const path = `transfer-private-${randomUUID()}.txt`;
  await write(page, f.workspace, f.codeRepo, path, "PRIVATE BEFORE TRANSFER\n");
  await landing(f.owner, f);
  await activateSidebarSection(page, "sidebar-source-control-tab");
  await page
    .getByTestId("source-control-menu")
    .getByRole("button", { name: "Show", exact: true })
    .click();
  const stream = page.getByTestId("source-control-diff-stream");
  await expect(stream).toContainText("PRIVATE BEFORE TRANSFER");
  const card = stream.locator("article").filter({ hasText: path });
  await card.getByLabel(`Actions for ${path}`, { exact: true }).click();
  await card.getByRole("button", { name: "View file", exact: true }).click();
  await expect(
    page.getByTestId("center-panel").getByTestId("code-editor"),
  ).toHaveAttribute("data-file-path", path);
  const transfer = async (from: typeof f.owner, to: typeof f.owner) => {
    const key = randomUUID();
    await expect
      .poll(
        async () => {
          const result = await api(
            from.page,
            `/api/v1/workspaces/${f.workspace.id}/transfer`,
            "POST",
            { toUserId: to.id, idempotencyKey: key },
          );
          expect(result.status, JSON.stringify(result.body)).toBe(200);
          return result.body.data.status;
        },
        { timeout: 120_000, intervals: [1000, 2000] },
      )
      .toBe("completed");
    f.retainedRuntimeParticipants.add(from.id);
    f.retainedRuntimeParticipants.add(to.id);
  };
  await transfer(f.owner, f.visitor);
  try {
    await expect(
      page.getByRole("tab", { name: path, exact: true }),
    ).toHaveCount(0);
    await expect(
      page.getByTestId("center-panel").getByTestId("code-editor"),
    ).toHaveCount(0);
    await page
      .getByRole("tab", { name: "Source Control", exact: true })
      .click();
    await expect(stream).toHaveCount(0);
    await activateSidebarSection(page, "sidebar-source-control-tab");
    await expect(
      page
        .getByTestId("source-control-menu")
        .getByRole("checkbox", { name: /^Not yet pushed in / }),
    ).toHaveCount(2);
    for (const row of await page
      .getByTestId("source-control-menu")
      .getByRole("checkbox", { name: /^Not yet pushed in / })
      .all()) {
      await expect(row).toBeDisabled();
      await expect(row).not.toBeChecked();
    }
    const read = await api(
      page,
      `/api/v1/workspaces/${f.workspace.id}/files/${path}?repoId=${f.codeRepo}`,
    );
    expect([403, 404]).toContain(read.status);
  } finally {
    await transfer(f.visitor, f.owner);
  }
});
