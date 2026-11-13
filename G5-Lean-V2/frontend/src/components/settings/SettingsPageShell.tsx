// =============================================================================
// SettingsPageShell — one scrollable sheet with an index on the left
// =============================================================================
// The shared page template for the user Settings view and the organization
// Admin view (ui g5.ui.settings-page, issue #746). The page is ONE scroll:
// every section sits on the sheet, the index highlights the section in view
// and scrolls to a section on click, and `?section=<id>` deep-links a
// section. The index may also carry plain links to pages that stay separate
// tabs (the Admin sub-pages), which it renders under their own heading.
//
// Ownership: the shell owns the scroll container, the scroll-spy and the
// `section` query parameter. Content components own which sections they
// render (gating); each `SettingsSection` registers itself, so the index is
// the sheet's sections in document order and cannot drift from them.
//
// On narrow screens the index is a row above the sheet instead of a column
// beside it; nothing is hidden. On wide screens the sheet keeps a readable
// width and is centered, with its index beside it, in whatever space the
// rail leaves: controls never stretch to fill the viewport.
// =============================================================================

import React, {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useSearchParams } from "react-router-dom";

import { classNames } from "../ui/UI";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface SettingsIndexEntry {
  /** Section id: the `id` of a `SettingsSection` on the sheet and the `?section=` value. */
  id: string;
  label: string;
}

export interface SettingsIndexLink {
  id: string;
  label: string;
  href: string;
  testId?: string;
}

export interface SettingsPageShellProps {
  /** Testid on the shell root. */
  testId?: string;
  /**
   * The `?view=` value this sheet belongs to. A `?section=` hint is consumed
   * only while the URL names this view, so a hint written for one sheet is
   * never taken by the other during a tab switch.
   */
  view: string;
  /** Scope path above the title, e.g. ["g5", "davide-test"]. */
  crumbs: ReadonlyArray<string | null | undefined>;
  title: string;
  /** One line under the title saying whose settings these are. */
  scopeLine?: React.ReactNode;
  /** Controls at the right of the header (chips, a primary action). */
  headerAside?: React.ReactNode;
  /** Heading above the in-page index entries. */
  indexTitle: string;
  /** Links to pages that are separate tabs, listed under their own heading. */
  links?: {
    title: string;
    items: ReadonlyArray<SettingsIndexLink>;
    testId?: string;
  };
  /** Note at the foot of the index (where the other scopes' settings are). */
  indexFooter?: React.ReactNode;
  children: React.ReactNode;
}

/** The query parameter that deep-links a section. */
export const SECTION_PARAM = "section";
/** The query parameter that names the center view. */
const VIEW_PARAM = "view";

/** How far below the sheet top a section counts as "in view". */
const SPY_OFFSET_PX = 96;
/** Scroll-spy pause after a smooth programmatic scroll so passing sections do not flash. */
const SPY_HOLD_MS = 800;
/**
 * How long a deep link keeps re-aligning its section while the sheet's
 * content is still arriving (sections above it grow as their data loads).
 */
const DEEP_LINK_SETTLE_MS = 3000;

function prefersReducedMotion(): boolean {
  return (
    typeof window !== "undefined" &&
    typeof window.matchMedia === "function" &&
    window.matchMedia("(prefers-reduced-motion: reduce)").matches
  );
}

function sectionElement(
  container: HTMLElement,
  id: string,
): HTMLElement | null {
  return container.querySelector<HTMLElement>(
    `[data-settings-section="${id}"]`,
  );
}

function headingId(id: string): string {
  return `settings-section-${id}-title`;
}

// ---------------------------------------------------------------------------
// Section registry — sections announce themselves; the index follows them
// ---------------------------------------------------------------------------

interface Registration {
  id: string;
  label: string;
  element: HTMLElement;
}

interface SheetRegistry {
  register: (entry: Registration) => () => void;
}

const SheetContext = createContext<SheetRegistry | null>(null);

function byDocumentOrder(a: Registration, b: Registration): number {
  if (a.element === b.element) return 0;
  const position = a.element.compareDocumentPosition(b.element);
  if (position & Node.DOCUMENT_POSITION_FOLLOWING) return -1;
  if (position & Node.DOCUMENT_POSITION_PRECEDING) return 1;
  return 0;
}

// ---------------------------------------------------------------------------
// Shell
// ---------------------------------------------------------------------------

