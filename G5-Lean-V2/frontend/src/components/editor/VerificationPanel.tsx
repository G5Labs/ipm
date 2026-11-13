// =============================================================================
// VerificationPanel — a statement's verification status and saved evidence
// =============================================================================
// `g5.documents.ontology.verification.details`: for each linked state machine,
// its title and saved checking question, then on request its saved bindings
// and recorded runs, shown as saved; then the bugs/issues and design
// simplifications its metadata links, with their severity and decision
// (`g5.documents.findings`). No status changes are offered.
// =============================================================================

import { useState } from "react";
import type { StateMachineLink } from "../../lib/currentOntology";
import {
  STATUS_LABEL,
  hasVerificationDetails,
  type LinkEvidence,
  type StatementStatus,
  type StatusKind,
} from "../../lib/verificationStatus";
import {
  artifactPath,
  concernRuns,
  concernTitle,
  outcomeMatch,
  parseManifest,
  runDate,
  type RunRow,
} from "../../lib/verificationArtifacts";
import {
  useFirstVerificationText,
  useRunIndex,
  useVerificationText,
  type Read,
} from "../../hooks/useVerificationEvidence";
import {
  parseFindings,
  type FindingLink,
  type Severity,
} from "../../lib/findings";
import {
  MarkdownPreview,
  type MarkdownPreviewProps,
} from "../files/MarkdownPreview";

export function VerificationStatusButton({
  status,
  open,
  controls,
  onToggle,
  dot = null,
}: {
  status: StatusKind;
  open: boolean;
  controls: string;
  onToggle: () => void;
  /** The most severe linked finding a Design verified status marks. */
  dot?: Severity | null;
}) {
  if (!hasVerificationDetails(status))
    return (
      <span
        data-testid="verification-status"
        data-status={status}
        className={`verification-status verification-status-${status}`}
        aria-label={`Verification: ${STATUS_LABEL[status]}`}
      >
        {STATUS_LABEL[status]}
      </span>
    );
  return (
    <button
      type="button"
      data-testid="verification-status"
      data-status={status}
      className={`verification-status verification-status-${status}`}
      aria-expanded={open}
      aria-controls={controls}
      aria-label={`Verification: ${STATUS_LABEL[status]}${dot ? `, with a ${dot} finding` : ""}`}
      title={
        dot
          ? `Verification details · most severe linked finding: ${dot}`
          : "Verification details"
      }
      onClick={onToggle}
    >
      {STATUS_LABEL[status]}
      {dot && (
        <span
          className={`verification-status-dot finding-dot-${dot.toLowerCase()}`}
          data-testid="verification-status-dot"
          data-severity={dot}
          aria-hidden="true"
        />
      )}
    </button>
  );
}

export interface VerificationPanelProps {
  id: string;
  workspaceId: string;
  repoId: string;
  status: StatementStatus;
  links: readonly StateMachineLink[];
  /** Design verified descendants, with the start of their lead. */
  verifiedChildren: readonly { id: string; text: string }[];
  onRevealStatement: (id: string) => void;
  /** Reads the evidence again after a failed read. */
  onRetry: () => void;
  onOpenFile?: (path: string, line?: number) => void;
  /** The statement's metadata details: its evidence for other tags. */
  details: string;
  /** Findings entries the statement's metadata links. */
  findings?: readonly FindingLink[];
  /** Opens a findings file at an entry. */
  onOpenFinding?: (path: string, id: string) => void;
  detailsContext: Pick<
    MarkdownPreviewProps,
    "filePath" | "workspaceId" | "repoId" | "onOpenWorkspaceFile" | "plugins"
  >;
}

