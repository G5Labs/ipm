import { expect, test, type Locator, type Page } from "@playwright/test";
import { createMockUser, MOCK_INSTANCE_INFO } from "../../src/test/fixtures";

const prose = "Ordinary words ".repeat(10).trim();
const longMessage = "Available content edge. ".repeat(64).trim();

async function serveReadingSurfaces(page: Page) {
  const user = createMockUser({ avatarUrl: null });
  const jwt = `e30.${Buffer.from(JSON.stringify({ sub: user.id, org: user.activeOrgId, scopes: [], exp: 9999999999 })).toString("base64url")}.sig`;
  const issue = {
    provider: "github",
    trackerId: "tracker",
    domainId: "domain",
    externalIssueId: "12",
    locator: { repositoryId: "11", number: 12 },
    displayKey: "acme/api#12",
    canonicalUrl: "https://github.com/acme/api/issues/12",
    title: "Width fixture",
    state: "open",
    stateReason: null,
    assignees: [],
    labels: [],
    updatedAt: "2026-10-27T00:00:00Z",
    createdAt: "2026-10-27T00:00:00Z",
    closedAt: null,
    contentTrusted: false,
    primaryWorkspace: null,
    author: { login: "author", avatarUrl: null },
    body: prose,
    commentCount: 1,
    commentsTruncated: false,
    links: [],
    comments: [
      {
        id: "comment",
        author: { login: "reader", avatarUrl: null },
        body: prose,
        createdAt: "2026-10-27T00:00:00Z",
        updatedAt: "2026-10-27T00:00:00Z",
        url: "https://github.com/acme/api/issues/12#comment",
      },
    ],
  };
  await page.route("**/api/**", (route) => {
    const path = new URL(route.request().url()).pathname;
    let data: unknown;
    if (path.endsWith("/auth/refresh")) data = { jwt };
    else if (path.endsWith("/users/me")) data = user;
    else if (path.endsWith("/instance/info"))
      data = { ...MOCK_INSTANCE_INFO, features: ["external_issues"] };
    else if (path === "/api/v1/orgs/org-g5labs/config")
      data = {
        scope: "org",
        settings: { external_issues_enabled: true },
        secretKeys: [],
        effectiveSettings: {},
        effectiveSecretKeys: [],
      };
    else if (path.endsWith("/issues/github/11/12")) data = issue;
    else if (path.endsWith("/activity/current")) data = { entries: [] };
    else if (path.endsWith("/activity"))
      data = {
        primaryIssue: {
          linkId: "issue",
          displayKey: issue.displayKey,
          canonicalUrl: issue.canonicalUrl,
          locator: issue.locator,
          contentTrusted: false,
        },
        issueError: null,
        issueHistoryTruncated: false,
        composer: null,
        earlierCursor: null,
        entries: [
          {
            key: "issue:12",
            at: issue.createdAt,
            source: "github",
            kind: "issue_comment",
            commentId: "comment",
            updatedAt: issue.updatedAt,
            author: issue.author,
            title: issue.title,
            body: prose,
            url: issue.canonicalUrl,
          },
        ],
      };
    else if (path.endsWith("/approvals/reviews/review/report"))
      data = { document: prose, sha256: "fixture-digest" };
    else
      return route.fulfill({
        status: 404,
        json: { error: { message: `Unprovided fixture: ${path}` } },
      });
    return route.fulfill({ json: { data } });
  });
}

async function lineCount(paragraph: Locator) {
  return paragraph.evaluate((element) => {
    const range = document.createRange();
    range.selectNodeContents(element);
    return new Set(Array.from(range.getClientRects(), (rect) => rect.top)).size;
  });
}

// Compare against the pane, accounting only for real spacing. Measuring just
// the immediate parent would miss a width cap higher in the component tree.
async function expectPaneWidth(paragraph: Locator) {
  const widths = await paragraph.evaluate((element) => {
    const pane = element.closest('[data-testid="content-pane"]')!;
    let available = pane.getBoundingClientRect().width;
    let child = element;
    for (let node = element.parentElement; node; node = node.parentElement) {
      const style = getComputedStyle(node);
      // Activity's timestamp and icon occupy leading grid columns. They are
      // real content alongside the prose, unlike a centered column's margins.
      if (style.display === "grid")
        available -=
          child.getBoundingClientRect().left -
          node.getBoundingClientRect().left -
          (parseFloat(style.paddingLeft) || 0) -
          (parseFloat(style.borderLeftWidth) || 0);
      for (const value of [
        style.paddingLeft,
        style.paddingRight,
        style.borderLeftWidth,
        style.borderRightWidth,
      ])
        available -= parseFloat(value) || 0;
      // Vertical scrollbars legitimately consume part of the pane.
      available -=
        node.offsetWidth -
        node.clientWidth -
        (parseFloat(style.borderLeftWidth) || 0) -
        (parseFloat(style.borderRightWidth) || 0);
      if (node === pane) break;
      child = node;
    }
    return { available, actual: element.getBoundingClientRect().width };
  });
  expect(widths.actual).toBeCloseTo(widths.available, 0);
}

