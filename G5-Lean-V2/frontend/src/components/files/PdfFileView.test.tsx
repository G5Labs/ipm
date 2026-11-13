import React from "react";
// `render` is aliased: vitest's mock hoisting reads the stand-in's `render`
// property keys as references to the import and would hoist it too early.
import {
  act,
  fireEvent,
  render as mount,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const downloads = vi.hoisted(() => ({ calls: [] as [Blob, string][] }));
vi.mock("../../lib/utils", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../lib/utils")>()),
  downloadBlob: (blob: Blob, name: string) =>
    downloads.calls.push([blob, name]),
}));

/** A PDF.js stand-in whose documents and loading tasks the test controls. */
const pdfjs = vi.hoisted(() => {
  interface Deferred<T> {
    promise: Promise<T>;
    resolve: (value: T) => void;
    reject: (error: unknown) => void;
  }
  function deferred<T>(): Deferred<T> {
    let resolve!: (value: T) => void;
    let reject!: (error: unknown) => void;
    const promise = new Promise<T>((res, rej) => {
      resolve = res;
      reject = rej;
    });
    return { promise, resolve, reject };
  }
  interface RenderCall {
    scale: number;
    rotation: number;
    canvas: HTMLCanvasElement;
    cancel: () => void;
  }
  function page(number: number, items: string[]) {
    const renders: RenderCall[] = [];
    return {
      number,
      rotate: 0,
      userUnit: 1,
      items,
      renders,
      getViewport: ({
        scale,
        rotation,
      }: {
        scale: number;
        rotation: number;
      }) => ({
        width: (rotation % 180 ? 792 : 612) * scale,
        height: (rotation % 180 ? 612 : 792) * scale,
        scale,
        rotation,
      }),
      render: ({
        canvas,
        viewport,
      }: {
        canvas: HTMLCanvasElement;
        viewport: { scale: number; rotation: number };
      }) => {
        const call: RenderCall = {
          scale: viewport.scale,
          rotation: viewport.rotation,
          canvas,
          cancel: vi.fn(),
        };
        renders.push(call);
        return { promise: Promise.resolve(), cancel: call.cancel };
      },
      streamTextContent: () => ({ items }),
      getTextContent: async () => ({ items: items.map((str) => ({ str })) }),
    };
  }
  type FakePage = ReturnType<typeof page>;
  function document(pages: string[][]) {
    const built = pages.map((items, index) => page(index + 1, items));
    return {
      numPages: built.length,
      pages: built,
      getPage: async (number: number): Promise<FakePage> => built[number - 1],
    };
  }
  type FakeDocument = ReturnType<typeof document>;
  interface Task {
    params: Record<string, unknown>;
    promise: Promise<FakeDocument>;
    resolve: (document: FakeDocument) => void;
    reject: (error: unknown) => void;
    destroy: ReturnType<typeof vi.fn>;
    onProgress: ((progress: { loaded: number; total: number }) => void) | null;
    onPassword:
      | ((respond: (password: string) => void, reason: number) => void)
      | null;
  }
  const tasks: Task[] = [];
  class TextLayer {
    textDivs: HTMLElement[] = [];
    textContentItemsStr: string[] = [];
    constructor({
      textContentSource,
      container,
    }: {
      textContentSource: { items: string[] };
      container: HTMLElement;
    }) {
      for (const str of textContentSource.items) {
        const span = window.document.createElement("span");
        span.textContent = str;
        container.appendChild(span);
        this.textDivs.push(span);
        this.textContentItemsStr.push(str);
      }
    }
    render = vi.fn(async () => undefined);
    cancel = vi.fn();
  }
  class PDFWorker {
    destroy = vi.fn();
  }
  const module = {
    version: "6.4.299",
    GlobalWorkerOptions: { workerSrc: "" },
    PasswordResponses: { NEED_PASSWORD: 1, INCORRECT_PASSWORD: 2 },
    PDFWorker,
    TextLayer,
    getDocument: vi.fn((params: Record<string, unknown>): Task => {
      const { promise, resolve, reject } = deferred<FakeDocument>();
      const task: Task = {
        params,
        promise,
        resolve,
        reject,
        destroy: vi.fn(async () => undefined),
        onProgress: null,
        onPassword: null,
      };
      tasks.push(task);
      return task;
    }),
  };
  const state = {
    module,
    tasks,
    document,
    load: (): Promise<typeof module> => Promise.resolve(module),
  };
  return state;
});

