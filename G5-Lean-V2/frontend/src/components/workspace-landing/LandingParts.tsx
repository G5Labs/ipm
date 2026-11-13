// =============================================================================
// Workspace landing — shared parts
// =============================================================================
// The pieces every landing section and reader shares (ui g5.workbench.landing.*):
// the one lifecycle status treatment, the row used by both repository lists
// and linked issues, the numbered external link, the clipped preview that is
// the reader's own rendering, and the reader shell with a single close
// control.
// =============================================================================

import { useEffect, useId, useRef, useState, type ReactNode } from "react";
import {
  ChevronDown,
  ChevronRight,
  CircleCheck,
  CircleDashed,
  CircleDot,
  CircleHelp,
  CircleX,
  ExternalLink,
  X,
} from "lucide-react";
import { Link } from "react-router-dom";
import { Modal } from "../ui/UI";
import { LIFECYCLE_LABEL, type LifecycleState } from "../../lib/landingStatus";

const LIFECYCLE_ICON = {
  open: CircleDot,
  draft: CircleDashed,
  merged: CircleCheck,
  closed: CircleX,
  unavailable: CircleHelp,
} as const;

const LIFECYCLE_COLOR: Record<LifecycleState, string> = {
  open: "text-[var(--g5-ok)]",
  draft: "text-[var(--g5-text2)]",
  merged: "text-[var(--g5-status-merged)]",
  closed: "text-[var(--g5-text2)]",
  unavailable: "text-[var(--g5-text2)]",
};

/** One lifecycle state: the same icon, colour and label wherever it appears. */
export function LifecycleStatus({
  state,
  testId,
  className = "",
}: {
  state: LifecycleState;
  testId?: string;
  className?: string;
}) {
  const Icon = LIFECYCLE_ICON[state];
  return (
    <span
      data-testid={testId}
      data-state={state}
      className={`inline-flex shrink-0 items-center gap-1 text-xs ${LIFECYCLE_COLOR[state]} ${className}`}
    >
      <Icon size={14} aria-hidden="true" />
      {LIFECYCLE_LABEL[state]}
    </span>
  );
}

const LINK_BUTTON =
  "inline-flex shrink-0 items-center gap-1.5 rounded border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-2.5 py-1 text-xs text-[var(--g5-text0)] hover:bg-[var(--g5-bg2)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]";

/**
 * A numbered link to where an issue or PR lives: a new tab for GitHub, the
 * in-app page for a native G5 issue.
 */
export function ExternalRecordLink({
  href,
  label,
  internal = false,
  testId,
}: {
  href: string;
  label: string;
  internal?: boolean;
  testId?: string;
}) {
  if (internal) {
    return (
      <Link to={href} data-testid={testId} className={LINK_BUTTON}>
        {label}
      </Link>
    );
  }
  return (
    <a
      href={href}
      target="_blank"
      rel="noopener noreferrer"
      data-testid={testId}
      className={LINK_BUTTON}
    >
      <ExternalLink size={13} aria-hidden="true" />
      {label}
    </a>
  );
}

/**
 * One list row: a leading name, an optional middle value and the trailing
 * status and link. Repository rows and linked-issue rows share it so their
 * spacing, separators and link treatment stay identical.
 */
export function LandingRow({
  lead,
  middle,
  trailing,
  testId,
  repoId,
}: {
  lead: ReactNode;
  middle?: ReactNode;
  trailing?: ReactNode;
  testId?: string;
  /** The repository a repository row stands for. */
  repoId?: string;
}) {
  return (
    <li
      data-testid={testId}
      data-repo-id={repoId}
      className={
        repoId
          ? "grid grid-cols-[minmax(0,1fr)_6rem] items-center gap-x-4 gap-y-1.5 border-t border-[var(--g5-stroke)] py-2.5 @min-[30rem]:grid-cols-[minmax(0,1fr)_6rem_14rem]"
          : "flex flex-wrap items-center gap-x-4 gap-y-1.5 border-t border-[var(--g5-stroke)] py-2.5"
      }
    >
      <div className="min-w-0 flex-1 basis-48">{lead}</div>
      {middle !== undefined && (
        <div className="min-w-0 shrink-0 basis-24 sm:flex-1">{middle}</div>
      )}
      {(trailing || repoId) && (
        <div className="col-span-2 flex min-w-0 flex-wrap items-center justify-end gap-2 @min-[30rem]:col-span-1">
          {trailing}
        </div>
      )}
    </li>
  );
}

