// =============================================================================
// MermaidDiagram.test.tsx — component contract for shared-components D8/D8a
// (issue G5Labs/G5#1540)
// =============================================================================
// D8: parse failures fall back to the raw code plus an error message.
// D8a: chunk-load (dynamic-import) failures — e.g. after a deploy replaced
// assets/mermaid.core-<hash>.js — must render the reload affordance instead
// of dead-ending on the raw import error text.
//
// The mermaid package is swapped per test via vi.doMock + vi.resetModules so
// the component's `await import("mermaid")` can be made to resolve or reject.
// =============================================================================

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import React from "react";

const CHUNK_LOAD_ERROR_MESSAGE =
  "Failed to fetch dynamically imported module: https://dev.g5labs.ai/assets/mermaid.core-RpCSaUEy.js";

const DIAGRAM_CODE = "graph TD\n  A[Start] --> B[End]";

// The issue #2325 trigger shape: a literal `\n` escape followed by a second
// colon inside a stateDiagram-v2 edge label (D8d containment regression).
const INVALID_STATE_DIAGRAM = [
  "stateDiagram-v2",
  "  awaiting_auth --> released: X / timeout / worker loss / cancel\\nidempotent cleanup: invalidate generation",
].join("\n");

beforeEach(() => {
  vi.resetModules();
});

afterEach(() => {
  // The theme-persistence test mutates <html data-theme>; keep tests isolated.
  document.documentElement.removeAttribute("data-theme");
});

async function renderMermaidDiagram(
  opts: {
    chunkLoadFailure?: Error;
    renderFailure?: Error;
    svgExtra?: string;
  } = {},
) {
  const { chunkLoadFailure, renderFailure, svgExtra = "" } = opts;
  // data-render-seq uniquely marks each mock render so tests can wait for a
  // NEW render to commit (same-code re-renders produce identical markup
  // otherwise, e.g. the theme-change path). svgExtra injects extra attributes
  // (e.g. mermaid's useMaxWidth style / viewBox) for the D8b scaling tests.
  const renderMock = vi.fn(async (_id: string, code: string) => {
    if (renderFailure) throw renderFailure;
    return {
      svg: `<svg data-mock-mermaid="true" data-render-seq="${renderMock.mock.calls.length}" ${svgExtra}><desc>${code}</desc></svg>`,
    };
  });
  // NOTE: a throwing vi.doMock factory gets wrapped in vitest's own mocking
  // error, which would hide the real browser message. The component uses one
  // try/catch around import + initialize + render, so throwing the genuine
  // chunk-load TypeError from `initialize` reaches the identical catch/
  // classification path with the verbatim message.
  vi.doMock("mermaid", () => ({
    default: {
      initialize: vi.fn(() => {
        if (chunkLoadFailure) throw chunkLoadFailure;
      }),
      render: renderMock,
    },
  }));

  const { MermaidDiagram } = await import("./MermaidDiagram");
  return {
    ...render(<MermaidDiagram code={DIAGRAM_CODE} />),
    MermaidDiagram,
    renderMock,
  };
}