export function SettingsPageShell({
  testId,
  view,
  crumbs,
  title,
  scopeLine,
  headerAside,
  indexTitle,
  links,
  indexFooter,
  children,
}: SettingsPageShellProps) {
  const [searchParams, setSearchParams] = useSearchParams();
  const scrollRef = useRef<HTMLDivElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const holdUntilRef = useRef(0);
  const holdTimerRef = useRef<number | null>(null);
  const consumedSectionRef = useRef<string | null>(null);
  // A deep link still settling: re-aligned on content growth until `until`
  // or until the person scrolls themselves.
  const settlingRef = useRef<{ id: string; until: number } | null>(null);

  const [registrations, setRegistrations] = useState<Registration[]>([]);
  const registry = useMemo<SheetRegistry>(
    () => ({
      register: (entry) => {
        setRegistrations((prev) =>
          [...prev.filter((r) => r.id !== entry.id), entry].sort(
            byDocumentOrder,
          ),
        );
        return () => {
          setRegistrations((prev) => prev.filter((r) => r !== entry));
        };
      },
    }),
    [],
  );
  const sections: SettingsIndexEntry[] = useMemo(
    () => registrations.map(({ id, label }) => ({ id, label })),
    [registrations],
  );
  const sectionIds = useMemo(() => sections.map((s) => s.id), [sections]);
  const sectionKey = sectionIds.join("\u0000");

  const [activeId, setActiveId] = useState<string | null>(null);
  const currentId = activeId ?? sectionIds[0] ?? null;

  // The section currently in view: the last one whose top has passed the
  // offset line, or the last section once the sheet is scrolled to its end
  // (a short last section could otherwise never become current).
  const spy = useCallback(() => {
    const container = scrollRef.current;
    if (!container || sectionIds.length === 0) return;
    if (Date.now() < holdUntilRef.current) return;
    const line = container.scrollTop + SPY_OFFSET_PX;
    let current = sectionIds[0];
    for (const id of sectionIds) {
      const el = sectionElement(container, id);
      if (el && el.offsetTop <= line) current = id;
    }
    const atEnd =
      container.scrollTop + container.clientHeight >=
      container.scrollHeight - 2;
    if (atEnd && container.scrollHeight > container.clientHeight) {
      current = sectionIds[sectionIds.length - 1];
    }
    setActiveId((prev) => (prev === current ? prev : current));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sectionKey]);

  useEffect(() => {
    const container = scrollRef.current;
    if (!container) return;
    let frame = 0;
    const onScroll = () => {
      if (frame) return;
      frame = window.requestAnimationFrame(() => {
        frame = 0;
        spy();
      });
    };
    container.addEventListener("scroll", onScroll, { passive: true });
    spy();
    return () => {
      container.removeEventListener("scroll", onScroll);
      if (frame) window.cancelAnimationFrame(frame);
    };
  }, [spy]);

  const scrollToSection = useCallback(
    (id: string, behavior: ScrollBehavior) => {
      const container = scrollRef.current;
      if (!container) return;
      const el = sectionElement(container, id);
      if (!el) return;
      const top = Math.max(0, el.offsetTop - 16);
      if (holdTimerRef.current !== null) {
        window.clearTimeout(holdTimerRef.current);
        holdTimerRef.current = null;
      }
      if (behavior === "smooth") {
        holdUntilRef.current = Date.now() + SPY_HOLD_MS;
        // Reconcile once the hold is over: a scroll interrupted meanwhile
        // (a scrollbar drag, say) would otherwise leave the index stale.
        holdTimerRef.current = window.setTimeout(() => {
          holdTimerRef.current = null;
          holdUntilRef.current = 0;
          spy();
        }, SPY_HOLD_MS);
      } else {
        holdUntilRef.current = 0;
      }
      setActiveId(id);
      if (typeof container.scrollTo === "function") {
        container.scrollTo({ top, behavior });
      } else {
        container.scrollTop = top;
      }
    },
    [spy],
  );

  useEffect(
    () => () => {
      if (holdTimerRef.current !== null) {
        window.clearTimeout(holdTimerRef.current);
      }
    },
    [],
  );

  // `?section=` → scroll there once per value, as soon as that section is on
  // the sheet (gated sections can appear after their data loads), and only
  // while the URL names this sheet's view.
  const requestedSection =
    searchParams.get(VIEW_PARAM) === view
      ? searchParams.get(SECTION_PARAM)
      : null;
  const requestedSectionRef = useRef<string | null>(null);
  requestedSectionRef.current = requestedSection;
  useEffect(() => {
    if (!requestedSection) {
      consumedSectionRef.current = null;
      settlingRef.current = null;
      return;
    }
    if (consumedSectionRef.current === requestedSection) return;
    if (!sectionIds.includes(requestedSection)) return;
    consumedSectionRef.current = requestedSection;
    settlingRef.current = {
      id: requestedSection,
      until: Date.now() + DEEP_LINK_SETTLE_MS,
    };
    scrollToSection(requestedSection, "auto");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [requestedSection, sectionKey, scrollToSection]);

  // Content arriving after the deep link (loaded rows above the section)
  // pushes the section down; keep it aligned while that settles.
  useEffect(() => {
    const content = contentRef.current;
    if (!content || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => {
      const settling = settlingRef.current;
      if (!settling) return;
      if (Date.now() > settling.until) {
        settlingRef.current = null;
        return;
      }
      scrollToSection(settling.id, "auto");
    });
    observer.observe(content);
    return () => observer.disconnect();
  }, [scrollToSection]);

  // The person taking over the scroll ends the settle, the hold and any
  // deep link whose section has not appeared yet. A scrollbar drag starts
  // with a pointerdown on the container itself.
  useEffect(() => {
    const container = scrollRef.current;
    if (!container) return;
    const onUserScrollIntent = () => {
      settlingRef.current = null;
      holdUntilRef.current = 0;
      consumedSectionRef.current = requestedSectionRef.current;
    };
    const events = ["wheel", "touchmove", "keydown", "pointerdown"] as const;
    for (const name of events) {
      container.addEventListener(name, onUserScrollIntent, { passive: true });
    }
    return () => {
      for (const name of events) {
        container.removeEventListener(name, onUserScrollIntent);
      }
    };
  }, []);

  const onIndexClick = useCallback(
    (id: string) => {
      consumedSectionRef.current = id;
      settlingRef.current = null;
      scrollToSection(id, prefersReducedMotion() ? "auto" : "smooth");
      // Focus follows: the next Tab moves on from the section, and a screen
      // reader reads its heading.
      const heading = document.getElementById(headingId(id));
      heading?.focus({ preventScroll: true });
      const next = new URLSearchParams(searchParams);
      next.set(SECTION_PARAM, id);
      setSearchParams(next, { replace: true });
    },
    [scrollToSection, searchParams, setSearchParams],
  );

  const crumbText = crumbs.filter((c): c is string => !!c);

  const indexItemClass = (active: boolean) =>
    classNames(
      "whitespace-nowrap rounded-[var(--g5-radius-sm)] px-2 py-1.5 text-left text-sm transition-colors duration-100",
      "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]",
      active
        ? "bg-[var(--g5-bg2)] font-medium text-[var(--g5-text0)]"
        : "text-[var(--g5-text1)] hover:bg-[var(--g5-overlay-hover)] hover:text-[var(--g5-text0)]",
    );

  return (
    <div
      data-testid={testId}
      className="flex h-full min-h-0 w-full flex-col bg-[var(--g5-bg0)] text-[var(--g5-text0)] md:flex-row md:justify-center"
    >
      {/* Index: outside the scroll container so it stays put. A row above
          the sheet on narrow screens, a column beside it otherwise. */}
      <nav
        aria-label={`${title} index`}
        data-testid="settings-index"
        className={classNames(
          "flex shrink-0 items-center gap-1 overflow-x-auto border-b border-[var(--g5-stroke)] px-4 py-2",
          "md:w-56 md:flex-col md:items-stretch md:gap-0 md:overflow-y-auto md:border-b-0 md:py-6",
        )}
      >
        <p className="hidden px-2 text-xs font-medium text-[var(--g5-text2)] md:block">
          {indexTitle}
        </p>
        <ul className="flex shrink-0 gap-0.5 md:mt-2 md:flex-col">
          {sections.map((section) => {
            const active = section.id === currentId;
            return (
              <li key={section.id} className="flex">
                <button
                  type="button"
                  data-testid={`settings-index-${section.id}`}
                  aria-current={active ? "location" : undefined}
                  onClick={() => onIndexClick(section.id)}
                  className={classNames("w-full", indexItemClass(active))}
                >
                  {section.label}
                </button>
              </li>
            );
          })}
        </ul>

        {links && links.items.length > 0 && (
          <div
            className="flex shrink-0 items-center gap-1 md:mt-6 md:flex-col md:items-stretch md:gap-0"
            data-testid={links.testId}
          >
            <p className="px-2 text-xs font-medium text-[var(--g5-text2)]">
              {links.title}
            </p>
            <ul className="flex shrink-0 gap-0.5 md:mt-2 md:flex-col">
              {links.items.map((link) => (
                <li key={link.id} className="flex">
                  <a
                    href={link.href}
                    data-testid={link.testId}
                    className={classNames(
                      "block w-full",
                      indexItemClass(false),
                    )}
                  >
                    {link.label}
                  </a>
                </li>
              ))}
            </ul>
          </div>
        )}

        {indexFooter && (
          <div className="shrink-0 whitespace-nowrap px-2 text-xs leading-5 text-[var(--g5-text2)] md:mt-auto md:whitespace-normal md:pt-8">
            {indexFooter}
          </div>
        )}
      </nav>

      {/* Sheet: the one scroll container, capped at a readable width and
          centered with the index by the root. `relative` so section offsets
          are measured against it. */}
      <div
        ref={scrollRef}
        data-testid="settings-sheet-scroll"
        className="relative min-h-0 min-w-0 flex-1 overflow-y-auto md:max-w-[1200px] md:pr-4"
      >
        <div
          ref={contentRef}
          className={classNames(
            "my-4 w-full px-6 pb-16 pt-6 md:px-10",
            "rounded-[var(--g5-radius-lg)] border border-[var(--g5-stroke)] bg-[var(--g5-bg1)]",
          )}
        >
          <header className="mb-6 flex flex-wrap items-start justify-between gap-4">
            <div className="min-w-0">
              {crumbText.length > 0 && (
                <p
                  className="truncate text-xs text-[var(--g5-text2)]"
                  data-testid="settings-crumbs"
                >
                  {crumbText.map((crumb, index) => (
                    <React.Fragment key={`${crumb}-${index}`}>
                      {index > 0 && <span className="mx-1.5">/</span>}
                      <span>{crumb}</span>
                    </React.Fragment>
                  ))}
                </p>
              )}
              <h1 className="mt-1 text-2xl font-semibold tracking-tight text-[var(--g5-text0)]">
                {title}
              </h1>
              {scopeLine && (
                <p className="mt-1 text-sm text-[var(--g5-text2)]">
                  {scopeLine}
                </p>
              )}
            </div>
            {headerAside && (
              <div className="flex shrink-0 flex-wrap items-center gap-2">
                {headerAside}
              </div>
            )}
          </header>

          <SheetContext.Provider value={registry}>
            <div className="flex flex-col">{children}</div>
          </SheetContext.Provider>
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Section and row
// ---------------------------------------------------------------------------

export interface SettingsSectionProps {
  /** The index entry's id and the `?section=` value. */
  id: string;
  title: string;
  description?: React.ReactNode;
  testId?: string;
  children: React.ReactNode;
}

/**
 * One indexed area of the sheet: a heading row, then its rows (or one
 * full-width component). Sections are separated by a hairline. Mounting a
 * section lists it in the shell's index at its place on the sheet.
 */
export function SettingsSection({
  id,
  title,
  description,
  testId,
  children,
}: SettingsSectionProps) {
  const registry = useContext(SheetContext);
  const ref = useRef<HTMLElement>(null);
  useLayoutEffect(() => {
    const element = ref.current;
    if (!registry || !element) return;
    return registry.register({ id, label: title, element });
  }, [registry, id, title]);
  return (
    <section
      ref={ref}
      data-settings-section={id}
      data-testid={testId}
      aria-labelledby={headingId(id)}
      className="border-t border-[var(--g5-stroke)] py-8"
    >
      <div className="mb-5">
        <h2
          id={headingId(id)}
          tabIndex={-1}
          className="text-lg font-semibold text-[var(--g5-text0)]"
        >
          {title}
        </h2>
        {description && (
          <p className="mt-1 max-w-prose text-sm text-[var(--g5-text2)]">
            {description}
          </p>
        )}
      </div>
      <div className="flex flex-col gap-6">{children}</div>
    </section>
  );
}

export interface SettingsRowProps {
  title: string;
  description?: React.ReactNode;
  /** One line stating what the setting currently produces. */
  summary?: React.ReactNode;
  /**
   * Draw the panel surface behind the controls. Off for content that brings
   * its own card, so cards never nest.
   */
  surface?: boolean;
  testId?: string;
  children: React.ReactNode;
}

/** A label column beside a panel of controls. */
export function SettingsRow({
  title,
  description,
  summary,
  surface = true,
  testId,
  children,
}: SettingsRowProps) {
  return (
    <div
      data-testid={testId}
      className="grid gap-3 md:grid-cols-[minmax(0,5fr)_minmax(0,7fr)] md:gap-8"
    >
      <div className="min-w-0">
        <h3 className="text-sm font-medium text-[var(--g5-text0)]">{title}</h3>
        {description && (
          <p className="mt-1 text-sm text-[var(--g5-text2)]">{description}</p>
        )}
        {summary && (
          <p className="mt-2 text-sm text-[var(--g5-text1)]">{summary}</p>
        )}
      </div>
      <div
        className={classNames(
          "min-w-0",
          surface && "rounded-[var(--g5-radius-md)] bg-[var(--g5-bg2)] p-4",
        )}
      >
        {children}
      </div>
    </div>
  );
}
