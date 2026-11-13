import { expect, test } from "@playwright/test";

// The real app/selector with a long illustrative history. Native Git and Save
// correctness live in flows/source-control-*.spec.ts; this catches real layout
// clipping that jsdom's visibility assertions cannot detect.
for (const mode of ["expanded", "rail"] as const) {
  test(`Source Control can apply a long history in the ${mode} sidebar`, async ({
    page,
  }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    await page.addInitScript((sidebarDisplayMode) => {
      localStorage.setItem(
        "g5-user-settings",
        JSON.stringify({
          sidebarDisplayMode,
          showGuidedJourney: true,
          showTabBar: true,
          actionBarStyle: "simplified",
        }),
      );
    }, mode);
    await page.goto("/g5labs/Task%20Graph%20Rework");
    await expect(page.getByTestId("phase-chevrons")).toBeVisible();
    await page.evaluate(() => {
      const previous = window.fetch;
      const repos = ["Code", "Docs"].map((name) => ({
        repoId: name,
        name,
        base: "base",
        pushed: "tip",
        error: null,
        truncated: false,
        live: null,
        commits: Array.from({ length: 60 }, (_, index) => ({
          sha: (index + 1).toString(16).padStart(40, "0"),
          summary: `Retain readable preview output and cover reconnect behavior ${index + 1}`,
          authorName: "Fixture author",
          committedAt: 1_700_000_000,
          parents: ["parent"],
        })),
      }));
      window.fetch = async (input, init) => {
        const url =
          typeof input === "string"
            ? input
            : input instanceof URL
              ? input.href
              : input.url;
        if (url.includes("/source-control/menu"))
          return Response.json({
            data: {
              repos,
              private: false,
              fingerprint: "layout",
              generation: 0,
            },
          });
        if (url.includes("/source-control/apply")) {
          sessionStorage.setItem(
            "source-control-layout-apply",
            String(init?.body),
          );
          return Response.json({
            data: { patches: [], fileCount: 0, private: false },
          });
        }
        return previous(input, init);
      };
    });
    if (mode === "rail") {
      await page.getByTestId("sidebar-rail-source-control").click();
    } else {
      const heading = page
        .getByTestId("sidebar-source-control-tab")
        .getByRole("button", { name: "Source Control", exact: true });
      if ((await heading.getAttribute("aria-expanded")) !== "true")
        await heading.click();
    }
    const menu = page.getByTestId("source-control-menu");
    await expect(
      menu.getByRole("checkbox", { name: "All changes", exact: true }),
    ).toBeChecked();
    await expect(menu.getByRole("checkbox")).toHaveCount(128);
    await expect(
      menu.getByRole("group", { name: "File types" }).getByRole("checkbox"),
    ).toHaveCount(3);
    const show = menu.getByRole("button", {
      name: "Show",
      exact: true,
    });
    // Rail footer is visible without scrolling through 120 commits. A stacked
    // sidebar must allow scrolling to and actually clicking its natural footer.
    if (mode === "expanded") await show.scrollIntoViewIfNeeded();
    await expect(show).toBeInViewport();
    await show.click();
    await expect(
      page.getByRole("heading", { name: "Pushed changes", exact: true }),
    ).toBeVisible();
    const applied = await page.evaluate(() =>
      JSON.parse(sessionStorage.getItem("source-control-layout-apply")!),
    );
    expect(
      applied.repos.map((repo: { repoId: string }) => repo.repoId),
    ).toEqual(["Code", "Docs"]);
    for (const repo of applied.repos) {
      expect(repo.commits).toHaveLength(60);
      expect(repo.includeUnpushed).toBe(false);
    }
  });
}

