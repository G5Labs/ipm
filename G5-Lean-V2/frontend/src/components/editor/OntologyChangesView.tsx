import {
  OntologyStatementMarkdown,
  type StatementFileLinks,
} from "./OntologyStatementMarkdown";
import { statementMetadataContext } from "./statementLinks";
import { useEffect, useRef, type ReactNode } from "react";
import { CurrentOntologyReference } from "./CurrentOntologyReference";
import type { NavigateTarget } from "./IntentTraceabilityPanel";
import { ArrowUpRight, ListFilter, RefreshCw } from "lucide-react";
import {
  useOntologyChanges,
  ontologyChangesError,
  type OntologyChangesResponse,
  type OntologyChange,
} from "../../hooks/useOntologyChanges";
import { SemanticRecordFrame } from "./SemanticRecordFrame";
import { OntologyBeforeStatement } from "./OntologyBeforeStatement";
import { OntologyComparisonPair } from "./OntologyComparisonPair";

export function OntologyChangesButton({
  workspaceId,
  selected,
  onClick,
}: {
  workspaceId: string;
  selected: boolean;
  onClick: () => void;
}) {
  // The sidebar observes the overview cache; navigation itself never reads the corpus.
  const query = useOntologyChanges(workspaceId, false);
  return (
    <button
      type="button"
      data-testid="ontology-changes-open"
      onClick={onClick}
      aria-current={selected ? "page" : undefined}
      className={`flex w-full items-center gap-2 rounded px-3 py-1.5 text-xs transition-colors ${selected ? "bg-[var(--g5-info)]/10 text-[var(--g5-info)]" : "text-[var(--g5-text1)] hover:bg-[var(--g5-bg2)]"}`}
    >
      <ListFilter size={14} />
      <span>View all changes</span>
      {(query.data || query.isError) && (
        <span
          className="ml-auto text-[10px]"
          aria-label="Statement change count"
          title="Last comparison"
        >
          {query.isError || query.data?.incomplete ? "—" : query.data?.count}
        </span>
      )}
    </button>
  );
}

function Baseline({
  repo,
}: {
  repo: OntologyChangesResponse["repositories"][number];
}) {
  return (
    <span
      className="text-xs font-normal text-[var(--g5-text2)]"
      title={`Baseline selection source ${repo.sourceCommit} · Parent ${repo.parentCommit}`}
    >
      Baseline {repo.baselineCommit.slice(0, 10)}
    </span>
  );
}

/** One changed statement: its current version above its read-only Before
 * version, or a deleted statement's prior version. Shared with the chat's
 * ontology changes card (chat g5.chat.ontology-card). */
export function OntologyChangeRow({
  change,
  actions,
  textSuffix,
  editor,
  links,
  scope,
}: {
  change: OntologyChange;
  /** Controls left of the statement's details toggle (the chat card's Edit). */
  actions?: ReactNode;
  /** Inline after the current version's text (the chat card's "(edited)"). */
  textSuffix?: ReactNode;
  /** Replaces the current version's text while it is edited in place. */
  editor?: ReactNode;
  /** Opens the current version's relative links (the chat card). */
  links?: StatementFileLinks;
  /** The statement's file on All changes: relative links and images resolve
   * against it as in the file's own Preview. */
  scope?: { filePath: string; workspaceId: string; repoId: string };
}) {
  if (!change.current)
    return <OntologyBeforeStatement statement={change.before!} deleted />;
  const statement = change.current;
  return (
    <OntologyComparisonPair
      status={change.status === "Added" ? "Added" : "Modified"}
      statementId={statement.record.id}
      before={change.before}
    >
      {editor ?? (
        <SemanticRecordFrame
          record={statement.record}
          tags={[]}
          metadata={statement.metadata}
          metadataContext={statementMetadataContext(links) ?? scope}
          navigationControl={actions}
        >
          <OntologyStatementMarkdown
            content={statement.body}
            suffix={textSuffix}
            links={links}
            {...scope}
          />
        </SemanticRecordFrame>
      )}
    </OntologyComparisonPair>
  );
}