for (const pane of ["center", "panel"])
  for (const role of ["user", "system"])
    test(`${pane} ${role} messages use the full available content width`, async ({
      page,
    }) => {
      await page.route("**/api/**", (route) =>
        route.fulfill({ json: { data: [] } }),
      );
      await page.setViewportSize({ width: 1800, height: 1000 });
      await page.goto(
        `/e2e/browser/markdown-layout.html?pane=${pane}&role=${role}&sidebar=240`,
      );
      const text = page.getByText(longMessage, { exact: true }).locator("..");
      await expect(text).toBeVisible();
      await expectPaneWidth(text);
      const wideLines = await lineCount(text);
      await page.setViewportSize({ width: 900, height: 1000 });
      await expectPaneWidth(text);
      expect(await lineCount(text)).toBeGreaterThan(wideLines);
      expect(
        await page.evaluate(
          () => document.documentElement.scrollWidth <= innerWidth,
        ),
      ).toBe(true);
    });

for (const pane of ["issue", "issue-page", "activity", "report"])
  for (const sidebar of [0, 240])
    test(`${pane} reading surface and header fill the pane with a ${sidebar}px sidebar`, async ({
      page,
    }) => {
      await serveReadingSurfaces(page);
      await page.setViewportSize({ width: 1800, height: 1000 });
      await page.goto(
        `/e2e/browser/markdown-layout.html?pane=${pane}&sidebar=${sidebar}`,
      );
      const paragraphs = page
        .getByTestId("content-pane")
        .locator(".g5-markdown-preview p")
        .filter({ hasText: prose });
      await expect(paragraphs).toHaveCount(pane === "report" ? 1 : 2);
      for (const width of [1800, 900]) {
        await page.setViewportSize({ width, height: 1000 });
        for (const paragraph of await paragraphs.all()) {
          await expectPaneWidth(paragraph);
          if (width === 1800) expect(await lineCount(paragraph)).toBe(1);
          else expect(await lineCount(paragraph)).toBeGreaterThan(1);
        }
        if (pane === "issue-page")
          await expectPaneWidth(
            page.getByTestId("issue-page-back").locator(".."),
          );
        expect(
          await page.evaluate(
            () => document.documentElement.scrollWidth <= innerWidth,
          ),
        ).toBe(true);
      }
    });

for (const pane of ["center", "panel", "transcript"])
  for (const sidebar of [0, 240])
    test(`${pane} uses the window width remaining after ${sidebar}px sidebar and padding`, async ({
      page,
    }) => {
      await page.route("**/api/**", (route) =>
        route.fulfill({ json: { data: [] } }),
      );
      await page.setViewportSize({ width: 1800, height: 1000 });
      await page.goto(
        `/e2e/browser/markdown-layout.html?pane=${pane}&sidebar=${sidebar}`,
      );
      const paragraph = page
        .getByTestId("content-pane")
        .locator(".g5-markdown-preview p")
        .first();
      await expect(paragraph).toBeVisible();
      await expectPaneWidth(paragraph);
      expect(await lineCount(paragraph)).toBe(1);
      await page.setViewportSize({ width: 900, height: 1000 });
      await expectPaneWidth(paragraph);
      expect(await lineCount(paragraph)).toBeGreaterThan(1);
      expect(
        await page.evaluate(
          () => document.documentElement.scrollWidth <= innerWidth,
        ),
      ).toBe(true);
    });

// `g5.documents.text-measure`: a file's prose reads in a centred column of
// at most 8 inches (768px) inside a document that fills its pane; a narrower
// pane fills instead. Tables are exempt (file-editor.spec.ts).
const MEASURE = 768;
const PADDING = 24;

