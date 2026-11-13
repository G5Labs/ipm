import type { ReactNode } from "react";
import {
  MarkdownPreview,
  type MarkdownPreviewProps,
} from "../files/MarkdownPreview";

/** Where a statement's relative links resolve and open: its file, as the
 * Files view resolves them (the chat card's statements). */
export interface StatementFileLinks {
  filePath: string;
  repoId: string;
  /** A link to a statement (`file.g5.md#id`, `#id`) also names it, so the
   * file opens at that statement. */
  onOpenFile: (
    filePath: string,
    repoId?: string,
    line?: number,
    statementId?: string,
  ) => void;
}

/** Statement and historical bodies share the sanitized file Markdown pipeline. */
export function OntologyStatementMarkdown({
  content,
  historical = false,
  suffix,
  links,
  filePath,
  workspaceId,
  repoId,
}: {
  content: string;
  historical?: boolean;
  /** Inline after the last paragraph's text (the chat card's "(edited)"). */
  suffix?: ReactNode;
  links?: StatementFileLinks;
} & Partial<
  Pick<MarkdownPreviewProps, "filePath" | "workspaceId" | "repoId">
>) {
  return (
    <div
      className={`ontology-statement-text ontology-statement-markdown g5-document-links${suffix ? " ontology-statement-suffixed" : ""}`}
      data-testid="ontology-statement-markdown"
    >
      <MarkdownPreview
        content={content}
        historical={historical}
        filePath={links?.filePath ?? filePath}
        workspaceId={workspaceId}
        repoId={links?.repoId ?? repoId}
        onOpenWorkspaceFile={links?.onOpenFile}
      />
      {suffix}
    </div>
  );
}