/** A concise SHA with the full value on hover and for assistive technology. */
export function ShortSha({ sha }: { sha: string | null | undefined }) {
  if (!sha) {
    return <span className="text-xs text-[var(--g5-text2)]">Unknown</span>;
  }
  return (
    <span
      className="font-mono text-xs text-[var(--g5-text1)]"
      title={sha}
      aria-label={`Commit ${sha}`}
    >
      {sha.slice(0, 7)}
    </span>
  );
}

/**
 * The opening of a rendered body: the reader's own rendering, clipped at
 * display time, so its beginning, formatting and links match the reader.
 */
export function ClippedPreview({
  children,
  testId,
}: {
  children: ReactNode;
  testId?: string;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [clipped, setClipped] = useState(false);
  useEffect(() => {
    const element = ref.current;
    if (!element) return;
    const measure = () =>
      setClipped(element.scrollHeight > element.clientHeight + 1);
    measure();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  }, [children]);
  return (
    <div
      ref={ref}
      data-testid={testId}
      data-clipped={clipped || undefined}
      className={`relative max-h-36 overflow-hidden text-sm ${clipped ? "[mask-image:linear-gradient(to_bottom,black_70%,transparent)]" : ""}`}
    >
      {children}
    </div>
  );
}

/** Read more, at the bottom of a section box. */
export function ReadMoreButton({
  onClick,
  testId,
}: {
  onClick: () => void;
  testId: string;
}) {
  return (
    <button
      type="button"
      data-testid={testId}
      onClick={onClick}
      className="mt-2 text-xs text-[var(--g5-info)] hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]"
    >
      Read more
    </button>
  );
}

/**
 * A long-form reader: the record's own title row, its content, and one ×
 * close control. Escape closes and focus returns to the opener.
 */
export function LandingReader({
  ariaLabel,
  contentLabel,
  heading,
  aside,
  onClose,
  testId,
  children,
}: {
  ariaLabel: string;
  /** Accessible name of the scrollable content region. */
  contentLabel: string;
  heading: ReactNode;
  aside?: ReactNode;
  onClose: () => void;
  testId?: string;
  children: ReactNode;
}) {
  return (
    <Modal
      ariaLabel={ariaLabel}
      size="lg"
      onClose={onClose}
      testId={testId}
      className="flex max-h-[85vh] flex-col p-0"
    >
      <div className="flex items-start gap-3 border-b border-[var(--g5-stroke)] px-5 py-4">
        <div className="min-w-0 flex-1">{heading}</div>
        {aside}
        <button
          type="button"
          data-testid="landing-reader-close"
          aria-label="Close"
          onClick={onClose}
          className="shrink-0 rounded p-1 hover:bg-[var(--g5-bg2)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]"
        >
          <X size={18} aria-hidden="true" />
        </button>
      </div>
      <div
        role="region"
        aria-label={contentLabel}
        // Readers need keyboard access to scroll long descriptions.
        // eslint-disable-next-line jsx-a11y/no-noninteractive-tabindex
        tabIndex={0}
        className="min-h-0 flex-1 overflow-auto px-5 py-4"
      >
        {children}
      </div>
    </Modal>
  );
}

/**
 * A section label outside its box, and the box. With `disclosure` the label
 * is a keyboard-operable toggle and a collapsed box stays mounted but hidden,
 * so its own state (readers, linked issues) survives.
 */
export function LandingSection({
  label,
  testId,
  children,
  disclosure,
}: {
  label: string;
  testId: string;
  children: ReactNode;
  disclosure?: { open: boolean; onToggle: () => void };
}) {
  const bodyId = useId();
  const labelClass =
    "text-xs font-medium uppercase tracking-wide text-[var(--g5-text2)]";
  const open = disclosure ? disclosure.open : true;
  return (
    <section aria-label={label} data-testid={testId} className="mb-6">
      <h2 className={`mb-2 ${labelClass}`}>
        {disclosure ? (
          <button
            type="button"
            data-testid={`${testId}-toggle`}
            aria-expanded={open}
            aria-controls={bodyId}
            onClick={disclosure.onToggle}
            className={`inline-flex items-center gap-1 ${labelClass} hover:text-[var(--g5-text0)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]`}
          >
            {open ? (
              <ChevronDown size={12} aria-hidden="true" />
            ) : (
              <ChevronRight size={12} aria-hidden="true" />
            )}
            {label}
          </button>
        ) : (
          label
        )}
      </h2>
      <div
        id={bodyId}
        hidden={!open}
        className="rounded-lg border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-5 py-4"
      >
        {children}
      </div>
    </section>
  );
}
