import { expect, test } from "@playwright/test";

test.describe.configure({ timeout: 60_000 });

test("large comparisons keep the tree current and render nearby diff lines", async ({
  page,
}, testInfo) => {
  await page.addInitScript(() =>
    localStorage.setItem(
      "g5-user-settings",
      JSON.stringify({ sidebarDisplayMode: "expanded", showTabBar: true }),
    ),
  );
  await page.goto("/g5labs/Task%20Graph%20Rework?view=source-control");
  await expect(page.getByTestId("phase-chevrons")).toBeVisible();
  await page.evaluate(() => {
    localStorage.setItem(
      "g5-user-settings",
      JSON.stringify({ sidebarDisplayMode: "expanded", showTabBar: true }),
    );
    const nativeFetch = window.fetch;
    const nativeObserver = window.IntersectionObserver;
    sessionStorage.setItem("render-comparisons", "0");
    sessionStorage.setItem("render-observers", "0");
    sessionStorage.setItem("render-max-observers", "0");
    sessionStorage.setItem("render-targets", "0");
    window.IntersectionObserver = class extends nativeObserver {
      tracked = false;
      targets = new Set<Element>();
      constructor(
        callback: IntersectionObserverCallback,
        options?: IntersectionObserverInit,
      ) {
        super(callback, options);
        if (
          options?.root instanceof Element &&
          options.root.classList.contains("source-control-diff-stream")
        ) {
          this.tracked = true;
          const active = Number(sessionStorage.getItem("render-observers")) + 1;
          sessionStorage.setItem("render-observers", String(active));
          sessionStorage.setItem(
            "render-max-observers",
            String(
              Math.max(
                active,
                Number(sessionStorage.getItem("render-max-observers")),
              ),
            ),
          );
        }
      }
      observe(target: Element) {
        super.observe(target);
        if (this.tracked && !this.targets.has(target)) {
          this.targets.add(target);
          sessionStorage.setItem(
            "render-targets",
            String(Number(sessionStorage.getItem("render-targets")) + 1),
          );
        }
      }
      unobserve(target: Element) {
        super.unobserve(target);
        if (this.targets.delete(target))
          sessionStorage.setItem(
            "render-targets",
            String(Number(sessionStorage.getItem("render-targets")) - 1),
          );
      }
      disconnect() {
        if (this.tracked) {
          sessionStorage.setItem(
            "render-observers",
            String(Number(sessionStorage.getItem("render-observers")) - 1),
          );
          sessionStorage.setItem(
            "render-targets",
            String(
              Number(sessionStorage.getItem("render-targets")) -
                this.targets.size,
            ),
          );
          this.targets.clear();
          this.tracked = false;
        }
        super.disconnect();
      }
    };
    const patch = (index: number) => {
      const path = `src/group-${Math.floor(index / 50)}/file-${String(index).padStart(4, "0")}.ts`;
      return {
        repoId: "profile",
        repoName: "Profile",
        fileKey: path,
        kind: ["code", "ontology", "docs"][index % 3],
        label: "Commit one",
        before: "base",
        after: "one",
        file: {
          path,
          oldPath: null,
          change: "added",
          diff: {
            status: "text",
            additions: 20,
            deletions: 0,
            hunks: [
              {
                startLine: 1,
                header: "@@ -0,0 +1,20 @@",
                lines: Array.from({ length: 20 }, (_, line) => ({
                  type: "addition",
                  newLine: line + 1,
                  content: `FILE ${index} LINE ${line}`,
                })),
              },
            ],
          },
        },
      };
    };
    const patches = Array.from({ length: 500 }, (_, index) => patch(index));
    window.fetch = async (input, init) => {
      const url =
        typeof input === "string"
          ? input
          : input instanceof URL
            ? input.href
            : input.url;
      if (url.endsWith("/source-control/checkpoint"))
        return Response.json({ data: { generation: 0 } });
      if (url.endsWith("/source-control/menu"))
        return Response.json({
          data: {
            private: false,
            fingerprint: "large-render-fixture",
            repos: [
              {
                repoId: "profile",
                name: "Profile",
                base: "base",
                pushed: "three",
                truncated: false,
                error: null,
                live: null,
                commits: ["one", "two", "three"].map((sha) => ({
                  sha,
                  summary: `Commit ${sha}`,
                  authorName: "Render fixture",
                  committedAt: 1700000000,
                  parents: ["base"],
                })),
              },
            ],
          },
        });
      if (url.endsWith("/source-control/apply")) {
        sessionStorage.setItem(
          "render-comparisons",
          String(Number(sessionStorage.getItem("render-comparisons")) + 1),
        );
        const commits: string[] = JSON.parse(String(init?.body)).repos[0]
          .commits;
        const result = commits.includes("two")
          ? [...patches, patch(500)]
          : patches;
        const changed = result.map((value, index) =>
          index === 0 && commits.includes("three")
            ? {
                ...value,
                file: {
                  ...value.file,
                  diff: {
                    ...value.file.diff,
                    hunks: [
                      {
                        ...value.file.diff.hunks[0],
                        lines: [
                          {
                            type: "addition",
                            newLine: 1,
                            content: "UPDATED FIRST FILE",
                          },
                        ],
                      },
                    ],
                  },
                },
              }
            : value,
        );
        return Response.json({
          data: { patches: changed, fileCount: changed.length, private: false },
        });
      }
      return nativeFetch(input, init);
    };
  });
  const heading = page
    .getByTestId("sidebar-source-control-tab")
    .getByRole("button", { name: "Source Control", exact: true });
  await expect(heading).toBeVisible();
  if ((await heading.getAttribute("aria-expanded")) !== "true")
    await heading.click();
  const menu = page.getByTestId("source-control-menu");
  const show = menu.getByRole("button", { name: "Show", exact: true });
  await expect(menu.getByText("3 commits", { exact: true })).toBeVisible();
  await menu
    .getByRole("checkbox", { name: "All changes", exact: true })
    .uncheck();
  for (const name of ["Code", "Ontology", "Docs"])
    await menu.getByRole("checkbox", { name, exact: true }).check();
  await menu
    .getByRole("checkbox", { name: "one in Profile", exact: true })
    .check();
  await show.click();
  const stream = page.getByTestId("source-control-diff-stream");
  const rows = page.getByTestId("source-control-file");
  const expectTargets = (count: number) =>
    expect
      .poll(() =>
        page.evaluate(() => Number(sessionStorage.getItem("render-targets"))),
      )
      .toBe(count);
  await expect(rows).toHaveCount(500);
  await expect(stream.locator("article")).toHaveCount(500);
  await expectTargets(500);
  await expect(
    stream.getByText("FILE 0 LINE 19", { exact: true }),
  ).toBeVisible();
  expect(await stream.getByTestId("diff-line-code").count()).toBeLessThan(1000);
  expect(
    await page.evaluate(() => sessionStorage.getItem("render-max-observers")),
  ).toBe("1");

  // Both panes must stay untouched while the menu has pending edits.
  await page.evaluate(
    () =>
      new Promise<void>((resolve) =>
        requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
      ),
  );
  await page.evaluate(() => {
    sessionStorage.setItem("render-mutations", "0");
    const observer = new MutationObserver((entries) =>
      sessionStorage.setItem(
        "render-mutations",
        String(
          Number(sessionStorage.getItem("render-mutations")) + entries.length,
        ),
      ),
    );
    for (const selector of [
      'nav[aria-label="Changed files"]',
      '[data-testid="source-control-diff-stream"]',
    ])
      observer.observe(document.querySelector(selector)!, {
        subtree: true,
        childList: true,
        attributes: true,
        characterData: true,
      });
    Object.assign(window, {
      stopRenderMutationObserver: () => observer.disconnect(),
    });
  });
  await menu.getByRole("checkbox", { name: "Docs", exact: true }).uncheck();
  expect(
    await page.evaluate(() => sessionStorage.getItem("render-mutations")),
  ).toBe("0");
  await page.evaluate(() =>
    (
      window as unknown as { stopRenderMutationObserver: () => void }
    ).stopRenderMutationObserver(),
  );
  await show.click();
  await expect(rows).toHaveCount(334);
  await expect(stream.locator("article")).toHaveCount(334);
  await expectTargets(334);
  await menu.getByRole("checkbox", { name: "Docs", exact: true }).check();
  await show.click();
  await expect(rows).toHaveCount(500);
  await expectTargets(500);
  expect(
    await page.evaluate(() => sessionStorage.getItem("render-comparisons")),
  ).toBe("1");

  // A distant keyboard jump must materialize the complete correct body.
  const last = page
    .getByRole("navigation", { name: "Changed files" })
    .getByRole("button", { name: "file-0499.ts", exact: true });
  await last.focus();
  await page.keyboard.press("Enter");
  await expect(
    stream.getByText("FILE 499 LINE 19", { exact: true }),
  ).toBeVisible();
  await expect(
    stream.locator("article").first().getByTestId("source-control-diff-body"),
  ).toHaveAttribute("data-rendered", "false");
  expect(await stream.getByTestId("diff-line-code").count()).toBeLessThan(1000);

  await menu
    .getByRole("checkbox", { name: "two in Profile", exact: true })
    .check();
  await show.click();
  await expect(rows).toHaveCount(501);
  await expect(stream.locator("article")).toHaveCount(501);
  await expectTargets(501);
  await page
    .getByRole("navigation", { name: "Changed files" })
    .getByRole("button", { name: "file-0500.ts", exact: true })
    .click();
  await expect(
    stream.getByText("FILE 500 LINE 19", { exact: true }),
  ).toBeVisible();
  await menu
    .getByRole("checkbox", { name: "three in Profile", exact: true })
    .check();
  await show.click();
  await page
    .getByRole("navigation", { name: "Changed files" })
    .getByRole("button", { name: "file-0000.ts", exact: true })
    .click();
  await expect(
    stream.getByText("UPDATED FIRST FILE", { exact: true }),
  ).toBeVisible();
  await expect(stream.getByText("FILE 0 LINE 19", { exact: true })).toHaveCount(
    0,
  );
  await menu
    .getByRole("checkbox", { name: "two in Profile", exact: true })
    .uncheck();
  await show.click();
  await expect(rows).toHaveCount(500);
  await expect(stream.locator("article")).toHaveCount(500);
  await expectTargets(500);
  await expect(rows.filter({ hasText: "file-0500.ts" })).toHaveCount(0);
  expect(
    await page.evaluate(() => sessionStorage.getItem("render-comparisons")),
  ).toBe("4");
  expect(
    await page.evaluate(() => sessionStorage.getItem("render-max-observers")),
  ).toBe("1");
  await testInfo.attach("rendered-lines", {
    body: JSON.stringify({
      files: 500,
      totalLines: 10000,
      mountedLines: await stream.getByTestId("diff-line-code").count(),
    }),
    contentType: "application/json",
  });
  for (const name of ["Code", "Ontology", "Docs"])
    await menu.getByRole("checkbox", { name, exact: true }).uncheck();
  await show.click();
  await expect(stream.locator("article")).toHaveCount(0);
  await expectTargets(0);
  for (const name of ["Code", "Ontology", "Docs"])
    await menu.getByRole("checkbox", { name, exact: true }).check();
  await show.click();
  await expectTargets(500);
  await page.getByRole("tab", { name: "Workspace", exact: true }).click();
  await expect(page.getByTestId("source-control-diff-workspace")).toHaveCount(
    0,
  );
  await expectTargets(0);
  expect(
    await page.evaluate(() => sessionStorage.getItem("render-observers")),
  ).toBe("0");
});
