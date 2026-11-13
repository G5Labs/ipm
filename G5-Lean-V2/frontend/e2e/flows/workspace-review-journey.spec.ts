import {
  test,
  expect,
  api,
  landing,
  approvals,
  approvalTimeline,
  submitPr,
  request,
  requestComposer,
  selectHuman,
  selectedPairs,
  previewRequests,
  submitRequests,
  requirements,
  decide,
  assertDecision,
  reviewDialog,
  vector,
  repositoryRows,
  timeline,
  timelineEntry,
  setPhase,
  stop,
  type Phase,
} from "../workspace-review";
import {
  deleteWorkspaceById,
  openWorkspacePath,
  readWorkspaceVm,
  waitForWorkspaceVmStatus,
} from "../helpers";
import { matrix } from "../matrix";

test.describe.configure({ timeout: 420_000 });

for (const [running, phases] of matrix<[boolean, Phase[]]>(
  [
    [true, ["be", "se", "ie"]],
    [false, ["be", "se", "ie"]],
  ],
  [
    [true, ["se"]],
    [false, ["ie"]],
  ],
)) {
  for (const phase of phases) {
    test(`RV1/RV2 ${phase} ${running ? "running" : "stopped"}: owner submits and requests; assigned and unrequested readers see the same landing`, async ({
      review: f,
    }) => {
      await setPhase(f, phase);
      if (!running) await stop(f);
      const before = await readWorkspaceVm(f.owner.page, f.workspace.id);
      for (const actor of [f.owner, f.reviewer, f.visitor]) {
        await landing(actor, f);
        await expect(
          actor.page.getByTestId("landing-implementation-empty"),
        ).toHaveText("No implementation description yet.");
        await expect(
          actor.page.getByTestId(/^phase-approval-meter-/),
        ).toHaveCount(0);
        // Untouched requirements are Pending: the summary counts both
        // requirements once each, never reviewers (g5.workbench.landing.summary,
        // g5.governance.approval.status). Nothing else claims progress.
        await expect(
          actor.page.getByTestId("landing-summary-approvals"),
        ).toContainText("0 approved, 0 rejected, 2 pending");
        await expect(
          timeline(actor.page)
            .getByRole("article")
            .filter({
              hasText: /approved|changes requested|review requested/i,
            }),
        ).toHaveCount(0);
        await expect(
          actor.page.getByRole("button", { name: /^Submit review$/ }),
        ).toHaveCount(0);
        await actor.page.getByTestId("workspace-read-task-description").click();
        const task = actor.page.getByRole("dialog", {
          name: "Workspace task",
          exact: true,
        });
        await expect(task).toContainText("END OF TASK");
        await expect(task.getByRole("button")).toHaveCount(1);
        await actor.page.keyboard.press("Escape");
        await expect(
          actor.page.getByTestId("workspace-read-task-description"),
        ).toBeFocused();
        await approvals(actor, f);
        await expect(
          actor.page.getByTestId("workspace-approvals-phase-filter"),
        ).toHaveCount(0);
        await expect(
          actor.page.getByTestId("ribbon-action-approvals"),
        ).toHaveCount(0);
      }
      // The landing offers no PR action; publication stays with the PR
      // owners (g5.workbench.landing.implementation).
      await landing(f.owner, f);
      await expect(
        f.owner.page.getByRole("button", { name: /Submit PR|Update PR/ }),
      ).toHaveCount(0);
      await submitPr(f);
      for (const actor of [f.owner, f.reviewer, f.visitor]) {
        await landing(actor, f);
        await actor.page.reload();
        await repositoryRows(
          actor.page.getByTestId("landing-implementation"),
          f.pushed,
        );
        await actor.page.getByTestId("workspace-read-implementation").click();
        const dialog = actor.page.getByRole("dialog", {
          name: "Implementation description",
          exact: true,
        });
        await expect(dialog).toContainText("END OF IMPLEMENTATION");
        await expect(dialog).not.toContainText("END OF TASK");
        await expect(dialog.getByRole("button")).toHaveCount(1);
        await actor.page.keyboard.press("Escape");
        await expect(
          actor.page.getByRole("button", { name: /^Submit review$/ }),
        ).toHaveCount(0);
      }
      await approvals(f.owner, f);
      for (const actor of [f.reviewer, f.second]) {
        const { dialog: picker, group } = await requestComposer(f.owner, f);
        await selectHuman(group, actor.name);
        expect(await selectedPairs(picker)).toEqual([
          `${f.security.name} → ${actor.name}`,
        ]);
        await picker
          .getByLabel("Note for reviewers (optional)")
          .fill("Check the rollback plan.");
        await previewRequests(picker);
        await expect(
          picker.getByTestId("approval-request-recipient"),
        ).toHaveText([`${f.security.name} → ${actor.name}`]);
        const requested = f.owner.page.waitForResponse(
          (r) =>
            r.request().method() === "POST" &&
            r
              .url()
              .endsWith(`/workspaces/${f.workspace.id}/approvals/requests`),
        );
        await submitRequests(picker);
        const admitted = await requested;
        expect(admitted.status()).toBe(201);
        expect(admitted.request().postDataJSON()).toMatchObject({
          definitionId: f.security.id,
          note: "Check the rollback plan.",
          actorUserIds: [actor.id],
        });
        // Full success closes the dialog and refreshes the roster; no
        // "Accepted" receipt reads as a decision
        // (g5.governance.approval.request-composer).
        await expect(picker).toHaveCount(0);
        await expect(
          f.owner.page
            .getByTestId("workspace-approvals-requirement")
            .filter({ hasText: f.security.name })
            .getByTestId("workspace-approvals-wait")
            .filter({ hasText: actor.name }),
        ).toContainText("Waiting");
      }
      const requirement = (await requirements(f)).find(
        (r) => r.definition.id === f.security.id,
      )!;
      expect(
        requirement.requests
          .filter((r) => r.state === "pending")
          .map((r) => r.actorUserId)
          .sort(),
      ).toEqual([f.reviewer.id, f.second.id].sort());
      for (const actor of [f.owner, f.reviewer, f.visitor]) {
        await landing(actor, f);
        await expect(timeline(actor.page)).toContainText(f.security.name);
        await expect(timeline(actor.page)).toContainText(f.reviewer.name);
        await approvals(actor, f);
      }
      await expect(
        f.reviewer.page.getByRole("button", {
          name: "Approve",
          exact: true,
        }),
      ).toBeEnabled();
      await expect(
        f.visitor.page.getByRole("button", { name: "Approve", exact: true }),
      ).toHaveCount(0);
      await expect(
        f.visitor.page.getByRole("button", {
          name: "Add workspace comment",
          exact: true,
        }),
      ).toBeEnabled();
      const review = await reviewDialog(f.reviewer, f, f.security, phase);
      await expect(review).toContainText("Check the rollback plan.");
      await f.reviewer.page.keyboard.press("Escape");
      await setPhase(f, phase === "be" ? "se" : "be");
      await approvals(f.owner, f);
      await f.owner.page.reload();
      const group = f.owner.page
        .getByTestId("workspace-approvals-requirement")
        .filter({ hasText: f.security.name });
      await expect(group).toHaveCount(1);
      await expect(group.getByTestId("workspace-approvals-wait")).toHaveCount(
        2,
      );
      await expect(
        group.getByTestId("review-request-status").first(),
      ).toContainText("Waiting");
      await expect(
        group.getByTestId("review-request-note").first(),
      ).toContainText("Check the rollback plan.");
      const after = await readWorkspaceVm(f.owner.page, f.workspace.id);
      expect({
        status: after.status,
        vmId: after.vmId,
        epoch: after.epoch,
      }).toEqual({
        status: before.status,
        vmId: before.vmId,
        epoch: before.epoch,
      });
    });
  }
}

