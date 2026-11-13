import React, { useMemo } from "react";
import type { Block } from "../../lib/utils";
import { classNames } from "../ui/UI";

export interface OutlineQuestionRecord {
  /** The record's start block, the frame's `data-block-index`. */
  index: number;
  id: string;
  title: string;
  state: "open" | "proposed" | "decided" | "invalid";
}

interface EditorOutlineProps {
  blocks: Block[];
  rootRef: React.RefObject<HTMLDivElement | null>;
  /** Unanswered `g5-question` count for this doc (D69); 0 hides the badge. */
  openQuestionCount?: number;
  /** Question records in document order; absent for documents without them. */
  questionRecords?: readonly OutlineQuestionRecord[];
}

type OutlineEntry =
  | {
      kind: "heading";
      index: number;
      block: Extract<Block, { type: "heading" }>;
    }
  | { kind: "question"; index: number; record: OutlineQuestionRecord };

const OUTLINE_QUESTION_DOT: Record<OutlineQuestionRecord["state"], string> = {
  open: "bg-[var(--g5-warn)]",
  proposed: "bg-[var(--g5-warn)]/50",
  decided: "bg-[var(--g5-ok)]",
  invalid: "bg-[var(--g5-blocked)]",
};

export const DocumentOutline: React.FC<EditorOutlineProps> = ({
  blocks,
  rootRef,
  questionRecords = [],
}) => {
  const entries = useMemo<OutlineEntry[]>(() => {
    const headings: OutlineEntry[] = blocks
      .map((b, i) => ({ block: b, index: i }))
      .filter(
        (
          e,
        ): e is {
          block: Extract<Block, { type: "heading" }>;
          index: number;
        } =>
          e.block.type === "heading" &&
          ("level" in e.block ? e.block.level <= 3 : false),
      )
      .map((e) => ({ kind: "heading", index: e.index, block: e.block }));
    const questions: OutlineEntry[] = questionRecords.map((record) => ({
      kind: "question",
      index: record.index,
      record,
    }));
    return [...headings, ...questions].sort((a, b) => a.index - b.index);
  }, [blocks, questionRecords]);
  const headings = entries.filter((e) => e.kind === "heading");

  if (entries.length === 0) return null;

  const scrollTo = (index: number) => {
    const el = rootRef.current?.querySelector(`[data-block-index="${index}"]`);
    if (el) {
      // Use the nearest scrollable ancestor (the flex-1 overflow-y-auto container)
      const scrollContainer = el.closest(".overflow-y-auto");
      if (scrollContainer && typeof scrollContainer.scrollTo === "function") {
        const containerRect = scrollContainer.getBoundingClientRect();
        const elRect = el.getBoundingClientRect();
        const scrollTop =
          scrollContainer.scrollTop + (elRect.top - containerRect.top) - 80;
        scrollContainer.scrollTo({
          top: Math.max(0, scrollTop),
          behavior: "smooth",
        });
      } else {
        el.scrollIntoView({ behavior: "smooth", block: "center" });
      }
    }
  };

  return (
    <div
      id="document-outline"
      data-testid="editor-outline"
      className={classNames(
        "group/outline shrink-0 sticky top-0 self-start",
        "w-8 hover:w-44 focus-within:w-44 transition-all duration-200 ease-out overflow-hidden",
        "border-r border-[var(--g5-stroke)]/40",
        "py-6 px-1",
        "max-h-screen overflow-y-auto scrollbar-none",
      )}
    >
      <div className="flex flex-col gap-1.5">
        {entries.map((entry) => {
          if (entry.kind === "question") {
            // Nested one level under the nearest heading above it.
            const above = headings.filter((h) => h.index < entry.index).at(-1);
            const level =
              above && above.kind === "heading" ? (above.block.level ?? 1) : 0;
            const { record } = entry;
            return (
              <button
                key={`q-${entry.index}`}
                type="button"
                data-testid="document-outline-question"
                data-question-state={record.state}
                className={classNames(
                  "flex items-center gap-2 text-left cursor-pointer",
                  "text-[var(--g5-text2)] hover:text-[var(--g5-text0)] transition-colors",
                  "whitespace-nowrap",
                )}
                style={{ paddingLeft: level * 8 }}
                onClick={() => scrollTo(entry.index)}
                title={`${record.id} · ${record.title} (${record.state})`}
              >
                <span
                  className={classNames(
                    "shrink-0 rounded-full w-1.5 h-1.5",
                    OUTLINE_QUESTION_DOT[record.state],
                  )}
                />
                <span
                  className={classNames(
                    "opacity-0 group-hover/outline:opacity-100 group-focus-within/outline:opacity-100 transition-opacity duration-200",
                    "text-[10px] truncate max-w-[120px]",
                    record.state === "decided" ? "line-through" : "font-normal",
                  )}
                >
                  {record.id} · {record.title}
                </span>
              </button>
            );
          }
          const { block, index } = entry;
          const level = block.level ?? 1;
          const indent = (level - 1) * 8;
          return (
            <button
              key={index}
              type="button"
              data-testid="document-outline-heading"
              className={classNames(
                "flex items-center gap-2 text-left cursor-pointer",
                "text-[var(--g5-text2)] hover:text-[var(--g5-text0)] transition-colors",
                "whitespace-nowrap",
              )}
              style={{ paddingLeft: indent }}
              onClick={() => scrollTo(index)}
              title={block.content}
            >
              {/* Dot indicator (always visible) */}
              <span
                className={classNames(
                  "shrink-0 rounded-full bg-[var(--g5-text2)]/40",
                  level === 1
                    ? "w-2 h-2"
                    : level === 2
                      ? "w-1.5 h-1.5"
                      : "w-1 h-1",
                )}
              />
              {/* Label (visible on hover) */}
              <span
                className={classNames(
                  "opacity-0 group-hover/outline:opacity-100 group-focus-within/outline:opacity-100 transition-opacity duration-200",
                  "text-[10px] truncate max-w-[120px]",
                  level === 1 ? "font-semibold" : "font-normal",
                )}
              >
                {block.content.replace(/[*_`#]/g, "")}
              </span>
            </button>
          );
        })}
      </div>
    </div>
  );
};
