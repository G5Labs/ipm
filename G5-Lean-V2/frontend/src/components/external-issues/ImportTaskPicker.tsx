import { useEffect, useRef, useState } from "react";
import { useAuth } from "../../hooks/useAuth";
import {
  useExternalIssueSearch,
  useExternalIssuesFeature,
  useLinkWorkspaceIssue,
  useRouteOrgMembership,
  useWorkspaceIssueLinks,
} from "../../hooks/useExternalIssues";
import { useIssueSearchText } from "../../hooks/useIssueSearchText";
import {
  describeFeatureCheckError,
  describeIssueRequestError,
  linkConflictOf,
} from "../../lib/externalIssueErrors";
import type { ExternalIssueSummaryDTO } from "../../lib/types";
import { Button, Modal } from "../ui/UI";
import { IssueErrorNotice } from "./ExternalIssueParts";
import { IssueLinkConflictNotice } from "./IssueLinkConflictNotice";
import { IssueSearchOutcome } from "./IssueSearchOutcome";

const identity = (issue: { trackerId: string; externalIssueId: string }) =>
  `${issue.trackerId}:${issue.externalIssueId}`;
type ImportResult = { ok: true } | { ok: false; error: unknown };

export function ImportTaskPicker({
  workspaceId,
  viewerOwnsWorkspace,
  writeBlockedReason,
  onClose,
}: {
  workspaceId: string;
  viewerOwnsWorkspace: boolean;
  writeBlockedReason?: string | null;
  onClose: () => void;
}) {
  const { hasScope } = useAuth();
  const membership = useRouteOrgMembership();
  const feature = useExternalIssuesFeature(membership?.orgId);
  const canWrite = viewerOwnsWorkspace && hasScope("workspaces:write");
  const enabled = canWrite && feature.status === "enabled";
  const { text, setText, query, searchNow } = useIssueSearchText();
  const search = useExternalIssueSearch(
    membership?.orgId ?? "",
    query,
    "open",
    "any",
    enabled,
  );
  const links = useWorkspaceIssueLinks(workspaceId, enabled);
  const linkIssue = useLinkWorkspaceIssue(workspaceId);
  const [selected, setSelected] = useState<
    Map<string, ExternalIssueSummaryDTO>
  >(new Map());
  const [results, setResults] = useState<Record<string, ImportResult>>({});
  const [pending, setPending] = useState(false);
  const alive = useRef(true);
  const importing = useRef(false);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);
  const linked = new Set(
    links.status === "ok" ? links.data.links.map(identity) : [],
  );
  const toImport = [...selected.values()].filter(
    (issue) => !linked.has(identity(issue)) && !results[identity(issue)]?.ok,
  );
  const blocked = !enabled || !!writeBlockedReason || links.status !== "ok";
  const writeAllowed = useRef(!blocked);
  useEffect(() => {
    writeAllowed.current = !blocked;
  }, [blocked]);
  const importIssues = async (issues: ExternalIssueSummaryDTO[]) => {
    if (blocked || importing.current) return;
    importing.current = true;
    setPending(true);
    try {
      for (const issue of issues) {
        if (!alive.current || !writeAllowed.current) break;
        const key = identity(issue);
        try {
          await linkIssue.mutateAsync({
            ref: issue.displayKey,
            role: "related",
          });
          if (alive.current) {
            setResults((prev) => ({ ...prev, [key]: { ok: true } }));
            setSelected((prev) => {
              const next = new Map(prev);
              next.delete(key);
              return next;
            });
          }
        } catch (error) {
          if (alive.current)
            setResults((prev) => ({ ...prev, [key]: { ok: false, error } }));
        }
      }
    } finally {
      importing.current = false;
      if (alive.current) setPending(false);
    }
  };
  if (!canWrite) return null;
  return (
    <Modal
      ariaLabel="Import task"
      onClose={onClose}
      testId="import-task-picker"
    >
      <h2 className="text-lg font-semibold text-[var(--g5-text0)]">
        Import task
      </h2>
      <input
        type="search"
        aria-label="Search issues"
        value={text}
        onChange={(event) => setText(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter" && !event.nativeEvent.isComposing)
            searchNow();
        }}
        className="my-3 w-full rounded border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] p-2 text-sm text-[var(--g5-text0)]"
      />
      {writeBlockedReason && <p role="status">{writeBlockedReason}</p>}
      {feature.status === "disabled" && (
        <p>External issues are not available for this organization.</p>
      )}
      {feature.status === "error" && (
        <IssueErrorNotice
          testId="import-task-feature-error"
          presentation={describeFeatureCheckError(feature.error)}
          onRetry={feature.retry}
        />
      )}
      {(feature.status === "loading" ||
        (enabled && links.status === "loading")) && (
        <p role="status">Loading issues…</p>
      )}
      {enabled && links.status === "error" && (
        <IssueErrorNotice
          testId="import-task-links-error"
          presentation={describeIssueRequestError(links.error)}
          onRetry={links.refetch}
        />
      )}
      {enabled && links.status !== "loading" && (
        <IssueSearchOutcome search={search} testIdPrefix="import-task">
          {(found) => (
            <ul className="max-h-80 space-y-2 overflow-y-auto">
              {found.map((issue) => {
                const key = identity(issue);
                const result = results[key];
                const conflict =
                  result && !result.ok ? linkConflictOf(result.error) : null;
                const alreadyLinked = linked.has(key) || result?.ok === true;
                return (
                  <li
                    key={key}
                    data-testid={`import-task-row-${issue.externalIssueId}`}
                    className="rounded border border-[var(--g5-stroke)] p-2 text-sm text-[var(--g5-text1)]"
                  >
                    <label className="flex items-start gap-2">
                      <input
                        type="checkbox"
                        aria-label={issue.displayKey}
                        checked={alreadyLinked || selected.has(key)}
                        disabled={alreadyLinked || blocked || pending}
                        onChange={(event) => {
                          const checked = event.target.checked;
                          setSelected((prev) => {
                            const next = new Map(prev);
                            if (checked) next.set(key, issue);
                            else next.delete(key);
                            return next;
                          });
                        }}
                      />
                      <span>
                        <span className="font-mono">{issue.displayKey}</span>{" "}
                        {issue.title}
                      </span>
                    </label>
                    {result?.ok && <p role="status">Imported</p>}
                    {conflict && (
                      <IssueLinkConflictNotice
                        conflict={conflict}
                        orgSlug={membership?.orgSlug ?? ""}
                        pending={pending || blocked}
                        onLinkAsRelated={() => void importIssues([issue])}
                      />
                    )}
                    {result && !result.ok && !conflict && (
                      <IssueErrorNotice
                        testId={`import-task-error-${issue.externalIssueId}`}
                        compact
                        presentation={describeIssueRequestError(result.error)}
                      />
                    )}
                  </li>
                );
              })}
            </ul>
          )}
        </IssueSearchOutcome>
      )}
      <div className="mt-4 flex justify-end gap-2">
        <Button kind="secondary" onClick={onClose}>
          Close
        </Button>
        <Button
          kind="primary"
          disabled={blocked || pending || toImport.length === 0}
          title={writeBlockedReason ?? undefined}
          onClick={() => void importIssues(toImport)}
        >
          {pending
            ? "Importing…"
            : `Import ${toImport.length} ${toImport.length === 1 ? "task" : "tasks"}`}
        </Button>
      </div>
    </Modal>
  );
}