vi.mock("./pdfjs", () => ({
  loadPdfJs: () => pdfjs.load(),
  pdfDocumentOptions: () => ({ cMapUrl: "/assets/pdfjs/6.4.299/cmaps/" }),
}));

import { PdfFileView } from "./PdfFileView";
import { CSS_UNITS } from "./pdfGeometry";

/** An IntersectionObserver the test drives. */
class FakeIntersectionObserver {
  static instances: FakeIntersectionObserver[] = [];
  observed = new Set<Element>();
  constructor(
    public callback: IntersectionObserverCallback,
    public options?: IntersectionObserverInit,
  ) {
    FakeIntersectionObserver.instances.push(this);
  }
  observe = (element: Element) => {
    this.observed.add(element);
  };
  unobserve = (element: Element) => {
    this.observed.delete(element);
  };
  disconnect = vi.fn();
  takeRecords = () => [];
  /** Reports the listed pages as near the viewport and the rest as away. */
  show(pages: number[]) {
    const entries = Array.from(this.observed, (target) => ({
      target,
      isIntersecting: pages.includes(
        Number((target as HTMLElement).dataset.pageNumber),
      ),
    })) as IntersectionObserverEntry[];
    act(() => this.callback(entries, this as unknown as IntersectionObserver));
  }
  static get latest() {
    return this.instances[this.instances.length - 1];
  }
}
const realIntersectionObserver = window.IntersectionObserver;

const bytesA = new Uint8Array([0x25, 0x50, 0x44, 0x46, 1]);
const bytesB = new Uint8Array([0x25, 0x50, 0x44, 0x46, 2]);

function view(bytes = bytesA, filePath = "docs/report.pdf") {
  return mount(
    <PdfFileView
      filePath={filePath}
      bytes={bytes}
      blob={new Blob([bytes], { type: "application/pdf" })}
      byteSize={bytes.length}
    />,
  );
}

async function task(index = 0) {
  await waitFor(() => expect(pdfjs.tasks.length).toBeGreaterThan(index));
  return pdfjs.tasks[index];
}

async function openDocument(pages: string[][], index = 0) {
  const current = await task(index);
  const document = pdfjs.document(pages);
  await act(async () => current.resolve(document));
  await screen.findByTestId("pdf-view-pages");
  return document;
}

beforeEach(() => {
  downloads.calls.length = 0;
  pdfjs.tasks.length = 0;
  pdfjs.module.getDocument.mockClear();
  pdfjs.load = () => Promise.resolve(pdfjs.module);
  FakeIntersectionObserver.instances.length = 0;
  window.IntersectionObserver =
    FakeIntersectionObserver as unknown as typeof IntersectionObserver;
});
afterEach(() => {
  window.IntersectionObserver = realIntersectionObserver;
});