for (const [running, phases] of matrix<[boolean, Phase[]]>(
  [
    [true, ["be", "se", "ie"]],
    [false, ["be", "se", "ie"]],
  ],
  [
    [true, ["be"]],
    [false, ["se"]],
  ],
)) {
  for (const phase of phases) {
    test(`RV3 ${phase} ${running ? "running" : "stopped"}: type and recipient decisions remain independent across phase moves and survive reload`, async ({
      review: f,
    }) => {
      await setPhase(f, phase);
      await submitPr(f);
      if (!running) await stop(f);
      const [r1, r2] = await request(
        f,
        f.security,
        [f.reviewer, f.second],
        phase,
      );
      const [business] = await request(f, f.business, [f.reviewer], phase);
      const otherPhase = phase === "be" ? "ie" : "be";
      await setPhase(f, otherPhase);
      const [same] = await request(f, f.security, [f.reviewer], otherPhase);
      expect(same).toBe(r1);
      const dialog = await reviewDialog(f.reviewer, f, f.business, phase);
      await dialog.getByRole("radio", { name: /^Request changes/ }).check();
      await dialog.getByLabel("Review comment").fill("   ");
      await expect(
        dialog.getByRole("button", { name: "Submit review", exact: true }),
      ).toBeDisabled();
      await f.reviewer.page.keyboard.press("Escape");
      await landing(f.owner, f);
      await decide(
        f.reviewer,
        f,
        f.security,
        phase,
        "Approve",
        "Security boundary verified",
      );
      await assertDecision(
        f,
        r1,
        f.reviewer,
        "approved",
        f.pushed,
        "Security boundary verified",
      );
      await expect(
        timeline(f.owner.page).getByRole("article").filter({
          hasText: "Security boundary verified",
        }),
      ).toHaveCount(1); // Owner sees the decision without reloading the landing.
      const untouched = (await requirements(f))
        .flatMap((r) => r.requests)
        .filter((r) => [r2, business].includes(r.requestId));
      expect(untouched).toHaveLength(2);
      expect(untouched.every((r) => r.state === "pending")).toBe(true);
      await decide(
        f.reviewer,
        f,
        f.business,
        phase,
        "Request changes",
        "Define the business rollback owner",
      );
      await assertDecision(
        f,
        business,
        f.reviewer,
        "rejected",
        f.pushed,
        "Define the business rollback owner",
      );
      await f.reviewer.page.reload();
      await expect(
        f.reviewer.page.getByRole("button", { name: "Approve", exact: true }),
      ).toHaveCount(0);
      const renewed = await api(
        f.owner.page,
        `/api/v1/workspaces/${f.workspace.id}/approvals/requests`,
        "POST",
        {
          definitionId: f.security.id,
          definitionVersion: 1,
          actorUserIds: [f.reviewer.id],
          note: "Please recheck the fix.",
        },
      );
      expect(renewed.status).toBe(201);
      await approvals(f.owner, f);
      await f.owner.page.reload();
      const group = f.owner.page
        .getByTestId("workspace-approvals-requirement")
        .filter({ hasText: f.security.name });
      const current = group
        .getByTestId("workspace-approvals-wait")
        .filter({ hasText: f.reviewer.name });
      await expect(current).toHaveCount(1);
      await expect(current.getByTestId("review-request-status")).toContainText(
        "Waiting",
      );
      await expect(current.getByTestId("review-request-note")).toContainText(
        "Please recheck the fix.",
      );
      await expect(
        await approvalTimeline(f.owner.page, f.security.name),
      ).toContainText("Security boundary verified");
      for (const actor of [f.owner, f.reviewer, f.second, f.visitor]) {
        const entry = await timelineEntry(
          actor,
          f,
          "Define the business rollback owner",
        );
        await expect(entry).toContainText(f.reviewer.name);
        await expect(entry).toContainText(f.business.name);
      }
    });
  }
}

