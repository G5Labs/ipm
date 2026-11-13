// Workspace-file deep links inside the sandboxed srcdoc preview (same
// [D66]/[D68] page-proposes/host-validates model as the ontology links):
// `g5://file/<path>` anchors must never navigate the frame; the injected
// script consumes the click and PROPOSES the sanitized repo-relative path to
// the host via postMessage. The suite executes the exact shipped script bytes
// (SANDBOXED_FILE_NAV_JS) against the jsdom document, mirroring
// sandboxedOntologyNav.test.ts.
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import {
  MAX_WORKSPACE_FILE_PATH_CHARS,
  MAX_WORKSPACE_REPO_REF_CHARS,
  SANDBOXED_FILE_NAV_JS,
  parseG5FileHref,
  sanitizeWorkspaceFilePath,
  sanitizeWorkspaceRepoRef,
} from "./sandboxedHtml";

function dispatchClick(
  element: Element,
  init: MouseEventInit = {},
): MouseEvent {
  const event = new MouseEvent("click", {
    bubbles: true,
    cancelable: true,
    ...init,
  });
  element.dispatchEvent(event);
  return event;
}

describe("sandboxed file navigation script", () => {
  let postSpy: ReturnType<typeof vi.spyOn>;

  beforeAll(() => {
    // Install the shipped listener once, exactly as the srcdoc document would.
    new Function(SANDBOXED_FILE_NAV_JS)();
  });

  afterEach(() => {
    document.body.innerHTML = "";
    vi.restoreAllMocks();
  });

  function renderLink(href: string): HTMLElement {
    document.body.innerHTML = `<a id="link" href="${href}"><span id="inner">open</span></a>`;
    return document.getElementById("link")!;
  }

  function spyParentPost(): void {
    // In jsdom the test document is the top-level browsing context, so the
    // script's `parent` IS this window — spy on its postMessage.
    postSpy = vi.spyOn(window, "postMessage").mockImplementation(() => {});
  }

  function postedMessages(): Array<Record<string, unknown>> {
    return postSpy.mock.calls.map((call) => call[0] as Record<string, unknown>);
  }

  it("intercepts a g5://file/ link and posts the repo-relative path", () => {
    spyParentPost();
    const link = renderLink("g5://file/g5-api/src/services/doc_service.rs");

    const event = dispatchClick(link);

    expect(event.defaultPrevented).toBe(true);
    expect(postedMessages()).toEqual([
      {
        source: "g5-html-doc",
        type: "open-workspace-file",
        filePath: "g5-api/src/services/doc_service.rs",
      },
    ]);
  });

  it("resolves clicks on descendants of the anchor", () => {
    spyParentPost();
    renderLink("g5://file/frontend/src/lib/editor.ts");

    const event = dispatchClick(document.getElementById("inner")!);

    expect(event.defaultPrevented).toBe(true);
    expect(postedMessages()).toEqual([
      {
        source: "g5-html-doc",
        type: "open-workspace-file",
        filePath: "frontend/src/lib/editor.ts",
      },
    ]);
  });

  it.each([
    ["path traversal segment", "g5://file/..%2Fetc%2Fpasswd"],
    ["absolute path", "g5://file/%2Fetc%2Fpasswd"],
    ["backslash path", "g5://file/foo%5Cbar.rs"],
    ["empty remainder", "g5://file/"],
    ["empty segment", "g5://file/foo//bar.rs"],
    ["dot segment", "g5://file/foo/./bar.rs"],
  ])("consumes but drops %s", (_label, href) => {
    spyParentPost();
    const link = renderLink(href);

    const event = dispatchClick(link);

    expect(event.defaultPrevented).toBe(true);
    expect(postSpy).not.toHaveBeenCalled();
  });

  it("trims surrounding whitespace before the scheme check — a padded link never escapes interception", () => {
    spyParentPost();
    const link = renderLink("  g5://file/README.md?repo=g5-code  ");

    const event = dispatchClick(link);

    expect(event.defaultPrevented).toBe(true);
    expect(postedMessages()).toEqual([
      {
        source: "g5-html-doc",
        type: "open-workspace-file",
        filePath: "README.md",
        repoRef: "g5-code",
      },
    ]);
  });

  it("posts the repo qualifier alongside the path (issue #1970)", () => {
    spyParentPost();
    const link = renderLink("g5://file/g5d/src/worker/mod.rs?repo=g5-code");

    const event = dispatchClick(link);

    expect(event.defaultPrevented).toBe(true);
    expect(postedMessages()).toEqual([
      {
        source: "g5-html-doc",
        type: "open-workspace-file",
        filePath: "g5d/src/worker/mod.rs",
        repoRef: "g5-code",
      },
    ]);
  });

  it("percent-decodes the repo qualifier value", () => {
    spyParentPost();
    const link = renderLink("g5://file/README.md?repo=g5%2Dcode");

    dispatchClick(link);

    expect(postedMessages()).toEqual([
      {
        source: "g5-html-doc",
        type: "open-workspace-file",
        filePath: "README.md",
        repoRef: "g5-code",
      },
    ]);
  });

  it.each([
    ["empty query", "g5://file/README.md?"],
    ["empty repo value", "g5://file/README.md?repo="],
    ["non-repo parameter", "g5://file/README.md?other=x"],
    ["multiple parameters", "g5://file/README.md?repo=a&repo=b"],
    ["second equals sign", "g5://file/README.md?repo=a=b"],
    ["invalid repo charset", "g5://file/README.md?repo=bad*chars"],
    ["path rules still apply", "g5://file/..%2Fetc%2Fpasswd?repo=g5-code"],
  ])("consumes but drops a malformed qualifier link: %s", (_label, href) => {
    spyParentPost();
    const link = renderLink(href);

    const event = dispatchClick(link);

    expect(event.defaultPrevented).toBe(true);
    expect(postSpy).not.toHaveBeenCalled();
  });

  it.each([
    ["a line", "g5://file/src/main.rs#L12", undefined, 12],
    [
      "a range, opened at its first line",
      "g5://file/src/main.rs#L12-L20",
      undefined,
      12,
    ],
    [
      "a qualifier and a line",
      "g5://file/src/main.rs?repo=g5-code#L7",
      "g5-code",
      7,
    ],
  ])(
    "posts the cited line of %s (g5.documents.file-links.line)",
    (_label, href, repoRef, line) => {
      spyParentPost();
      const link = renderLink(href);

      const event = dispatchClick(link);

      expect(event.defaultPrevented).toBe(true);
      expect(postedMessages()).toStrictEqual([
        {
          source: "g5-html-doc",
          type: "open-workspace-file",
          filePath: "src/main.rs",
          repoRef,
          line,
        },
      ]);
    },
  );

  it.each([
    ["a heading", "g5://file/docs/plan.md#goals"],
    ["line zero", "g5://file/docs/plan.md#L0"],
    ["a bare L", "g5://file/docs/plan.md#L"],
    ["an empty fragment", "g5://file/docs/plan.md#"],
  ])("opens the file without a line for %s", (_label, href) => {
    spyParentPost();
    const link = renderLink(href);

    dispatchClick(link);

    expect(postedMessages()).toHaveLength(1);
    expect(postedMessages()[0]).toMatchObject({ filePath: "docs/plan.md" });
    expect(postedMessages()[0].line).toBeUndefined();
  });

  it.each([
    ["malformed qualifier", "g5://file/a.md?repo=a&repo=b#L3"],
    ["traversal path", "g5://file/..%2Fetc%2Fpasswd#L3"],
    ["qualifier after the fragment", "g5://file/src/main.rs#L12?repo=g5-code"],
  ])("a line never rescues a malformed link: %s", (_label, href) => {
    spyParentPost();
    const link = renderLink(href);

    const event = dispatchClick(link);

    expect(event.defaultPrevented).toBe(true);
    expect(postSpy).not.toHaveBeenCalled();
  });

  it("ignores non-g5-file links entirely", () => {
    spyParentPost();
    const link = renderLink("https://example.com/page");

    const event = dispatchClick(link);

    expect(event.defaultPrevented).toBe(false);
    expect(postSpy).not.toHaveBeenCalled();
  });

  it("ignores modified clicks so browser affordances stay intact", () => {
    spyParentPost();
    const link = renderLink("g5://file/g5d/src/worker/mod.rs");

    const event = dispatchClick(link, { metaKey: true });

    expect(event.defaultPrevented).toBe(false);
    expect(postSpy).not.toHaveBeenCalled();
  });
});

