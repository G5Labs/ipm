import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { SandboxedHtmlFrame } from "./SandboxedHtmlFrame";
import {
  collectPlatformThemeCss,
  withSandboxedPlatformTheme,
} from "./sandboxedHtml";

function installThemeStylesheet(css: string): HTMLStyleElement {
  const style = document.createElement("style");
  style.dataset.testThemeStylesheet = "true";
  style.textContent = css;
  document.head.appendChild(style);
  return style;
}

const PALETTES = `
:root { --g5-font-sans: system-ui, sans-serif; }
[data-theme="editor"][data-mode="dark"] { --g5-bg0: rgb(10 12 16); --g5-text0: rgb(236 242 255); color-scheme: dark; }
[data-theme="editor"][data-mode="light"] { --g5-bg0: rgb(250 250 250); --g5-text0: rgb(20 24 31); color-scheme: light; }
`;

afterEach(() => {
  document.documentElement.removeAttribute("data-theme");
  document.documentElement.removeAttribute("data-mode");
  document.head
    .querySelectorAll("style[data-test-theme-stylesheet]")
    .forEach((element) => element.remove());
});

describe("SandboxedHtmlFrame", () => {
  it("renders authored HTML in an opaque-origin sandbox with egress blocked", () => {
    render(
      <SandboxedHtmlFrame
        html="<!doctype html><html><body><h1>Uploaded HTML</h1></body></html>"
        title="HTML preview"
      />,
    );

    const frame = screen.getByTitle("HTML preview") as HTMLIFrameElement;
    expect(frame.tagName).toBe("IFRAME");
    expect(frame.getAttribute("sandbox")).toBe("allow-scripts");
    expect(frame.getAttribute("sandbox")).not.toContain("allow-same-origin");
    expect(frame.getAttribute("referrerpolicy")).toBe("no-referrer");

    const srcdoc = frame.getAttribute("srcdoc") ?? "";
    expect(srcdoc).toContain("Content-Security-Policy");
    expect(srcdoc.startsWith("<!doctype html>")).toBe(true);
    expect(srcdoc).toContain("default-src 'none'");
    expect(srcdoc).toContain("connect-src 'none'");
    expect(srcdoc).toContain("<h1>Uploaded HTML</h1>");
    expect(srcdoc.indexOf("Content-Security-Policy")).toBeGreaterThan(
      srcdoc.indexOf("<!doctype html>"),
    );
    expect(srcdoc.indexOf("Content-Security-Policy")).toBeLessThan(
      srcdoc.indexOf("<h1>Uploaded HTML</h1>"),
    );
    // [D64] in-doc anchor navigation script rides along after the CSP.
    expect(srcdoc.indexOf("Content-Security-Policy")).toBeLessThan(
      srcdoc.indexOf("data-g5-anchor-nav"),
    );
    expect(srcdoc.indexOf("data-g5-anchor-nav")).toBeLessThan(
      srcdoc.indexOf("<h1>Uploaded HTML</h1>"),
    );
  });

  it("preserves a leading doctype before the CSP to avoid quirks mode", () => {
    render(
      <SandboxedHtmlFrame
        html={
          '<!DOCTYPE html><html><head><meta charset="utf-8"></head><body></body></html>'
        }
        title="HTML preview with doctype"
      />,
    );

    const frame = screen.getByTitle(
      "HTML preview with doctype",
    ) as HTMLIFrameElement;
    const srcdoc = frame.getAttribute("srcdoc") ?? "";
    expect(srcdoc.startsWith("<!DOCTYPE html>")).toBe(true);
    expect(srcdoc.indexOf("Content-Security-Policy")).toBeGreaterThan(
      srcdoc.indexOf("<!DOCTYPE html>"),
    );
    expect(srcdoc.indexOf("Content-Security-Policy")).toBeLessThan(
      srcdoc.indexOf("<html>"),
    );
  });

  it("prepends the CSP before attacker-controlled head decoys", () => {
    render(
      <SandboxedHtmlFrame
        html="<!-- <head> --><html><head><title>Real</title></head><body></body></html>"
        title="HTML preview with decoy"
      />,
    );

    const frame = screen.getByTitle(
      "HTML preview with decoy",
    ) as HTMLIFrameElement;
    const srcdoc = frame.getAttribute("srcdoc") ?? "";
    expect(
      srcdoc.startsWith('<meta http-equiv="Content-Security-Policy"'),
    ).toBe(true);
    expect(srcdoc.indexOf("Content-Security-Policy")).toBeLessThan(
      srcdoc.indexOf("<!-- <head> -->"),
    );
  });

  it("injects the active platform theme tokens after the CSP and follows a theme switch", async () => {
    installThemeStylesheet(PALETTES);
    document.documentElement.setAttribute("data-theme", "editor");
    document.documentElement.setAttribute("data-mode", "dark");

    render(
      <SandboxedHtmlFrame
        html="<!doctype html><html><body><h1>Doc</h1></body></html>"
        title="Themed preview"
      />,
    );

    const frame = screen.getByTitle("Themed preview") as HTMLIFrameElement;
    const srcdoc = frame.getAttribute("srcdoc") ?? "";
    expect(srcdoc).toContain("data-g5-platform-theme");
    expect(srcdoc).toContain("--g5-bg0: rgb(10 12 16);");
    expect(srcdoc).toContain("color-scheme: dark;");
    expect(srcdoc).toContain("--g5-font-sans:");
    // CSP stays ahead of the theme block, and both precede authored bytes.
    expect(srcdoc.indexOf("Content-Security-Policy")).toBeLessThan(
      srcdoc.indexOf("data-g5-platform-theme"),
    );
    expect(srcdoc.indexOf("data-g5-platform-theme")).toBeLessThan(
      srcdoc.indexOf("<h1>Doc</h1>"),
    );
    // [D64]: the theme block splices between the CSP and the anchor-nav script.
    expect(srcdoc.indexOf("data-g5-platform-theme")).toBeLessThan(
      srcdoc.indexOf("data-g5-anchor-nav"),
    );

    document.documentElement.setAttribute("data-mode", "light");
    await waitFor(() => {
      const updated = frame.getAttribute("srcdoc") ?? "";
      expect(updated).toContain("--g5-bg0: rgb(250 250 250);");
      expect(updated).toContain("color-scheme: light;");
    });
  });

  describe("onOpenWorkspaceFile", () => {
    function mountWithFileHandler() {
      const onOpenWorkspaceFile = vi.fn();
      render(
        <SandboxedHtmlFrame
          html="<!doctype html><html><body><h1>Doc</h1></body></html>"
          title="File nav preview"
          onOpenWorkspaceFile={onOpenWorkspaceFile}
        />,
      );
      const frame = screen.getByTitle("File nav preview") as HTMLIFrameElement;
      const post = (data: unknown, source?: unknown) => {
        fireEvent(
          window,
          new MessageEvent("message", {
            data,
            source: (source ?? frame.contentWindow) as MessageEventSource,
          }),
        );
      };
      return { onOpenWorkspaceFile, post };
    }

    it("dispatches a valid file proposal directly (D68 direct dispatch)", () => {
      const { onOpenWorkspaceFile, post } = mountWithFileHandler();

      post({
        source: "g5-html-doc",
        type: "open-workspace-file",
        filePath: "g5-api/src/services/doc_service.rs",
      });

      expect(onOpenWorkspaceFile).toHaveBeenCalledTimes(1);
      expect(onOpenWorkspaceFile).toHaveBeenCalledWith(
        "g5-api/src/services/doc_service.rs",
        undefined,
        undefined,
      );
    });

    it("forwards a re-validated repo qualifier (issue #1970)", () => {
      const { onOpenWorkspaceFile, post } = mountWithFileHandler();

      post({
        source: "g5-html-doc",
        type: "open-workspace-file",
        filePath: "g5d/src/worker/mod.rs",
        repoRef: "g5-code",
      });

      expect(onOpenWorkspaceFile).toHaveBeenCalledTimes(1);
      expect(onOpenWorkspaceFile).toHaveBeenCalledWith(
        "g5d/src/worker/mod.rs",
        "g5-code",
        undefined,
      );
    });

    it("drops the whole proposal on an invalid repo qualifier — never reinterprets it as ownerless", () => {
      const { onOpenWorkspaceFile, post } = mountWithFileHandler();
      const send = (repoRef: unknown) =>
        post({
          source: "g5-html-doc",
          type: "open-workspace-file",
          filePath: "README.md",
          repoRef,
        });

      send("bad*chars");
      send("two words");
      send("");
      send(42);
      send({});

      expect(onOpenWorkspaceFile).not.toHaveBeenCalled();
    });

    it("forwards a re-validated cited line (g5.documents.file-links.line)", () => {
      const { onOpenWorkspaceFile, post } = mountWithFileHandler();

      post({
        source: "g5-html-doc",
        type: "open-workspace-file",
        filePath: "g5d/src/worker/mod.rs",
        repoRef: "g5-code",
        line: 42,
      });

      expect(onOpenWorkspaceFile).toHaveBeenCalledTimes(1);
      expect(onOpenWorkspaceFile).toHaveBeenCalledWith(
        "g5d/src/worker/mod.rs",
        "g5-code",
        42,
      );
    });

    it("drops the whole proposal on a forged line", () => {
      const { onOpenWorkspaceFile, post } = mountWithFileHandler();
      const send = (line: unknown) =>
        post({
          source: "g5-html-doc",
          type: "open-workspace-file",
          filePath: "README.md",
          line,
        });

      send(0);
      send(-3);
      send(1.5);
      send("12");
      send(Number.MAX_SAFE_INTEGER + 1);
      send(Number.NaN);
      send(null);

      expect(onOpenWorkspaceFile).not.toHaveBeenCalled();
    });

    it("re-validates the path host-side and drops bad paths", () => {
      const { onOpenWorkspaceFile, post } = mountWithFileHandler();
      const send = (filePath: string) =>
        post({ source: "g5-html-doc", type: "open-workspace-file", filePath });

      send("../etc/passwd");
      send("/etc/passwd");
      send("foo\\bar.rs");
      send("foo//bar.rs");
      send("");

      expect(onOpenWorkspaceFile).not.toHaveBeenCalled();
    });
  });
});