test("RV3/RV7 independent review drafts survive requirement switching, close, navigation and reload", async ({
  review: f,
}) => {
  await submitPr(f);
  await request(f);
  await request(f, f.business);
  let dialog = await reviewDialog(f.reviewer, f, f.security);
  await dialog.getByLabel("Review comment").fill("Security draft is private");
  await f.reviewer.page.keyboard.press("Escape");
  dialog = await reviewDialog(f.reviewer, f, f.business);
  await expect(dialog.getByLabel("Review comment")).toHaveValue("");
  await dialog
    .getByLabel("Review comment")
    .fill("Business draft is independent");
  await dialog.getByRole("radio", { name: /^Request changes/ }).check();
  await f.reviewer.page.keyboard.press("Escape");
  await landing(f.reviewer, f);
  await f.reviewer.page.reload();
  dialog = await reviewDialog(f.reviewer, f, f.security);
  await expect(dialog.getByLabel("Review comment")).toHaveValue(
    "Security draft is private",
  );
  await expect(dialog.getByRole("radio", { name: /^Approve/ })).toBeChecked();
  await f.reviewer.page.keyboard.press("Escape");
  dialog = await reviewDialog(f.reviewer, f, f.business);
  await expect(dialog.getByLabel("Review comment")).toHaveValue(
    "Business draft is independent",
  );
  await expect(
    dialog.getByRole("radio", { name: /^Request changes/ }),
  ).toBeChecked();
  await landing(f.owner, f);
  await expect(timeline(f.owner.page)).not.toContainText("draft is");
  expect((await requirements(f)).flatMap((r) => r.decisionLog)).toHaveLength(0);
});