describe("sanitizeWorkspaceFilePath (host-side mirror)", () => {
  it("accepts ordinary repo-relative paths", () => {
    expect(
      sanitizeWorkspaceFilePath("g5-api/src/services/doc_service.rs"),
    ).toBe("g5-api/src/services/doc_service.rs");
    expect(sanitizeWorkspaceFilePath("README.md")).toBe("README.md");
  });

  it("rejects traversal, absolute, backslash, and empty-segment shapes", () => {
    expect(sanitizeWorkspaceFilePath("../etc/passwd")).toBeNull();
    expect(sanitizeWorkspaceFilePath("/etc/passwd")).toBeNull();
    expect(sanitizeWorkspaceFilePath("foo\\bar.rs")).toBeNull();
    expect(sanitizeWorkspaceFilePath("foo//bar.rs")).toBeNull();
    expect(sanitizeWorkspaceFilePath("foo/./bar.rs")).toBeNull();
    expect(sanitizeWorkspaceFilePath("")).toBeNull();
  });

  it("bounds the length", () => {
    // Valid in every other respect (single clean segment, allowed charset)
    // so only the length rule can reject it — a trailing "/" would trip the
    // empty-segment rule first and mask a removed length bound.
    const long = `${"a".repeat(MAX_WORKSPACE_FILE_PATH_CHARS)}.rs`;
    expect(sanitizeWorkspaceFilePath(long)).toBeNull();
    expect(
      sanitizeWorkspaceFilePath("a".repeat(MAX_WORKSPACE_FILE_PATH_CHARS)),
    ).not.toBeNull();
  });
});

