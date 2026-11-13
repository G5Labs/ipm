import { readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import assert from "node:assert/strict";
import { chromium, expect } from "@playwright/test";

// Manual production memory probe; run against the same authenticated preview
// and viewport for both builds. It replaces only Source Control read responses.
// MEMORY_STORAGE_STATE=/tmp/auth.json MEMORY_DIST=frontend/dist \
//   MEMORY_OUTPUT=/tmp/memory.json node frontend/e2e/source-control-memory.mjs
// MEMORY_CYCLES, MEMORY_NAVIGATIONS, MEMORY_FILES, MEMORY_LINES and
// MEMORY_CONTENT_REPEAT control stress. Inspect post-GC samples after warmup;
// there is no machine-independent memory threshold in ordinary CI.
const frontend = fileURLToPath(new URL("..", import.meta.url));
const dist = resolve(process.env.MEMORY_DIST || resolve(frontend, "dist"));
const baseURL = process.env.MEMORY_BASE_URL || "http://localhost:13000";
const workspacePath =
  process.env.MEMORY_WORKSPACE_PATH || "/g5labs/source-control-with-pr";
const storageState = process.env.MEMORY_STORAGE_STATE;
assert.ok(
  storageState,
  "MEMORY_STORAGE_STATE must name an authenticated Playwright storage state",
);
const files = Number(process.env.MEMORY_FILES || 500);
const lines = Number(process.env.MEMORY_LINES || 20);
const repeats = Number(process.env.MEMORY_CONTENT_REPEAT || 3);
const cycles = Number(process.env.MEMORY_CYCLES || 30);
const navigations = Number(process.env.MEMORY_NAVIGATIONS || 10);
const output =
  process.env.MEMORY_OUTPUT || "/tmp/source-control-memory-current.json";
const browser = await chromium.launch({
  headless: true,
  args: ["--enable-precise-memory-info"],
});
try {
  const page = await browser.newPage({
    viewport: { width: 1440, height: 1000 },
    storageState,
  });
  const cdp = await page.context().newCDPSession(page);
  const html = await readFile(resolve(dist, "index.html"), "utf8");
  await page.route("**/assets/**", async (route) => {
    const name = new URL(route.request().url()).pathname;
    await route.fulfill({ path: resolve(dist, "." + name) });
  });
  const workspaceURL = new URL(workspacePath + "?view=source-control", baseURL)
    .href;
  await page.route(workspaceURL, async (route) => {
    if (route.request().isNavigationRequest())
      await route.fulfill({ body: html, contentType: "text/html" });
    else await route.continue();
  });
  await page.route(/https:\/\/fonts\.(googleapis|gstatic)\.com\//, (route) =>
    route.abort(),
  );
  await page.addInitScript(
    ({ files, lines, repeats }) => {
      const nativeFetch = window.fetch;
      const nativeObserver = window.IntersectionObserver;
      const state = (window.memoryProbe = {
        requests: 0,
        observers: 0,
        targets: 0,
        maxObservers: 0,
        maxTargets: 0,
        weakBodies: [],
        weakLines: [],
      });
      window.IntersectionObserver = class extends nativeObserver {
        tracked = false;
        targets = new Set();
        constructor(callback, options) {
          super(callback, options);
          if (
            options?.root instanceof Element &&
            options.root.classList.contains("source-control-diff-stream")
          ) {
            this.tracked = true;
            state.observers++;
            state.maxObservers = Math.max(state.maxObservers, state.observers);
          }
        }
        observe(target) {
          super.observe(target);
          if (this.tracked && !this.targets.has(target)) {
            this.targets.add(target);
            state.targets++;
            state.maxTargets = Math.max(state.maxTargets, state.targets);
          }
        }
        unobserve(target) {
          super.unobserve(target);
          if (this.targets.delete(target)) state.targets--;
        }
        disconnect() {
          super.disconnect();
          if (this.tracked) {
            state.observers--;
            state.targets -= this.targets.size;
            this.targets.clear();
            this.tracked = false;
          }
        }
      };
      const make = (index, revision) => {
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
              additions: lines,
              deletions: 0,
              hunks: [
                {
                  startLine: 1,
                  header: `@@ -0,0 +1,${lines} @@`,
                  lines: Array.from({ length: lines }, (_, n) => ({
                    type: "addition",
                    newLine: n + 1,
                    content: `FILE ${index} LINE ${n} ${"export const example = 123; ".repeat(repeats)}revision ${revision}`,
                  })),
                },
              ],
            },
          },
        };
      };
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
              fingerprint: "memory-probe",
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
                    summary: "Commit " + sha,
                    authorName: "Memory fixture",
                    committedAt: 1700000000,
                    parents: ["base"],
                  })),
                },
              ],
            },
          });
        if (url.endsWith("/source-control/apply")) {
          state.requests++;
          const commits = JSON.parse(init.body).repos[0].commits;
          const count = files + Number(commits.includes("two"));
          const revision = commits.includes("three") ? state.requests : 0;
          return Response.json({
            data: {
              patches: Array.from({ length: count }, (_, index) =>
                make(index, revision),
              ),
              fileCount: count,
              private: false,
            },
          });
        }
        return nativeFetch(input, init);
      };
    },
    { files, lines, repeats },
  );
  await page.goto(workspaceURL);
  // Let the workspace's asynchronous saved-tab hydration settle before the
  // first measured selection, especially for very small, fast comparisons.
  await page.waitForLoadState("networkidle");
  await expect(page.getByTestId("source-control-diff-workspace")).toBeVisible();
  const heading = page
    .getByTestId("sidebar-source-control-tab")
    .getByRole("button", { name: "Source Control", exact: true });
  if ((await heading.getAttribute("aria-expanded")) !== "true")
    await heading.click();
  const menu = page.getByTestId("source-control-menu");
  const show = menu.getByRole("button", { name: "Show", exact: true });
  const stream = page.getByTestId("source-control-diff-stream");
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
  await expect(stream.locator("article")).toHaveCount(files);
  const settle = async () => {
    await page.evaluate(
      () =>
        new Promise((resolve) =>
          requestAnimationFrame(() => requestAnimationFrame(resolve)),
        ),
    );
    await page.waitForTimeout(40);
  };
  const sample = async (phase) => {
    await settle();
    const beforeGC = await cdp.send("Runtime.getHeapUsage");
    await cdp.send("HeapProfiler.collectGarbage");
    await settle();
    await cdp.send("HeapProfiler.collectGarbage");
    const heap = await cdp.send("Runtime.getHeapUsage");
    const dom = await cdp.send("Memory.getDOMCounters");
    const view = await page.evaluate(() => ({
      rows: document.querySelectorAll('[data-testid="source-control-file"]')
        .length,
      cards: document.querySelectorAll(".source-control-diff-card").length,
      lines: document.querySelectorAll('[data-testid="diff-line-code"]').length,
      observers: window.memoryProbe.observers,
      targets: window.memoryProbe.targets,
      maxObservers: window.memoryProbe.maxObservers,
      maxTargets: window.memoryProbe.maxTargets,
      requests: window.memoryProbe.requests,
      survivingBodies: window.memoryProbe.weakBodies.filter((ref) =>
        ref.deref(),
      ).length,
      survivingLines: window.memoryProbe.weakLines.filter((ref) => ref.deref())
        .length,
    }));
    const result = {
      phase,
      beforeGCUsed: beforeGC.usedSize,
      ...heap,
      ...dom,
      ...view,
    };
    console.log(JSON.stringify(result));
    return result;
  };
  const measurements = [];
  measurements.push(await sample("initial"));
  assert.equal(measurements[0].rows, files);
  assert.equal(measurements[0].cards, files);
  const optimized = measurements[0].observers === 1;
  const type = (name) => menu.getByRole("checkbox", { name, exact: true });
  const commit = (sha) =>
    menu.getByRole("checkbox", { name: `${sha} in Profile`, exact: true });
  const apply = async (count) => {
    await show.click();
    await expect(stream.locator("article")).toHaveCount(count);
    await settle();
  };
  for (let cycle = 1; cycle <= cycles; cycle++) {
    await type("Docs").uncheck();
    await apply(files - Math.floor(files / 3));
    await type("Docs").check();
    await apply(files);
    await stream.evaluate((el) => {
      el.scrollTop = el.scrollHeight;
    });
    await settle();
    await stream.evaluate((el) => {
      el.scrollTop = 0;
    });
    await settle();
    await commit("two").check();
    await apply(files + 1);
    await commit("three").check();
    await apply(files + 1);
    const revision = await page.evaluate(() => window.memoryProbe.requests);
    await expect(stream.getByTestId("diff-line-code").first()).toContainText(
      `revision ${revision}`,
    );
    await commit("three").uncheck();
    await apply(files + 1);
    await expect(stream.getByTestId("diff-line-code").first()).toContainText(
      "revision 0",
    );
    await commit("two").uncheck();
    await apply(files);
    if (cycle === 1 || cycle % 5 === 0 || cycle === cycles)
      measurements.push(await sample(`cycle-${cycle}`));
    else console.log(`Completed cycle ${cycle}`);
  }
  await page.evaluate(() => {
    window.memoryProbe.weakBodies = Array.from(
      document.querySelectorAll('[data-testid="source-control-diff-body"]'),
      (el) => new WeakRef(el),
    );
    window.memoryProbe.weakLines = Array.from(
      document.querySelectorAll('[data-testid="diff-line-code"]'),
      (el) => new WeakRef(el),
    );
  });
  for (const name of ["Code", "Ontology", "Docs"]) await type(name).uncheck();
  await apply(0);
  const empty = await sample("no-categories");
  measurements.push(empty);
  if (optimized) {
    assert.equal(empty.targets, 0, "removed bodies must be unobserved");
    assert.equal(
      empty.survivingBodies,
      0,
      "removed body DOM must be collectible",
    );
  }
  assert.equal(empty.survivingLines, 0, "removed line DOM must be collectible");
  for (const name of ["Code", "Ontology", "Docs"]) await type(name).check();
  await apply(files);
  const navigate = async (view) => {
    await page.evaluate((view) => {
      const url = new URL(location.href);
      url.searchParams.set("view", view);
      history.pushState(
        { ...history.state, idx: (history.state?.idx ?? 0) + 1 },
        "",
        url,
      );
      dispatchEvent(new PopStateEvent("popstate", { state: history.state }));
    }, view);
    await settle();
  };
  for (let i = 0; i < navigations; i++) {
    await navigate("activity");
    await expect(page.getByTestId("source-control-diff-workspace")).toHaveCount(
      0,
    );
    if (optimized)
      assert.deepEqual(
        await page.evaluate(() => [
          window.memoryProbe.observers,
          window.memoryProbe.targets,
        ]),
        [0, 0],
      );
    if (i === 0 || i === navigations - 1)
      measurements.push(await sample(`pane-closed-${i + 1}`));
    await navigate("source-control");
    await expect(stream.locator("article")).toHaveCount(files);
  }
  measurements.push(await sample("final"));
  if (optimized) {
    assert.equal(measurements.at(-1).observers, 1);
    assert.equal(measurements.at(-1).targets, files);
    assert.equal(measurements.at(-1).maxObservers, 1);
  }
  const result = {
    dist,
    files,
    lines,
    repeats,
    cycles,
    navigations,
    optimized,
    measurements,
  };
  await writeFile(output, JSON.stringify(result, null, 2) + "\n");
  console.log(`Saved ${output}`);
} finally {
  await browser.close();
}
