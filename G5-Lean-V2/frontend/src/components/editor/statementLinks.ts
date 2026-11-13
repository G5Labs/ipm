import type { MouseEvent } from "react";
import {
  isCurrentOntologyPath,
  resolveOntologyLink,
} from "../../lib/currentOntology";
import { lineFromFragment } from "../files/repoLinks";
import type { StatementFileLinks } from "./OntologyStatementMarkdown";

/** The statement frame's details footer resolves and opens its links the
 * same way as the statement's text. */
export function statementMetadataContext(
  links: StatementFileLinks | undefined,
) {
  return links
    ? {
        filePath: links.filePath,
        repoId: links.repoId,
        onOpenWorkspaceFile: links.onOpenFile,
      }
    : undefined;
}

/** A link to an ontology statement, in this file (`#statement-id`) or
 * another (`chat.g5.md#statement-id`), opens that file at the statement, as
 * the Files view's own statement links do. A single statement's preview
 * cannot follow an in-file link, and the file opener alone keeps only line
 * fragments. Line links, other files' links and historical text (which has
 * no live links) are left to the Markdown preview. */
export function openStatementLinks(links: StatementFileLinks | undefined) {
  if (!links) return undefined;
  return (event: MouseEvent) => {
    const link =
      event.target instanceof Element
        ? event.target.closest<HTMLAnchorElement>("a[href]")
        : null;
    if (!link) return;
    let target: { path: string; fragment: string } | null;
    try {
      target = resolveOntologyLink(
        links.filePath,
        link.getAttribute("href") ?? "",
      );
    } catch {
      return;
    }
    if (
      !target?.fragment ||
      lineFromFragment(target.fragment) ||
      !isCurrentOntologyPath(target.path)
    )
      return;
    event.preventDefault();
    event.stopPropagation();
    links.onOpenFile(target.path, links.repoId, undefined, target.fragment);
  };
}