test("applied file types filter filenames and diffs while Activity stays independent", async ({
  page,
}, testInfo) => {
  await page.addInitScript(() =>
    localStorage.setItem(
      "g5-user-settings",
      JSON.stringify({
        sidebarDisplayMode: "expanded",
        showTabBar: true,
        actionBarStyle: "simplified",
      }),
    ),
  );
  await page.goto("/g5labs/Task%20Graph%20Rework?view=activity");
  await expect(page.getByTestId("phase-chevrons")).toBeVisible();
  await page.evaluate(() => {
    const previous = window.fetch;
    const repos = ["Code", "Docs"].map((name) => ({
      repoId: name,
      name,
      base: "base",
      pushed: "tip",
      error: null,
      truncated: false,
      live: null,
      commits: [
        {
          sha: "1".repeat(40),
          summary: `Refine ${name.toLowerCase()} changes`,
          authorName: "Preview author",
          committedAt: 1700000000,
          parents: ["base"],
        },
      ],
    }));
    const patches = [
      {
        repoId: "Code",
        repoName: "g5-api",
        kind: "code",
        path: "src/workspace.ts",
      },
      {
        repoId: "Code",
        repoName: "g5-api",
        kind: "ontology",
        path: ".g5/ontology/workspace.g5.md",
      },
      {
        repoId: "Docs",
        repoName: "project-docs",
        kind: "docs",
        path: "docs/implementation.md",
      },
    ].map(({ path, ...file }) => ({
      ...file,
      fileKey: path,
      label: "Pushed changes vs workspace base",
      before: "base",
      after: "tip",
      file: {
        path,
        oldPath: null,
        change: "modified",
        diff: {
          status: "text",
          additions: 1,
          deletions: 1,
          hunks: [
            {
              startLine: 1,
              header: "@@ -1 +1 @@",
              lines: [
                { type: "deletion", oldLine: 1, content: `previous ${path}` },
                { type: "addition", newLine: 1, content: `updated ${path}` },
              ],
            },
          ],
        },
      },
    }));
    const heads = repos.map((repo) => ({
      repoId: repo.repoId,
      headCommit: "tip",
      branchName: "preview",
    }));
    window.fetch = async (input, init) => {
      const url =
        typeof input === "string"
          ? input
          : input instanceof URL
            ? input.href
            : input.url;
      if (url.includes("/source-control/menu"))
        return Response.json({
          data: {
            repos,
            private: false,
            fingerprint: "types-preview",
            generation: 0,
          },
        });
      if (url.includes("/source-control/checkpoint"))
        return Response.json({ data: { generation: 0 } });
      if (/\/source-control\/(apply|pushed)$/.test(url)) {
        sessionStorage.setItem(
          "type-preview-applies",
          String(
            Number(sessionStorage.getItem("type-preview-applies") ?? 0) + 1,
          ),
        );
        return Response.json({
          data: { patches, private: false, fileCount: 3 },
        });
      }
      if (url.includes("/review-flow/changes"))
        return Response.json({
          data: {
            baselineAvailable: true,
            afterHeads: heads,
            beforeHeads: [],
            file: null,
            files: patches.map((p) => ({
              repoId: p.repoId,
              path: p.fileKey,
              kind: p.kind,
              status: "modified",
            })),
          },
        });
      if (url.endsWith("/review-flow"))
        return Response.json({
          data: {
            workspace: { ownerUserId: "preview", phase: "se", readOnly: true },
            pr: null,
            heads,
            baseHeads: [],
            parentHeads: [],
            pushedAt: {},
            repos: repos.map((r) => ({
              id: r.repoId,
              name: r.name,
              role: r.repoId.toLowerCase(),
              rootPath: "/work",
            })),
            requests: [],
            decisions: [],
          },
        });
      return previous(input, init);
    };
  });
  await page
    .getByTestId("sidebar-source-control-tab")
    .getByRole("button", { name: "Source Control", exact: true })
    .click();
  const control = page.getByTestId("source-control-menu");
  const show = control.getByRole("button", {
    name: "Show",
    exact: true,
  });
  await show.click();
  const summary = page.getByTestId("source-control-type-summary");
  await expect(summary).toHaveText(
    "Code 1 · Ontology 1 · Docs 1 — 3 matching of 3 changed files",
  );
  await page.screenshot({ path: testInfo.outputPath("file-types-all.png") });
  const stream = page.getByTestId("source-control-diff-stream");
  const text = await stream.innerText();
  for (const name of ["Ontology", "Docs"])
    await control.getByRole("checkbox", { name, exact: true }).uncheck();
  await expect(summary).toContainText("3 matching of 3");
  await show.click();
  await expect(summary).toHaveText("Code 1 — 1 matching of 3 changed files");
  await expect(stream).toContainText("updated src/workspace.ts");
  await expect(stream).not.toContainText(
    "updated .g5/ontology/workspace.g5.md",
  );
  await expect(stream).not.toContainText("updated docs/implementation.md");
  await expect(stream.locator("article")).toHaveCount(1);
  await expect(page.getByTestId("source-control-file")).toHaveCount(1);
  await page.screenshot({ path: testInfo.outputPath("file-types-code.png") });
  const help = page.getByLabel("About changed file types", { exact: true });
  await help.focus();
  await page.keyboard.press("Enter");
  await expect(help.locator("..")).toHaveAttribute("open", "");
  await expect(
    page.getByText(/Only files matching the applied types appear/),
  ).toBeVisible();
  const helpBounds = await help.locator("..").locator("div").boundingBox();
  expect(helpBounds!.x).toBeGreaterThanOrEqual(0);
  expect(helpBounds!.x + helpBounds!.width).toBeLessThanOrEqual(1440);
  await page.keyboard.press("Escape");
  await expect(help).toBeFocused();
  await expect(help.locator("..")).not.toHaveAttribute("open", "");
  await control.getByRole("checkbox", { name: "Code", exact: true }).uncheck();
  await show.click();
  await expect(summary).toHaveText("0 matching of 3 changed files");
  await expect(page.getByTestId("source-control-file")).toHaveCount(0);
  await expect(stream.locator("article")).toHaveCount(0);
  await expect(stream).toHaveText(
    "No changed files match the selected file types.",
  );
  expect(
    await page.evaluate(() => sessionStorage.getItem("type-preview-applies")),
  ).toBe("1");
  await page.screenshot({ path: testInfo.outputPath("file-types-none.png") });
  // Growing a type selection restores the captured files and exact diff bytes.
  for (const name of ["Code", "Ontology", "Docs"])
    await control.getByRole("checkbox", { name, exact: true }).check();
  await show.click();
  await expect(page.getByTestId("source-control-file")).toHaveCount(3);
  await expect(stream).toHaveText(text, { useInnerText: true });
  expect(
    await page.evaluate(() => sessionStorage.getItem("type-preview-applies")),
  ).toBe("1");
  await page.getByRole("tab", { name: "Workspace", exact: true }).click();
  const activity = page.getByTestId("landing-summary-changed");
  await expect(activity).toHaveText("3 changed files");
  await expect(page.getByTestId("landing-summary-code")).toHaveText("Code 1");
  await expect(page.getByTestId("landing-summary-ontology")).toHaveText(
    "Ontology 1",
  );
  await expect(page.getByTestId("landing-summary-docs")).toHaveText("Docs 1");
  await page.screenshot({
    path: testInfo.outputPath("file-types-activity.png"),
  });
  await activity.click();
  await expect(summary).toHaveText(
    "Code 1 · Ontology 1 · Docs 1 — 3 matching of 3 changed files",
  );
  // A captured pushed link must yield to the user's subsequent checklist.
  // Exercise the actual tab/URL shell, rather than only the provider.
  for (const name of ["Code", "Ontology", "Docs"])
    await control.getByRole("checkbox", { name, exact: true }).check();
  await control.getByRole("checkbox", { name: "Code", exact: true }).uncheck();
  await expect(
    page.getByText(
      "Selection changed. Showing the previous selection until you click Show.",
    ),
  ).toBeVisible();
  await show.click();
  await expect(summary).toHaveText(
    "Ontology 1 · Docs 1 — 2 matching of 3 changed files",
  );
  await expect(
    page.getByText(
      "Selection changed. Showing the previous selection until you click Show.",
    ),
  ).toHaveCount(0);
});
