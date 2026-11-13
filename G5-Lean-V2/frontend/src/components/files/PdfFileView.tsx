// =============================================================================
// G5 Files — PdfFileView
// =============================================================================
// A PDF rendered in the browser by Mozilla PDF.js (`g5.documents.viewers.pdf`):
// page navigation, fit to width, zoom presets, rotation, text selection,
// search over the document's own text, and Download. Only pages near the
// viewport are rendered, each to a canvas under a pixel cap with its text
// layer; pages that scroll away release their canvas. A replaced or closed
// document destroys its loading task and worker, and a stale result is
// discarded. Opening shows progress with Cancel; a protected document asks
// for its password with retry and Cancel; a corrupt one says so. Every card
// keeps Download. No annotation layer is rendered, so links and forms inside
// a document are inert, and no scripting manager is ever created.
// =============================================================================

import React, {
  memo,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import {
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  ChevronUp,
  Download,
  RotateCw,
  ZoomIn,
  ZoomOut,
} from "lucide-react";
import { classNames } from "../ui/UI";
import { downloadBlob } from "../../lib/utils";
import { cardButton, FileNoticeCard } from "./FileNoticeCard";
import { formatByteSize } from "./fileBytes";
import {
  loadPdfJs,
  pdfDocumentOptions,
  type PdfJsModule,
  type PDFDocumentLoadingTask,
  type PDFDocumentProxy,
  type PDFPageProxy,
  type RenderTask,
  type TextLayer,
} from "./pdfjs";
import {
  canvasOutputScale,
  CSS_UNITS,
  describePdfError,
  fitWidthScale,
  nextZoomPreset,
  rotatedSize,
  ZOOM_PRESETS,
  type PdfZoom,
  type Size,
} from "./pdfGeometry";
import "./pdfTextLayer.css";
import "./fileViewers.css";

const PAGE_GAP = 16;
const SEARCH_DEBOUNCE_MS = 250;

type DocState =
  | { kind: "loading"; progress: number | null }
  | { kind: "password"; incorrect: boolean }
  | {
      kind: "ready";
      pdfjs: PdfJsModule;
      pageCount: number;
      /** Page 1 at scale 1, the placeholder size of every page. */
      baseSize: Size;
    }
  | { kind: "cancelled" }
  | { kind: "error"; message: string };

const LOADING: DocState = { kind: "loading", progress: null };

interface Load {
  bytes: Uint8Array;
  attempt: number;
}

/** One opening of a document: everything a cleanup must cancel and release. */
interface Session {
  cancelled: boolean;
  task: PDFDocumentLoadingTask | null;
  doc: PDFDocumentProxy | null;
  pages: Map<number, Promise<PDFPageProxy>>;
  text: Map<number, Promise<string[]>>;
  password: ((password: string) => void) | null;
}

interface SearchResult {
  term: string;
  load: Load;
  /** Per page (1-based), the indices of the text runs containing the term. */
  pages: number[][];
  total: number;
  done: boolean;
}

interface Match {
  page: number;
  item: number;
}

const toolbarButton =
  "flex shrink-0 items-center gap-1 rounded px-1.5 py-1 text-xs text-[var(--g5-text1)] hover:bg-[var(--g5-overlay-hover)] disabled:cursor-not-allowed disabled:opacity-50";
const toolbarInput =
  "rounded border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-1.5 py-0.5 text-xs text-[var(--g5-text0)]";

export interface PdfFileViewProps {
  filePath: string;
  /** The document's bytes; each opening hands PDF.js its own copy. */
  bytes: Uint8Array;
  /** The same bytes as `application/pdf`, for Download. */
  blob: Blob;
  byteSize: number;
}

export const PdfFileView: React.FC<PdfFileViewProps> = ({
  filePath,
  bytes,
  blob,
  byteSize,
}) => {
  const name = filePath.split("/").pop() || filePath;
  const [attempt, setAttempt] = useState(0);
  const load = useMemo<Load>(() => ({ bytes, attempt }), [bytes, attempt]);
  // The outcome recorded for the opening now shown; another opening starts
  // over, so a late result of an earlier one is never shown.
  const [stored, setStored] = useState<{ load: Load; state: DocState } | null>(
    null,
  );
  const state: DocState = stored?.load === load ? stored.state : LOADING;
  const sessionRef = useRef<Session | null>(null);

  useEffect(() => {
    const session: Session = {
      cancelled: false,
      task: null,
      doc: null,
      pages: new Map(),
      text: new Map(),
      password: null,
    };
    sessionRef.current = session;
    void (async () => {
      try {
        const pdfjs = await loadPdfJs();
        if (session.cancelled) return;
        // No worker is supplied: PDF.js then creates one for this task alone
        // and destroys it with the task, which it would not do for a worker
        // handed in (it treats that one as shared).
        const task = pdfjs.getDocument({
          data: load.bytes.slice(),
          ...pdfDocumentOptions(),
        });
        session.task = task;
        task.onProgress = ({
          loaded,
          total,
        }: {
          loaded: number;
          total: number;
        }) => {
          if (session.cancelled) return;
          setStored((previous) =>
            previous?.load !== load || previous.state.kind === "loading"
              ? {
                  load,
                  state: {
                    kind: "loading",
                    progress: total > 0 ? Math.min(1, loaded / total) : null,
                  },
                }
              : previous,
          );
        };
        task.onPassword = (
          respond: (password: string) => void,
          reason: number,
        ) => {
          if (session.cancelled) return;
          session.password = respond;
          setStored({
            load,
            state: {
              kind: "password",
              incorrect: reason === pdfjs.PasswordResponses.INCORRECT_PASSWORD,
            },
          });
        };
        const document = await task.promise;
        if (session.cancelled) return;
        session.doc = document;
        const first = await document.getPage(1);
        if (session.cancelled) return;
        session.pages.set(1, Promise.resolve(first));
        const viewport = first.getViewport({ scale: 1 });
        setStored({
          load,
          state: {
            kind: "ready",
            pdfjs,
            pageCount: document.numPages,
            baseSize: { width: viewport.width, height: viewport.height },
          },
        });
      } catch (error) {
        if (session.cancelled) return;
        setStored({
          load,
          state: { kind: "error", message: describePdfError(error) },
        });
      }
    })();
    return () => {
      session.cancelled = true;
      if (sessionRef.current === session) sessionRef.current = null;
      void session.task?.destroy().catch(() => undefined);
    };
  }, [load]);

  const cancelOpen = () => {
    const session = sessionRef.current;
    if (session) {
      session.cancelled = true;
      void session.task?.destroy().catch(() => undefined);
    }
    setStored({ load, state: { kind: "cancelled" } });
  };
  const retry = () => setAttempt((value) => value + 1);

  const [password, setPassword] = useState("");
  const submitPassword = () => {
    const session = sessionRef.current;
    const respond = session?.password;
    if (!session || !respond) return;
    session.password = null;
    setStored({ load, state: LOADING });
    respond(password);
    setPassword("");
  };

  const getPage = useCallback((pageNumber: number): Promise<PDFPageProxy> => {
    const session = sessionRef.current;
    if (!session?.doc) return Promise.reject(new Error("The PDF is closed."));
    let page = session.pages.get(pageNumber);
    if (!page) {
      page = session.doc.getPage(pageNumber);
      session.pages.set(pageNumber, page);
    }
    return page;
  }, []);
  const getText = useCallback(
    (pageNumber: number): Promise<string[]> => {
      const session = sessionRef.current;
      if (!session?.doc) return Promise.resolve([]);
      let text = session.text.get(pageNumber);
      if (!text) {
        text = getPage(pageNumber)
          .then((page) => page.getTextContent())
          .then((content) =>
            content.items.map((item) => ("str" in item ? item.str : "")),
          );
        session.text.set(pageNumber, text);
      }
      return text;
    },
    [getPage],
  );

  // Geometry: fit width is the opening zoom and follows the stage.
  const ready = state.kind === "ready" ? state : null;
  const [zoom, setZoom] = useState<PdfZoom>({ mode: "fit-width" });
  const [rotation, setRotation] = useState(0);
  const scrollerRef = useRef<HTMLDivElement>(null);
  const [stageWidth, setStageWidth] = useState(0);
  useEffect(() => {
    const scroller = scrollerRef.current;
    if (!scroller) return;
    const observer = new ResizeObserver(() =>
      setStageWidth(scroller.clientWidth),
    );
    observer.observe(scroller);
    return () => observer.disconnect();
  }, []);
  const base = ready ? rotatedSize(ready.baseSize, rotation) : null;
  const pdfScale = base
    ? zoom.mode === "fit-width"
      ? fitWidthScale(stageWidth - 2 * PAGE_GAP - 2, base.width)
      : zoom.zoom * CSS_UNITS
    : CSS_UNITS;
  const zoomPercent = Math.round((pdfScale / CSS_UNITS) * 100);
  const placeholder = base
    ? {
        width: Math.round(base.width * pdfScale),
        height: Math.round(base.height * pdfScale),
      }
    : null;

  // Which pages are near the viewport, from an observer over the stage.
  const [visible, setVisible] = useState<ReadonlySet<number>>(() => new Set());
  const pageElements = useRef(new Map<number, HTMLElement>());
  const observerRef = useRef<IntersectionObserver | null>(null);
  useEffect(() => {
    const root = scrollerRef.current;
    if (!root || !ready) return;
    const observer = new IntersectionObserver(
      (entries) => {
        setVisible((previous) => {
          let next: Set<number> | null = null;
          for (const entry of entries) {
            const pageNumber = Number(
              (entry.target as HTMLElement).dataset.pageNumber,
            );
            if (!pageNumber) continue;
            const shown = previous.has(pageNumber);
            if (entry.isIntersecting && !shown) {
              (next ??= new Set(previous)).add(pageNumber);
            } else if (!entry.isIntersecting && shown) {
              (next ??= new Set(previous)).delete(pageNumber);
            }
          }
          return next ?? previous;
        });
      },
      { root, rootMargin: "100% 0px" },
    );
    observerRef.current = observer;
    for (const element of pageElements.current.values())
      observer.observe(element);
    return () => {
      observer.disconnect();
      observerRef.current = null;
    };
  }, [ready]);
  const registerPage = useCallback(
    (pageNumber: number, element: HTMLElement | null) => {
      const elements = pageElements.current;
      const previous = elements.get(pageNumber);
      if (previous && previous !== element)
        observerRef.current?.unobserve(previous);
      if (element) {
        elements.set(pageNumber, element);
        observerRef.current?.observe(element);
      } else {
        elements.delete(pageNumber);
      }
    },
    [],
  );

  // The current page is the one under the upper third of the stage.
  const [currentPage, setCurrentPage] = useState(1);
  const [pageDraft, setPageDraft] = useState<string | null>(null);
  const frame = useRef(0);
  const onScroll = () => {
    if (frame.current) return;
    frame.current = requestAnimationFrame(() => {
      frame.current = 0;
      const scroller = scrollerRef.current;
      if (!scroller) return;
      const probe =
        scroller.getBoundingClientRect().top + scroller.clientHeight * 0.3;
      let found = 1;
      for (const [pageNumber, element] of pageElements.current) {
        const rect = element.getBoundingClientRect();
        if (rect.top > probe) break;
        found = pageNumber;
        if (rect.bottom > probe) break;
      }
      setCurrentPage(found);
    });
  };
  useEffect(() => () => cancelAnimationFrame(frame.current), []);
  const scrollToPage = useCallback((pageNumber: number) => {
    const scroller = scrollerRef.current;
    const element = pageElements.current.get(pageNumber);
    if (!scroller || !element) return;
    scroller.scrollTop +=
      element.getBoundingClientRect().top -
      scroller.getBoundingClientRect().top -
      PAGE_GAP;
    setCurrentPage(pageNumber);
  }, []);
  const goToPage = (value: number) => {
    if (!ready || !Number.isFinite(value)) return;
    scrollToPage(Math.min(ready.pageCount, Math.max(1, Math.round(value))));
  };

  // Search: each page's text runs are fetched once per opening and kept.
  const [query, setQuery] = useState("");
  const term = query.trim().toLowerCase();
  const [matches, setMatches] = useState<SearchResult | null>(null);
  const search =
    matches && matches.term === term && matches.load === load ? matches : null;
  useEffect(() => {
    if (!term || !ready) return;
    let cancelled = false;
    const timer = setTimeout(() => {
      void (async () => {
        const pages: number[][] = [];
        let total = 0;
        for (let pageNumber = 1; pageNumber <= ready.pageCount; pageNumber++) {
          let items: string[] = [];
          try {
            items = await getText(pageNumber);
          } catch {
            items = [];
          }
          if (cancelled) return;
          const hits: number[] = [];
          items.forEach((text, index) => {
            if (text.toLowerCase().includes(term)) hits.push(index);
          });
          pages[pageNumber] = hits;
          total += hits.length;
          const done = pageNumber === ready.pageCount;
          if (done || pageNumber % 8 === 0)
            setMatches({ term, load, pages: pages.slice(), total, done });
        }
      })();
    }, SEARCH_DEBOUNCE_MS);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [term, ready, load, getText]);
  const flat = useMemo(() => {
    const out: Match[] = [];
    search?.pages.forEach((hits, page) => {
      for (const item of hits) out.push({ page, item });
    });
    return out;
  }, [search]);
  const [currentFor, setCurrentFor] = useState<{
    term: string;
    index: number;
  } | null>(null);
  const current =
    currentFor?.term === term
      ? Math.min(currentFor.index, Math.max(flat.length - 1, 0))
      : 0;
  const currentMatch = flat[current] ?? null;
  const stepMatch = (direction: 1 | -1) => {
    if (!flat.length) return;
    setCurrentFor({
      term,
      index: (current + direction + flat.length) % flat.length,
    });
  };
  const jumped = useRef<string | null>(null);
  useEffect(() => {
    if (!currentMatch) {
      jumped.current = null;
      return;
    }
    const key = `${term}:${currentMatch.page}:${currentMatch.item}`;
    if (jumped.current === key) return;
    jumped.current = key;
    scrollToPage(currentMatch.page);
  }, [currentMatch, scrollToPage, term]);
  const searchLabel = !term
    ? ""
    : !search
      ? "Searching…"
      : search.total === 0
        ? search.done
          ? "No matches"
          : "Searching…"
        : `${current + 1} of ${search.total}${search.done ? "" : "…"}`;

  const download = () => downloadBlob(blob, name);

  return (
    <div
      data-testid="pdf-view"
      data-file-path={filePath}
      className="flex h-full min-h-0 flex-col bg-[var(--g5-bg1)]"
    >
      <div
        data-testid="pdf-view-file-row"
        className="flex shrink-0 flex-wrap items-center gap-1 border-b border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-2 py-1 text-xs text-[var(--g5-text2)]"
      >
        <span className="min-w-0 truncate font-medium text-[var(--g5-text0)]">
          {name}
        </span>
        <span className="shrink-0">{formatByteSize(byteSize)}</span>
        <div className="flex-1" />
        <button
          type="button"
          data-testid="pdf-view-prev"
          aria-label="Previous page"
          title="Previous page"
          className={toolbarButton}
          disabled={!ready || currentPage <= 1}
          onClick={() => goToPage(currentPage - 1)}
        >
          <ChevronLeft className="h-3.5 w-3.5" />
        </button>
        <input
          data-testid="pdf-view-page"
          aria-label="Page"
          type="number"
          min={1}
          max={ready?.pageCount ?? 1}
          className={classNames(toolbarInput, "w-12 text-right tabular-nums")}
          disabled={!ready}
          value={pageDraft ?? String(ready ? currentPage : "")}
          onChange={(event) => setPageDraft(event.target.value)}
          onKeyDown={(event) => {
            if (event.key !== "Enter") return;
            event.preventDefault();
            goToPage(Number(pageDraft ?? currentPage));
            setPageDraft(null);
          }}
          onBlur={() => {
            if (pageDraft === null) return;
            goToPage(Number(pageDraft));
            setPageDraft(null);
          }}
        />
        <span data-testid="pdf-view-page-count" className="shrink-0">
          / {ready?.pageCount ?? "–"}
        </span>
        <button
          type="button"
          data-testid="pdf-view-next"
          aria-label="Next page"
          title="Next page"
          className={toolbarButton}
          disabled={!ready || currentPage >= ready.pageCount}
          onClick={() => goToPage(currentPage + 1)}
        >
          <ChevronRight className="h-3.5 w-3.5" />
        </button>
        <span className="mx-1 h-4 w-px bg-[var(--g5-stroke)]" />
        <button
          type="button"
          data-testid="pdf-view-zoom-out"
          aria-label="Zoom out"
          title="Zoom out"
          className={toolbarButton}
          disabled={!ready || zoomPercent <= ZOOM_PRESETS[0] * 100}
          onClick={() =>
            setZoom({
              mode: "fixed",
              zoom: nextZoomPreset(pdfScale / CSS_UNITS, -1),
            })
          }
        >
          <ZoomOut className="h-3.5 w-3.5" />
        </button>
        <span
          data-testid="pdf-view-zoom-level"
          className="w-10 shrink-0 text-center tabular-nums"
        >
          {ready ? `${zoomPercent}%` : ""}
        </span>
        <button
          type="button"
          data-testid="pdf-view-zoom-in"
          aria-label="Zoom in"
          title="Zoom in"
          className={toolbarButton}
          disabled={
            !ready || zoomPercent >= ZOOM_PRESETS[ZOOM_PRESETS.length - 1] * 100
          }
          onClick={() =>
            setZoom({
              mode: "fixed",
              zoom: nextZoomPreset(pdfScale / CSS_UNITS, 1),
            })
          }
        >
          <ZoomIn className="h-3.5 w-3.5" />
        </button>
        <button
          type="button"
          data-testid="pdf-view-fit-width"
          className={classNames(
            toolbarButton,
            zoom.mode === "fit-width" && "text-[var(--g5-active)]",
          )}
          disabled={!ready}
          onClick={() => setZoom({ mode: "fit-width" })}
        >
          Fit width
        </button>
        <button
          type="button"
          data-testid="pdf-view-rotate"
          aria-label="Rotate"
          title="Rotate clockwise"
          className={toolbarButton}
          disabled={!ready}
          onClick={() => setRotation((value) => (value + 90) % 360)}
        >
          <RotateCw className="h-3.5 w-3.5" />
        </button>
        <span className="mx-1 h-4 w-px bg-[var(--g5-stroke)]" />
        <input
          data-testid="pdf-view-search"
          aria-label="Find in document"
          type="search"
          placeholder="Find in document"
          className={classNames(toolbarInput, "w-36")}
          disabled={!ready}
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={(event) => {
            if (event.key !== "Enter") return;
            event.preventDefault();
            stepMatch(event.shiftKey ? -1 : 1);
          }}
        />
        <span
          data-testid="pdf-view-search-count"
          className="min-w-16 shrink-0 tabular-nums"
          role="status"
        >
          {searchLabel}
        </span>
        <button
          type="button"
          data-testid="pdf-view-search-prev"
          aria-label="Previous match"
          title="Previous match"
          className={toolbarButton}
          disabled={!flat.length}
          onClick={() => stepMatch(-1)}
        >
          <ChevronUp className="h-3.5 w-3.5" />
        </button>
        <button
          type="button"
          data-testid="pdf-view-search-next"
          aria-label="Next match"
          title="Next match"
          className={toolbarButton}
          disabled={!flat.length}
          onClick={() => stepMatch(1)}
        >
          <ChevronDown className="h-3.5 w-3.5" />
        </button>
        <button
          type="button"
          data-testid="pdf-view-download"
          className={toolbarButton}
          onClick={download}
        >
          <Download className="h-3.5 w-3.5" />
          Download
        </button>
      </div>
      <div
        ref={scrollerRef}
        data-testid="pdf-view-stage"
        className="min-h-0 flex-1 overflow-auto bg-[var(--g5-bg2)]"
        onScroll={onScroll}
      >
        {ready && placeholder ? (
          <div data-testid="pdf-view-pages" className="g5-pdf-pages">
            {Array.from({ length: ready.pageCount }, (_, index) => {
              const pageNumber = index + 1;
              return (
                <PdfPage
                  key={pageNumber}
                  pdfjs={ready.pdfjs}
                  pageNumber={pageNumber}
                  getPage={getPage}
                  pdfScale={pdfScale}
                  rotation={rotation}
                  visible={visible.has(pageNumber)}
                  placeholder={placeholder}
                  register={registerPage}
                  term={term}
                  currentItem={
                    currentMatch?.page === pageNumber ? currentMatch.item : null
                  }
                />
              );
            })}
          </div>
        ) : state.kind === "loading" ? (
          <div
            data-testid="pdf-view-loading"
            role="status"
            className="flex h-full flex-col items-center justify-center gap-3 p-6 text-sm text-[var(--g5-text1)]"
          >
            <div className="flex items-center gap-2">
              <div className="h-4 w-4 animate-spin rounded-full border-2 border-current border-t-transparent" />
              Opening PDF…
              {state.progress !== null && (
                <span data-testid="pdf-view-progress">
                  {Math.round(state.progress * 100)}%
                </span>
              )}
            </div>
            <button
              type="button"
              data-testid="pdf-view-cancel"
              className={cardButton}
              onClick={cancelOpen}
            >
              Cancel
            </button>
          </div>
        ) : state.kind === "password" ? (
          <form
            data-testid="pdf-view-password"
            className="flex h-full flex-col items-center justify-center gap-3 p-6 text-sm text-[var(--g5-text0)]"
            onSubmit={(event) => {
              event.preventDefault();
              submitPassword();
            }}
          >
            <p role="status">
              {state.incorrect
                ? "That password is incorrect. Try again."
                : "This PDF is password protected."}
            </p>
            <label className="flex items-center gap-2 text-xs">
              Password
              <input
                data-testid="pdf-view-password-input"
                type="password"
                autoComplete="off"
                className={toolbarInput}
                value={password}
                onChange={(event) => setPassword(event.target.value)}
              />
            </label>
            <div className="flex items-center gap-2">
              <button
                type="submit"
                data-testid="pdf-view-password-submit"
                className="flex items-center gap-1 rounded bg-[var(--g5-active)] px-3 py-1 text-xs font-medium text-white disabled:opacity-50"
                disabled={!password}
              >
                Open
              </button>
              <button
                type="button"
                data-testid="pdf-view-password-cancel"
                className={cardButton}
                onClick={cancelOpen}
              >
                Cancel
              </button>
            </div>
          </form>
        ) : state.kind === "cancelled" ? (
          <FileNoticeCard
            testId="pdf-view-notice"
            reason="cancelled"
            filePath={filePath}
            detail="Opening was cancelled."
            onDownload={download}
          >
            <button
              type="button"
              data-testid="pdf-view-retry"
              className={cardButton}
              onClick={retry}
            >
              Try again
            </button>
          </FileNoticeCard>
        ) : state.kind === "error" ? (
          <FileNoticeCard
            testId="pdf-view-notice"
            reason="error"
            filePath={filePath}
            detail={state.message}
            onDownload={download}
          >
            <button
              type="button"
              data-testid="pdf-view-retry"
              className={cardButton}
              onClick={retry}
            >
              Try again
            </button>
          </FileNoticeCard>
        ) : null}
      </div>
    </div>
  );
};

interface PdfPageProps {
  pdfjs: PdfJsModule;
  pageNumber: number;
  getPage: (pageNumber: number) => Promise<PDFPageProxy>;
  pdfScale: number;
  rotation: number;
  visible: boolean;
  placeholder: Size;
  register: (pageNumber: number, element: HTMLElement | null) => void;
  term: string;
  /** The index of this page's text run holding the current match. */
  currentItem: number | null;
}

/**
 * One page: a placeholder of page 1's size until its own page loads, a
 * canvas and text layer while near the viewport, released when it leaves.
 */
const PdfPage = memo(function PdfPage({
  pdfjs,
  pageNumber,
  getPage,
  pdfScale,
  rotation,
  visible,
  placeholder,
  register,
  term,
  currentItem,
}: PdfPageProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const textRef = useRef<HTMLDivElement>(null);
  const textLayerRef = useRef<TextLayer | null>(null);
  const geometry = `${pdfScale}:${rotation}`;
  const [actual, setActual] = useState<{
    geometry: string;
    size: Size;
    userUnit: number;
  } | null>(null);
  const [outcome, setOutcome] = useState<{
    geometry: string;
    status: "rendered" | "failed";
    message?: string;
  } | null>(null);
  const size = actual?.geometry === geometry ? actual.size : placeholder;
  const userUnit = actual?.userUnit ?? 1;
  const rendered =
    outcome?.geometry === geometry && outcome.status === "rendered";
  const failed =
    outcome?.geometry === geometry && outcome.status === "failed"
      ? outcome.message
      : null;

  const ref = useCallback(
    (element: HTMLDivElement | null) => register(pageNumber, element),
    [pageNumber, register],
  );

  useEffect(() => {
    if (!visible) return;
    const canvas = canvasRef.current;
    const container = textRef.current;
    if (!canvas || !container) return;
    let cancelled = false;
    let renderTask: RenderTask | null = null;
    let textLayer: TextLayer | null = null;
    void (async () => {
      try {
        const page = await getPage(pageNumber);
        if (cancelled) return;
        const viewport = page.getViewport({
          scale: pdfScale,
          rotation: (page.rotate + rotation) % 360,
        });
        setActual({
          geometry,
          size: { width: viewport.width, height: viewport.height },
          userUnit: page.userUnit ?? 1,
        });
        const outputScale = canvasOutputScale(
          viewport.width,
          viewport.height,
          window.devicePixelRatio,
        );
        canvas.width = Math.max(1, Math.floor(viewport.width * outputScale));
        canvas.height = Math.max(1, Math.floor(viewport.height * outputScale));
        renderTask = page.render({
          canvas,
          viewport,
          transform:
            outputScale === 1
              ? undefined
              : [outputScale, 0, 0, outputScale, 0, 0],
        });
        await renderTask.promise;
        if (cancelled) return;
        container.replaceChildren();
        textLayer = new pdfjs.TextLayer({
          textContentSource: page.streamTextContent(),
          container,
          viewport,
        });
        textLayerRef.current = textLayer;
        await textLayer.render();
        if (cancelled) return;
        setOutcome({ geometry, status: "rendered" });
      } catch (error) {
        const name = (error as { name?: unknown } | null)?.name;
        if (cancelled || name === "RenderingCancelledException") return;
        setOutcome({
          geometry,
          status: "failed",
          message: error instanceof Error ? error.message : String(error),
        });
      }
    })();
    return () => {
      cancelled = true;
      renderTask?.cancel();
      textLayer?.cancel();
      textLayerRef.current = null;
      canvas.width = 0;
      canvas.height = 0;
      container.replaceChildren();
      // What this run drew is gone; a later run records its own outcome.
      setOutcome((current) =>
        current?.geometry === geometry ? null : current,
      );
    };
  }, [visible, pageNumber, getPage, pdfScale, rotation, geometry, pdfjs]);

  // Matches are marked on the rendered text runs; the current one is
  // brought into view.
  useEffect(() => {
    const layer = textLayerRef.current;
    if (!layer || !rendered) return;
    const { textDivs, textContentItemsStr } = layer;
    textDivs.forEach((div: HTMLElement, index: number) => {
      const hit =
        term !== "" &&
        (textContentItemsStr[index] ?? "").toLowerCase().includes(term);
      div.classList.toggle("g5-pdf-match", hit);
      const isCurrent = hit && index === currentItem;
      div.classList.toggle("g5-pdf-match-current", isCurrent);
      if (isCurrent)
        div.scrollIntoView?.({ block: "center", inline: "nearest" });
    });
  }, [rendered, term, currentItem]);

  return (
    <div
      ref={ref}
      data-testid="pdf-page"
      data-page-number={pageNumber}
      data-rendered={rendered ? "true" : "false"}
      className="g5-pdf-page"
      style={
        {
          width: size.width,
          height: size.height,
          "--scale-factor": String(pdfScale),
          "--user-unit": String(userUnit),
        } as React.CSSProperties
      }
      aria-label={`Page ${pageNumber}`}
    >
      <canvas ref={canvasRef} width={0} height={0} />
      <div ref={textRef} className="textLayer" />
      {failed !== null && (
        <div className="g5-pdf-page-failed" role="status">
          Page {pageNumber} could not be rendered. {failed}
        </div>
      )}
    </div>
  );
});