for (const submitted of matrix([false, true], [false])) {
  test(`RV5 ${submitted ? "submitted PR" : "before PR"}: configured but unrequested visitor leaves feedback and owner replies, without an approval`, async ({
    review: f,
  }) => {
    if (submitted) {
      await submitPr(f);
      await request(f);
    }
    await stop(f);
    const before = await requirements(f);
    await approvals(f.visitor, f);
    await expect(
      f.visitor.page.getByRole("button", { name: "Approve", exact: true }),
    ).toHaveCount(0);
    await f.visitor.page
      .getByRole("button", { name: "Add workspace comment", exact: true })
      .click();
    const dialog = f.visitor.page.getByRole("dialog", {
      name: "Add workspace comment",
      exact: true,
    });
    await expect(dialog.getByRole("radio")).toHaveCount(0);
    await expect(
      dialog.getByRole("button", { name: "Post comment" }),
    ).toBeDisabled();
    await dialog
      .getByLabel("Comment", { exact: true })
      .fill("Please explain the customer impact");
    await dialog.getByRole("button", { name: "Post comment" }).click();
    await expect(dialog).toHaveCount(0);
    const entry = await timelineEntry(
      f.owner,
      f,
      "Please explain the customer impact",
    );
    await entry.getByRole("button", { name: "Reply", exact: true }).click();
    await f.owner.page
      .getByLabel("Write a reply")
      .fill("The customer keeps their saved work");
    await f.owner.page
      .getByRole("button", { name: "Post reply", exact: true })
      .click();
    await f.visitor.page.reload();
    // The comment and its reply are separate Activity rows, each once at its
    // own time; the comment's row opens the complete thread.
    await timelineEntry(f.visitor, f, "Please explain the customer impact");
    const visitorReply = await timelineEntry(
      f.visitor,
      f,
      "The customer keeps their saved work",
    );
    await expect(visitorReply).toContainText("replied");
    expect(await requirements(f)).toEqual(before);
    expect((await readWorkspaceVm(f.owner.page, f.workspace.id)).status).toBe(
      "stopped",
    );
  });
}