// `g5.documents.viewers.pdf`
describe("PdfFileView", () => {
  it("opens a document with a task-owned worker and a copy of the bytes, renders pages near the viewport and releases pages that leave", async () => {
    view();
    expect(screen.getByTestId("pdf-view-loading")).toBeInTheDocument();
    const current = await task();
    // The task owns its worker, so destroying the task destroys the worker.
    expect(current.params.worker).toBeUndefined();
    expect(current.params.cMapUrl).toBe("/assets/pdfjs/6.4.299/cmaps/");
    const data = current.params.data as Uint8Array;
    expect(Array.from(data)).toEqual(Array.from(bytesA));
    expect(data).not.toBe(bytesA);

    const document = await openDocument([
      ["Hello G5 page 1 of 3", "The quarterly report"],
      ["Hello G5 page 2 of 3"],
      ["Hello G5 page 3 of 3"],
    ]);
    expect(screen.getByTestId("pdf-view-page-count")).toHaveTextContent("/ 3");
    const pages = screen.getAllByTestId("pdf-page");
    expect(pages).toHaveLength(3);
    // Nothing renders until the observer reports a page near the viewport.
    expect(document.pages[0].renders).toHaveLength(0);
    // Fit width without a measured stage shows the page at 100%.
    expect(screen.getByTestId("pdf-view-zoom-level")).toHaveTextContent("100%");
    expect(pages[0].style.width).toBe(`${Math.round(612 * CSS_UNITS)}px`);

    FakeIntersectionObserver.latest.show([1, 2]);
    await waitFor(() =>
      expect(pages[0]).toHaveAttribute("data-rendered", "true"),
    );
    await waitFor(() =>
      expect(pages[1]).toHaveAttribute("data-rendered", "true"),
    );
    expect(pages[2]).toHaveAttribute("data-rendered", "false");
    expect(document.pages[0].renders).toHaveLength(1);
    expect(document.pages[0].renders[0].scale).toBeCloseTo(CSS_UNITS);
    expect(document.pages[2].renders).toHaveLength(0);
    const canvas = pages[0].querySelector("canvas")!;
    expect(canvas.width).toBe(Math.floor(612 * CSS_UNITS));
    expect(pages[0].querySelectorAll(".textLayer span")).toHaveLength(2);
    expect(pages[0].style.getPropertyValue("--scale-factor")).toBe(
      String(CSS_UNITS),
    );

    // Page 2 scrolls away: its canvas and text layer are released.
    FakeIntersectionObserver.latest.show([1]);
    await waitFor(() =>
      expect(pages[1]).toHaveAttribute("data-rendered", "false"),
    );
    expect(pages[1].querySelector("canvas")!.width).toBe(0);
    expect(pages[1].querySelectorAll(".textLayer span")).toHaveLength(0);
    expect(document.pages[1].renders[0].cancel).toHaveBeenCalled();
    expect(pages[1].style.width).toBe(`${Math.round(612 * CSS_UNITS)}px`);

    fireEvent.click(screen.getByTestId("pdf-view-download"));
    expect(downloads.calls).toEqual([[expect.any(Blob), "report.pdf"]]);
    expect(downloads.calls[0][0].type).toBe("application/pdf");
  });

  it("shows progress, and Cancel destroys the task and offers Download and Try again", async () => {
    view();
    const current = await task();
    act(() => current.onProgress?.({ loaded: 50, total: 100 }));
    expect(screen.getByTestId("pdf-view-progress")).toHaveTextContent("50%");
    fireEvent.click(screen.getByTestId("pdf-view-cancel"));
    expect(current.destroy).toHaveBeenCalled();
    const notice = screen.getByTestId("pdf-view-notice");
    expect(notice).toHaveAttribute("data-reason", "cancelled");
    // A result arriving after Cancel changes nothing.
    await act(async () => current.resolve(pdfjs.document([["late"]])));
    expect(screen.getByTestId("pdf-view-notice")).toBeInTheDocument();
    fireEvent.click(screen.getByTestId("pdf-view-notice-download"));
    expect(downloads.calls[0][1]).toBe("report.pdf");

    fireEvent.click(screen.getByTestId("pdf-view-retry"));
    expect(screen.getByTestId("pdf-view-loading")).toBeInTheDocument();
    await openDocument([["again"]], 1);
    expect(screen.getByTestId("pdf-view-page-count")).toHaveTextContent("/ 1");
  });

  it("asks for a password, re-asks after a wrong one and cancels", async () => {
    view();
    const current = await task();
    const respond = vi.fn();
    act(() =>
      current.onPassword?.(
        respond,
        pdfjs.module.PasswordResponses.NEED_PASSWORD,
      ),
    );
    const form = screen.getByTestId("pdf-view-password");
    expect(form).toHaveTextContent("password protected");
    expect(screen.getByTestId("pdf-view-password-submit")).toBeDisabled();
    fireEvent.change(screen.getByTestId("pdf-view-password-input"), {
      target: { value: "wrong" },
    });
    fireEvent.click(screen.getByTestId("pdf-view-password-submit"));
    expect(respond).toHaveBeenCalledWith("wrong");
    expect(screen.getByTestId("pdf-view-loading")).toBeInTheDocument();

    act(() =>
      current.onPassword?.(
        respond,
        pdfjs.module.PasswordResponses.INCORRECT_PASSWORD,
      ),
    );
    expect(screen.getByTestId("pdf-view-password")).toHaveTextContent(
      "incorrect",
    );
    fireEvent.change(screen.getByTestId("pdf-view-password-input"), {
      target: { value: "secret" },
    });
    fireEvent.submit(screen.getByTestId("pdf-view-password"));
    expect(respond).toHaveBeenLastCalledWith("secret");
    await openDocument([["opened"]]);

    // Cancel on the prompt destroys the task and keeps Download.
    const second = view(bytesB, "docs/other.pdf");
    const task2 = await task(1);
    act(() =>
      task2.onPassword?.(vi.fn(), pdfjs.module.PasswordResponses.NEED_PASSWORD),
    );
    fireEvent.click(
      second.container.querySelector<HTMLElement>(
        '[data-testid="pdf-view-password-cancel"]',
      )!,
    );
    expect(task2.destroy).toHaveBeenCalled();
    expect(
      second.container.querySelector('[data-testid="pdf-view-notice"]'),
    ).toHaveAttribute("data-reason", "cancelled");
  });

  it("names an invalid document and keeps Download", async () => {
    view();
    const current = await task();
    await act(async () =>
      current.reject({ name: "InvalidPDFException", message: "bad xref" }),
    );
    const notice = screen.getByTestId("pdf-view-notice");
    expect(notice).toHaveAttribute("data-reason", "error");
    expect(notice).toHaveTextContent("not a valid PDF");
    fireEvent.click(screen.getByTestId("pdf-view-notice-download"));
    expect(downloads.calls[0][1]).toBe("report.pdf");
  });

  it("destroys a replaced document's task and ignores its late result; unmounting destroys too", async () => {
    const first = view(bytesA);
    const task1 = await task();
    first.rerender(
      <PdfFileView
        filePath="docs/report.pdf"
        bytes={bytesB}
        blob={new Blob([bytesB], { type: "application/pdf" })}
        byteSize={bytesB.length}
      />,
    );
    expect(task1.destroy).toHaveBeenCalled();
    const task2 = await task(1);
    await act(async () => task1.resolve(pdfjs.document([["old"], ["old"]])));
    expect(screen.getByTestId("pdf-view-loading")).toBeInTheDocument();
    await act(async () => task2.resolve(pdfjs.document([["new"]])));
    expect(screen.getByTestId("pdf-view-page-count")).toHaveTextContent("/ 1");
    first.unmount();
    expect(task2.destroy).toHaveBeenCalled();
  });

  it("re-renders visible pages for zoom presets and rotation, and fit width follows the stage", async () => {
    view();
    const document = await openDocument([["one"], ["two"]]);
    const pages = screen.getAllByTestId("pdf-page");
    FakeIntersectionObserver.latest.show([1]);
    await waitFor(() => expect(document.pages[0].renders).toHaveLength(1));

    fireEvent.click(screen.getByTestId("pdf-view-zoom-in"));
    expect(screen.getByTestId("pdf-view-zoom-level")).toHaveTextContent("125%");
    await waitFor(() => expect(document.pages[0].renders).toHaveLength(2));
    expect(document.pages[0].renders[1].scale).toBeCloseTo(1.25 * CSS_UNITS);
    expect(document.pages[0].renders[0].cancel).toHaveBeenCalled();
    // A page not near the viewport only takes the new placeholder size.
    expect(document.pages[1].renders).toHaveLength(0);
    expect(pages[1].style.width).toBe(
      `${Math.round(612 * 1.25 * CSS_UNITS)}px`,
    );

    fireEvent.click(screen.getByTestId("pdf-view-zoom-out"));
    expect(screen.getByTestId("pdf-view-zoom-level")).toHaveTextContent("100%");
    await waitFor(() => expect(document.pages[0].renders).toHaveLength(3));
    // A zoom before the previous render started cancels it: the page then
    // renders once, at the newest zoom.
    fireEvent.click(screen.getByTestId("pdf-view-zoom-out"));
    expect(screen.getByTestId("pdf-view-zoom-level")).toHaveTextContent("75%");
    fireEvent.click(screen.getByTestId("pdf-view-rotate"));
    await waitFor(() => expect(document.pages[0].renders).toHaveLength(4));
    expect(document.pages[0].renders[3].rotation).toBe(90);
    expect(document.pages[0].renders[3].scale).toBeCloseTo(0.75 * CSS_UNITS);
    await waitFor(() =>
      expect(pages[0].style.width).toBe(
        `${Math.round(792 * 0.75 * CSS_UNITS)}px`,
      ),
    );
    expect(pages[1].style.height).toBe(
      `${Math.round(612 * 0.75 * CSS_UNITS)}px`,
    );

    fireEvent.click(screen.getByTestId("pdf-view-fit-width"));
    expect(screen.getByTestId("pdf-view-zoom-level")).toHaveTextContent("100%");
  });

  it("navigates pages with the buttons and the page field", async () => {
    view();
    await openDocument([["one"], ["two"], ["three"]]);
    const field = screen.getByTestId<HTMLInputElement>("pdf-view-page");
    expect(field.value).toBe("1");
    expect(screen.getByTestId("pdf-view-prev")).toBeDisabled();
    fireEvent.click(screen.getByTestId("pdf-view-next"));
    expect(field.value).toBe("2");
    fireEvent.change(field, { target: { value: "9" } });
    fireEvent.keyDown(field, { key: "Enter" });
    expect(field.value).toBe("3");
    expect(screen.getByTestId("pdf-view-next")).toBeDisabled();
    fireEvent.click(screen.getByTestId("pdf-view-prev"));
    expect(field.value).toBe("2");
  });

  it("counts text runs containing the term, highlights them and steps through matches across pages", async () => {
    view();
    const document = await openDocument([
      ["Hello G5", "The quarterly report", "Quarterly again"],
      ["Nothing here"],
      ["quarterly on page three"],
    ]);
    const pages = screen.getAllByTestId("pdf-page");
    FakeIntersectionObserver.latest.show([1]);
    await waitFor(() =>
      expect(pages[0]).toHaveAttribute("data-rendered", "true"),
    );
    fireEvent.change(screen.getByTestId("pdf-view-search"), {
      target: { value: "Quarterly" },
    });
    await waitFor(() =>
      expect(screen.getByTestId("pdf-view-search-count")).toHaveTextContent(
        "1 of 3",
      ),
    );
    const spans = pages[0].querySelectorAll(".textLayer span");
    expect(spans[0]).not.toHaveClass("g5-pdf-match");
    expect(spans[1]).toHaveClass("g5-pdf-match", "g5-pdf-match-current");
    expect(spans[2]).toHaveClass("g5-pdf-match");
    expect(spans[2]).not.toHaveClass("g5-pdf-match-current");

    fireEvent.click(screen.getByTestId("pdf-view-search-next"));
    expect(screen.getByTestId("pdf-view-search-count")).toHaveTextContent(
      "2 of 3",
    );
    expect(spans[2]).toHaveClass("g5-pdf-match-current");
    // Enter in the field steps too, and the third match is on page 3.
    fireEvent.keyDown(screen.getByTestId("pdf-view-search"), { key: "Enter" });
    expect(screen.getByTestId("pdf-view-search-count")).toHaveTextContent(
      "3 of 3",
    );
    expect(screen.getByTestId<HTMLInputElement>("pdf-view-page").value).toBe(
      "3",
    );
    FakeIntersectionObserver.latest.show([3]);
    await waitFor(() =>
      expect(pages[2].querySelector(".textLayer span")).toHaveClass(
        "g5-pdf-match-current",
      ),
    );
    fireEvent.click(screen.getByTestId("pdf-view-search-next"));
    expect(screen.getByTestId("pdf-view-search-count")).toHaveTextContent(
      "1 of 3",
    );
    expect(document.pages[1].renders).toHaveLength(0);

    fireEvent.change(screen.getByTestId("pdf-view-search"), {
      target: { value: "" },
    });
    expect(screen.getByTestId("pdf-view-search-count")).toHaveTextContent("");
  });

  it("finds nothing in a document without text", async () => {
    view();
    await openDocument([[], []]);
    fireEvent.change(screen.getByTestId("pdf-view-search"), {
      target: { value: "anything" },
    });
    await waitFor(() =>
      expect(screen.getByTestId("pdf-view-search-count")).toHaveTextContent(
        "No matches",
      ),
    );
    expect(screen.getByTestId("pdf-view-search-next")).toBeDisabled();
  });
});