describe("collectPlatformThemeCss", () => {
  it("collects only rules matching the active theme attributes, later rules winning", () => {
    installThemeStylesheet(PALETTES);
    document.documentElement.setAttribute("data-theme", "editor");
    document.documentElement.setAttribute("data-mode", "dark");

    const css = collectPlatformThemeCss();
    expect(css.startsWith(":root {")).toBe(true);
    expect(css).toContain("--g5-bg0: rgb(10 12 16);");
    expect(css).toContain("--g5-text0: rgb(236 242 255);");
    expect(css).toContain("--g5-font-sans: system-ui, sans-serif;");
    expect(css).not.toContain("rgb(250 250 250)");

    installThemeStylesheet(
      '[data-theme="editor"][data-mode="dark"] { --g5-bg0: rgb(1 2 3); }',
    );
    expect(collectPlatformThemeCss()).toContain("--g5-bg0: rgb(1 2 3);");
  });

  it("returns an empty string when no theme rules match", () => {
    installThemeStylesheet(PALETTES.split("\n").slice(2).join("\n"));
    expect(collectPlatformThemeCss()).toBe("");
  });
});

describe("withSandboxedPlatformTheme", () => {
  it("keeps CSP-only output when there is no theme CSS", () => {
    const out = withSandboxedPlatformTheme("<p>x</p>", "");
    expect(out).toContain("Content-Security-Policy");
    expect(out).not.toContain("data-g5-platform-theme");
  });

  it("places an explicit theme block after the CSP under a preserved doctype", () => {
    const out = withSandboxedPlatformTheme(
      "<!DOCTYPE html><html><body></body></html>",
      ":root { --g5-bg0: red; }",
    );
    expect(out.startsWith("<!DOCTYPE html>")).toBe(true);
    expect(out.indexOf("Content-Security-Policy")).toBeLessThan(
      out.indexOf("data-g5-platform-theme"),
    );
    expect(out).toContain(":root { --g5-bg0: red; }");
  });
});