for (const running of matrix([true, false], [true])) {
  test(`RV2/RV3/RV10 reviewer quorum and self approval ${running ? "running" : "stopped"}`, async ({
    review: f,
  }) => {
    if (!running) await stop(f);
    await submitPr(f);
    await setPhase(f, "ie");
    await landing(f.owner, f);
    const control = f.owner.page.getByTestId("ribbon-workspace-approvals");
    await expect(control).toBeVisible();
    await expect(f.owner.page.getByTestId("sidebar-approvals-tab")).toHaveCount(
      0,
    );
    await expect(
      f.owner.page.getByTestId("ribbon-end-actions").getByRole("button").last(),
    ).toHaveAttribute("data-testid", "ribbon-workspace-approvals");
    await expect(
      f.owner.page
        .getByTestId("ribbon-trailing")
        .getByTestId("ribbon-workspace-approvals"),
    ).toHaveCount(0);
    await control.click();
    await expect(
      f.owner.page.getByRole("heading", {
        name: /^Approvals Required to merge into /,
      }),
    ).toBeVisible();
    await request(f, f.security, [f.reviewer, f.second], "ie");
    await decide(
      f.reviewer,
      f,
      f.security,
      "ie",
      "Approve",
      "First vote meets the default quorum",
    );
    let rows = await requirements(f);
    const row = rows.find(
      (r) => r.definition.id === f.security.id && r.phase === "ie",
    )!;
    expect(row.definition.requireAllReviewers).toBe(false);
    expect(row.definition.allowSelfApproval).toBe(false);
    expect(row.state).toBe("approved");
    expect(
      row.requests.find(
        (r) => r.actorUserId === f.second.id && r.state === "pending",
      ),
    ).toBeTruthy();
    await decide(
      f.second,
      f,
      f.security,
      "ie",
      "Request changes",
      "A denial vetoes any quorum",
    );
    rows = await requirements(f);
    expect(
      rows.find((r) => r.definition.id === f.security.id && r.phase === "ie")!
        .state,
    ).toBe("rejected");
    const publish = async (all: boolean, self: boolean) => {
      const saved = await api(
        f.owner.page,
        `/api/v1/orgs/${f.orgId}/approval-definitions/${f.security.id}/versions`,
        "POST",
        {
          name: f.security.name,
          humanUserIds: [f.owner.id, f.reviewer.id, f.second.id],
          phases: ["ie"],
          invalidatesPrevious: true,
          requireAllReviewers: all,
          allowSelfApproval: self,
        },
      );
      expect(saved.status, JSON.stringify(saved.body)).toBe(200);
      return saved.body.data;
    };
    const all = await publish(true, false);
    await request(f, all, [f.reviewer, f.second], "ie");
    await decide(
      f.reviewer,
      f,
      all,
      "ie",
      "Approve",
      "All mode waits for the other reviewer",
    );
    expect(
      (await requirements(f)).find(
        (r) => r.definition.id === all.id && r.phase === "ie",
      )!.state,
    ).toBe("pending");
    await decide(
      f.second,
      f,
      all,
      "ie",
      "Approve",
      "Second eligible vote completes all mode",
    );
    expect(
      (await requirements(f)).find(
        (r) => r.definition.id === all.id && r.phase === "ie",
      )!.state,
    ).toBe("approved");
    const self = await publish(false, true);
    await request(f, self, [f.owner], "ie");
    await approvals(f.owner, f);
    await expect(
      f.owner.page.getByTestId("workspace-request-review"),
    ).toBeVisible();
    await expect(
      f.owner.page.getByTestId("workspace-submit-review"),
    ).toBeVisible();
    await decide(
      f.owner,
      f,
      self,
      "ie",
      "Approve",
      "Explicitly permitted self review",
    );
    await f.owner.page.reload();
    await approvals(f.owner, f);
    const finalRows = await requirements(f);
    const finalRow = finalRows.find(
      (r) => r.definition.id === self.id && r.phase === "ie",
    )!;
    expect(finalRow.state).toBe("approved");
    expect(
      finalRow.decisionLog.some(
        (d) => d.actor.userId === f.owner.id && d.outcome === "approved",
      ),
    ).toBe(true);
  });
}

