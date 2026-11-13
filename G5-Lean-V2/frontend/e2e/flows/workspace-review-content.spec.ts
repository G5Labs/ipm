import { createHash, randomUUID } from "node:crypto";
import type { VectorRevision } from "../../src/lib/types";
import {
  test,
  expect,
  api,
  submitPr,
  setPhase,
  request,
  decide,
  assertDecision,
  revise,
  stop,
  openDiff,
  contentView,
  landing,
  timeline,
  timelineEntry,
  write,
  commit,
  push,
  vectorTable,
  repositoryRows,
  vector,
  reviewDialog,
  requirements,
  shell,
  interceptNextPost,
} from "../workspace-review";
import {
  openWorkspacePath,
  openFileSource,
  expandRepositoryFileRoot,
  readWorkspaceVm,
  runWorkspaceVmAction,
  waitForWorkspaceVmStatus,
  waitForLiveWorkspaceChanges,
} from "../helpers";
import { matrix } from "../matrix";

test.describe.configure({ timeout: 420_000 });

for (const [role, views] of matrix<
  ["owner" | "reviewer", ("source-control" | "docs" | "files")[]]
>(
  [
    ["owner", ["source-control", "docs", "files"]],
    ["reviewer", ["source-control", "docs", "files"]],
  ],
  [
    ["owner", ["source-control"]],
    // reviewer/files is the only variant that exercises the failed-save
    // recovery branch (draft kept, saved comment id reused on retry).
    ["reviewer", ["docs", "files"]],
  ],
)) {
  for (const view of views) {
    test(`RV4 ${role} reviews a ${view} diff, comments, follows its timeline anchor and sees the owner's reply after reload`, async ({
      review: f,
    }) => {
      await submitPr(f);
      await request(f);
      await stop(f);
      await setPhase(
        f,
        view === "docs" ? "be" : view === "source-control" ? "se" : "ie",
      );
      const actor = f[role];
      const path = view === "docs" ? f.docPath : f.codePath;
      const repoId = view === "docs" ? f.docsRepo : f.codeRepo;
      const content =
        view === "docs" ? "# Review document\nfirst pushed docs\n" : f.after;
      const diff = await openDiff(actor, f, view, path);
      await expect(diff.getByTestId("diff-after")).toContainText(
        view === "docs" ? "first pushed docs" : "after review",
      );
      if (view !== "docs")
        await expect(diff.getByTestId("diff-before")).toContainText(
          "before review",
        );
      await diff
        .getByRole("button", { name: "Comment on added line 2", exact: true })
        .click();
      const text = `Explain this ${view} change ${randomUUID()}`;
      await diff.getByLabel("Comment", { exact: true }).fill(text);
      const posted = actor.page.waitForResponse(
        (r) =>
          r.request().method() === "POST" &&
          r.url().endsWith(`/workspaces/${f.workspace.id}/comments`),
      );
      await diff.getByTestId("draft-comment-save").click();
      const response = await posted;
      expect(response.status()).toBe(201);
      expect(response.request().postDataJSON()).toMatchObject({
        scope: "line",
        repoId,
        filePath: path,
        lineNumber: 2,
        commitSha: f.pushed[repoId],
        contentHash: createHash("sha256").update(content).digest("hex"),
      });
      const id = String((await response.json()).data.id);
      expect(id).toBeTruthy();
      const entry = await timelineEntry(f.owner, f, text);
      await expect(entry).toHaveAttribute("data-comment-id", id);
      await entry.getByRole("button", { name: "Reply", exact: true }).click();
      await f.owner.page
        .getByLabel("Write a reply")
        .fill("It preserves the workspace review boundary");
      const replied = f.owner.page.waitForResponse(
        (r) =>
          r.request().method() === "POST" &&
          r.url().endsWith(`/comments/${id}/replies`),
      );
      await f.owner.page
        .getByRole("button", { name: "Post reply", exact: true })
        .click();
      expect((await replied).status()).toBe(201);
      await actor.page.reload();
      const readerEntry = await timelineEntry(actor, f, text);
      await readerEntry
        .getByRole("button", {
          name: new RegExp(path.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")),
        })
        .click();
      const thread = actor.page.getByTestId(`comment-thread-${id}`);
      await expect(thread).toContainText(text);
      await expect(thread).toContainText(
        "It preserves the workspace review boundary",
      );
      await expect(thread.getByTestId("comment-thread-anchor")).toHaveText(
        "L2",
      );
      // Reload the anchor URL rather than relying on transient selected-file state.
      const deepLink = actor.page.url();
      await actor.page.goto(deepLink);
      await expect(
        actor.page.getByTestId(`comment-thread-${id}`),
      ).toContainText(text);
      const replies = await api(
        f.visitor.page,
        `/api/v1/comments/${id}/replies`,
      );
      expect(replies.status).toBe(200);
      expect(
        JSON.stringify(replies.body).match(
          /It preserves the workspace review boundary/g,
        ),
      ).toHaveLength(1);
      expect(
        (await requirements(f)).flatMap((r) => r.decisionLog),
      ).toHaveLength(0);
      // The native document/file reader must share the same comment system,
      // including for a non-owner. A diff-only implementation is insufficient.
      if (view !== "source-control") {
        if (actor !== f.owner) await landing(f.owner, f);
        await contentView(actor, f, view, "All files");
        if (view === "docs") {
          await actor.page
            .getByTestId("sidebar-docs-tab")
            .getByTestId("doc-item")
            .filter({ hasText: path.split("/").at(-1)! })
            .click();
        } else {
          await expandRepositoryFileRoot(actor.page, repoId);
          await actor.page
            .getByTestId("sidebar-files-tab")
            .getByTestId("file-tree-entry")
            .filter({ hasText: path })
            .click();
        }
        const editor = await openFileSource(actor.page, false);
        await expect(editor).toHaveAttribute("data-file-path", path);
        const firstLine = editor.locator(".monaco-editor .view-line").first();
        await expect(firstLine).toContainText(content.split("\n")[0]);
        const commentGlyph = editor.locator(".g5-monaco-add-comment").first();
        // Wait for Monaco's model and hover handlers, as the native file
        // reader journey does; its first DOM row alone is not readiness.
        await expect(async () => {
          await firstLine.hover({ position: { x: 4, y: 10 } });
          await expect(commentGlyph).toBeVisible({ timeout: 1_000 });
        }).toPass({ timeout: 15_000, intervals: [250] });
        await commentGlyph.click();
        const nativeText = `Native ${view} question ${randomUUID()}`;
        await actor.page
          .getByLabel("Comment", { exact: true })
          .fill(nativeText);
        let savedNativeId: string | undefined;
        if (role === "reviewer" && view === "files") {
          await interceptNextPost(
            actor.page,
            `**/api/v1/workspaces/${f.workspace.id}/comments`,
            async (route) => {
              const saved = await route.fetch();
              expect(saved.status()).toBe(201);
              savedNativeId = String((await saved.json()).data.id);
              await route.abort("failed");
            },
          );
          await actor.page.getByTestId("draft-comment-save").click();
          await expect(
            actor.page.getByTestId("draft-comment-error"),
          ).toBeVisible();
          await expect(
            actor.page.getByLabel("Comment", { exact: true }),
          ).toHaveValue(nativeText);
          expect(savedNativeId).toBeTruthy();
        }
        const nativePosted = actor.page.waitForResponse(
          (r) =>
            r.request().method() === "POST" &&
            r.url().endsWith(`/workspaces/${f.workspace.id}/comments`),
        );
        await actor.page.getByTestId("draft-comment-save").click();
        const nativeResponse = await nativePosted;
        expect(nativeResponse.status()).toBe(201);
        expect(nativeResponse.request().postDataJSON()).toMatchObject({
          repoId,
          filePath: path,
          commitSha: f.pushed[repoId],
          contentHash: createHash("sha256").update(content).digest("hex"),
        });
        const nativeId = String((await nativeResponse.json()).data.id);
        if (savedNativeId) {
          expect(nativeId).toBe(savedNativeId);
          const persisted = await api(
            actor.page,
            `/api/v1/workspaces/${f.workspace.id}/comments?${new URLSearchParams({ repoId, filePath: path })}`,
          );
          expect(persisted.status).toBe(200);
          expect(
            persisted.body.data.filter(
              (c: { content: string }) => c.content === nativeText,
            ),
          ).toHaveLength(1);
        }
        if (actor !== f.owner) {
          // The owner's already-open timeline updates from the native editor
          // in another browser context, without a reload or navigation.
          await expect(
            timeline(f.owner.page)
              .getByRole("article")
              .filter({ hasText: nativeText }),
          ).toHaveAttribute("data-comment-id", nativeId);
        }
        const nativeEntry = await timelineEntry(f.owner, f, nativeText);
        await expect(nativeEntry).toHaveAttribute("data-comment-id", nativeId);
        await actor.page.reload();
        await expect(await timelineEntry(actor, f, nativeText)).toHaveAttribute(
          "data-comment-id",
          nativeId,
        );
      }
      expect((await readWorkspaceVm(f.owner.page, f.workspace.id)).status).toBe(
        "stopped",
      );
    });
  }
}

for (const running of matrix([true, false], [false])) {
  test(`RV8 ${running ? "running" : "stopped"}: owner comparisons distinguish dirty, unpushed, base and parent; readers never get current code or docs`, async ({
    review: f,
  }) => {
    await submitPr(f);
    const unpushedPath = `unpushed-${randomUUID()}.txt`;
    const dirtyPath = `dirty-${randomUUID()}.txt`;
    await write(
      f.owner.page,
      f.workspace,
      f.codeRepo,
      unpushedPath,
      "private committed code\n",
    );
    await commit(
      f.owner.page,
      f.workspace,
      [{ repoId: f.codeRepo, path: unpushedPath }],
      "Unpushed owner code",
    );
    await write(
      f.owner.page,
      f.workspace,
      f.codeRepo,
      dirtyPath,
      "private dirty code\n",
    );
    await write(
      f.owner.page,
      f.workspace,
      f.docsRepo,
      f.docPath,
      "# PRIVATE dirty document\n",
    );
    if (!running) await stop(f);
    const beforeVm = await readWorkspaceVm(f.owner.page, f.workspace.id);
    for (const actor of [f.owner, f.reviewer, f.visitor]) {
      await contentView(actor, f, "source-control", "All files");
      const menu = actor.page.getByTestId("source-control-menu");
      const mayReadLive = actor === f.owner && running;
      const unpushedRows = menu.getByRole("checkbox", {
        name: /^Not yet pushed in /,
      });
      await expect(unpushedRows).toHaveCount(2);
      for (const row of await unpushedRows.all()) {
        if (mayReadLive) {
          await expect(row).toBeEnabled();
          await expect(row).toBeChecked();
        } else {
          await expect(row).toBeDisabled();
          await expect(row).not.toBeChecked();
        }
      }
      await expect(
        actor.page.getByLabel("File view", { exact: true }),
      ).toHaveCount(0);
      await menu.getByRole("button", { name: "Show", exact: true }).click();
      const stream = actor.page.getByTestId("source-control-diff-stream");
      await expect(stream).toContainText("after review");
      if (mayReadLive) {
        await expect(stream).toContainText("private dirty code");
        await expect(stream).toContainText("private committed code");
        await expect(stream).toContainText("PRIVATE dirty document");
      } else {
        await expect(stream).not.toContainText(dirtyPath);
        await expect(stream).not.toContainText(unpushedPath);
        await expect(stream).not.toContainText("PRIVATE dirty document");
      }
      if (actor !== f.owner) {
        for (const [repoId, path, expected] of [
          [f.codeRepo, f.codePath, f.after],
          [f.docsRepo, f.docPath, "# Review document\nfirst pushed docs\n"],
        ]) {
          for (const query of [
            "",
            "&source=live",
            "&mode=uncommitted",
            "&mode=unpushed",
          ]) {
            const r = await api(
              actor.page,
              `/api/v1/workspaces/${f.workspace.id}/files/${path}?repoId=${repoId}${query}`,
            );
            if (r.status === 200) {
              expect(r.body.data.content).toBe(expected);
              expect(r.body.data.repoOid).toBe(f.pushed[repoId]);
              expect(r.body.data.source).not.toBe("live");
            } else expect([400, 403]).toContain(r.status);
          }
        }
        for (const path of [dirtyPath, unpushedPath]) {
          const r = await api(
            actor.page,
            `/api/v1/workspaces/${f.workspace.id}/files/${path}?repoId=${f.codeRepo}`,
          );
          expect([403, 404], JSON.stringify(r.body)).toContain(r.status);
        }
        await openWorkspacePath(
          actor.page,
          `${f.workspace.path}?view=files&mode=uncommitted`,
        );
        await expect(actor.page.getByTestId("center-panel")).not.toContainText(
          "private dirty code",
        );
      }
    }
    const afterVm = await readWorkspaceVm(f.owner.page, f.workspace.id);
    expect({
      status: afterVm.status,
      vmId: afterVm.vmId,
      epoch: afterVm.epoch,
    }).toEqual({
      status: beforeVm.status,
      vmId: beforeVm.vmId,
      epoch: beforeVm.epoch,
    });
  });
}

test("RV8 saved base remains stable when parent advances; the Ontology count opens the richer tools", async ({
  review: f,
}) => {
  await landing(f.owner, f);
  // The saved-base baseline is implicit; parent advancement shows only when
  // it happened (g5.workbench.landing.summary).
  await expect(f.owner.page.getByTestId("landing-summary")).toBeVisible();
  await expect(f.owner.page.getByTestId("landing-summary")).not.toContainText(
    /workspace base/i,
  );
  await expect(
    f.owner.page.getByTestId("landing-summary-parent-advanced"),
  ).toHaveCount(0);
  const counts = await f.owner.page
    .getByTestId("workspace-change-summary")
    .getByRole("link")
    .allTextContents();
  await stop(f);
  await runWorkspaceVmAction(f.owner.page, f.parent.id, "resume");
  await waitForWorkspaceVmStatus(f.owner.page, f.parent.id, "running");
  const parentPath = "parent-advanced.md";
  await write(
    f.owner.page,
    f.parent,
    f.docsRepo,
    parentPath,
    "parent only docs\n",
  );
  await commit(
    f.owner.page,
    f.parent,
    [{ repoId: f.docsRepo, path: parentPath }],
    "Parent docs advanced",
  );
  await push(f.owner.page, f.parent);
  for (const actor of [f.owner, f.visitor]) {
    await landing(actor, f);
    await expect(
      actor.page.getByTestId("workspace-change-summary").getByRole("link"),
    ).toHaveText(counts);
    await expect(
      actor.page.getByTestId("landing-summary-parent-advanced"),
    ).toHaveCount(0);
    await expect(
      actor.page.getByRole("link", {
        name: "Compare with parent",
        exact: true,
      }),
    ).toHaveCount(0);
    const filesSummary = actor.page.getByTestId("landing-summary-changed");
    await expect(filesSummary).toHaveAttribute("href", /view=source-control$/);
    await filesSummary.click();
    await expect(actor.page).not.toHaveURL(/scope=|heads=/);
    await expect(
      actor.page.getByTestId("source-control-diff-stream"),
    ).toContainText("after review");
    await contentView(actor, f, "files", "Pushed vs parent");
    await expect(
      actor.page
        .getByTestId("review-content-file")
        .filter({ hasText: parentPath }),
    ).toHaveCount(1);
  }
  await runWorkspaceVmAction(f.owner.page, f.parent.id, "stop");
  await waitForWorkspaceVmStatus(f.owner.page, f.parent.id, "stopped");
  await runWorkspaceVmAction(f.owner.page, f.workspace.id, "resume");
  await waitForWorkspaceVmStatus(f.owner.page, f.workspace.id, "running");
  const ontologyPath = ".g5/ontology/review-acceptance.g5.md";
  await write(
    f.owner.page,
    f.workspace,
    f.codeRepo,
    ontologyPath,
    '# Review acceptance\n\n<a id="review.evidence"></a>\n[I · review.evidence]\n\nKeep workspace review evidence together.\n\n## Metadata\n\n### review.evidence\n\n- agent\n',
  );
  await commit(
    f.owner.page,
    f.workspace,
    [{ repoId: f.codeRepo, path: ontologyPath }],
    "Add ontology intent",
  );
  await push(f.owner.page, f.workspace);
  for (const actor of [f.owner, f.visitor]) {
    await landing(actor, f);
    await actor.page.getByTestId("landing-summary-ontology").click();
    const graph = actor.page.getByTestId("source-control-change-graph");
    await expect(graph).toBeVisible();
    // Current-format ontology (.g5/ontology/features/diff_graph.g5.md): the
    // card names the page without its root and suffix.
    const artifact = graph.getByRole("button", {
      name: /^review-acceptance, Ontology, Added/,
    });
    await expect(artifact).toHaveAttribute("data-column", "ontology");
    await artifact.click();
    // The richer tool first names the file; selecting it loads its semantic
    // evidence. Verify that evidence for both owner and pushed-only reader.
    await expect(graph.getByTestId("change-graph-node-details")).toContainText(
      ontologyPath,
    );
    await expect(graph.getByTestId("change-graph-semantic-diff")).toContainText(
      "Keep workspace review evidence together.",
    );
    await expect(
      actor.page.getByRole("button", { name: /Return to workspace/i }),
    ).toHaveCount(0);
  }
});

test("RV6/RV4 request changes → reply → docs-only, code-only and combined pushes → independent last-reviewed checkpoints", async ({
  review: f,
}) => {
  await submitPr(f);
  const [initial] = await request(f);
  await request(f, f.business);
  await decide(
    f.reviewer,
    f,
    f.security,
    "se",
    "Request changes",
    "Please document rollback",
  );
  await assertDecision(
    f,
    initial,
    f.reviewer,
    "rejected",
    f.pushed,
    "Please document rollback",
  );
  const entry = await timelineEntry(f.owner, f, "Please document rollback");
  await entry.getByRole("button", { name: "Reply", exact: true }).click();
  await f.owner.page
    .getByLabel("Write a reply")
    .fill("Rollback is now documented");
  await f.owner.page
    .getByRole("button", { name: "Post reply", exact: true })
    .click();
  let previous = f.pushed;
  for (const kind of ["docs", "code", "both"] as const) {
    const next = await revise(f, kind, `${kind} checkpoint`);
    for (const repoId of Object.keys(previous)) {
      if (
        (repoId === f.docsRepo && kind === "code") ||
        (repoId === f.codeRepo && kind === "docs")
      )
        expect(next[repoId]).toBe(previous[repoId]);
    }
    await contentView(f.reviewer, f, "source-control", "Since my last review");

    await vectorTable(
      f.reviewer.page.getByTestId("comparison-before"),
      previous,
    );
    await vectorTable(f.reviewer.page.getByTestId("comparison-after"), next);
    // Another type and another person have no checkpoint; neither borrows this one.
    await openWorkspacePath(
      f.reviewer.page,
      `${f.workspace.path}?view=source-control&mode=last_review&reviewPath=&definitionId=${f.business.id}&phase=se`,
    );
    await expect(
      f.reviewer.page.getByRole("status").filter({
        hasText: "No review has been submitted for this type yet.",
      }),
    ).toBeVisible();
    await contentView(f.second, f, "source-control", "Since my last review");
    await expect(
      f.second.page
        .getByRole("status")
        .filter({ hasText: "No review has been submitted for this type yet." }),
    ).toBeVisible();
    await request(f); // Reuses an existing pending request or admits the next addressed request.
    const pending = (await requirements(f))
      .find((r) => r.definition.id === f.security.id)!
      .requests.find(
        (r) => r.actorUserId === f.reviewer.id && r.state === "pending",
      )!;
    await decide(
      f.reviewer,
      f,
      f.security,
      "se",
      "Approve",
      `${kind} revision verified`,
    );
    await assertDecision(
      f,
      pending.requestId,
      f.reviewer,
      "approved",
      next,
      `${kind} revision verified`,
    );
    previous = next;
  }
  await f.reviewer.page.reload();
  await contentView(f.reviewer, f, "files", "Since my last review");
  await expect(f.reviewer.page.getByTestId("review-content-file")).toHaveCount(
    0,
  );
});

test("RV7 a new push cannot silently retarget an open review; submission records the displayed complete vector", async ({
  review: f,
}) => {
  await submitPr(f);
  const [id] = await request(f);
  const dialog = await reviewDialog(f.reviewer, f);
  await dialog
    .getByLabel("Review comment")
    .fill("Reviewed the original pushed snapshot");
  await vectorTable(dialog, f.pushed);
  const newer = await revise(f, "docs", "Concurrent docs push");
  expect(newer[f.codeRepo]).toBe(f.pushed[f.codeRepo]);
  expect(newer[f.docsRepo]).not.toBe(f.pushed[f.docsRepo]);
  await expect(dialog.getByRole("status")).toContainText(/new push/i);
  await expect(dialog.getByLabel("Review comment")).toHaveValue(
    "Reviewed the original pushed snapshot",
  );
  await vectorTable(dialog, f.pushed);
  await dialog
    .getByRole("button", { name: "Submit review", exact: true })
    .click();
  await expect(dialog).toHaveCount(0);
  await assertDecision(
    f,
    id,
    f.reviewer,
    "approved",
    f.pushed,
    "Reviewed the original pushed snapshot",
  );
  const event = await timelineEntry(
    f.owner,
    f,
    "Reviewed the original pushed snapshot",
  );
  await expect(event).toContainText(/changed since|newer push/i);
});

test("RV9 the landing shows the PR's recorded vector; retained history keeps inherited revisions, push times and adjacent comparisons", async ({
  review: f,
}) => {
  await submitPr(f);
  const previous = f.pushed;
  const pushedAfter = Date.now();
  const latest = await revise(f, "docs", "History docs push");
  const pushedBefore = Date.now();
  await write(
    f.owner.page,
    f.workspace,
    f.codeRepo,
    "private-history.txt",
    "not part of pushed history\n",
  );
  await commit(
    f.owner.page,
    f.workspace,
    [{ repoId: f.codeRepo, path: "private-history.txt" }],
    "Private unpushed history",
  );
  for (const actor of [f.owner, f.reviewer, f.visitor]) {
    let recordedHeads = latest;
    // The landing's repository rows are the PR's recorded vector, not a
    // separate pushed-revision popup (g5.workbench.review-prototype).
    // The push above may still be refreshing the open PR, so read the
    // record before the landing and retry until both settle on one vector.
    await expect(async () => {
      const recorded = await api(
        actor.page,
        `/api/v1/workspaces/${f.workspace.id}/pull-requests?role=source&status=open`,
      );
      expect(recorded.status).toBe(200);
      recordedHeads = vector(recorded.body.data[0].sourceHeads);
      await landing(actor, f);
      await repositoryRows(
        actor.page.getByTestId("landing-implementation"),
        recordedHeads,
      );
    }).toPass({ timeout: 120_000 });
    await expect(
      actor.page.getByRole("button", { name: "PR submitted", exact: true }),
    ).toHaveCount(0);
    await expect(
      actor.page.getByTestId("workspace-commit-history"),
    ).toHaveCount(0);
    await actor.page.getByTestId("workspace-read-implementation").click();
    const reader = actor.page.getByRole("dialog", {
      name: "Implementation description",
      exact: true,
    });
    await repositoryRows(reader, recordedHeads);
    await expect(
      reader.getByText("Commit History", { exact: true }),
    ).toHaveCount(0);
    await actor.page.keyboard.press("Escape");
    // Removing the landing shortcut does not change retained revision evidence
    // or immutable comparison links consumed by existing reviews/comments.
    const history = await api(
      actor.page,
      `/api/v1/workspaces/${f.workspace.id}/review-flow/history`,
    );
    expect(history.status).toBe(200);
    const rows = history.body.data.rows as VectorRevision[];
    const current = rows.find(
      (row) =>
        row.heads.length === Object.keys(latest).length &&
        row.heads.every((head) => latest[head.repoId] === head.headCommit),
    );
    expect(current).toBeTruthy();
    const time = current!.pushedAt[f.docsRepo];
    expect(Date.parse(time)).toBeGreaterThanOrEqual(pushedAfter - 5_000);
    expect(Date.parse(time)).toBeLessThanOrEqual(pushedBefore + 5_000);
    expect(rows.some((row) => row.workspaceId !== f.workspace.id)).toBe(true);
    expect(JSON.stringify(rows)).not.toContain("Private unpushed history");
    await openWorkspacePath(
      actor.page,
      `${f.workspace.path}?view=source-control&mode=history&revisionId=${encodeURIComponent(current!.id)}`,
    );
    await vectorTable(actor.page.getByTestId("comparison-before"), previous);
    await vectorTable(actor.page.getByTestId("comparison-after"), latest);
    await expect(
      actor.page
        .getByTestId("workspace-change-row")
        .filter({ hasText: f.codePath }),
    ).toHaveCount(0);
    await expect(
      actor.page
        .getByTestId("workspace-change-row")
        .filter({ hasText: f.docPath }),
    ).toHaveCount(1);
  }
});

test("RV4 renamed and deleted files retain their original discussion instead of attaching it to unrelated current text", async ({
  review: f,
}) => {
  const diff = await openDiff(f.reviewer, f, "files", f.codePath);
  await diff
    .getByRole("button", { name: "Comment on added line 2", exact: true })
    .click();
  await diff
    .getByLabel("Comment", { exact: true })
    .fill("Keep the historical context");
  await diff.getByTestId("draft-comment-save").click();
  await expect(diff).toContainText("Keep the historical context");
  const renamed = `renamed-${f.codePath}`;
  const live = await waitForLiveWorkspaceChanges(f.owner.page, f.workspace.id);
  const root = live.members.find((m) => m.repo_id === f.codeRepo)!.root_path;
  const q = (s: string) => `'${s.replaceAll("'", "'\\''")}'`;
  const git = `git -C ${q(root)}`;
  for (const command of [
    `${git} mv -- ${q(f.codePath)} ${q(renamed)} && ${git} -c user.email=e2e@g5.dev -c user.name=Acceptance commit -qm 'Rename reviewed file'`,
    `${git} rm -- ${q(renamed)} && ${git} -c user.email=e2e@g5.dev -c user.name=Acceptance commit -qm 'Delete reviewed file'`,
  ]) {
    await shell(f.owner.page, f.workspace, command);
    await push(f.owner.page, f.workspace);
    const event = await timelineEntry(
      f.reviewer,
      f,
      "Keep the historical context",
    );
    await event
      .getByRole("button", {
        name: new RegExp(f.codePath.replaceAll(".", "\\.")),
      })
      .click();
    await expect(
      f.reviewer.page.getByTestId("workspace-review-diff"),
    ).toContainText("after review");
    await expect(
      f.reviewer.page.getByTestId("workspace-review-diff"),
    ).toContainText("Keep the historical context");
    await expect(
      f.reviewer.page.getByTestId("workspace-review-diff").getByRole("status"),
    ).toContainText(/historical|renamed|deleted/i);
  }
});

test("RV6 a new PR in the same workspace cannot inherit the reviewer's previous PR checkpoint", async ({
  review: f,
}) => {
  const previousPr = await submitPr(f);
  await request(f);
  await decide(f.reviewer, f, f.security, "se", "Approve", "First PR reviewed");
  const closed = await api(
    f.owner.page,
    `/api/v1/workspaces/${f.workspace.id}/pull-requests/${previousPr}/close`,
    "POST",
    {},
  );
  expect(closed.status, JSON.stringify(closed.body)).toBe(200);
  // The landing keeps presenting the latest PR, now Closed.
  await landing(f.owner, f);
  await expect(
    f.owner.page.getByTestId("landing-implementation-status"),
  ).toHaveText("Closed");
  // Closing a PR removes formal actions for it, not its historical evidence.
  await expect(timeline(f.owner.page)).toContainText("First PR reviewed");
  const currentPr = await submitPr(f);
  expect(currentPr).not.toBe(previousPr);
  await request(f);
  await contentView(f.reviewer, f, "source-control", "Since my last review");
  await expect(
    f.reviewer.page
      .getByRole("status")
      .filter({ hasText: "No review has been submitted for this type yet." }),
  ).toBeVisible();
  await expect(
    (await reviewDialog(f.reviewer, f)).getByLabel("Review comment"),
  ).toHaveValue("");
});