describe("sanitizeWorkspaceRepoRef (host-side mirror)", () => {
  it("accepts repo ids and repo names in the conservative charset", () => {
    expect(sanitizeWorkspaceRepoRef("g5-code")).toBe("g5-code");
    expect(
      sanitizeWorkspaceRepoRef("01a00c43-eff9-77b2-aafe-bc203a2a9ad2"),
    ).toBe("01a00c43-eff9-77b2-aafe-bc203a2a9ad2");
    expect(sanitizeWorkspaceRepoRef("My_Repo.v2")).toBe("My_Repo.v2");
  });

  it("rejects empty, spaced, slashed, and over-long values", () => {
    expect(sanitizeWorkspaceRepoRef("")).toBeNull();
    expect(sanitizeWorkspaceRepoRef("two words")).toBeNull();
    expect(sanitizeWorkspaceRepoRef("org/repo")).toBeNull();
    expect(sanitizeWorkspaceRepoRef("bad*chars")).toBeNull();
    expect(
      sanitizeWorkspaceRepoRef("a".repeat(MAX_WORKSPACE_REPO_REF_CHARS + 1)),
    ).toBeNull();
    expect(
      sanitizeWorkspaceRepoRef("a".repeat(MAX_WORKSPACE_REPO_REF_CHARS)),
    ).not.toBeNull();
  });
});

describe("parseG5FileHref (markdown delegate parser)", () => {
  it("parses an unqualified link", () => {
    expect(parseG5FileHref("g5://file/docs/ws-1/plan.md")).toEqual({
      filePath: "docs/ws-1/plan.md",
    });
  });

  it("parses a repo-qualified link", () => {
    expect(parseG5FileHref("g5://file/g5d/src/main.rs?repo=g5-code")).toEqual({
      filePath: "g5d/src/main.rs",
      repoRef: "g5-code",
    });
  });

  it("percent-decodes path and qualifier separately", () => {
    expect(
      parseG5FileHref("g5://file/docs/ws-1/a%20b.md?repo=g5%2Dcode"),
    ).toEqual({ filePath: "docs/ws-1/a b.md", repoRef: "g5-code" });
  });

  it("reads a line fragment after the path or qualifier", () => {
    expect(parseG5FileHref("g5://file/docs/ws-1/plan.md#L12")).toEqual({
      filePath: "docs/ws-1/plan.md",
      line: 12,
    });
    expect(
      parseG5FileHref("g5://file/g5d/src/main.rs?repo=g5-code#L12-L20"),
    ).toEqual({ filePath: "g5d/src/main.rs", repoRef: "g5-code", line: 12 });
  });

  it("ignores any other fragment without changing the file", () => {
    for (const fragment of ["goals", "L0", "L", "", "L12x", "l12"]) {
      expect(
        parseG5FileHref(`g5://file/docs/ws-1/plan.md?repo=docs#${fragment}`),
      ).toStrictEqual({ filePath: "docs/ws-1/plan.md", repoRef: "docs" });
    }
  });

  it("keeps refusing malformed links that carry a line", () => {
    expect(parseG5FileHref("g5://file/../etc#L1")).toBeNull();
    expect(parseG5FileHref("g5://file/a.md?repo=a&repo=b#L1")).toBeNull();
    expect(parseG5FileHref("g5://file/a.md?#L1")).toBeNull();
    expect(parseG5FileHref("g5://file/#L1")).toBeNull();
    // A misplaced qualifier is never dropped as part of the fragment.
    expect(parseG5FileHref("g5://file/a.md#L1?repo=code")).toBeNull();
    expect(parseG5FileHref("g5://file/a.md#goals?x")).toBeNull();
  });

  it("returns null for non-g5-file hrefs and malformed links", () => {
    expect(parseG5FileHref("https://example.com")).toBeNull();
    expect(parseG5FileHref("g5://file/")).toBeNull();
    expect(parseG5FileHref("g5://file/../etc")).toBeNull();
    expect(parseG5FileHref("g5://file/a.md?")).toBeNull();
    expect(parseG5FileHref("g5://file/a.md?repo=")).toBeNull();
    expect(parseG5FileHref("g5://file/a.md?other=x")).toBeNull();
    expect(parseG5FileHref("g5://file/a.md?repo=a&repo=b")).toBeNull();
    expect(parseG5FileHref("g5://file/a.md?repo=a=b")).toBeNull();
    expect(parseG5FileHref("g5://file/a.md?repo=bad*chars")).toBeNull();
  });
});
