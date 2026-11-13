import { useWorkspaceBranches } from "../../hooks/useWorkspace";
import { parseG5FileHref } from "../shared/sandboxedHtml";
import {
  useCallback,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type RefObject,
} from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { fetchWorkspaceFile } from "../../hooks/useFiles";
import { ApiError } from "../../lib/api";
import {
  currentRecordSpans,
  isCurrentOntologyPath,
  linkPathCandidates,
  ontologyFragmentIndex,
  resolveOntologyLink,
} from "../../lib/currentOntology";
import { lineFromFragment } from "../files/repoLinks";
import { useWorkspaceReadAuthority } from "../../lib/workspaceLiveGate";
import { parseMarkdownToBlocks } from "../../lib/utils";
import { MarkdownPreview } from "../files/MarkdownPreview";
import { spacedLinks } from "../files/spacedLinks";
import { ExternalLink, Info, X } from "lucide-react";
import { RECORD_KIND_META } from "./SemanticRecordFrame";
import { AnchoredPopover } from "./AnchoredPopover";
import type { NavigateTarget } from "./IntentTraceabilityPanel";

interface Target {
  path: string;
  repoRef?: string;
  fragment: string;
  anchor: HTMLElement;
  sourceId?: string;
  error?: string;
}
export function CurrentOntologyReference({
  workspaceId,
  repoId,
  path,
  content,
  onNavigate,
  onOpenWorkspaceFile,
  rootRef,
}: {
  workspaceId?: string;
  repoId?: string;
  path: string;
  content: string;
  onNavigate: (target: NavigateTarget) => void;
  /** Opens a file at a line, for `#L<n>` links (`g5.documents.ontology.links`). */
  onOpenWorkspaceFile?: (
    filePath: string,
    repoRef?: string,
    line?: number,
    from?: string,
  ) => void;
  rootRef: RefObject<HTMLDivElement | null>;
}) {
  const previewRef = useRef<HTMLDivElement>(null);
  const queryClient = useQueryClient();
  const authority = useWorkspaceReadAuthority();
  // Whether a link candidate exists, by reading it, so folders hidden from
  // Files resolve too. Unknown (null) when the read fails for another reason,
  // such as a folder; a failed check never becomes another view's error. A
  // file that exists, and that the file view has not read, gets this read as
  // the file view's, so the file a link opens shows at once instead of being
  // read again. A newer read already there is never replaced.
  const exists = useCallback(
    async (candidate: string, repo: string): Promise<boolean | null> => {
      if (!workspaceId) return null;
      try {
        const file = await fetchWorkspaceFile(workspaceId, repo, candidate);
        const fileViewKey = [
          "workspace-file",
          workspaceId,
          repo,
          candidate,
          ...(authority ? [authority] : []),
        ];
        if (queryClient.getQueryData(fileViewKey) === undefined)
          queryClient.setQueryData(fileViewKey, file);
        return true;
      } catch (error) {
        return error instanceof ApiError && error.status === 404 ? false : null;
      }
    },
    [workspaceId, queryClient, authority],
  );
  // Only the latest click's resolution may show its target.
  const latestClick = useRef(0);
  const [target, setTarget] = useState<Target | null>(null);
  const [detailsOpen, setDetailsOpen] = useState(false);
  const detailsId = useId();
  const branches = useWorkspaceBranches(
    target?.repoRef && target.repoRef !== repoId ? (workspaceId ?? "") : "",
  );
  const owners =
    target?.repoRef && branches.status === "ok"
      ? branches.data.filter(
          (b) => b.repoId === target.repoRef || b.repoName === target.repoRef,
        )
      : [];
  const targetRepoId =
    target?.repoRef && target.repoRef !== repoId
      ? owners.length === 1
        ? owners[0].repoId
        : undefined
      : repoId;
  const repoError =
    target?.repoRef && target.repoRef !== repoId
      ? branches.status === "error"
        ? branches.error.message
        : branches.status === "ok" && owners.length !== 1
          ? "Repository target is missing or ambiguous in this workspace"
          : undefined
      : undefined;
  const navigate = useCallback(
    (t: Target) => {
      onNavigate({
        type: isCurrentOntologyPath(t.path) ? "spec" : "file",
        id: t.path,
        workspaceId,
        repoId: targetRepoId,
        intentRecordNumber: t.fragment ? `#${t.fragment}` : undefined,
        sourceFileName: path,
        sourceIntentRecordNumber: t.sourceId,
        sourceFileType: "spec",
      });
      setTarget(null);
    },
    [onNavigate, workspaceId, targetRepoId, path],
  );
  useEffect(() => {
    const root = rootRef.current;
    if (!root) return;
    const click = (event: MouseEvent) => {
      if (
        event.target instanceof Element &&
        event.target.closest("[data-ontology-before]")
      )
        return;
      const a =
        event.target instanceof Element
          ? event.target.closest<HTMLAnchorElement>("a[href]")
          : null;
      if (!a) return;
      const fromPreview = !!previewRef.current?.contains(a);
      if (!root.contains(a) && !fromPreview) return;
      // Content rendered from another file resolves links from that file.
      const linkBase =
        a.closest<HTMLElement>("[data-link-base]")?.dataset.linkBase;
      const sourcePath =
        linkBase ?? (fromPreview && target ? target.path : path);
      const href = a.getAttribute("href") ?? "";
      if (
        (!href.startsWith("g5://file/") && /^[a-z][a-z0-9+.-]*:/i.test(href)) ||
        href.startsWith("//")
      )
        return;
      event.preventDefault();
      event.stopPropagation();
      const clickId = ++latestClick.current;
      const ontology = isCurrentOntologyPath(path);
      const anchor = fromPreview && target ? target.anchor : a;
      // Back returns to the statement or finding the link was followed from.
      const sourceId =
        a.closest<HTMLElement>("[data-record-span]")?.dataset.recordSpan ??
        a.closest<HTMLElement>("[data-finding-id]")?.dataset.findingId;
      const show = (resolved: {
        path: string;
        fragment: string;
        repoRef?: string;
      }) => {
        const repoRef =
          resolved.repoRef ?? (fromPreview ? targetRepoId : undefined);
        const line = ontology ? lineFromFragment(resolved.fragment) : undefined;
        // In an ontology file a line fragment opens the file at that line.
        if (line !== undefined && onOpenWorkspaceFile) {
          setTarget(null);
          onOpenWorkspaceFile(resolved.path, repoRef ?? repoId, line, sourceId);
          return;
        }
        setDetailsOpen(false);
        setTarget({ ...resolved, repoRef, anchor, sourceId });
      };
      const fail = (error: unknown) => {
        setDetailsOpen(false);
        setTarget({ path, fragment: "", anchor: a, error: String(error) });
      };
      try {
        const [address, rawFragment = ""] = href.split("#", 2);
        let fragment = rawFragment;
        try {
          fragment = decodeURIComponent(rawFragment);
        } catch {
          // keep the raw fragment
        }
        const qualified = href.startsWith("g5://file/")
          ? parseG5FileHref(address)
          : null;
        if (href.startsWith("g5://file/") && !qualified)
          throw new Error("Invalid repository link");
        if (qualified) {
          show({
            path: qualified.filePath,
            repoRef: qualified.repoRef,
            fragment,
          });
          return;
        }
        // Other documents keep their links as authored.
        if (!ontology) {
          const resolved = resolveOntologyLink(sourcePath, href);
          if (resolved) show(resolved);
          return;
        }
        // Ontology links: as authored, then from the repository root, then
        // from the `.g5` folder; the first file that exists opens
        // (`g5.documents.ontology.links`).
        const candidates = linkPathCandidates(sourcePath, href);
        if (!address || candidates[0] === sourcePath) {
          show({ path: candidates[0] ?? sourcePath, fragment });
          return;
        }
        if (!candidates.length) throw new Error("Link escapes the repository");
        const repo = (fromPreview ? targetRepoId : undefined) ?? repoId;
        void (async () => {
          for (const candidate of candidates) {
            const found = repo ? await exists(candidate, repo) : null;
            if (clickId !== latestClick.current) return;
            // Unknown, as for a folder: this candidate opens as written.
            if (found !== false) return show({ path: candidate, fragment });
          }
          fail(`Couldn't find ${href}. Tried: ${candidates.join(", ")}`);
        })();
      } catch (error) {
        fail(error);
      }
    };
    document.addEventListener("click", click, true);
    return () => document.removeEventListener("click", click, true);
  }, [
    path,
    rootRef,
    target,
    targetRepoId,
    repoId,
    exists,
    onOpenWorkspaceFile,
  ]);
  const direct =
    !!target && (!target.fragment || !isCurrentOntologyPath(target.path));
  const sameDocument = target?.path === path && targetRepoId === repoId;
  const query = useQuery({
    queryKey: ["workspace-file", workspaceId, targetRepoId, target?.path],
    queryFn: ({ signal }) =>
      fetchWorkspaceFile(workspaceId!, targetRepoId!, target!.path, signal),
    enabled:
      !!target &&
      !target.error &&
      !direct &&
      !sameDocument &&
      !repoError &&
      !!workspaceId &&
      !!targetRepoId,
  });
  const text = sameDocument
    ? content
    : query.data?.encoding === "utf-8"
      ? query.data.content
      : undefined;
  const parsed = useMemo(
    () => (text !== undefined ? parseMarkdownToBlocks(text) : null),
    [text],
  );
  const index =
    parsed && target ? ontologyFragmentIndex(parsed, target.fragment) : -1;
  const record = parsed
    ? (currentRecordSpans(parsed) ?? []).find(
        (r) => r.current?.headingIndex === index || r.spanStart === index,
      )
    : undefined;
  const error =
    target?.error ??
    repoError ??
    (query.data?.encoding === "base64"
      ? "Target is not a UTF-8 document"
      : undefined) ??
    (query.error ? query.error.message : undefined) ??
    (!direct && text !== undefined && index < 0
      ? `Missing target: ${target?.path}#${target?.fragment}`
      : undefined) ??
    record?.current?.diagnostic;
  useEffect(() => {
    if (
      target &&
      !error &&
      (!target.repoRef || targetRepoId) &&
      (direct || (parsed && index >= 0 && !record))
    )
      navigate(target);
  }, [target, targetRepoId, direct, parsed, index, record, error, navigate]);
  if (!target) return null;
  const bodyStart =
    parsed?.[record ? record.spanStart + 1 : -1]?.sourceRange?.start;
  const bodyEnd =
    parsed?.[record ? record.spanEnd : -1]?.sourceRange?.start ?? text?.length;
  const metadataStart = record?.current?.metadataStart ?? -1;
  const metadataEnd = record?.current?.metadataEnd ?? -1;
  const metadata =
    parsed && text !== undefined && metadataStart >= 0
      ? text.slice(
          parsed[metadataStart + 1]?.sourceRange?.start ?? text.length,
          parsed[metadataEnd]?.sourceRange?.start ?? text.length,
        )
      : "";
  const referencePath = `${target.repoRef ? `${target.repoRef}:` : ""}${target.path.replace(/^\.g5\/ontology\//, "")}`;
  const ready = !!record && !error && text !== undefined;
  const actionClass =
    "rounded p-1 text-[var(--g5-text2)] hover:bg-[var(--g5-overlay-hover)] hover:text-[var(--g5-text0)] disabled:opacity-40 disabled:cursor-not-allowed";
  return (
    <AnchoredPopover
      anchor={target.anchor}
      ariaLabel="Ontology reference"
      testId="ontology-reference"
      onDismiss={() => setTarget(null)}
      className="text-[var(--g5-text0)]"
    >
      <div ref={previewRef}>
        <div className="mb-1.5 flex items-center justify-between gap-2">
          <span
            className="min-w-0 truncate font-mono text-xs text-[var(--g5-info)]"
            title={target.path}
          >
            {referencePath}
          </span>
          <div className="flex shrink-0 items-center gap-1">
            <button
              type="button"
              data-testid="ontology-reference-details"
              aria-label="Reference details"
              title="Reference details"
              aria-expanded={detailsOpen}
              aria-controls={detailsId}
              disabled={!ready}
              className={actionClass}
              onClick={() => setDetailsOpen((open) => !open)}
            >
              <Info className="h-3.5 w-3.5" aria-hidden="true" />
            </button>
            <button
              type="button"
              data-testid="ontology-reference-open-document"
              aria-label="Open document"
              title="Open document"
              disabled={!ready}
              className={actionClass}
              onClick={() => navigate(target)}
            >
              <ExternalLink className="h-3.5 w-3.5" aria-hidden="true" />
            </button>
            <button
              type="button"
              data-testid="ontology-reference-close"
              aria-label="Close reference"
              title="Close reference"
              className={actionClass}
              onClick={() => {
                target.anchor.focus();
                setTarget(null);
              }}
            >
              <X className="h-3.5 w-3.5" aria-hidden="true" />
            </button>
          </div>
        </div>
        <div className="g5-editor-prose g5-document-links max-h-72 overflow-auto rounded border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] text-[var(--g5-text0)]">
          <div className="p-2" data-testid="ontology-reference-content">
            {error ? (
              <p
                role="alert"
                className="break-words text-xs text-[var(--g5-blocked)]"
              >
                {error}
              </p>
            ) : !ready ? (
              <p role="status" className="text-xs text-[var(--g5-text2)]">
                Loading reference…
              </p>
            ) : (
              <MarkdownPreview
                content={
                  bodyStart === undefined ? "" : text.slice(bodyStart, bodyEnd)
                }
                plugins={[spacedLinks]}
              />
            )}
          </div>
          <div
            id={detailsId}
            hidden={!ready || !detailsOpen}
            className="border-t border-[var(--g5-stroke)] bg-[var(--g5-bg2)] p-2 text-xs text-[var(--g5-text1)]"
            data-testid="ontology-reference-metadata"
          >
            {record && (
              <div className="space-y-2">
                <div className="flex min-w-0 flex-wrap gap-x-4 gap-y-1">
                  <span className="font-semibold text-[var(--g5-text0)]">
                    Metadata
                  </span>
                  <span>Type: {RECORD_KIND_META[record.kind].label}</span>
                  <span className="min-w-0 break-words">
                    ID: <code>{record.id}</code>
                  </span>
                </div>
                {metadata && (
                  <MarkdownPreview content={metadata} plugins={[spacedLinks]} />
                )}
              </div>
            )}
          </div>
        </div>
      </div>
    </AnchoredPopover>
  );
}