describe("file navigation dispatches directly with abuse bounds", () => {
  function postProposal(frame: HTMLIFrameElement, filePath: string) {
    fireEvent(
      window,
      new MessageEvent("message", {
        data: { source: "g5-html-doc", type: "open-workspace-file", filePath },
        source: frame.contentWindow,
      }),
    );
  }

  it("rate-limits a forged proposal loop to a single dispatch", () => {
    const onOpen = vi.fn();
    render(
      <SandboxedHtmlFrame
        html="<!doctype html><html><body>doc</body></html>"
        title="HTML preview"
        onOpenWorkspaceFile={onOpen}
      />,
    );
    const frame = screen.getByTitle("HTML preview") as HTMLIFrameElement;

    // Direct dispatch (user decision 2026-08-10): the first valid proposal
    // navigates; a hostile proposal loop is bounded by the per-frame rate
    // limit instead of a confirm step.
    for (let i = 0; i < 25; i += 1) {
      postProposal(frame, "g5-api/services/doc_service");
    }
    expect(onOpen).toHaveBeenCalledTimes(1);
    expect(onOpen).toHaveBeenCalledWith(
      "g5-api/services/doc_service",
      undefined,
      undefined,
    );
  });

  it("ignores messages that are not from the frame's own contentWindow", () => {
    const onOpen = vi.fn();
    render(
      <SandboxedHtmlFrame
        html="<!doctype html><html><body>doc</body></html>"
        title="HTML preview"
        onOpenWorkspaceFile={onOpen}
      />,
    );
    fireEvent(
      window,
      new MessageEvent("message", {
        data: {
          source: "g5-html-doc",
          type: "open-workspace-file",
          filePath: "g5-api/routes/docs",
        },
        source: window,
      }),
    );
    expect(onOpen).not.toHaveBeenCalled();
  });
});