describe("MermaidDiagram", () => {
  it("renders the diagram SVG when the mermaid chunk loads", async () => {
    const { container } = await renderMermaidDiagram();

    await waitFor(() => {
      expect(
        container.querySelector('svg[data-mock-mermaid="true"]'),
      ).toBeTruthy();
    });
    expect(screen.queryByText(/Diagram error/i)).toBeNull();
  });

  it("keeps the raw-code fallback for mermaid parse failures (D8)", async () => {
    await renderMermaidDiagram({
      renderFailure: new Error("Parse error on line 2:\ngraph TD;;\n---^"),
    });

    await waitFor(() => {
      expect(screen.getByText(/Diagram error/i)).toBeInTheDocument();
    });
    // Raw code fallback stays available for parse failures…
    expect(screen.getByText(/A\[Start\] --> B\[End\]/)).toBeInTheDocument();
    // …and the chunk-reload affordance must NOT hijack parse errors.
    expect(screen.queryByTestId("chunk-reload-affordance")).toBeNull();
  });

  it("shows the reload affordance instead of the raw import error on chunk-load failure (D8a — issue 1540 regression)", async () => {
    await renderMermaidDiagram({
      chunkLoadFailure: new TypeError(CHUNK_LOAD_ERROR_MESSAGE),
    });

    // Desired contract (shared-components D8a): a deploy-invalidated chunk is
    // not a diagram authoring error — offer recovery, don't dump the fetch
    // failure on the user.
    const affordance = await screen.findByTestId("chunk-reload-affordance");
    expect(affordance).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /reload/i })).toBeInTheDocument();
    expect(
      screen.queryByText(/Failed to fetch dynamically imported module/i),
    ).toBeNull();
  });

  // Parse-error containment — shared-components D8d (issue G5Labs/G5#2325):
  // mermaid.render appends a temporary `div#d<id>` directly under
  // document.body and, without suppressErrorRendering, draws its full-width
  // "Syntax error in text" SVG into it and throws BEFORE removing it. A
  // mocked rejected render() cannot observe that DOM side effect (the
  // pre-fix test gap), so this scenario runs the REAL mermaid parser.
  describe("parse-error containment (D8d — issue 2325 regression)", () => {
    it("leaves no mermaid temp/error node under document.body after a real parse failure", async () => {
      // Undo any module mock from sibling tests: the real mermaid package
      // must parse (and reject) this diagram.
      vi.doUnmock("mermaid");
      const { MermaidDiagram } = await import("./MermaidDiagram");
      render(<MermaidDiagram code={INVALID_STATE_DIAGRAM} />);

      // The inline parse-error fallback renders inside the component (D8)…
      await waitFor(
        () => {
          expect(screen.getByText(/Diagram error/i)).toBeInTheDocument();
        },
        { timeout: 10_000 },
      );
      expect(
        screen.getByText(/awaiting_auth --> released/),
      ).toBeInTheDocument();

      // …and nothing leaks directly under document.body: no temporary
      // `d<id>` wrapper, no injected error SVG for a generated render id.
      const leaked = Array.from(document.body.children).filter((el) =>
        /^d?g5-mermaid-/.test(el.id),
      );
      expect(leaked.map((el) => `${el.tagName}#${el.id}`)).toEqual([]);
      // Mermaid's error banner text must not appear anywhere in the page.
      expect(document.body.textContent).not.toContain("Syntax error in text");
    });
  });

  // Zoom controls — shared-components D8b (issue G5Labs/G5#1184): diagrams
  // render too small and need in-place magnification.
  describe("zoom controls (D8b)", () => {
    async function renderAndWaitForSvg() {
      const result = await renderMermaidDiagram();
      await waitFor(() => {
        expect(
          result.container.querySelector('svg[data-mock-mermaid="true"]'),
        ).toBeTruthy();
      });
      return result;
    }

    // jsdom's CSSStyleDeclaration drops the `zoom` property, so the component
    // mirrors the level onto `data-zoom` as the observable anchor.
    function viewportZoom(): string | null {
      return screen
        .getByTestId("mermaid-zoom-viewport")
        .getAttribute("data-zoom");
    }

    it("shows zoom in/out/reset controls only for a rendered diagram", async () => {
      await renderAndWaitForSvg();

      expect(screen.getByTestId("mermaid-zoom-controls")).toBeInTheDocument();
      expect(
        screen.getByRole("button", { name: "Zoom in" }),
      ).toBeInTheDocument();
      expect(
        screen.getByRole("button", { name: "Zoom out" }),
      ).toBeInTheDocument();
      expect(
        screen.getByRole("button", { name: "Reset zoom" }),
      ).toHaveTextContent("100%");
      expect(viewportZoom()).toBe("1");
    });

    it("hides zoom controls on the parse-error fallback", async () => {
      await renderMermaidDiagram({
        renderFailure: new Error("Parse error on line 2:\ngraph TD;;\n---^"),
      });

      await waitFor(() => {
        expect(screen.getByText(/Diagram error/i)).toBeInTheDocument();
      });
      expect(screen.queryByTestId("mermaid-zoom-controls")).toBeNull();
    });

    it("zooms the diagram in and out in ×1.25 steps and resets to 100%", async () => {
      const user = userEvent.setup();
      await renderAndWaitForSvg();

      await user.click(screen.getByRole("button", { name: "Zoom in" }));
      await waitFor(() => expect(viewportZoom()).toBe("1.25"));
      expect(
        screen.getByRole("button", { name: "Reset zoom" }),
      ).toHaveTextContent("125%");

      await user.click(screen.getByRole("button", { name: "Zoom in" }));
      await waitFor(() => expect(viewportZoom()).toBe("1.5625"));

      await user.click(screen.getByRole("button", { name: "Reset zoom" }));
      await waitFor(() => expect(viewportZoom()).toBe("1"));
      expect(
        screen.getByRole("button", { name: "Reset zoom" }),
      ).toHaveTextContent("100%");

      await user.click(screen.getByRole("button", { name: "Zoom out" }));
      await waitFor(() => expect(viewportZoom()).toBe("0.8"));
    });

    it("clamps zoom to the 50%–600% range and disables the step buttons at the clamps", async () => {
      const user = userEvent.setup();
      await renderAndWaitForSvg();

      const zoomOut = screen.getByRole("button", { name: "Zoom out" });
      // 1 → 0.8 → 0.64 → 0.512 → clamp at 0.5.
      const zoomOutLadder = ["0.8", "0.64", "0.512", "0.5"];
      for (const expected of zoomOutLadder) {
        await user.click(zoomOut);
        await waitFor(() => expect(viewportZoom()).toBe(expected));
      }
      expect(zoomOut).toBeDisabled();
      expect(
        screen.getByRole("button", { name: "Reset zoom" }),
      ).toHaveTextContent("50%");

      const zoomIn = screen.getByRole("button", { name: "Zoom in" });
      // 0.5 climbs the ×1.25 ladder until the 6× clamp (0.5·1.25¹¹ ≈ 5.82,
      // so the 12th step clamps).
      for (let i = 0; i < 12; i++) {
        await user.click(zoomIn);
      }
      expect(viewportZoom()).toBe("6");
      expect(zoomIn).toBeDisabled();
      expect(
        screen.getByRole("button", { name: "Reset zoom" }),
      ).toHaveTextContent("600%");
    });

    // Natural-size pinning (D8b, chat 8cb31169): mermaid's useMaxWidth form
    // (width 100% + px max-width) re-fits the panel under CSS zoom, capping
    // the diagram to the box. The component must pin the SVG to its natural
    // pixel width so zoom can outgrow the panel.
    it("pins the rendered SVG to its natural pixel width from mermaid's max-width style", async () => {
      const { container } = await renderMermaidDiagram({
        svgExtra: 'style="max-width: 640.5px;" viewBox="0 0 640.5 480"',
      });

      await waitFor(() => {
        const svg = container.querySelector<SVGElement>(
          'svg[data-mock-mermaid="true"]',
        );
        expect(svg?.style.width).toBe("640.5px");
        expect(svg?.style.maxWidth).toBe("none");
      });
    });

    it("falls back to the viewBox width when mermaid emits no pixel max-width", async () => {
      const { container } = await renderMermaidDiagram({
        svgExtra: 'viewBox="0 0 800 600"',
      });

      await waitFor(() => {
        const svg = container.querySelector<SVGElement>(
          'svg[data-mock-mermaid="true"]',
        );
        expect(svg?.style.width).toBe("800px");
        expect(svg?.style.maxWidth).toBe("none");
      });
    });

    it("leaves an SVG without max-width or viewBox untouched", async () => {
      const { container } = await renderMermaidDiagram();

      await waitFor(() => {
        expect(
          container.querySelector('svg[data-mock-mermaid="true"]'),
        ).toBeTruthy();
      });
      const svg = container.querySelector<SVGElement>(
        'svg[data-mock-mermaid="true"]',
      );
      expect(svg?.style.width).toBe("");
    });

    it("keeps the zoom level across code and theme re-renders", async () => {
      const user = userEvent.setup();
      const { rerender, container, MermaidDiagram } =
        await renderAndWaitForSvg();

      await user.click(screen.getByRole("button", { name: "Zoom in" }));
      await waitFor(() => expect(viewportZoom()).toBe("1.25"));

      // Code-edit re-render path (SpecEditor debounce updates the code prop).
      const editedCode = "graph LR\n  X[Edited] --> Y[Diagram]";
      rerender(<MermaidDiagram code={editedCode} />);
      await waitFor(() => {
        expect(
          screen.getByTestId("mermaid-zoom-viewport").textContent,
        ).toContain("X[Edited] --> Y[Diagram]");
      });
      expect(viewportZoom()).toBe("1.25");
      expect(
        screen.getByRole("button", { name: "Reset zoom" }),
      ).toHaveTextContent("125%");

      // Theme-change re-render path (MutationObserver on <html> data-theme).
      // Wait for a NEW render to commit — the seq marker distinguishes the
      // theme-triggered SVG from the still-mounted prior one.
      const seqBefore = container
        .querySelector('svg[data-mock-mermaid="true"]')
        ?.getAttribute("data-render-seq");
      document.documentElement.setAttribute("data-theme", "light");
      await waitFor(() => {
        const svg = container.querySelector('svg[data-mock-mermaid="true"]');
        expect(svg?.getAttribute("data-render-seq")).toBeTruthy();
        expect(svg?.getAttribute("data-render-seq")).not.toBe(seqBefore);
      });
      expect(viewportZoom()).toBe("1.25");
      expect(
        screen.getByRole("button", { name: "Reset zoom" }),
      ).toHaveTextContent("125%");
    });
  });

  // Drag panning — shared-components D8c (chat 8cb31169): a zoomed diagram
  // larger than the panel pans by mouse drag against the scroll offsets.
  describe("drag panning (D8c)", () => {
    // jsdom has no PointerEvent, and fireEvent's plain-Event fallback drops
    // button/pointerId/clientX entirely. Dispatch a MouseEvent (which carries
    // button + coordinates) with the pointer fields defined on top.
    function firePointer(
      el: Element,
      type: "pointerdown" | "pointermove" | "pointerup",
      init: {
        pointerId: number;
        pointerType: "mouse" | "touch";
        clientX?: number;
        clientY?: number;
        button?: number;
      },
    ) {
      const event = new MouseEvent(type, {
        bubbles: true,
        cancelable: true,
        button: init.button ?? 0,
        clientX: init.clientX ?? 0,
        clientY: init.clientY ?? 0,
      });
      Object.defineProperty(event, "pointerId", { value: init.pointerId });
      Object.defineProperty(event, "pointerType", { value: init.pointerType });
      fireEvent(el, event);
    }

    async function renderAndGetPanSurface() {
      const { container } = await renderMermaidDiagram();
      await waitFor(() => {
        expect(
          container.querySelector('svg[data-mock-mermaid="true"]'),
        ).toBeTruthy();
      });
      return screen.getByTestId("mermaid-pan-surface");
    }

    it("scrolls the diagram panel opposite the mouse drag and stops on release", async () => {
      const surface = await renderAndGetPanSurface();

      firePointer(surface, "pointerdown", {
        pointerId: 1,
        pointerType: "mouse",
        clientX: 100,
        clientY: 90,
      });
      firePointer(surface, "pointermove", {
        pointerId: 1,
        pointerType: "mouse",
        clientX: 60,
        clientY: 70,
      });
      expect(surface.scrollLeft).toBe(40);
      expect(surface.scrollTop).toBe(20);

      firePointer(surface, "pointerup", { pointerId: 1, pointerType: "mouse" });
      firePointer(surface, "pointermove", {
        pointerId: 1,
        pointerType: "mouse",
        clientX: 0,
        clientY: 0,
      });
      // Released: further movement no longer pans.
      expect(surface.scrollLeft).toBe(40);
      expect(surface.scrollTop).toBe(20);
    });

    it("ignores touch pointers so native overflow scrolling stays in charge", async () => {
      const surface = await renderAndGetPanSurface();

      firePointer(surface, "pointerdown", {
        pointerId: 2,
        pointerType: "touch",
        clientX: 100,
        clientY: 90,
      });
      firePointer(surface, "pointermove", {
        pointerId: 2,
        pointerType: "touch",
        clientX: 40,
        clientY: 40,
      });
      expect(surface.scrollLeft).toBe(0);
      expect(surface.scrollTop).toBe(0);
    });
  });
});