test("RV1/RV5 summary-only published review has zero items and general comments have one entry surface", async ({
  review: f,
}) => {
  await stop(f);
  await landing(f.visitor, f);
  await expect(
    f.visitor.page.getByRole("button", {
      name: "Add workspace comment",
      exact: true,
    }),
  ).toHaveCount(0);
  await approvals(f.visitor, f);
  const before = await requirements(f);
  await expect(
    f.visitor.page.getByTestId("approvals-reviews-toggle"),
  ).toContainText("0 open review items · 0 published reviews");
  await f.visitor.page.getByTestId("review-workspace-start").click();
  await expect(
    f.visitor.page.getByTestId("workspace-review-banner"),
  ).toBeVisible({ timeout: 60_000 });
  const context = await api(
    f.visitor.page,
    `/api/v1/workspaces/${f.workspace.id}/review`,
  );
  expect(context.status, JSON.stringify(context.body)).toBe(200);
  const review = context.body.data.asSource;
  expect(review.reviewWorkspaceId).toBeTruthy();
  // The completed review fork keeps its same-org audit foreign keys after
  // deleting the copy, just as the transfer journey keeps its audit record.
  f.retainedRuntimeParticipants.add(f.visitor.id);
  try {
    await waitForWorkspaceVmStatus(
      f.visitor.page,
      review.reviewWorkspaceId,
      "running",
    );
    await openWorkspacePath(
      f.visitor.page,
      `/${f.workspace.path.split("/")[1]}/${encodeURIComponent(review.reviewWorkspaceName)}?view=reviews`,
    );
    await f.visitor.page.getByTestId("my-review-push").click();
    const dialog = f.visitor.page.getByTestId("push-review-dialog");
    await dialog
      .getByLabel("Summary for the owner", { exact: true })
      .fill("Summary-only review without file edits");
    await dialog.getByTestId("push-review-verdict").selectOption("approve");
    const published = f.visitor.page.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        response
          .url()
          .includes(`/workspaces/${review.reviewWorkspaceId}/review/publish`),
    );
    await dialog.getByTestId("push-review-confirm").click();
    expect((await published).status()).toBe(201);
    await expect(dialog).not.toBeVisible();
    await approvals(f.owner, f);
    await expect(
      f.owner.page.getByTestId("approvals-reviews-toggle"),
    ).toContainText("0 open review items · 1 published review");
    await expect(f.owner.page.getByTestId("reviews-panel")).toContainText(
      "Summary-only review without file edits",
    );
    expect(await requirements(f)).toEqual(before);
    await f.owner.page
      .getByRole("button", { name: "Add workspace comment", exact: true })
      .click();
    const feedback = f.owner.page.getByRole("dialog", {
      name: "Add workspace comment",
      exact: true,
    });
    await feedback
      .getByLabel("Comment", { exact: true })
      .fill("General feedback stays separate from published reviews");
    await feedback
      .getByRole("button", { name: "Post comment", exact: true })
      .click();
    await expect(feedback).not.toBeVisible();
    await f.owner.page.reload();
    await approvals(f.owner, f);
    await expect(
      f.owner.page.getByText(
        "General feedback stays separate from published reviews",
        { exact: true },
      ),
    ).toBeVisible();
    await expect(
      f.owner.page.getByTestId("approvals-reviews-toggle"),
    ).toContainText("0 open review items · 1 published review");
    expect(await requirements(f)).toEqual(before);
  } finally {
    await deleteWorkspaceById(f.visitor.page, review.reviewWorkspaceId);
  }
});
