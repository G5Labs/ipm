import React, { createContext, useContext, useMemo } from "react";
import type { Components } from "react-markdown";
import type { Element, Root } from "hast";
import { createMarkdownHeadingSlug } from "../../lib/currentOntology";
import {
  MarkdownPreview,
  type MarkdownPreviewProps,
} from "../files/MarkdownPreview";
import { documentMarkup } from "../files/documentMarkup";
import { SemanticRecordFrame } from "../editor/SemanticRecordFrame";

export interface MarkdownRendererProps {
  content: string;
  className?: string;
  /** Optional base URL/path for relative image sources. */
  basePath?: string;
  /** Keep the User Guide’s established anchor slugs. */
  headingIdMode?: "slug";
  /**
   * Whether the content comes from a trusted author (default `true`). When
   * `false` the renderer is restricted Markdown for externally authored text
   * (e.g. GitHub issues from a domain not marked trusted): mermaid, React
   * Flow, sandboxed-JS widget and question fences render as plain escaped code
   * blocks; `g5://` links and relative/app-route links render as plain text;
   * images render as links and are never loaded; G5 semantic record labels
   * render as ordinary paragraphs.
   */
  trustedContent?: boolean;
  /** A host's rendering of trusted images; see `MarkdownPreview`. */
  renderImage?: MarkdownPreviewProps["renderImage"];
}

const RecordContext = createContext<ReturnType<typeof documentMarkup> | null>(
  null,
);
const components = {
  "g5-record": function ReadOnlyRecord({
    node,
    children,
  }: {
    node: Element;
    children: React.ReactNode;
  }) {
    const markup = useContext(RecordContext)!;
    const record = markup.records[Number(node.properties.recordIndex)];
    return (
      <div
        data-testid={
          record.kind === "Q" ? "markdown-question-frame" : undefined
        }
        className="my-4"
      >
        <SemanticRecordFrame record={record} metadata={markup.metadata(record)}>
          {children}
        </SemanticRecordFrame>
      </div>
    );
  },
} as unknown as Components;

/** Read-only compatibility adapter; parsing and sanitization are shared with
 * files. Memoized on its props, and its plugin list on the parsed markup, so a
 * transcript of thousands of unchanged blocks re-renders none of them when a
 * sibling changes (chat g5.chat.live-tail-recovery). */
export const MarkdownRenderer = React.memo(function MarkdownRenderer({
  content,
  className,
  headingIdMode,
  basePath,
  trustedContent = true,
  renderImage,
}: MarkdownRendererProps) {
  const markup = useMemo(() => documentMarkup(content || ""), [content]);
  const plugins = useMemo(() => {
    const headings = () => (tree: Root) => {
      const counts = new Map<string, number>();
      const visit = (parent: Root | Element) => {
        for (const node of parent.children)
          if (node.type === "element") {
            if (/^h[1-6]$/.test(node.tagName)) {
              const index = markup.blocks.findIndex(
                (b) =>
                  b.type === "heading" &&
                  b.sourceRange?.start === node.position?.start.offset,
              );
              const block = markup.blocks[index];
              if (block?.type === "heading" && headingIdMode === "slug") {
                const base =
                  createMarkdownHeadingSlug(block.content) ||
                  `section-${index}`;
                const count = (counts.get(base) ?? 0) + 1;
                counts.set(base, count);
                const slug = count === 1 ? base : `${base}-${count}`;
                node.properties.id = slug;
              }
            }
            visit(node);
          }
      };
      visit(tree);
    };
    return [...(trustedContent ? [markup.plugin] : []), headings];
  }, [markup, headingIdMode, trustedContent]);
  if (!content) return null;
  return (
    <div
      className={className}
      data-testid="markdown-renderer"
      data-content-trust={trustedContent ? "trusted" : "restricted"}
    >
      <RecordContext.Provider value={markup}>
        <MarkdownPreview
          content={content}
          trustedContent={trustedContent}
          basePath={basePath}
          renderImage={renderImage}
          plugins={plugins}
          components={trustedContent ? components : undefined}
        />
      </RecordContext.Provider>
    </div>
  );
});
