import { OntologyStatementMarkdown } from "./OntologyStatementMarkdown";
import { useLayoutEffect, useRef, type ReactNode } from "react";
import { SemanticRecordFrame } from "./SemanticRecordFrame";
import { BEFORE_SOURCE_ATTRIBUTE } from "./useChangeHighlights";
import type { ComparisonStatement } from "../../lib/ontologyComparison";
import type { BeforeExcerpt } from "../../lib/ontologyChangeHighlights";

/**
 * A read-only Before row, under its Current row's bullet in the same pair.
 * Historical display has no editing/navigation identity in the current
 * document. A deleted statement is a comparison item, and bullet, of its own.
 * A Modified pair's full Before row is the `source` its change highlights
 * read, hidden while the pair shows an `excerpt` of the sentences that lost
 * text, or nothing when none did (g5.documents.ontology.change-highlights).
 */
export function OntologyBeforeStatement({
  statement,
  deleted = false,
  className,
  source = false,
  hidden = false,
  excerpt,
  control,
}: {
  statement: ComparisonStatement;
  deleted?: boolean;
  /** The open document's own layout class for a deleted statement's row. */
  className?: string;
  /** The full Before row change highlights compare. */
  source?: boolean;
  hidden?: boolean;
  /** Shown instead of the whole statement body, as plain text. */
  excerpt?: BeforeExcerpt;
  /** At the bottom of the label column: the pair's control for how much of
   * Before it shows. */
  control?: ReactNode;
}) {
  const ref = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const root = ref.current;
    if (!root) return;
    root.querySelectorAll("a[href]").forEach((el) => {
      const target = el.getAttribute("href");
      el.setAttribute("title", `Historical navigation unavailable: ${target}`);
      el.setAttribute(
        "aria-label",
        `${el.textContent}. Historical navigation unavailable: ${target}`,
      );
      el.setAttribute("role", "link");
      el.setAttribute("aria-disabled", "true");
      el.removeAttribute("href");
    });
  });
  const row = (
    <div
      ref={ref}
      data-ontology-before="true"
      data-ontology-deleted-id={deleted ? statement.record.id : undefined}
      {...(source && { [BEFORE_SOURCE_ATTRIBUTE]: "true" })}
      hidden={hidden}
      role="region"
      aria-label={`${deleted ? "Deleted" : "Before"} statement ${statement.record.id}`}
      onKeyDown={(e) => e.stopPropagation()}
      className="ontology-comparison-row ontology-comparison-before"
      onMouseUp={(e) => e.stopPropagation()}
      onContextMenu={(e) => {
        e.preventDefault();
        e.stopPropagation();
      }}
    >
      <div
        className="ontology-comparison-label"
        data-testid="ontology-before-label"
      >
        {deleted ? "Deleted" : "Before"}
        {control}
      </div>
      <SemanticRecordFrame
        historical
        record={statement.record}
        tags={[]}
        metadata={statement.metadata}
      >
        {excerpt ? (
          <div
            className="ontology-statement-text ontology-before-excerpt"
            data-testid="ontology-before-excerpt"
          >
            <p>
              {excerpt.sentences.map((sentence, i) => (
                <span key={i}>
                  {sentence.gapBefore ? (i ? " … " : "… ") : i ? " " : ""}
                  {sentence.parts.map((part, j) =>
                    part.removed ? (
                      <span key={j} className="ontology-excerpt-removed">
                        {part.text}
                      </span>
                    ) : (
                      part.text
                    ),
                  )}
                </span>
              ))}
              {excerpt.gapAfter && " …"}
            </p>
          </div>
        ) : (
          <OntologyStatementMarkdown content={statement.body} historical />
        )}
      </SemanticRecordFrame>
      <p className="sr-only">
        Historical content is read-only. Historical link navigation is
        unavailable.
      </p>
    </div>
  );
  // A deleted statement's item sits in a plain block, as a changed pair does.
  return deleted ? (
    <div className={className}>
      <div className="ontology-comparison-pair ontology-comparison-deleted">
        {row}
      </div>
    </div>
  ) : (
    row
  );
}