for (const ontology of [false, true])
  test(`${ontology ? "Core Ontology" : "Document"} preview fills its pane and centres prose at the 8-inch measure`, async ({
    page,
  }) => {
    test.setTimeout(60_000);
    await page.setViewportSize({ width: 1800, height: 1000 });
    await page.goto(
      `/e2e/fixtures/file-editor.html${ontology ? "?ontology=1" : ""}`,
    );
    await expect(page.getByTestId("document-preview")).toBeVisible();
    const prose = "Ordinary words ".repeat(10).trim();
    await page.evaluate(
      ({ ontology, prose }) => {
        const fixture = (window as any).editorTest;
        fixture.external(
          fixture.initial.replace(
            ontology ? "Original statement." : "Original.",
            prose,
          ),
        );
      },
      { ontology, prose },
    );
    const column = page.getByTestId("document-preview-content");
    const markdown = column.locator(":scope > .g5-markdown-preview");
    const paragraph = markdown.locator("p").filter({ hasText: prose }).first();
    await expect(paragraph).toBeVisible();
    const [paneBox, previewBox, columnBox, markdownBox, paragraphBox] =
      await Promise.all([
        page.getByTestId("center-panel").boundingBox(),
        page.getByTestId("document-preview").boundingBox(),
        column.boundingBox(),
        markdown.boundingBox(),
        paragraph.boundingBox(),
      ]);
    expect(previewBox!.width).toBeCloseTo(paneBox!.width, 0);
    // The column fills the room beside the outline, so a table may use it;
    // prose keeps the measure, centred, with equal wide margins.
    expect(columnBox!.x + columnBox!.width).toBeCloseTo(
      previewBox!.x + previewBox!.width,
      0,
    );
    expect(markdownBox!.width).toBeCloseTo(columnBox!.width, 0);
    const contentLeft = markdownBox!.x + PADDING;
    const contentRight = markdownBox!.x + markdownBox!.width - PADDING;
    if (ontology) {
      // The edited statement is a Modified pair in the centred block: its prose
      // follows the bullet indent, the change label and the details area.
      expect(paragraphBox!.width).toBeLessThan(MEASURE);
      expect(paragraphBox!.width).toBeGreaterThanOrEqual(MEASURE - 160);
    } else {
      expect(paragraphBox!.width).toBeCloseTo(MEASURE, 0);
      const leftMargin = paragraphBox!.x - contentLeft;
      const rightMargin =
        contentRight - (paragraphBox!.x + paragraphBox!.width);
      expect(leftMargin).toBeGreaterThan(100);
      expect(leftMargin).toBeCloseTo(rightMargin, 0);
    }
    // 150 characters of prose exceed 8 inches: they wrap.
    expect(await lineCount(paragraph)).toBeGreaterThan(1);
    expect(
      await markdown.evaluate((element) => {
        const style = getComputedStyle(element);
        return [
          style.paddingTop,
          style.paddingRight,
          style.paddingBottom,
          style.paddingLeft,
        ];
      }),
    ).toEqual(["16px", "24px", "32px", "24px"]);

    // At 900px the pane still holds the measure; at 700px it does not, and
    // prose fills the room it has.
    await page.setViewportSize({ width: 700, height: 1000 });
    const [narrowColumn, narrowMarkdown, narrowParagraph] = await Promise.all([
      column.boundingBox(),
      markdown.boundingBox(),
      paragraph.boundingBox(),
    ]);
    expect(narrowMarkdown!.width).toBeCloseTo(narrowColumn!.width, 0);
    expect(narrowParagraph!.width).toBeLessThan(MEASURE);
    if (!ontology)
      expect(narrowParagraph!.width).toBeCloseTo(
        narrowMarkdown!.width - 2 * PADDING,
        0,
      );
    expect(await lineCount(paragraph)).toBeGreaterThan(1);
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    ).toBe(true);
  });

