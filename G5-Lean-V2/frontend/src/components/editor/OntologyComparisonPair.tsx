import {
  useLayoutEffect,
  useRef,
  useState,
  type MouseEvent,
  type ReactNode,
} from "react";
import { OntologyBeforeStatement } from "./OntologyBeforeStatement";
import { useChangeHighlights } from "./useChangeHighlights";
import type { ComparisonStatement } from "../../lib/ontologyComparison";

/**
 * One statement in a comparison. Per-file Preview and All changes both render
 * an added or modified statement through this component, so their
 * presentation cannot drift (g5.documents.ontology.statement-changes,
 * g5.documents.ontology.all-changes).
 *
 * With a `status`, the Current row (labelled) is followed by its read-only
 * Before row with no gap. The pair is one list item with the statements'
 * fixed presentation bullet, and its labels, backgrounds, hover outline and
 * metadata footers start after it; `children` is the statement's frame,
 * rendered without a bullet of its own. Without a `status` the statement is
 * unchanged and renders as is. A Modified pair marks what changed inside the
 * statement and, until the reader asks for all of it, shows no Before row
 * when nothing was removed, or only the Before sentences that lost text
 * (g5.documents.ontology.change-highlights). The full Before row the
 * highlights read stays mounted meanwhile, hidden, so its marks survive.
 * The control sits at the bottom of a row's label column: Show before and
 * Hide before on the Current row, Show full and Show less on the Before row.
 * Collapsing leaves the statement where it is; nothing scrolls.
 */
export function OntologyComparisonPair({
  status,
  statementId,
  before,
  id,
  sourceLine,
  className,
  children,
}: {
  status?: "Added" | "Modified";
  statementId: string;
  before?: ComparisonStatement;
  /** Navigation anchor of the current statement in the open document. */
  id?: string;
  /** Source line of the current statement in the open document. */
  sourceLine?: number;
  /** The open document's own layout class for the statement's row. */
  className?: string;
  children: ReactNode;
}) {
  const pair = useRef<HTMLDivElement>(null);
  const layout = useChangeHighlights(pair, status === "Modified" && !!before);
  const [expanded, setExpanded] = useState(false);
  // Whether the clicked control had focus.
  const focused = useRef(false);
  const collapsed = layout.kind !== "full" && !expanded;
  useLayoutEffect(() => {
    if (!focused.current) return;
    focused.current = false;
    const control = [
      ...(pair.current?.querySelectorAll<HTMLElement>(
        "[data-testid='ontology-comparison-toggle']",
      ) ?? []),
    ].find((el) => !el.closest("[hidden]"));
    // Show full and Show less are different buttons, so focus follows.
    if (control && !control.contains(document.activeElement))
      control.focus({ preventScroll: true });
  }, [expanded]);
  const onToggle = (e: MouseEvent<HTMLButtonElement>) => {
    focused.current = e.currentTarget.contains(document.activeElement);
    setExpanded(!expanded);
  };
  const toggle = (label: string) => (
    <button
      type="button"
      className="ontology-comparison-toggle"
      data-testid="ontology-comparison-toggle"
      aria-expanded={expanded}
      aria-label={`${label}, statement ${statementId}`}
      onClick={onToggle}
    >
      {label}
    </button>
  );
  if (!status)
    return (
      <div id={id} data-source-line={sourceLine} className={className}>
        {children}
      </div>
    );
  // Like an unchanged statement, the pair sits in a plain block, which the
  // document column centres at the text measure, so its own indent and
  // bullet stay those of the statements around it (`g5.documents.text-measure`).
  return (
    <div id={id} data-source-line={sourceLine} className={className}>
      <div
        ref={pair}
        className="ontology-comparison-pair"
        data-testid="ontology-comparison-pair"
      >
        <div
          className="ontology-comparison-row ontology-comparison-current"
          role="region"
          aria-label={`${status} statement ${statementId}`}
        >
          <div
            className="ontology-comparison-label"
            data-testid="ontology-change-label"
          >
            {status}
            {layout.kind === "added" &&
              toggle(expanded ? "Hide before" : "Show before")}
          </div>
          {children}
        </div>
        {before && collapsed && layout.kind === "excerpt" && (
          <OntologyBeforeStatement
            statement={before}
            excerpt={layout.excerpt}
            control={toggle("Show full")}
          />
        )}
        {before && (
          <OntologyBeforeStatement
            statement={before}
            source
            hidden={collapsed}
            control={
              layout.kind === "excerpt" && !collapsed
                ? toggle("Show less")
                : undefined
            }
          />
        )}
      </div>
    </div>
  );
}