export function VerificationPanel({
  id,
  workspaceId,
  repoId,
  status,
  links,
  verifiedChildren,
  onRevealStatement,
  onRetry,
  onOpenFile,
  details,
  detailsContext,
  findings = [],
  onOpenFinding,
}: VerificationPanelProps) {
  const [tag] = status.tags.length === 1 ? status.tags : [];
  const designTag = tag === "design-verified";
  return (
    <section
      id={id}
      className="verification-panel"
      data-testid="verification-panel"
      aria-label="Verification details"
    >
      <h3 className="verification-panel-title">{STATUS_LABEL[status.shown]}</h3>
      {status.tags.length > 1 && (
        <p data-testid="verification-tag-conflict">
          This statement has more than one verification tag (
          {status.tags.join(", ")}), so it sets no status.
        </p>
      )}
      {verifiedChildren.length > 0 && (
        <div data-testid="verification-children">
          <p>Design verified through child statements:</p>
          <ul>
            {verifiedChildren.map((child) => (
              <li key={child.id}>
                <button
                  type="button"
                  className="verification-link"
                  data-testid="verification-child-link"
                  onClick={() => onRevealStatement(child.id)}
                >
                  {child.text}
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}
      {tag && !designTag && status.own !== status.shown && (
        <p>Its own tag: {STATUS_LABEL[tag]}.</p>
      )}
      {/* Other tags show what the statement's metadata records. */}
      {tag &&
        !designTag &&
        (details.trim() ? (
          <div
            className="verification-metadata"
            data-testid="verification-metadata"
          >
            <MarkdownPreview {...detailsContext} content={details} />
          </div>
        ) : (
          <p>Its metadata records no further details.</p>
        ))}
      {designTag && links.length === 0 && (
        <p data-testid="verification-missing">
          Tagged design-verified, but its metadata links no state machine.
        </p>
      )}
      {designTag &&
        links.map((link, index) => (
          <StateMachineDetails
            key={link.concern}
            workspaceId={workspaceId}
            repoId={repoId}
            link={link}
            evidence={status.evidence[index] ?? { state: "checking" }}
            onRetry={onRetry}
            onOpenFile={onOpenFile}
          />
        ))}
      {status.shown === "design-verified" && findings.length > 0 && (
        <div
          className="verification-findings"
          data-testid="verification-findings"
        >
          <h4>Related bugs and simplifications</h4>
          <ul>
            {findings.map((link) => (
              <RelatedFinding
                key={`${link.file}#${link.id}`}
                workspaceId={workspaceId}
                repoId={repoId}
                link={link}
                onOpen={onOpenFinding}
              />
            ))}
          </ul>
        </div>
      )}
    </section>
  );
}

/** One linked findings entry: severity, title and decision, read as saved. */
function RelatedFinding({
  workspaceId,
  repoId,
  link,
  onOpen,
}: {
  workspaceId: string;
  repoId: string;
  link: FindingLink;
  onOpen?: (path: string, id: string) => void;
}) {
  const { read, refetch } = useFirstVerificationText(
    workspaceId,
    repoId,
    link.paths,
  );
  if (read.status === "loading")
    return <li data-testid="verification-finding">Reading {link.id}…</li>;
  if (read.status !== "ok")
    return (
      <li data-testid="verification-finding">
        {link.id}:{" "}
        {read.status === "absent"
          ? `${link.file} was not found.`
          : `${link.file} could not be read: ${read.message}`}{" "}
        {read.status === "error" && (
          <button
            type="button"
            className="verification-link"
            data-testid="verification-finding-retry"
            onClick={() => void refetch()}
          >
            Retry
          </button>
        )}
      </li>
    );
  const entry = parseFindings(read.data.text).entries.find(
    (e) => e.id === link.id,
  );
  if (!entry)
    return (
      <li data-testid="verification-finding">
        {link.id} was not found in {link.file}.
      </li>
    );
  return (
    <li data-testid="verification-finding" data-entry-id={entry.id}>
      <span
        className={`finding-severity finding-severity-${entry.severity.toLowerCase()}`}
      >
        {entry.severity}
      </span>{" "}
      {onOpen ? (
        <button
          type="button"
          className="verification-link"
          data-testid="verification-finding-open"
          onClick={() => onOpen(read.data.path, entry.id)}
        >
          {entry.title}
        </button>
      ) : (
        entry.title
      )}
      {entry.decision && entry.decision !== "?" && (
        <span className="verification-finding-decision">
          {" · "}
          {entry.decision}
          {entry.issue && (
            <>
              {" "}
              <a
                href={entry.issue.url}
                target="_blank"
                rel="noopener noreferrer"
              >
                {entry.issue.key}
              </a>
            </>
          )}
        </span>
      )}
    </li>
  );
}

function ReadNote({
  read,
  what,
  onRetry,
}: {
  read: Read<unknown>;
  what: string;
  onRetry: () => void;
}) {
  if (read.status === "loading") return <p role="status">Loading {what}…</p>;
  if (read.status === "absent") return <p>No {what} was found.</p>;
  if (read.status === "error")
    return (
      <p role="alert">
        {what.charAt(0).toUpperCase() + what.slice(1)} could not be read:{" "}
        {read.message}{" "}
        <button
          type="button"
          className="verification-link"
          data-testid="verification-retry"
          onClick={onRetry}
        >
          Retry
        </button>
      </p>
    );
  return null;
}

function EvidenceNote({
  evidence,
  onRetry,
}: {
  evidence: LinkEvidence;
  onRetry: () => void;
}) {
  if (evidence.state === "checking")
    return <p role="status">Checking the saved evidence…</p>;
  if (evidence.state === "found") return null;
  return (
    <p data-testid="verification-missing">
      {evidence.reason}
      {evidence.state === "unreadable" && (
        <>
          {" "}
          <button
            type="button"
            className="verification-link"
            data-testid="verification-retry"
            onClick={onRetry}
          >
            Retry
          </button>
        </>
      )}
    </p>
  );
}

function StateMachineDetails({
  workspaceId,
  repoId,
  link,
  evidence,
  onRetry,
  onOpenFile,
}: {
  workspaceId: string;
  repoId: string;
  link: StateMachineLink;
  evidence: LinkEvidence;
  onRetry: () => void;
  onOpenFile?: (path: string, line?: number) => void;
}) {
  // Where the manifest was found; without one there is nothing more to show.
  const root = evidence.state === "checking" ? undefined : evidence.root;
  const manifestPath = root ? artifactPath.manifest(root, link.concern) : null;
  const manifest = useVerificationText(workspaceId, repoId, manifestPath);
  const [open, setOpen] = useState(false);
  if (!root)
    return (
      <div className="verification-machine" data-testid="verification-machine">
        <h4>{concernTitle(link.concern)}</h4>
        <EvidenceNote evidence={evidence} onRetry={onRetry} />
      </div>
    );
  const parsed =
    manifest.read.status === "ok" ? parseManifest(manifest.read.data) : null;
  const title =
    (parsed && "manifest" in parsed && parsed.manifest.title) ||
    concernTitle(link.concern);
  return (
    <div className="verification-machine" data-testid="verification-machine">
      <h4>
        {title}: <strong>What this state machine checks</strong>
      </h4>
      <ReadNote
        read={manifest.read}
        what="manifest"
        onRetry={() => void manifest.refetch()}
      />
      {parsed && "manifest" in parsed && (
        <p className="verification-question">{parsed.manifest.question}</p>
      )}
      {parsed && "unsupported" in parsed && (
        <p data-testid="verification-unsupported">
          Unsupported format: {parsed.unsupported}{" "}
          <FileButton path={manifestPath!} onOpenFile={onOpenFile} />
        </p>
      )}
      <EvidenceNote evidence={evidence} onRetry={onRetry} />
      <details
        className="verification-disclosure"
        onToggle={(event) => setOpen(event.currentTarget.open)}
      >
        <summary>State machine and verification details</summary>
        {open && (
          <MachineEvidence
            workspaceId={workspaceId}
            repoId={repoId}
            root={root}
            concern={link.concern}
            declared={
              parsed && "manifest" in parsed ? parsed.manifest.runs : []
            }
            onOpenFile={onOpenFile}
          />
        )}
      </details>
    </div>
  );
}

function FileButton({
  path,
  label,
  onOpenFile,
}: {
  path: string;
  label?: string;
  onOpenFile?: (path: string, line?: number) => void;
}) {
  return (
    <button
      type="button"
      className="verification-link"
      data-testid="verification-file"
      title={path}
      disabled={!onOpenFile}
      onClick={() => onOpenFile?.(path)}
    >
      {label ?? path.split("/").pop()}
    </button>
  );
}

function MachineEvidence({
  workspaceId,
  repoId,
  root,
  concern,
  declared,
  onOpenFile,
}: {
  workspaceId: string;
  repoId: string;
  root: string;
  concern: string;
  declared: ReturnType<typeof concernRuns>["declared"][number]["run"][];
  onOpenFile?: (path: string, line?: number) => void;
}) {
  const folder = artifactPath.folder(root, concern);
  const bindingsPath = artifactPath.bindings(root, concern);
  const bindings = useVerificationText(workspaceId, repoId, bindingsPath);
  const index = useRunIndex(workspaceId, repoId, root);
  const rows =
    index.read.status === "ok"
      ? index.read.data.rows.filter((row) => row.concern === concern)
      : [];
  const runs = concernRuns(rows, declared);
  const kinds = new Set([
    ...runs.declared.map(({ run, latest }) => latest?.kind ?? run.kind),
    ...runs.earlier.map((row) => row.kind),
  ]);
  const files = [
    "manifest.json",
    "BINDINGS.md",
    ...new Set(
      declared
        .flatMap((run) => [run.spec, run.cfg])
        .filter(Boolean) as string[],
    ),
  ];
  return (
    <div className="verification-evidence">
      <h5>Bindings</h5>
      {bindings.read.status === "ok" ? (
        // Its links resolve from its own folder (`g5.documents.ontology.links`).
        <div data-link-base={bindingsPath} className="verification-bindings">
          <MarkdownPreview
            content={bindings.read.data}
            filePath={bindingsPath}
            workspaceId={workspaceId}
            repoId={repoId}
            onOpenWorkspaceFile={
              onOpenFile
                ? (path, _repo, line) => onOpenFile(path, line)
                : undefined
            }
          />
        </div>
      ) : (
        <ReadNote
          read={bindings.read}
          what="BINDINGS.md"
          onRetry={() => void bindings.refetch()}
        />
      )}
      <h5>Recorded checks</h5>
      {index.read.status === "ok" ? (
        <>
          {index.read.data.unreadable > 0 && (
            <p>
              {index.read.data.unreadable} unreadable run index rows were
              skipped.
            </p>
          )}
          <RunTable
            rows={runs.declared.map(({ run, latest }, i) => ({
              key: `${i}`,
              label: run.label,
              row: latest,
              kind: latest?.kind ?? run.kind,
              variant: latest?.variant ?? run.variant,
              expect: latest?.expect ?? run.expect,
            }))}
            root={root}
            concern={concern}
            onOpenFile={onOpenFile}
          />
          {runs.earlier.length > 0 && (
            <details className="verification-disclosure">
              <summary>
                Earlier and retired runs · {runs.earlier.length}
              </summary>
              <RunTable
                rows={runs.earlier.map((row, i) => ({
                  key: `${i}`,
                  label: row.label,
                  row,
                  kind: row.kind,
                  variant: row.variant,
                  expect:
                    row.expect ??
                    declared.find((run) => run.label === row.label)?.expect ??
                    null,
                }))}
                root={root}
                concern={concern}
                onOpenFile={onOpenFile}
              />
            </details>
          )}
          <p className="verification-legend">
            {Object.entries(KIND_NOTE)
              .filter(([kind]) => kinds.has(kind))
              .map(([kind, note]) => `${kind}: ${note}; `)
              .join("")}
            Green marks an outcome that matched its expectation; nothing is
            re-run here.
          </p>
        </>
      ) : (
        <ReadNote
          read={index.read}
          what="run index"
          onRetry={() => void index.refetch()}
        />
      )}
      <h5>Files</h5>
      <p className="verification-files">
        {files.map((name) => (
          <FileButton
            key={name}
            path={`${folder}/${name}`}
            onOpenFile={onOpenFile}
          />
        ))}
      </p>
    </div>
  );
}

// The run kinds an uplift's model-checking runner records; others are shown as saved.
const KIND_NOTE: Record<string, string> = {
  safety: "a property held",
  control: "a deliberately broken protection was detected",
  divergence: "a modeled implementation gap was reproduced",
  witness: "a target state was reached",
};

export interface RunTableRow {
  key: string;
  label: string;
  row: RunRow | null;
  kind: string | null;
  variant: string | null;
  expect: string | null;
  /** The run's verification root and state machine, when not the table's. */
  root?: string;
  concern?: string;
  /** A reader's name for the run's state machine, shown under its label. */
  machine?: string;
  /** What to say instead of the outcome when no run is linked. */
  missing?: string;
}

/**
 * Recorded runs, as saved. `compact` keeps the run, its outcome, its explored
 * states and its receipt, as findings entries show them (`g5.documents.findings.files`).
 */
export function RunTable({
  rows,
  root,
  concern,
  onOpenFile,
  compact = false,
  receiptLabel = "Open",
}: {
  rows: RunTableRow[];
  root?: string;
  concern?: string;
  onOpenFile?: (path: string, line?: number) => void;
  compact?: boolean;
  receiptLabel?: string;
}) {
  return (
    <div className="verification-table-wrap">
      <table
        className={`verification-runs${compact ? " verification-runs-compact" : ""}`}
      >
        <thead>
          <tr>
            <th scope="col">Run</th>
            {!compact && <th scope="col">Kind / expected</th>}
            <th scope="col">Recorded outcome</th>
            <th scope="col">Distinct states</th>
            {!compact && <th scope="col">Generated / depth</th>}
            {!compact && <th scope="col">Date</th>}
            <th scope="col">Receipt</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((entry) => {
            const { key, label, row, kind, variant, expect } = entry;
            const runRoot = entry.root ?? root;
            const runConcern = entry.concern ?? concern;
            return (
              <tr key={key} data-testid="verification-run-row">
                <th scope="row">
                  {label}
                  {compact ? (
                    <small>
                      {[entry.machine, kind, variant]
                        .filter(Boolean)
                        .join(" · ")}
                    </small>
                  ) : (
                    variant && <small>{variant}</small>
                  )}
                </th>
                {!compact && (
                  <td>
                    {kind ?? "—"}
                    {expect && <small>{expect}</small>}
                  </td>
                )}
                <td>
                  {row ? (
                    <span
                      className={`verification-outcome verification-outcome-${outcomeMatch(row.outcome, expect)}`}
                      data-testid="verification-run-outcome"
                    >
                      {row.outcome}
                    </span>
                  ) : entry.missing ? (
                    entry.missing
                  ) : compact ? (
                    "Not in the run index"
                  ) : (
                    "Not run"
                  )}
                  {compact && expect && <small>expected {expect}</small>}
                </td>
                <td>{row?.distinctStates?.toLocaleString() ?? "—"}</td>
                {!compact && (
                  <td>
                    {row?.statesGenerated != null || row?.depth != null
                      ? `${row?.statesGenerated?.toLocaleString() ?? "—"} / ${row?.depth ?? "—"}`
                      : "—"}
                  </td>
                )}
                {!compact && <td>{row ? (runDate(row.runId) ?? "—") : "—"}</td>}
                <td>
                  {row?.runId && runRoot && runConcern ? (
                    <FileButton
                      path={artifactPath.receipt(
                        runRoot,
                        runConcern,
                        row.runId,
                      )}
                      label={receiptLabel}
                      onOpenFile={onOpenFile}
                    />
                  ) : (
                    "—"
                  )}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