for (const surface of [
  "shared",
  "chat",
  "restricted",
  "statement",
  "historical",
])
  test(`${surface}: Markdown uses its host width and preserves authored breaks`, async ({
    page,
  }) => {
    await page.setViewportSize({ width: 1600, height: 1000 });
    await page.goto("/e2e/browser/markdown-layout.html");
    const host = page.getByTestId(surface);
    const paragraphs = host.locator("p");
    await expect(paragraphs).toHaveCount(4);

    // This prose fits the host. A shared 980px cap or page padding must
    // not make an embedded renderer wrap early or leave an unused right column.
    const [hostBox, paragraphBox] = await Promise.all([
      host.boundingBox(),
      paragraphs.first().boundingBox(),
    ]);
    expect(paragraphBox!.width).toBeCloseTo(hostBox!.width, 0);
    expect(paragraphBox!.x).toBeCloseTo(hostBox!.x, 0);
    expect(await lineCount(paragraphs.first())).toBe(1);
    expect(await lineCount(paragraphs.nth(1))).toBe(1);
    await expect(paragraphs.nth(1).locator("br")).toHaveCount(0);
    for (const paragraph of [paragraphs.nth(2), paragraphs.nth(3)]) {
      await expect(paragraph.locator("br")).toHaveCount(1);
      expect(await lineCount(paragraph)).toBe(2);
    }
    expect(await host.locator("pre code").textContent()).toBe(
      "line one\n  line two",
    );

    // The same text wraps naturally when the host becomes narrow.
    await page.setViewportSize({ width: 360, height: 900 });
    expect(await lineCount(paragraphs.first())).toBeGreaterThan(1);
    expect(await lineCount(paragraphs.nth(1))).toBe(1);
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    ).toBe(true);
  });

test("Rich text shares the reading view's 8-inch prose measure inside a full-width surface", async ({
  page,
}) => {
  test.setTimeout(60_000);
  await page.setViewportSize({ width: 1800, height: 1000 });
  await page.goto("/e2e/fixtures/file-editor.html");
  await expect(page.getByTestId("document-preview")).toBeVisible();
  await page.evaluate((prose) => {
    const fixture = (window as any).editorTest;
    fixture.external(fixture.initial.replace("Original.", prose));
  }, prose);
  await expect(page.getByTestId("document-preview")).toContainText(prose);
  const reading = page
    .getByTestId("document-preview-content")
    .locator("p")
    .filter({ hasText: prose })
    .first();
  const readingBox = (await reading.boundingBox())!;
  const readingLines = await lineCount(reading);
  await page.getByTestId("file-view-edit").click();
  const editor = page.getByTestId("rich-editor");
  const surface = page.getByTestId("rich-editor-surface");
  const content = page.getByTestId("rich-editor-content");
  const paragraph = content.locator("p").filter({ hasText: prose }).first();
  await expect(paragraph).toBeVisible();
  // The reading view's padding, so text keeps the width it had in Preview.
  expect(
    await surface.evaluate((element) => {
      const style = getComputedStyle(element);
      return [
        style.paddingTop,
        style.paddingRight,
        style.paddingBottom,
        style.paddingLeft,
      ];
    }),
  ).toEqual(["16px", "24px", "32px", "24px"]);
  for (const width of [1800, 700]) {
    await page.setViewportSize({ width, height: 1000 });
    const [paneBox, editorBox, surfaceBox, contentBox, paragraphBox] =
      await Promise.all([
        page.getByTestId("center-panel").boundingBox(),
        editor.boundingBox(),
        surface.boundingBox(),
        content.boundingBox(),
        paragraph.boundingBox(),
      ]);
    expect(editorBox!.width).toBeCloseTo(paneBox!.width, 0);
    expect(contentBox!.x).toBeCloseTo(surfaceBox!.x + PADDING, 0);
    expect(contentBox!.width).toBeCloseTo(surfaceBox!.width - 2 * PADDING, 0);
    // The surface fills the pane (a table may use it); prose keeps the
    // measure, centred in the content box.
    expect(surfaceBox!.width).toBeCloseTo(paneBox!.width, 0);
    const lines = await lineCount(paragraph);
    if (width === 1800) {
      // The same measure as the reading view: the text wraps as it did.
      expect(paragraphBox!.width).toBeCloseTo(MEASURE, 0);
      expect(paragraphBox!.width).toBeCloseTo(readingBox.width, 0);
      expect(lines).toBe(readingLines);
      expect(paragraphBox!.x - contentBox!.x).toBeCloseTo(
        contentBox!.x +
          contentBox!.width -
          (paragraphBox!.x + paragraphBox!.width),
        0,
      );
    } else {
      expect(paragraphBox!.width).toBeCloseTo(contentBox!.width, 0);
      expect(paragraphBox!.width).toBeLessThan(MEASURE);
      expect(lines).toBeGreaterThan(1);
    }
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    ).toBe(true);
  }
});
