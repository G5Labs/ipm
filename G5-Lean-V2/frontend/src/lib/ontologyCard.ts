import type { ChatMessageDTO } from "./types";
import {
  ontologyStatements,
  type ComparisonStatement,
} from "./ontologyComparison";

/** A turn's ontology changes card as stored on its reply
 * (`chat_messages.artifacts.ontology_card`, chat g5.chat.ontology-card). */
export interface OntologyCardStatement {
  /** Authored source from the statement's anchor to its end. */
  source: string;
  /** Its footer metadata entry, without the `### id` heading. */
  metadata: string;
}
export interface OntologyCardChange {
  id: string;
  change: "added" | "modified" | "deleted";
  before?: OntologyCardStatement;
  after?: OntologyCardStatement;
}
export interface OntologyCardFile {
  repoId: string;
  rootPath: string;
  path: string;
  refusal?: string;
  changes?: OntologyCardChange[];
}
export interface OntologyCardGoverning {
  id: string;
  repoId: string;
  path: string;
  reason: string;
  /** Absent when the name was not found when the turn ended. */
  statement?: OntologyCardStatement | null;
}
export interface OntologyCardEdit {
  idempotencyKey: string;
  repoId: string;
  path: string;
  statementId: string;
  text: string;
  editedBy: string;
  editedAt: string;
}
export interface OntologyCard {
  schema: "g5.ontology-card.v1";
  files?: OntologyCardFile[];
  governing?: OntologyCardGoverning[];
  /** The turn's changes were too large to carry on the card. */
  tooLarge?: boolean;
  edits?: OntologyCardEdit[];
}

/** The card a reply carries, if any. Platform objects share `artifacts`. */
export function messageOntologyCard(
  message: Pick<ChatMessageDTO, "artifacts" | "role">,
): OntologyCard | null {
  if (message.role !== "assistant") return null;
  const artifacts = message.artifacts as unknown;
  if (!artifacts || typeof artifacts !== "object" || Array.isArray(artifacts))
    return null;
  const card = (artifacts as Record<string, unknown>).ontology_card;
  if (!card || typeof card !== "object") return null;
  return (card as OntologyCard).schema === "g5.ontology-card.v1"
    ? (card as OntologyCard)
    : null;
}

/** Rebuild one stored statement version with the editor's own parser, as
 * the server rebuilds it: `{source}## Metadata\n\n### {id}{metadata}`. */
export function cardStatement(
  id: string,
  part: OntologyCardStatement | null | undefined,
): ComparisonStatement | null {
  if (
    !part ||
    typeof part.source !== "string" ||
    typeof part.metadata !== "string"
  )
    return null;
  try {
    return (
      ontologyStatements(
        `${part.source}## Metadata\n\n### ${id}${part.metadata}`,
      ).find((statement) => statement.record.id === id) ?? null
    );
  } catch {
    return null;
  }
}

export interface OntologyCardEntry {
  key: string;
  id: string;
  status: "Added" | "Modified" | "Deleted";
  current: ComparisonStatement | null;
  before: ComparisonStatement | null;
  /** The owner's latest edit of this statement on the card. */
  edit: OntologyCardEdit | null;
}
export interface OntologyCardFileEntries {
  key: string;
  file: OntologyCardFile;
  title: string;
  entries: OntologyCardEntry[];
}

const STATUS = {
  added: "Added",
  modified: "Modified",
  deleted: "Deleted",
} as const;

/** Each member's root page first, then by path. */
function byFileOrder(a: { path: string }, b: { path: string }): number {
  return (
    (a.path === ".g5/ontology/root.g5.md"
      ? -1
      : b.path === ".g5/ontology/root.g5.md"
        ? 1
        : 0) || a.path.localeCompare(b.path)
  );
}

function fileTitle(path: string): string {
  return path.replace(/^\.g5\/ontology\//, "").replace(/(?:\.g5)?\.md$/, "");
}

/** The array a card field holds, or none: the card is stored data, so a
 * malformed one shows less rather than breaking the chat. */
function list<T>(value: T[] | undefined): T[] {
  return Array.isArray(value) ? value : [];
}

function named(value: { repoId?: unknown; path?: unknown }): boolean {
  return typeof value.repoId === "string" && typeof value.path === "string";
}

/** The card's files in file order (each member's root page first), with each
 * statement rebuilt and the owner's latest edit attached. */
export function cardFiles(card: OntologyCard): OntologyCardFileEntries[] {
  const edits = new Map<string, OntologyCardEdit>();
  for (const edit of list(card.edits))
    edits.set(
      `${edit.repoId}\u0000${edit.path}\u0000${edit.statementId}`,
      edit,
    );
  return list(card.files)
    .filter((file) => file && named(file))
    .sort(byFileOrder)
    .map((file) => ({
      key: `${file.repoId}\u0000${file.path}`,
      file: {
        ...file,
        refusal: typeof file.refusal === "string" ? file.refusal : undefined,
      },
      title: fileTitle(file.path),
      entries: list(file.changes)
        .filter((change) => change && typeof change.id === "string")
        .map((change) => ({
          key: `${file.repoId}\u0000${file.path}\u0000${change.id}`,
          id: change.id,
          status: STATUS[change.change] ?? "Modified",
          current: cardStatement(change.id, change.after),
          before: cardStatement(change.id, change.before),
          edit:
            edits.get(`${file.repoId}\u0000${file.path}\u0000${change.id}`) ??
            null,
        })),
    }));
}

export function cardCounts(files: OntologyCardFileEntries[]) {
  const counts = { Added: 0, Modified: 0, Deleted: 0 };
  for (const file of files)
    for (const entry of file.entries) counts[entry.status] += 1;
  return counts;
}

/** The newest reply carrying a card: the only one whose statements can be
 * edited (chat g5.chat.ontology-card.edit). */
export function newestOntologyCardMessageId(
  messages: ChatMessageDTO[],
): string | null {
  let newest: ChatMessageDTO | null = null;
  for (const message of messages)
    if (
      messageOntologyCard(message) &&
      (!newest || message.messageIndex > newest.messageIndex)
    )
      newest = message;
  return newest?.id ?? null;
}

export interface OntologyCardGoverningFile {
  key: string;
  repoId: string;
  path: string;
  title: string;
  entries: OntologyCardGoverning[];
}

/** The governing statements grouped by file, in the changes' file order and
 * the agent's order within a file. A name among the card's own changes is left
 * out, because the card already shows it; matching by repository and ID keeps
 * a statement moved between files out too. */
export function cardGoverningFiles(
  card: OntologyCard,
): OntologyCardGoverningFile[] {
  const changed = new Set<string>();
  for (const file of list(card.files))
    for (const change of list(file?.changes))
      changed.add(`${file.repoId}\u0000${change?.id}`);
  const files = new Map<string, OntologyCardGoverningFile>();
  for (const raw of list(card.governing)) {
    if (!raw || !named(raw) || typeof raw.id !== "string") continue;
    const entry = {
      ...raw,
      reason: typeof raw.reason === "string" ? raw.reason : "",
    };
    if (changed.has(`${entry.repoId}\u0000${entry.id}`)) continue;
    const key = `${entry.repoId}\u0000${entry.path}`;
    let file = files.get(key);
    if (!file) {
      file = {
        key,
        repoId: entry.repoId,
        path: entry.path,
        title: fileTitle(entry.path),
        entries: [],
      };
      files.set(key, file);
    }
    file.entries.push(entry);
  }
  return [...files.values()].sort(byFileOrder);
}