export function OntologyChangesView({
  workspaceId,
  focusFile,
  onOpenFile,
  onNavigate,
}: {
  workspaceId: string;
  /** Open at one file, including a deleted one (g5.documents.ontology.all-changes). */
  focusFile?: { repoId: string; path: string; seq: number };
  onNavigate: (target: NavigateTarget) => void;
  onOpenFile: (file: OntologyChangesResponse["files"][number]) => void;
}) {
  const query = useOntologyChanges(workspaceId);
  const data = query.data;
  const rootRef = useRef<HTMLDivElement>(null);
  const handledFocus = useRef<number | null>(null);
  useEffect(() => {
    if (!focusFile || !data || handledFocus.current === focusFile.seq) return;
    const target = [
      ...(rootRef.current?.querySelectorAll<HTMLElement>(
        "[data-ontology-changes-file]",
      ) ?? []),
    ].find(
      (el) =>
        el.dataset.repoId === focusFile.repoId &&
        el.dataset.ontologyChangesFile === focusFile.path,
    );
    if (!target) return;
    handledFocus.current = focusFile.seq;
    target.scrollIntoView({ behavior: "smooth", block: "start" });
  }, [data, focusFile]);
  const repositories =
    data?.repositories.filter((repo) =>
      data.files.some((file) => file.repoId === repo.repoId),
    ) ?? [];
  return (
    <div
      ref={rootRef}
      className="h-full overflow-y-auto bg-[var(--g5-bg1)]"
      data-testid="ontology-changes-view"
    >
      <div className="flex items-center gap-3 border-b border-[var(--g5-stroke)] px-4 py-2 text-xs text-[var(--g5-text2)]">
        <span>Read-only</span>
        <span>Includes all saved changes, but not unsaved edits.</span>
        <button
          type="button"
          data-testid="ontology-changes-refresh"
          aria-label="Refresh changes"
          aria-busy={query.isFetching}
          title="Refresh changes"
          disabled={query.isFetching}
          onClick={() => void query.refetch()}
          className="ml-auto rounded p-1 hover:bg-[var(--g5-bg2)]"
        >
          <RefreshCw
            size={14}
            className={
              query.isFetching
                ? "animate-spin motion-reduce:animate-none"
                : undefined
            }
          />
        </button>
      </div>
      <div className="px-6 py-6">
        <h1 className="flex items-center gap-2 text-2xl font-semibold text-[var(--g5-text0)]">
          All changes
          {!query.isError && data && (
            <span className="rounded bg-[var(--g5-info)]/10 px-1.5 py-0.5 text-xs text-[var(--g5-info)]">
              {data.count}
              {data.incomplete ? "+" : ""}
            </span>
          )}
        </h1>
        {query.isPending && (
          <p role="status" className="mt-6 text-sm text-[var(--g5-text2)]">
            Loading statement changes…
          </p>
        )}
        {query.isError ? (
          <p role="alert" className="mt-6 text-sm text-[var(--g5-blocked)]">
            Comparison unavailable: {ontologyChangesError(query.error)}
          </p>
        ) : (
          data && (
            <>
              <p role="status" className="mt-1 text-xs text-[var(--g5-text2)]">
                {data.incomplete
                  ? "Some files could not be compared. The count is incomplete."
                  : `${data.files.length} changed ontology ${data.files.length === 1 ? "file" : "files"}`}
                {data.repositories.length === 1 && (
                  <>
                    <span aria-hidden="true"> · </span>
                    <Baseline repo={data.repositories[0]} />
                  </>
                )}
              </p>
              {data.files.length === 0 && (
                <p className="mt-6 text-sm text-[var(--g5-text2)]">
                  No statement changes. Prose, headings and metadata-only
                  changes are not compared.
                </p>
              )}
              {repositories.map((repo) => (
                <section key={repo.repoId} className="mt-4">
                  {data.repositories.length > 1 && (
                    <h2 className="mb-3 flex flex-wrap items-baseline gap-x-3 gap-y-1 text-lg font-semibold">
                      {repo.repoName}
                      <Baseline repo={repo} />
                    </h2>
                  )}
                  {data.files
                    .filter((file) => file.repoId === repo.repoId)
                    .map((file) => (
                      <FileScope
                        workspaceId={workspaceId}
                        file={file}
                        onNavigate={onNavigate}
                        key={file.path}
                        className="mb-7"
                        aria-label={file.path}
                        data-ontology-changes-file={file.path}
                        data-repo-id={file.repoId}
                      >
                        <div className="mb-3 flex flex-wrap items-center gap-2 border-b border-[var(--g5-stroke)] pb-2">
                          <div className="min-w-0 flex-1">
                            <h3 className="break-words text-base font-semibold">
                              {file.title}
                            </h3>
                          </div>
                          <span className="rounded bg-[var(--g5-info)]/10 px-1.5 text-xs text-[var(--g5-info)]">
                            {file.error ? "—" : file.entries.length}
                          </span>
                          {file.current === null ? (
                            <span className="text-xs text-[var(--g5-text2)]">
                              Deleted file
                            </span>
                          ) : (
                            <button
                              type="button"
                              data-testid="ontology-changes-open-file"
                              onClick={() => onOpenFile(file)}
                              className="flex items-center gap-1 rounded border border-[var(--g5-stroke)] px-2 py-1 text-xs text-[var(--g5-text2)] hover:bg-[var(--g5-bg2)]"
                            >
                              Open file <ArrowUpRight size={12} />
                            </button>
                          )}
                        </div>
                        {file.error ? (
                          <p
                            role="alert"
                            className="text-sm text-[var(--g5-blocked)]"
                          >
                            {file.error}
                          </p>
                        ) : (
                          <div className="ontology-statement-list g5-editor-prose">
                            {file.entries.map((change) => (
                              <OntologyChangeRow
                                key={
                                  change.current?.record.id ??
                                  change.before!.record.id
                                }
                                change={change}
                                scope={{
                                  filePath: file.path,
                                  workspaceId,
                                  repoId: file.repoId,
                                }}
                              />
                            ))}
                          </div>
                        )}
                      </FileScope>
                    ))}
                </section>
              ))}
            </>
          )
        )}
      </div>
    </div>
  );
}

function FileScope({
  workspaceId,
  file,
  onNavigate,
  children,
  ...props
}: {
  workspaceId: string;
  file: OntologyChangesResponse["files"][number];
  onNavigate: (target: NavigateTarget) => void;
  children: React.ReactNode;
  className?: string;
  "aria-label"?: string;
  "data-ontology-changes-file"?: string;
  "data-repo-id"?: string;
}) {
  const rootRef = useRef<HTMLDivElement>(null);
  return (
    <div ref={rootRef} {...props}>
      {children}
      {file.current !== null && (
        <CurrentOntologyReference
          workspaceId={workspaceId}
          repoId={file.repoId}
          path={file.path}
          content={file.current}
          rootRef={rootRef}
          onNavigate={onNavigate}
        />
      )}
    </div>
  );
}
