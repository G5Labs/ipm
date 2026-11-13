// [D64] regression tests for issue #1541: in-doc fragment links inside the
// sandboxed srcdoc preview must scroll to the section instead of navigating
// the iframe to the embedding origin. The suite executes the exact shipped
// script bytes (SANDBOXED_ANCHOR_NAV_JS) against the jsdom document.
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { SANDBOXED_ANCHOR_NAV_JS, withSandboxedHtmlCsp } from "./sandboxedHtml";

function clickEvent(init: MouseEventInit = {}): MouseEvent {
  return new MouseEvent("click", { bubbles: true, cancelable: true, ...init });
}

function dispatchClick(
  element: Element,
  init: MouseEventInit = {},
): MouseEvent {
  const event = clickEvent(init);
  element.dispatchEvent(event);
  return event;
}

describe("sandboxed anchor navigation script", () => {
  beforeAll(() => {
    // Install the shipped listener once, exactly as the srcdoc document would.
    new Function(SANDBOXED_ANCHOR_NAV_JS)();
  });

  afterEach(() => {
    document.body.innerHTML = "";
    vi.restoreAllMocks();
  });

  function renderDoc(bodyHtml: string): void {
    document.body.innerHTML = bodyHtml;
  }

  function spyScroll(id: string): ReturnType<typeof vi.fn> {
    const element = document.getElementById(id);
    if (!element) throw new Error(`missing #${id}`);
    const spy = vi.fn();
    (element as HTMLElement).scrollIntoView = spy;
    return spy;
  }

  it("intercepts a fragment-only link and scrolls its section into view", () => {
    renderDoc(
      '<a id="link" href="#workshop">go</a><h2 id="workshop">Workshop</h2>',
    );
    const scrolled = spyScroll("workshop");

    const event = dispatchClick(document.getElementById("link")!);

    expect(event.defaultPrevented).toBe(true);
    expect(scrolled).toHaveBeenCalledWith({ block: "start" });
  });

  it("resolves clicks on descendants of the anchor", () => {
    renderDoc(
      '<a href="#workshop"><span id="inner">go</span></a><h2 id="workshop">W</h2>',
    );
    const scrolled = spyScroll("workshop");

    const event = dispatchClick(document.getElementById("inner")!);

    expect(event.defaultPrevented).toBe(true);
    expect(scrolled).toHaveBeenCalled();
  });

  it("supports legacy named anchors and percent-encoded fragments", () => {
    renderDoc(
      '<a id="link" href="#section%20one">go</a><a name="section one" id="legacy"></a>',
    );
    const scrolled = spyScroll("legacy");

    const event = dispatchClick(document.getElementById("link")!);

    expect(event.defaultPrevented).toBe(true);
    expect(scrolled).toHaveBeenCalled();
  });

  it("treats a standalone self-reference (page.html#frag) as in-doc when the target exists", () => {
    renderDoc(
      '<a id="link" href="integration_questions.html#workshop">go</a><h2 id="workshop">W</h2>',
    );
    const scrolled = spyScroll("workshop");

    const event = dispatchClick(document.getElementById("link")!);

    expect(event.defaultPrevented).toBe(true);
    expect(scrolled).toHaveBeenCalled();
  });

  it("intercepts an absolute self-link (base URL + fragment) whose target exists", () => {
    // In a srcdoc document the base URL is the embedding page URL, so an
    // authored absolute link to the doc's own URL plus a fragment is in-doc.
    const selfUrl = new URL(document.baseURI);
    selfUrl.hash = "#Section1";
    renderDoc(
      `<a id="link" href="${selfUrl.href}">go</a><h2 id="Section1">S1</h2>`,
    );
    const scrolled = spyScroll("Section1");

    const event = dispatchClick(document.getElementById("link")!);

    expect(event.defaultPrevented).toBe(true);
    expect(scrolled).toHaveBeenCalled();
  });

  it("never hijacks an external absolute link whose fragment collides with a local id", () => {
    renderDoc(
      '<a id="link" href="https://example.com/page#workshop">out</a><h2 id="workshop">W</h2>',
    );
    const scrolled = spyScroll("workshop");

    const event = dispatchClick(document.getElementById("link")!);

    expect(event.defaultPrevented).toBe(false);
    expect(scrolled).not.toHaveBeenCalled();
  });

  it("does not intercept root-relative links with a colliding fragment", () => {
    renderDoc(
      '<a id="link" href="/other/page#workshop">out</a><h2 id="workshop">W</h2>',
    );
    const scrolled = spyScroll("workshop");

    const event = dispatchClick(document.getElementById("link")!);

    expect(event.defaultPrevented).toBe(false);
    expect(scrolled).not.toHaveBeenCalled();
  });

  it("leaves URL+fragment links alone when the fragment target does not exist here", () => {
    renderDoc('<a id="link" href="https://example.com/page#elsewhere">out</a>');

    const event = dispatchClick(document.getElementById("link")!);

    expect(event.defaultPrevented).toBe(false);
  });

  it("leaves URL+fragment links alone when another browsing context is requested", () => {
    renderDoc(
      '<a id="link" href="https://example.com/page#workshop" target="_blank">out</a><h2 id="workshop">W</h2>',
    );
    const scrolled = spyScroll("workshop");

    const event = dispatchClick(document.getElementById("link")!);

    expect(event.defaultPrevented).toBe(false);
    expect(scrolled).not.toHaveBeenCalled();
  });

  it("still intercepts a fragment-only link even with target=_blank (no allow-popups: scroll beats a dead click)", () => {
    renderDoc(
      '<a id="link" href="#workshop" target="_blank">go</a><h2 id="workshop">W</h2>',
    );
    const scrolled = spyScroll("workshop");

    const event = dispatchClick(document.getElementById("link")!);

    expect(event.defaultPrevented).toBe(true);
    expect(scrolled).toHaveBeenCalled();
  });

  it('treats target="_SELF" case-insensitively for URL+fragment self-references', () => {
    renderDoc(
      '<a id="link" href="page.html#workshop" target="_SELF">go</a><h2 id="workshop">W</h2>',
    );
    const scrolled = spyScroll("workshop");

    const event = dispatchClick(document.getElementById("link")!);

    expect(event.defaultPrevented).toBe(true);
    expect(scrolled).toHaveBeenCalled();
  });

  it("keeps a fragment-only click inert (no navigation) when the target is missing", () => {
    renderDoc('<a id="link" href="#missing">go</a>');
    const scrollTo = vi.fn();
    vi.stubGlobal("scrollTo", scrollTo);

    const event = dispatchClick(document.getElementById("link")!);

    expect(event.defaultPrevented).toBe(true);
    expect(scrollTo).not.toHaveBeenCalled();
    vi.unstubAllGlobals();
  });

  it('scrolls to the top for href="#" and the reserved top fragment', () => {
    renderDoc(
      '<a id="empty" href="#">up</a><a id="top-link" href="#top">up</a>',
    );
    const scrollTo = vi.fn();
    vi.stubGlobal("scrollTo", scrollTo);

    expect(
      dispatchClick(document.getElementById("empty")!).defaultPrevented,
    ).toBe(true);
    expect(
      dispatchClick(document.getElementById("top-link")!).defaultPrevented,
    ).toBe(true);
    expect(scrollTo).toHaveBeenCalledTimes(2);
    expect(scrollTo).toHaveBeenCalledWith(0, 0);
    vi.unstubAllGlobals();
  });

  it("respects clicks already canceled by authored handlers (JS-button idiom)", () => {
    renderDoc('<a id="link" href="#workshop">go</a><h2 id="workshop">W</h2>');
    const scrolled = spyScroll("workshop");
    document
      .getElementById("link")!
      .addEventListener("click", (event) => event.preventDefault());

    dispatchClick(document.getElementById("link")!);

    expect(scrolled).not.toHaveBeenCalled();
  });

  it("ignores modified and non-primary clicks", () => {
    renderDoc('<a id="link" href="#workshop">go</a><h2 id="workshop">W</h2>');
    const scrolled = spyScroll("workshop");

    expect(
      dispatchClick(document.getElementById("link")!, { ctrlKey: true })
        .defaultPrevented,
    ).toBe(false);
    expect(
      dispatchClick(document.getElementById("link")!, { metaKey: true })
        .defaultPrevented,
    ).toBe(false);
    expect(
      dispatchClick(document.getElementById("link")!, { button: 1 })
        .defaultPrevented,
    ).toBe(false);
    expect(scrolled).not.toHaveBeenCalled();
  });

  it("ignores fragment-free links and non-anchor clicks", () => {
    renderDoc(
      '<a id="plain" href="https://example.com/page">out</a><button id="btn">b</button>',
    );

    expect(
      dispatchClick(document.getElementById("plain")!).defaultPrevented,
    ).toBe(false);
    expect(
      dispatchClick(document.getElementById("btn")!).defaultPrevented,
    ).toBe(false);
  });
});

describe("withSandboxedHtmlCsp anchor-nav injection", () => {
  it("injects the [D64] script after the CSP and before authored bytes", () => {
    const out = withSandboxedHtmlCsp(
      "<!doctype html><html><body><h1>Doc</h1></body></html>",
    );
    expect(out).toContain("data-g5-anchor-nav");
    expect(out.indexOf("Content-Security-Policy")).toBeLessThan(
      out.indexOf("data-g5-anchor-nav"),
    );
    expect(out.indexOf("data-g5-anchor-nav")).toBeLessThan(
      out.indexOf("<h1>Doc</h1>"),
    );
  });
});
