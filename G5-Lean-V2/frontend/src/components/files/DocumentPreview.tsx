import React, {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from "react";
import { createPortal } from "react-dom";
import type { Element, Root, RootContent } from "hast";
import type { Components } from "react-markdown";
import { MarkdownPreview, type MarkdownPreviewProps } from "./MarkdownPreview";
import { documentMarkup } from "./documentMarkup";
import { spacedLinks } from "./spacedLinks";
import { findingsMarkup } from "./findingsMarkup";
import {
  FindingEntryBlocks,
  FindingEvidence,
  FindingsContextBlocks,
  FindingsLayer,
  FindingsSectionHeading,
  FindingStatusRow,
  type FindingsHost,
} from "./FindingsControls";
import {
  findingLinks,
  findingsDot,
  findingsFileOf,
  parseFindings,
  type FindingEntry,
  type FindingLink,
} from "../../lib/findings";
import { createStatementState, type StatementState } from "./statementState";
import {
  RECORD_KIND_META,
  SemanticRecordFrame,
  type RecordQuestionAccent,
  type StatementLayout,
} from "../editor/SemanticRecordFrame";
import {
  DocumentOutline,
  type OutlineQuestionRecord,
} from "../editor/DocumentOutline";
import { CurrentOntologyReference } from "../editor/CurrentOntologyReference";
import { OntologyBeforeStatement } from "../editor/OntologyBeforeStatement";
import { OntologyComparisonPair } from "../editor/OntologyComparisonPair";
import {
  currentOntologyDiagnostics,
  isCurrentOntologyPath,
  ontologyFragmentIndex,
  markdownLinkDestinations,
  stateMachineLinks,
  metadataDetails,
  statementLeadBlock,
  statementVerificationTags,
  staysWithLead,
} from "../../lib/currentOntology";
import {
  hasVerificationDetails,
  statementStatuses,
} from "../../lib/verificationStatus";
import {
  useFirstVerificationTexts,
  useVerificationEvidence,
} from "../../hooks/useVerificationEvidence";
import {
  VerificationPanel,
  VerificationStatusButton,
} from "../editor/VerificationPanel";
import { compareOntologyStatements } from "../../lib/ontologyComparison";
import { useOntologyComparison } from "../../hooks/useOntologyComparison";
import {
  collectQuestionFences,
  answeredQuestionRecordStarts,
  rewriteQuestionInMarkdown,
  repairQuestionInMarkdown,
} from "../editor/questionAnswerWrite";
import {
  StructuredQuestionEditor,
  type QuestionApplyResult,
} from "../editor/StructuredQuestionEditor";
import { ReferencedQuestionCard } from "../editor/ReferencedQuestionCard";
import {
  applyAnswer,
  clearAnswer,
  type ParsedQuestion,
} from "../../lib/questionBlock";
import { isQuestionRefFenceLanguage } from "../../lib/questionSet";
import type { NavigateTarget } from "../editor/IntentTraceabilityPanel";
import {
  BlockSuggestions,
  resolveDisplayAnchor,
  type SuggestionOverlayProps,
} from "../workspace/ReviewSuggestionOverlay";
import {
  groupSpecReviews,
  projectSpecReviews,
} from "../editor/specReviewProjection";
import { SpecReviewSuggestions } from "../editor/SpecReviewSuggestions";
import { blockToRawMarkdown, type Block } from "../../lib/utils";
import {
  lineOfOffset,
  blockIndexAtLine,
  renderedBlockForLine,
  selectionSourceLine,
} from "./documentSource";
import { SelectionCommentBubble } from "../comments/SelectionCommentBubble";
import { resolveRepoLink } from "./repoLinks";
import { useReviewSourceDigest } from "../../hooks/useReviewSourceDigest";
import { useSuggestionCardNodes } from "../../hooks/useSuggestionCardNodes";
import {
  ReviewMarginItemSlot,
  reviewMarginItems,
} from "../editor/SpecReviewSuggestions";
import {
  DocumentMargin,
  useRowStacked,
  type DocumentMarginItem,
} from "./DocumentMargin";
import { CommentThread, placedLine } from "../comments/CommentThread";
import type { CommentThreadAction } from "../comments/CommentThread";
import { CommentsNotice, NoThreadsNote } from "../comments/CommentColumn";
import { CommentDraftCard } from "../comments/CommentDraftCard";
import type { SideThreadsDraft } from "../comments/SideThreads";
import { useThreadsBesideView } from "../comments/commentThreadModel";
import type { CommentDTO } from "../../lib/types";
import { AlertTriangle, Columns2, User } from "lucide-react";
import {
  previewToolbarButtonClass,
  previewToolbarCountClass,
} from "./previewToolbar";
export interface DocumentPreviewProps extends MarkdownPreviewProps {
  sourceContent?: string;
  fragment?: string;
  filePath: string;
  workspaceId: string;
  repoId: string;
  repoOid?: string | null;
  sessionKey?: string;
  canAnswer?: boolean;
  currentUserId?: string;
  currentUserEmail?: string;
  currentUserAvatarUrl?: string | null;
  onRewrite?: (expected: string, next: string) => boolean;
  onDiscussQuestion?: (question: ParsedQuestion, title?: string) => void;
  /** Seeds the chat composer with a findings entry (`g5.documents.findings`). */
  onDiscussFinding?: (text: string) => void;
  /**
   * In a findings file, whether and how the owner decides entries
   * (`g5.documents.findings.decision`): why not now, and the live write.
   */
  findings?: Pick<FindingsHost, "blockedReason" | "write">;
  onNavigate?: (target: NavigateTarget) => void;
  /**
   * Selecting text offers Comment: a thread on the selection's first source
   * line, quoting the selection (`g5.documents.comments.identity`). Absent
   * while the person may not start a thread here.
   */
  onCommentSelection?: (at: { line: number; quote: string }) => void;
  /** Reading state kept while the file's views switch. */
  readingState?: PreviewReadingState;
  /**
   * The file row's place for Compare and the filters (`g5.documents.editor.modes`);
   * without it they sit above the document.
   */
  toolbarSlot?: HTMLElement | null;
  suggestionOverlay?: SuggestionOverlayProps | null;
  /**
   * The file's discussion, shown in the margin beside the document with the
   * suggestion cards (`g5.documents.comments.identity`). Absent where the
   * host lists threads elsewhere.
   */
  comments?: DocumentMarginComments;
  target?: string | null;
  targetSeq?: number;
  /**
   * A double-click on document text asks to start editing there
   * (`g5.documents.preview.rich-editing`); absent while the person may not
   * edit or already is.
   */
  onRequestEdit?: (at: PreviewEditRequest) => void;
}

/**
 * Preview's reading state, kept by the file view across switches between its
 * views (`g5.documents.editor.modes`), and the last navigation request it
 * handled, so a remount neither resets nor jumps again.
 */
export interface PreviewReadingState {
  compare?: boolean;
  humanOnly?: boolean;
  riskOnly?: boolean;
  openDetails?: ReadonlySet<string>;
  /** Statement folds and verification details, and Expand all. */
  statements?: StatementState;
  expandAll?: boolean;
  scrollTop?: number;
  handledTarget?: string;
  /** The handled target's highlight and when it ends. */
  highlight?: { target: string; until: number };
}

/** A file's threads and draft as the margin beside the document shows them. */
export interface DocumentMarginComments {
  /** Whether Comments is on: off hides the threads and the draft, not the suggestion cards. */
  shown: boolean;
  threads: readonly CommentDTO[];
  /** The draft of a new thread, level with its line when given. */
  draft?: SideThreadsDraft | null;
  /** How to start a thread, shown with the empty margin when one can be. */
  emptyHint?: string | null;
  /** Why the threads shown may be missing or out of date. */
  notice: string | null;
  onReply?: CommentThreadAction<[threadId: string, content: string]>;
  onResolve?: CommentThreadAction<[threadId: string]>;
  onUnresolve?: CommentThreadAction<[threadId: string]>;
  onDelete?: CommentThreadAction<[threadId: string]>;
  /** A placed card's line label was activated: open that line in plain text. */
  onOpenLine?: (line: number) => void;
  testId?: string;
}

const NO_THREADS: readonly CommentDTO[] = [];

/** Where a double-click asked Rich text to open: a block and a word in it. */
export interface PreviewEditRequest {
  offset: number | null;
  word: string | null;
}

/** Controls and cards that keep their own double-click. */
const OWN_DOUBLE_CLICK = [
  "a",
  "button",
  "input",
  "textarea",
  "select",
  "summary",
  "label",
  "[role='button']",
  "[contenteditable='true']",
  "[data-testid='block-suggestions']",
  "[data-testid='suggestion-fallback-list']",
  "[data-testid='question-card']",
  "[data-testid='question-card-fallback']",
  "[data-testid='semantic-record-identity']",
  "[data-testid='semantic-record-metadata']",
  "[data-testid='verification-panel']",
].join(", ");

/** What is not current document content, so offers no Comment. */
const NOT_COMMENTABLE = [
  "input",
  "textarea",
  "button",
  "[data-ontology-before]",
  "[data-ontology-deleted-id]",
  "[data-testid='semantic-record-metadata']",
  "[data-testid='question-card']",
  "[data-testid='question-card-fallback']",
  "[data-testid='block-suggestions']",
  "[data-testid='suggestion-fallback-list']",
  "[data-testid^='spec-review']",
  // Generated from the verification material, not the file's text.
  "[data-testid='verification-panel']",
  ".verification-table-wrap",
  "[data-testid='finding-row']",
  "[data-testid='findings-toolbar']",
].join(", ");

/** The brief highlight on a navigation target. */
const TARGET_HIGHLIGHT = ["ring-2", "ring-[var(--g5-info)]", "rounded"];
const TARGET_HIGHLIGHT_MS = 2000;

/** The nearest ancestor that scrolls the preview. */
function scrollParent(element: HTMLElement | null): HTMLElement | null {
  for (let el = element?.parentElement; el; el = el.parentElement) {
    const overflow = getComputedStyle(el).overflowY;
    if (overflow === "auto" || overflow === "scroll") return el;
  }
  return null;
}

type RecordProps = {
  node: { properties: { recordIndex: number } };
  children: React.ReactNode;
};

/** A lead's words without inline Markdown, for labels read aloud. */
function plainText(markdown: string): string {
  return markdown
    .replace(/!?\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/<[^>]+>/g, "")
    .replace(/(\*\*|__|\*|_|`+|~~)(\S(?:[\s\S]*?\S)?)\1/g, "$2");
}

/** Whether two ranges select the same text. */
function sameRange(a: Range, b: Range): boolean {
  return (
    a.compareBoundaryPoints(Range.START_TO_START, b) === 0 &&
    a.compareBoundaryPoints(Range.END_TO_END, b) === 0
  );
}

const PreviewContext = createContext<{
  record: (props: RecordProps) => React.ReactNode;
  review: (index: number) => React.ReactNode;
  /** The statement ID a record renders, for its fold and panel choices. */
  recordId: (node: RecordProps["node"]) => string | undefined;
  statementState: StatementState;
  /** Scrolls to a target once the fold that held it has opened. */
  revealPending: () => void;
}>(null!);
/** A statement's own details and its child statements, placed by its record. */
function StatementPart({ children }: { children?: React.ReactNode }) {
  return <>{children}</>;
}
const previewComponents = {
  "g5-record": function PreviewRecord(props: RecordProps) {
    const preview = useContext(PreviewContext);
    const id = preview.recordId(props.node);
    // Its own fold or panel choice re-renders this statement alone.
    useSyncExternalStore(preview.statementState.subscribe, () =>
      id === undefined ? 0 : preview.statementState.version(id),
    );
    useLayoutEffect(() => preview.revealPending());
    return preview.record(props);
  },
  "g5-details": StatementPart,
  "g5-kids": StatementPart,
  "g5-finding-entry": FindingEntryBlocks,
  "g5-finding-status": FindingStatusRow,
  "g5-findings-context": FindingsContextBlocks,
  "g5-findings-heading": FindingsSectionHeading,
  "g5-finding-evidence": FindingEvidence,
  "g5-review": function PreviewReview({ node }: { node: Element }) {
    return useContext(PreviewContext).review(
      Number(node.properties.reviewIndex),
    );
  },
} as unknown as Components;

/** A findings file's controls around its document; other files pass through. */
function FindingsWrap({
  host,
  children,
}: {
  host: FindingsHost | null;
  children: React.ReactNode;
}) {
  return host ? (
    <FindingsLayer host={host}>{children}</FindingsLayer>
  ) : (
    <>{children}</>
  );
}

/** Domain controls over a read-only, sanitized source projection. */
export function DocumentPreview(props: DocumentPreviewProps) {
  return (
    <FileDocumentPreview
      key={`${props.workspaceId}/${props.repoId}/${props.filePath}`}
      {...props}
    />
  );
}

function FileDocumentPreview(props: DocumentPreviewProps) {
  const {
    content,
    filePath,
    workspaceId,
    repoId,
    canAnswer = false,
    onRewrite,
    suggestionOverlay,
    comments,
  } = props;
  const root = useRef<HTMLDivElement>(null);
  // Answers write through the host's current rewrite, also from a findings
  // document kept from an earlier render.
  const latestRewrite = useRef(onRewrite);
  useEffect(() => {
    latestRewrite.current = onRewrite;
  });
  // The row holding the outline, the document and, beside it, the margin.
  const row = useRef<HTMLDivElement>(null);
  const stacked = useRowStacked(row);
  // The margin's items keep their nodes while this document is shown, so a
  // card keeps its component when its place changes.
  const marginNodes = useSuggestionCardNodes();
  const [activeItem, setActiveItem] = useState<string | null>(null);
  const [expandedThread, setExpandedThread] = useState<string | null>(null);
  const revealMarginItem = (itemId: string) => {
    requestAnimationFrame(() =>
      root.current
        ?.querySelector(`[data-margin-item="${CSS.escape(itemId)}"]`)
        ?.scrollIntoView?.({ block: "nearest" }),
    );
  };
  // A draft that just opened is set level with its line, so it is not pushed
  // under the cards already there and its focus does not scroll away.
  const draftKey = comments?.draft?.draft
    ? `${comments.draft.draft.lineNumber}:${comments.draft.draft.quotedText}`
    : null;
  const [seenDraft, setSeenDraft] = useState<string | null>(null);
  if (draftKey !== seenDraft) {
    setSeenDraft(draftKey);
    if (draftKey !== null) setActiveItem("comment-draft");
  }
  const threads = useThreadsBesideView(
    content,
    comments?.threads ?? NO_THREADS,
  );
  const ontology = isCurrentOntologyPath(filePath);
  const markup = useMemo(
    () => documentMarkup(content, ontology),
    [content, ontology],
  );
  const { blocks, records, parents } = markup;
  // A findings file's entries (`g5.documents.findings.files`).
  const findingsFile = ontology ? null : findingsFileOf(filePath);
  const findings = useMemo(
    () => (findingsFile ? parseFindings(content).entries : null),
    [findingsFile, content],
  );
  const findingsPlugin = useMemo(
    () =>
      findings && findingsFile ? findingsMarkup(findings, findingsFile) : null,
    [findings, findingsFile],
  );
  const findingsHost: FindingsHost | null =
    findingsFile && findings
      ? {
          workspaceId,
          repoId,
          filePath,
          file: findingsFile,
          content,
          entries: findings,
          blockedReason:
            props.findings?.blockedReason ??
            (props.findings ? null : "This view cannot record decisions."),
          write: props.findings?.write,
          onDiscuss: props.onDiscussFinding,
          onOpenFile: props.onOpenWorkspaceFile
            ? (path) => props.onOpenWorkspaceFile!(path, repoId)
            : undefined,
        }
      : null;
  const hasRecords = records.length > 0;
  const ownReading = useRef<PreviewReadingState>({});
  const reading = props.readingState ?? ownReading.current;
  // Statement folds (`g5.documents.ontology.disclosure`): one per top-level
  // statement with details, closed until opened by the reader, Expand all,
  // or something inside that needs to be seen.
  const nests = ontology && records.some((r) => r.current);
  const topOf = useMemo(
    () =>
      records.map((_, i) => {
        let top = i;
        while (parents[top] >= 0) top = parents[top];
        return top;
      }),
    [records, parents],
  );
  const indexById = useMemo(
    () => new Map(records.map((r, i) => [r.id, i] as const)),
    [records],
  );
  const recordAtBlock = (block: number) =>
    records.findIndex((r) => block >= r.spanStart && block < r.spanEnd);
  // Fold and verification-panel choices (`statementState`).
  // Kept with the reading view's other state across view switches.
  const [statementState] = useState(
    () => (reading.statements ??= createStatementState()),
  );
  const [expandAll, setExpandAllState] = useState(reading.expandAll ?? false);
  const setExpandAll = (enabled: boolean) => {
    reading.expandAll = enabled;
    setExpandAllState(enabled);
  };
  // Verification status (`g5.documents.ontology.verification.status`): each
  // statement's tag and the evidence of the state machines it links.
  const verification = useMemo(() => {
    if (!nests) return null;
    return {
      tags: records.map((r) => statementVerificationTags(blocks, r)),
      links: records.map((r) =>
        stateMachineLinks(markup.metadata(r), filePath),
      ),
      findings: records.map((r) =>
        findingLinks(markdownLinkDestinations(markup.metadata(r)), filePath),
      ),
    };
  }, [nests, records, blocks, markup, filePath]);
  const evidence = useVerificationEvidence(
    workspaceId,
    repoId,
    verification ? verification.links.flat() : [],
    content,
  );
  const statuses = verification
    ? statementStatuses(parents, verification.tags, (i) =>
        verification.links[i].map(evidence.evidenceOf),
      )
    : null;
  // The findings files the statements link, read once each for the dot a
  // Design verified status shows (`g5.documents.ontology.verification.status`).
  const findingFiles = useMemo(() => {
    const sets = new Map<string, readonly string[]>();
    for (const link of verification?.findings.flat() ?? [])
      sets.set(link.paths.join("\u0000"), link.paths);
    return [...sets];
  }, [verification]);
  const findingReads = useFirstVerificationTexts(
    workspaceId,
    repoId,
    findingFiles.map(([, paths]) => paths),
  );
  const parsedFindings = useRef(
    new Map<string, { text: string; byId: Map<string, FindingEntry> }>(),
  );
  const findingEntry = (link: FindingLink) => {
    const key = link.paths.join("\u0000");
    const read = findingReads[findingFiles.findIndex(([k]) => k === key)];
    if (read?.status !== "ok") return undefined;
    let parsed = parsedFindings.current.get(key);
    if (parsed?.text !== read.data.text) {
      parsed = {
        text: read.data.text,
        byId: new Map(
          parseFindings(read.data.text).entries.map((e) => [e.id, e]),
        ),
      };
      parsedFindings.current.set(key, parsed);
    }
    return parsed.byId.get(link.id);
  };
  const childIndexes = useMemo(() => {
    const children = parents.map(() => [] as number[]);
    parents.forEach((parent, i) => {
      if (parent >= 0) children[parent].push(i);
    });
    return children;
  }, [parents]);
  // A parent's dot counts its descendants' findings too.
  const statusDot = (index: number) => {
    if (!verification) return null;
    const linked: FindingEntry[] = [];
    const visit = (i: number) => {
      for (const link of verification.findings[i]) {
        const entry = findingEntry(link);
        if (entry) linked.push(entry);
      }
      childIndexes[i].forEach(visit);
    };
    visit(index);
    return findingsDot(linked);
  };
  // The statement each identity heading belongs to, for the outline.
  const recordByHeading = useMemo(
    () =>
      new Map(
        records.flatMap((r) =>
          r.current && r.current.headingIndex >= 0
            ? [[r.current.headingIndex, r] as const]
            : [],
        ),
      ),
    [records],
  );
  // Compare starts on when a document opens and keeps its state across views.
  const [compareOn, setCompareOn] = useState(reading.compare ?? true);
  const compare = ontology && compareOn;
  const setCompare = (enabled: boolean) => {
    reading.compare = enabled;
    setCompareOn(enabled);
  };
  const [humanOnly, setHumanOnlyState] = useState(reading.humanOnly ?? false);
  const setHumanOnly = (enabled: boolean) => {
    reading.humanOnly = enabled;
    setHumanOnlyState(enabled);
  };
  const [riskOnly, setRiskOnlyState] = useState(reading.riskOnly ?? false);
  const setRiskOnly = (enabled: boolean) => {
    reading.riskOnly = enabled;
    setRiskOnlyState(enabled);
  };
  const [openDetails, setOpenDetails] = useState<ReadonlySet<string>>(
    () => reading.openDetails ?? new Set(),
  );
  const setDetailsOpen = (id: string, open: boolean) =>
    setOpenDetails((current) => {
      const next = new Set(current);
      if (open) next.add(id);
      else next.delete(id);
      reading.openDetails = next;
      return next;
    });
  const baseline = useOntologyComparison(
    workspaceId,
    repoId,
    filePath,
    ontology && compare,
    props.repoOid ?? undefined,
  );
  const comparison = useMemo(() => {
    if (!compare || !ontology || !baseline.data || baseline.isError)
      return null;
    try {
      return {
        result: compareOntologyStatements(content, baseline.data.content),
        error: null,
      };
    } catch (error) {
      return { result: null, error: String(error) };
    }
  }, [content, ontology, compare, baseline.data, baseline.isError]);
  const changes = comparison?.result;
  const fences = useMemo(
    () => collectQuestionFences(content, blocks),
    [content, blocks],
  );
  const accentFor = (
    start: number,
    end: number,
  ): RecordQuestionAccent | null => {
    const f = fences.find((f) => f.blockIndex >= start && f.blockIndex < end);
    if (!f) return null;
    if (!f.parse.ok || f.duplicateId)
      return {
        severity: null,
        answered: false,
        humanAnswered: false,
        invalidPayload: true,
      };
    const q = f.parse.question;
    return {
      severity: q.severity ?? null,
      answered: q.status === "answered",
      humanAnswered:
        q.status === "answered" && q.answer.answered_by?.kind === "user",
    };
  };
  const questions: OutlineQuestionRecord[] = records
    .filter((r) => r.kind === "Q")
    .map((r) => {
      const a = accentFor(r.spanStart, r.spanEnd);
      return {
        index: r.spanStart,
        id: r.id,
        title: r.title,
        state: a?.invalidPayload
          ? "invalid"
          : a?.humanAnswered
            ? "decided"
            : a?.answered
              ? "proposed"
              : "open",
      };
    });
  const digest = useReviewSourceDigest(
    `${workspaceId}:${repoId}:${filePath}`,
    content,
    !!suggestionOverlay?.suggestions.some(
      (s) => s.status === "open" && s.kind === "hunk",
    ),
    props.sourceContent === undefined || content === props.sourceContent,
  );
  const projections = useMemo(
    () =>
      projectSpecReviews(
        content,
        blocks,
        suggestionOverlay?.suggestions.filter(
          (s) => s.kind === "hunk" && s.status === "open",
        ) ?? [],
        digest?.digest,
      ),
    [content, blocks, suggestionOverlay?.suggestions, digest?.digest],
  );
  const groups = useMemo(() => groupSpecReviews(projections), [projections]);
  const humanQuestionRecords = useMemo(
    () => (humanOnly ? answeredQuestionRecordStarts(content, blocks) : null),
    [humanOnly, content, blocks],
  );
  const recordIsRisk = (r: (typeof records)[number]) => {
    const accent = r.kind === "Q" ? accentFor(r.spanStart, r.spanEnd) : null;
    return (
      !!r.metadataTags?.some((t) => t.kind === "uncertainty") ||
      (r.kind === "Q" && !!(ontology || accent) && !accent?.answered)
    );
  };
  const recordMatches = (r: (typeof records)[number]) => {
    if (compare) return true;
    const human =
      humanQuestionRecords?.has(r.spanStart) ||
      r.metadataTags?.some((t) => t.kind === "human");
    return (!humanOnly || human) && (!riskOnly || recordIsRisk(r));
  };
  const filtering = hasRecords && !compare && (humanOnly || riskOnly);
  const visible = new Set<number>();
  // Statements shown only for a matching descendant: their lead stays.
  const shownForChild = new Set<number>();
  if (filtering) {
    for (const r of records)
      if (recordMatches(r))
        for (let i = r.spanStart; i < r.spanEnd; i++) visible.add(i);
    records.forEach((r, i) => {
      if (!recordMatches(r)) return;
      for (let p = parents[i]; p >= 0; p = parents[p]) {
        shownForChild.add(p);
        visible.add(records[p].spanStart);
        const lead = statementLeadBlock(blocks, records[p]);
        if (lead >= 0) visible.add(lead);
      }
    });
    const ancestors: number[] = [];
    blocks.forEach((block, index) => {
      if (block.type === "heading") {
        while (
          ancestors.length &&
          (blocks[ancestors.at(-1)!] as { level: number }).level >= block.level
        )
          ancestors.pop();
        ancestors.push(index);
        if (index === blocks.findIndex((b) => b.type === "heading"))
          visible.add(index);
      } else if (visible.has(index)) ancestors.forEach((i) => visible.add(i));
    });
  }
  const riskCount = records.filter(recordIsRisk).length;
  // The reading position follows the person across view switches.
  useEffect(() => {
    const container = scrollParent(root.current);
    if (!container) return;
    if (reading.scrollTop && !props.target && !props.fragment)
      container.scrollTop = reading.scrollTop;
    const keep = () => {
      reading.scrollTop = container.scrollTop;
    };
    container.addEventListener("scroll", keep, { passive: true });
    return () => container.removeEventListener("scroll", keep);
    // Restored once per mount; later scrolling is the person's.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  // A navigation target is resolved once per request: by the ontology's own
  // anchor rule where it applies, so Preview, the popover and Diff Graph
  // agree (`g5.documents.ontology.links`), then by rendered ids and records.
  const targetElement = (target: string): HTMLElement | null => {
    const container = root.current;
    if (!container) return null;
    const atBlock = (index: number) =>
      container.querySelector<HTMLElement>(`[data-block-index="${index}"]`);
    if (ontology) {
      const index = ontologyFragmentIndex(blocks, target);
      const record =
        index < 0
          ? undefined
          : records.find(
              (r) =>
                r.spanStart === index ||
                r.current?.anchorIndex === index ||
                r.current?.headingIndex === index,
            );
      const found = record
        ? (container.querySelector<HTMLElement>(
            `[data-record-span="${CSS.escape(record.id)}"]`,
          ) ?? atBlock(record.spanStart))
        : index >= 0
          ? atBlock(index)
          : null;
      if (found) return found;
    }
    return (
      Array.from(
        container.querySelectorAll<HTMLElement>(
          "[id],[data-record-span],[data-ontology-deleted-id],[data-finding-id]",
        ),
      ).find(
        (el) =>
          el.id === target ||
          el.id === `user-content-${target}` ||
          el.dataset.recordSpan === target ||
          el.dataset.ontologyDeletedId === target ||
          el.dataset.findingId === target,
      ) ?? null
    );
  };
  const target = props.target?.replace(/^#/, "") || props.fragment || null;
  const request = target ? `${target}\u0000${props.targetSeq ?? ""}` : null;
  const comparisonPending = compare && baseline.isFetching;
  // Folds opened for what is inside them, by reason: a Compare change in a
  // statement or a child, a filter match in a child, and suggestions anchored
  // inside.
  const autoOpen = new Map<string, Set<string>>();
  if (nests) {
    const openFor = (index: number | undefined, reason: string) => {
      if (index === undefined || index < 0) return;
      const id = records[topOf[index]].id;
      autoOpen.set(id, (autoOpen.get(id) ?? new Set()).add(reason));
    };
    if (changes) {
      for (const id of [
        ...changes.changes.keys(),
        ...changes.deletedBefore.keys(),
        ...changes.deletedAfter.keys(),
      ])
        openFor(indexById.get(id), "compare");
    }
    if (filtering)
      records.forEach((r, i) => {
        if (parents[i] >= 0 && recordMatches(r)) openFor(i, "filter");
      });
    for (const group of groups) {
      const at = group[0].firstBlock ?? group[0].boundaryBlock;
      if (at != null) openFor(recordAtBlock(at), "review");
    }
  }
  const reasonsFor = (id: string) => [...(autoOpen.get(id) ?? [])].join(" ");
  // A reader's choice stands until a new reason opens the fold again.
  const foldOpen = (id: string) => {
    const reasons = reasonsFor(id);
    const choice = statementState.fold(id);
    if (choice && (!reasons || choice.reasons === reasons)) return choice.open;
    return expandAll || !!reasons;
  };
  const setFold = (id: string, open: boolean) =>
    statementState.setFold(id, { open, reasons: reasonsFor(id) });
  // Top-level statements with something to open: child statements or own
  // content beyond the lead.
  const foldIds = nests
    ? records
        .filter(
          (r, i) =>
            parents[i] < 0 &&
            (parents.includes(i) ||
              Array.from(
                { length: r.spanEnd - r.spanStart - 1 },
                (_, k) => r.spanStart + 1 + k,
              ).some(
                (b) => !markup.hidden.has(b) && !staysWithLead(blocks, r, b),
              )),
        )
        .map((r) => r.id)
    : [];
  const [missing, setMissing] = useState<string | null>(null);
  // A target inside a closed fold opens it first, then comes into view.
  const pendingReveal = useRef<HTMLElement | null>(null);
  useEffect(() => {
    if (!target || !request) {
      setMissing(null);
      return;
    }
    if (reading.handledTarget === request) return;
    const el = targetElement(target);
    if (!el) {
      // A deleted statement's Before row appears once the comparison loads.
      if (comparisonPending) return;
      reading.handledTarget = request;
      setMissing(target);
      return;
    }
    reading.handledTarget = request;
    setMissing(null);
    // Opening re-renders the statement, which then reveals the target.
    const folded = el.closest<HTMLElement>("[data-fold-region][hidden]");
    if (folded) {
      pendingReveal.current = el;
      // Opening needs no reason: a fold that something inside opens is open.
      statementState.setFold(folded.dataset.foldRegion!, {
        open: true,
        reasons: "",
      });
    } else el.scrollIntoView?.({ block: "center" });
    reading.highlight = { target, until: Date.now() + TARGET_HIGHLIGHT_MS };
    return showHighlight();
    // targetElement reads the rendered tree these inputs produce.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [request, content, changes, comparisonPending]);
  // The highlight lasts its full time across later renders and remounts, such
  // as the reading view returning from Rich text with the draft written: each
  // render puts it on whatever element now holds the target.
  const showHighlight = () => {
    const highlight = reading.highlight;
    if (!highlight) return;
    const el = targetElement(highlight.target);
    const left = highlight.until - Date.now();
    if (left <= 0) {
      // Spent: later renders no longer look the target up.
      reading.highlight = undefined;
      el?.classList.remove(...TARGET_HIGHLIGHT);
      return;
    }
    if (!el) return;
    el.classList.add(...TARGET_HIGHLIGHT);
    const timer = window.setTimeout(
      () => el.classList.remove(...TARGET_HIGHLIGHT),
      left,
    );
    return () => window.clearTimeout(timer);
  };
  useLayoutEffect(showHighlight);
  const revealPending = useCallback(() => {
    const el = pendingReveal.current;
    if (!el || el.closest("[data-fold-region][hidden]")) return;
    pendingReveal.current = null;
    el.scrollIntoView?.({ block: "center" });
  }, []);
  // Each statement also reveals after it re-renders alone (`PreviewRecord`).
  useEffect(() => revealPending());
  // Selecting text while reading offers Comment. The bubble follows the
  // selection and goes when it collapses or the page scrolls.
  const [selectionBubble, setSelectionBubble] = useState<{
    position: { top: number; left: number };
    line: number;
    quote: string;
    /** The selection the bubble is about; another one replaces it. */
    range: Range;
  } | null>(null);
  const contentRoot = useRef<HTMLDivElement>(null);
  const findSelection = useCallback(() => {
    const sel = window.getSelection();
    const host = contentRoot.current;
    if (!host || !sel || sel.isCollapsed || sel.rangeCount === 0) return null;
    const quote = sel.toString().replace(/\s+/g, " ").trim();
    if (quote.length < 2) return null;
    const range = sel.getRangeAt(0);
    if (!host.contains(range.commonAncestorContainer)) return null;
    const start =
      range.startContainer instanceof Element
        ? range.startContainer
        : range.startContainer.parentElement;
    const end =
      range.endContainer instanceof Element
        ? range.endContainer
        : range.endContainer.parentElement;
    // Controls and cards keep their own selection, and Before and deleted
    // rows are not current text (`g5.documents.comments.identity`).
    if (
      !start ||
      start.closest(OWN_DOUBLE_CLICK) ||
      start.closest(NOT_COMMENTABLE) ||
      end?.closest(NOT_COMMENTABLE)
    )
      return null;
    const block = start.closest<HTMLElement>("[data-block-index]");
    if (!block) return null;
    const line = selectionSourceLine(
      content,
      blocks[Number(block.getAttribute("data-block-index"))],
      quote,
    );
    if (line === null) return null;
    const rect = range.getBoundingClientRect();
    return {
      position: { top: rect.top, left: rect.left + rect.width / 2 },
      line,
      quote,
      range: range.cloneRange(),
    };
  }, [content, blocks]);
  // Selection tracking runs whenever a thread can be started: a selection
  // made by keyboard or by a drag settles without a mouse-up, and another
  // selection replaces the bubble, or removes it when it collapses.
  const canCommentSelection = !!props.onCommentSelection;
  useEffect(() => {
    if (!canCommentSelection) {
      // Nothing can be started now; a bubble kept from before would offer
      // a stale line once starting is possible again.
      setSelectionBubble(null);
      return;
    }
    let timer: ReturnType<typeof setTimeout> | null = null;
    const settle = () => {
      timer = null;
      const next = findSelection();
      setSelectionBubble((current) =>
        current && next && sameRange(current.range, next.range)
          ? current
          : next,
      );
    };
    const onSelectionChange = () => {
      if (timer) clearTimeout(timer);
      const sel = window.getSelection();
      if (!sel || sel.isCollapsed || sel.rangeCount === 0) {
        setSelectionBubble(null);
        return;
      }
      timer = setTimeout(settle, 150);
    };
    document.addEventListener("selectionchange", onSelectionChange);
    return () => {
      if (timer) clearTimeout(timer);
      document.removeEventListener("selectionchange", onSelectionChange);
    };
  }, [canCommentSelection, findSelection]);
  useEffect(() => {
    if (!selectionBubble) return;
    const hide = () => setSelectionBubble(null);
    document.addEventListener("scroll", hide, true);
    return () => document.removeEventListener("scroll", hide, true);
  }, [selectionBubble]);
  const applyRewrite = (
    next: ReturnType<typeof rewriteQuestionInMarkdown>,
  ): { status: "applied" } | { status: "conflict"; message: string } => {
    if (!canAnswer || !onRewrite)
      return { status: "conflict", message: "This file is read-only." };
    if (!next.ok) return { status: "conflict", message: next.message };
    return latestRewrite.current!(content, next.content)
      ? { status: "applied" }
      : {
          status: "conflict",
          message: "The file changed. Review it and retry.",
        };
  };
  const rewrite = (
    id: string,
    body: string,
    change: (question: ParsedQuestion) => ParsedQuestion,
  ): QuestionApplyResult => {
    if (!canAnswer || !onRewrite)
      return { status: "conflict", message: "This file is read-only." };
    const fence = fences.find((f) => f.id === id && !f.duplicateId);
    if (!fence?.closed || !fence.parse.ok || fence.body !== body)
      return {
        status: "conflict",
        message: "The question changed. Review it and retry.",
      };
    try {
      const next = rewriteQuestionInMarkdown(
        content,
        fence.parse.question,
        change,
      );
      return applyRewrite(next);
    } catch (error) {
      return { status: "conflict", message: String(error) };
    }
  };
  const previewProps = {
    filePath,
    workspaceId,
    repoId,
    onOpenWorkspaceFile: props.onOpenWorkspaceFile,
  };
  // Ontology metadata links may contain unencoded spaces (`spacedLinks`).
  const metadataContext = ontology
    ? { ...previewProps, plugins: [spacedLinks] }
    : previewProps;
  const renderBlock = (
    block: Block,
    inline?: (text: string) => React.ReactNode,
  ) => {
    if (inline) {
      if (block.type === "paragraph") return <p>{inline(block.content)}</p>;
      if (block.type === "heading")
        return React.createElement(
          `h${block.level}`,
          {},
          inline(block.content),
        );
      if (block.type === "blockquote")
        return <blockquote>{inline(block.content)}</blockquote>;
    }
    return (
      <MarkdownPreview {...previewProps} content={blockToRawMarkdown(block)} />
    );
  };
  const suggestions = suggestionOverlay?.suggestions;
  const notes = useMemo(
    () =>
      suggestions?.filter((s) => s.kind !== "hunk" || s.status !== "open") ??
      [],
    [suggestions],
  );
  const noteBlocks = useMemo(
    () =>
      notes.map((suggestion) => {
        const line = resolveDisplayAnchor(
          suggestion,
          content,
          digest?.digest,
        ).line;
        if (line === null || suggestion.kind === "new_file") return -1;
        return blockIndexAtLine(content, blocks, line);
      }),
    [notes, content, blocks, digest?.digest],
  );
  const renderReview = (index: number, filtered = false) => {
    const [projection, ...peers] = groups[index];
    return (
      suggestionOverlay && (
        <SpecReviewSuggestions
          projection={projection}
          peers={peers}
          overlay={{ ...suggestionOverlay, selectedSource: digest }}
          renderBlock={renderBlock}
          filtered={filtered}
          marginNodes={marginNodes}
          onActivate={(itemId) => {
            setActiveItem(itemId);
            // Listed under the document, the activated card cannot be set
            // level with its mark: it comes into view instead.
            if (stacked) revealMarginItem(itemId);
          }}
          onOpenRepoLink={(href) => {
            const open = props.onOpenWorkspaceFile;
            const target = open ? resolveRepoLink(filePath, href) : null;
            if (!target) return false;
            open!(target.path, repoId, target.line);
            return true;
          }}
        />
      )
    );
  };
  // The margin beside the document: suggestion cards (one box per conflict
  // set), notes of decided and non-text suggestions, the draft and the
  // threads, each level with its text (`g5.documents.comments.identity`,
  // `g5.documents.review-placement`). Built again only when something in it
  // changed: the margin measures every item each time.
  const commentsShown = !!comments?.shown;
  const draft = comments?.draft;
  const notice = comments?.notice ?? null;
  const onReply = comments?.onReply;
  const onResolve = comments?.onResolve;
  const onUnresolve = comments?.onUnresolve;
  const onDelete = comments?.onDelete;
  const onOpenLine = comments?.onOpenLine;
  const marginItems = useMemo(() => {
    const host = () => root.current;
    // Text inside a closed fold has no place of its own: its card sits level
    // with the statement that holds it (`g5.documents.ontology.disclosure`).
    const shownAnchor = (element: HTMLElement | null) => {
      const folded = element?.closest<HTMLElement>(
        "[data-fold-region][hidden]",
      );
      if (!folded) return element;
      return (
        folded
          .closest<HTMLElement>("[data-testid='semantic-record-shell']")
          ?.querySelector<HTMLElement>("[data-record-span]") ?? element
      );
    };
    const within = (selector: string) =>
      shownAnchor(host()?.querySelector<HTMLElement>(selector) ?? null);
    const byId = (id: string) => within(`[id="${CSS.escape(id)}"]`);
    const blockElement = (index: number) =>
      index >= 0 ? within(`[data-block-index="${index}"]`) : null;
    const blockForLine = (line: number) => {
      const element = host();
      return element
        ? shownAnchor(renderedBlockForLine(element, content, blocks, line))
        : null;
    };
    const items: DocumentMarginItem[] = [];
    groups.forEach((group) => {
      const groupId = group[0].suggestion.id;
      for (const item of reviewMarginItems(group)) {
        const placeable = group.some(
          (p) => item.suggestionIds.includes(p.suggestion.id) && !p.attention,
        );
        items.push({
          id: item.id,
          anchor: () =>
            placeable
              ? (within(
                  item.suggestionIds
                    .map((id) => `[data-review-anchor="${CSS.escape(id)}"]`)
                    .join(","),
                ) ??
                byId(`spec-review-preview-${groupId}`) ??
                byId(`spec-review-group-${groupId}`))
              : null,
          node: <ReviewMarginItemSlot item={item} nodes={marginNodes} />,
        });
      }
    });
    notes.forEach((suggestion, index) => {
      items.push({
        id: `note:${suggestion.id}`,
        anchor: () => blockElement(noteBlocks[index]),
        node: suggestionOverlay && (
          <BlockSuggestions
            overlay={{ ...suggestionOverlay, selectedSource: digest }}
            suggestions={[suggestion]}
            content={content}
          />
        ),
      });
    });
    if (!commentsShown) return items;
    const top = () => contentRoot.current;
    if (notice)
      items.push({
        id: "comments-notice",
        anchor: top,
        node: <CommentsNotice notice={notice} />,
      });
    if (draft?.draft) {
      const line = draft.draft.lineNumber;
      items.push({
        id: "comment-draft",
        anchor: () => blockForLine(line) ?? top(),
        // Level with its line, the draft takes focus without scrolling;
        // listed under the document, it comes into view.
        node: <CommentDraftCard {...draft} focusScroll={stacked} />,
      });
    }
    for (const thread of threads) {
      const line = placedLine(thread);
      const anchor = () => (line === null ? null : blockForLine(line));
      items.push({
        id: `thread:${thread.id}`,
        anchor,
        node: (
          <CommentThread
            thread={thread}
            expanded={expandedThread === thread.id}
            onToggle={() => {
              const expand = expandedThread !== thread.id;
              setExpandedThread(expand ? thread.id : null);
              if (!expand) return;
              setActiveItem(`thread:${thread.id}`);
              // Listed under the document, the card cannot be set level
              // with its text: its block comes to the top instead.
              if (stacked) anchor()?.scrollIntoView?.({ block: "start" });
            }}
            onReply={onReply}
            onResolve={onResolve}
            onUnresolve={onUnresolve}
            onDelete={onDelete}
            onOpenLine={onOpenLine}
          />
        ),
      });
    }
    return items;
  }, [
    groups,
    notes,
    noteBlocks,
    threads,
    expandedThread,
    stacked,
    commentsShown,
    draft,
    notice,
    onReply,
    onResolve,
    onUnresolve,
    onDelete,
    onOpenLine,
    content,
    blocks,
    digest,
    suggestionOverlay,
    marginNodes,
  ]);
  const marginEmpty = commentsShown && marginItems.length === 0;
  const marginShown = commentsShown || marginItems.length > 0;
  // The panel's id says Comments is on; with it off, open suggestions alone
  // keep the margin, under its own name.
  const margin = marginShown && (
    <DocumentMargin
      items={marginItems}
      activeId={activeItem}
      rowRef={row}
      contentRef={contentRoot}
      stacked={stacked}
      testId={commentsShown ? comments?.testId : undefined}
    >
      {marginEmpty && <NoThreadsNote hint={comments?.emptyHint} />}
    </DocumentMargin>
  );
  const reviewPlugin = () => (tree: Root) => {
    const placed = new Set<number>();
    const covered = new Set(
      groups.flatMap((g) =>
        g
          .filter((p) => !p.attention && !p.sourceDiff)
          .flatMap((p) => p.coveredBlocks),
      ),
    );
    const reviewNode = (index: number): Element => ({
      type: "element",
      tagName: "g5-review",
      properties: { reviewIndex: index },
      children: [],
    });
    const visit = (parent: Root | Element) => {
      parent.children = (parent.children as RootContent[]).flatMap<RootContent>(
        (node) => {
          if (node.type !== "element") return [node];
          const blockIndex = Number(node.properties["data-block-index"]);
          const groupIndex = groups.findIndex(
            (g) =>
              g[0].firstBlock === blockIndex ||
              g[0].boundaryBlock === blockIndex,
          );
          const insertion = groupIndex >= 0 && !placed.has(groupIndex);
          if (insertion) placed.add(groupIndex);
          if (node.tagName === "g5-record") {
            visit(node);
            if (insertion) node.children.unshift(reviewNode(groupIndex));
            return [node];
          }
          // A statement's details and children hold blocks of their own.
          if (node.tagName === "g5-details" || node.tagName === "g5-kids") {
            visit(node);
            return [node];
          }
          const block = blocks[blockIndex];
          const domain =
            block?.type === "code" && /^g5-question/.test(block.language);
          const keep =
            (!covered.has(blockIndex) || domain) &&
            (!filtering || visible.has(blockIndex));
          return [
            ...(insertion ? [reviewNode(groupIndex)] : []),
            ...(keep ? [node] : []),
          ];
        },
      ) as typeof parent.children;
    };
    visit(tree);
    groups.forEach((_, index) => {
      if (!placed.has(index)) tree.children.push(reviewNode(index));
    });
  };
  // A statement's rendered parts: the lead stays in the frame, `g5-details`
  // and `g5-kids` go to the fold (`documentMarkup`).
  const partOf = (child: React.ReactNode) =>
    React.isValidElement<{ node?: Element }>(child)
      ? child.props.node?.tagName
      : undefined;
  const leadParts = (children: React.ReactNode) =>
    React.Children.toArray(children).filter(
      (child) => partOf(child) !== "g5-details" && partOf(child) !== "g5-kids",
    );
  const leadTextOf = (index: number) => {
    const lead = blocks[statementLeadBlock(blocks, records[index])];
    const text =
      lead && "content" in lead
        ? plainText(lead.content).replace(/\s+/g, " ").trim()
        : records[index].id;
    return text.length > 80 ? `${text.slice(0, 79).trimEnd()}…` : text;
  };
  // Bring a statement into view, opening the fold that holds it.
  const revealStatement = (id: string) => {
    const el = root.current?.querySelector<HTMLElement>(
      `[data-record-span="${CSS.escape(id)}"]`,
    );
    const folded = el?.closest<HTMLElement>("[data-fold-region][hidden]");
    if (el && folded) {
      pendingReveal.current = el;
      setFold(folded.dataset.foldRegion!, true);
    } else el?.scrollIntoView?.({ block: "center" });
  };
  const statementLayout = (
    index: number,
    children: React.ReactNode,
  ): StatementLayout => {
    const parts = React.Children.toArray(children);
    const details = parts.find((child) => partOf(child) === "g5-details");
    const kids = parts.find((child) => partOf(child) === "g5-kids");
    const record = records[index];
    const top = parents[index] < 0;
    const status = statuses?.[index];
    const panelId = `verification-${record.id}`;
    const panelOpen = statementState.panelOpen(record.id);
    return {
      marker: !top ? "child" : details || kids ? "fold" : "dot",
      open: top ? foldOpen(record.id) : true,
      onToggle: () => setFold(record.id, !foldOpen(record.id)),
      leadText: leadTextOf(index),
      details,
      childStatements: kids,
      status: status && (
        <VerificationStatusButton
          status={status.shown}
          open={panelOpen}
          controls={panelId}
          onToggle={() => statementState.togglePanel(record.id)}
          dot={status.shown === "design-verified" ? statusDot(index) : null}
        />
      ),
      panel: status && panelOpen && hasVerificationDetails(status.shown) && (
        <VerificationPanel
          id={panelId}
          workspaceId={workspaceId}
          repoId={repoId}
          status={status}
          links={verification!.links[index]}
          verifiedChildren={status.viaChildren.map((d) => ({
            id: records[d].id,
            text: leadTextOf(d),
          }))}
          onRevealStatement={revealStatement}
          onRetry={evidence.retry}
          details={metadataDetails(markup.metadata(record))}
          detailsContext={metadataContext}
          findings={verification!.findings[index]}
          onOpenFinding={
            props.onNavigate
              ? (path, id) =>
                  props.onNavigate!({
                    type: "file",
                    id: path,
                    workspaceId,
                    repoId,
                    intentRecordNumber: `#${id}`,
                    sourceFileName: filePath,
                    sourceIntentRecordNumber: record.id,
                    sourceFileType: "spec",
                  })
              : undefined
          }
          onOpenFile={
            props.onOpenWorkspaceFile
              ? (path, line) => props.onOpenWorkspaceFile!(path, repoId, line)
              : undefined
          }
        />
      ),
    };
  };
  const renderRecord = ({ node, children }: RecordProps) => {
    const index = Number(node.properties.recordIndex);
    const r = records[index];
    if (!r) return null;
    const accent = r.kind === "Q" ? accentFor(r.spanStart, r.spanEnd) : null;
    if (!recordMatches(r) && !shownForChild.has(index)) {
      const hiddenReviews = groups
        .map((group, index) => ({ group, index }))
        .filter(({ group }) => {
          const at = group[0].firstBlock ?? group[0].boundaryBlock;
          return at != null && at >= r.spanStart && at < r.spanEnd;
        });
      return hiddenReviews.length ? (
        <div>
          <p>Review target hidden by filters.</p>
          {hiddenReviews.map(({ index }) => (
            <React.Fragment key={index}>
              {renderReview(index, true)}
            </React.Fragment>
          ))}
        </div>
      ) : null;
    }
    const label = changes?.changes.get(r.id);
    const line = lineOfOffset(
      content,
      blocks[r.spanStart].sourceRange?.start ?? 0,
    );
    // A statement under a heading answers for that heading in the outline
    // and its links, though Preview hides the heading.
    const heading = r.current?.headingIndex ?? -1;
    return (
      <>
        {changes?.deletedBefore.get(r.id)?.map((s) => (
          <OntologyBeforeStatement
            key={s.record.id}
            statement={s}
            deleted
            className={nests ? "ontology-statement-row" : undefined}
          />
        ))}
        {heading >= 0 && <span data-block-index={heading} />}
        <OntologyComparisonPair
          id={`user-content-${r.id}`}
          sourceLine={line}
          status={label}
          statementId={r.id}
          before={changes?.before.get(r.id)}
          className={nests ? "ontology-statement-row" : undefined}
        >
          {r.current?.aliases.map((alias) => (
            // Former IDs stay link targets for the statement.
            <span key={alias} id={`user-content-${alias}`} />
          ))}
          <SemanticRecordFrame
            record={r}
            questionAccent={accent}
            presentationBullet={ontology && !label}
            metadata={markup.metadata(r)}
            metadataContext={metadataContext}
            detailsOpen={openDetails.has(r.id)}
            onDetailsOpenChange={(open) => setDetailsOpen(r.id, open)}
            statement={nests ? statementLayout(index, children) : undefined}
          >
            {nests ? leadParts(children) : children}
            {r.current?.diagnostic && (
              <p role="alert" data-change-highlight-ignore>
                {r.current.diagnostic}
              </p>
            )}
          </SemanticRecordFrame>
        </OntologyComparisonPair>
        {changes?.deletedAfter.get(r.id)?.map((s) => (
          <OntologyBeforeStatement
            key={s.record.id}
            statement={s}
            deleted
            className={nests ? "ontology-statement-row" : undefined}
          />
        ))}
      </>
    );
  };
  // The baseline query cannot run without a workspace and repository.
  const comparisonUnavailable =
    !workspaceId || !repoId || baseline.isError || !!comparison?.error;
  const comparisonStatus = !compare
    ? "Compare statement changes with the parent baseline. Prose, headings and metadata-only changes are not compared."
    : !workspaceId || !repoId
      ? "Comparison unavailable: workspace and repository are required."
      : baseline.isError
        ? `Comparison unavailable: ${baseline.error.message}`
        : (comparison?.error ??
          (baseline.data
            ? `${baseline.isFetching ? "Updating comparison… " : ""}Baseline ${baseline.data.baselineCommit.slice(0, 12)} · ${changes?.count ?? 0} statement changes`
            : "Updating comparison…"));
  // Compare and the filters: in the file row when FileView gives them a place
  // there, so they stay in view; otherwise above the document.
  const toolbar = (ontology || hasRecords) && (
    <div
      className={
        props.toolbarSlot
          ? "flex items-center gap-1"
          : "flex flex-wrap items-center gap-1 p-2"
      }
      data-testid="document-preview-toolbar"
    >
      {ontology && (
        <button
          type="button"
          data-testid="document-preview-compare"
          aria-label="Compare"
          title={comparisonStatus}
          aria-pressed={compare}
          className={previewToolbarButtonClass(compare, "info")}
          onClick={() => setCompare(!compare)}
        >
          <Columns2 className="h-3.5 w-3.5" aria-hidden="true" />
          <span>Compare</span>
          {compare && (
            <>
              <span
                data-testid="ontology-comparison-count"
                aria-hidden="true"
                className={previewToolbarCountClass("info")}
              >
                {changes?.count ?? (comparisonUnavailable ? "!" : "…")}
              </span>
              <span
                role="status"
                data-testid="ontology-comparison-status"
                className="sr-only"
              >
                {comparisonStatus}
              </span>
            </>
          )}
        </button>
      )}
      {ontology && hasRecords && (
        <div
          className="mx-1 h-4 w-px bg-[var(--g5-stroke)]"
          data-testid="document-preview-toolbar-divider"
          aria-hidden="true"
        />
      )}
      {hasRecords && (
        <>
          <button
            type="button"
            data-testid="toolbar-tag-human"
            aria-label="Show human-authored blocks"
            disabled={ontology && compare}
            aria-pressed={humanOnly}
            className={previewToolbarButtonClass(humanOnly, "ok")}
            onClick={() => setHumanOnly(!humanOnly)}
          >
            <User className="h-3.5 w-3.5" aria-hidden="true" />
            <span>Human</span>
          </button>
          <button
            type="button"
            data-testid="toolbar-tag-uncertainty"
            aria-label="Show uncertain blocks and open questions"
            disabled={compare}
            aria-pressed={riskOnly}
            className={previewToolbarButtonClass(riskOnly, "blocked")}
            onClick={() => setRiskOnly(!riskOnly)}
          >
            <AlertTriangle className="h-3.5 w-3.5" aria-hidden="true" />
            <span>Risk/Question</span>
            {riskCount > 0 && (
              <span
                data-testid="toolbar-risk-question-count"
                className={previewToolbarCountClass("blocked")}
              >
                {riskCount}
              </span>
            )}
          </button>
        </>
      )}
      {foldIds.length > 0 && (
        <>
          <button
            type="button"
            data-testid="ontology-expand-all"
            className={previewToolbarButtonClass(false, "info")}
            onClick={() => {
              setExpandAll(true);
              statementState.setFolds(new Map());
            }}
          >
            Expand all
          </button>
          <button
            type="button"
            data-testid="ontology-collapse-all"
            className={previewToolbarButtonClass(false, "info")}
            onClick={() => {
              setExpandAll(false);
              statementState.setFolds(
                new Map(
                  foldIds.map((id) => [
                    id,
                    { open: false, reasons: reasonsFor(id) },
                  ]),
                ),
              );
            }}
          >
            Collapse all
          </button>
        </>
      )}
      {props.toolbarSlot && (
        // Apart from the row's mode and editing controls that follow.
        <div
          className="mx-1 h-4 w-px bg-[var(--g5-stroke)]"
          aria-hidden="true"
        />
      )}
    </div>
  );
  const renderedDocument = (
    <MarkdownPreview
      {...previewProps}
      className="px-6 pt-4 pb-8"
      content={content}
      plugins={
        ontology
          ? [markup.plugin, reviewPlugin, spacedLinks]
          : findingsPlugin
            ? [markup.plugin, reviewPlugin, findingsPlugin]
            : [markup.plugin, reviewPlugin]
      }
      components={previewComponents}
      renderCode={(language, text, offset) => {
        if (isQuestionRefFenceLanguage(language))
          return (
            <ReferencedQuestionCard content={text} workspaceId={workspaceId} />
          );
        const f = fences.find(
          (f) => blocks[f.blockIndex].sourceRange?.start === offset,
        );
        if (!f) return null;
        const block = blocks[f.blockIndex];
        if (block.type !== "code") return null;
        const r = records.find(
          (r) => f.blockIndex >= r.spanStart && f.blockIndex < r.spanEnd,
        );
        return (
          <StructuredQuestionEditor
            block={block}
            docPath={props.sessionKey ?? `${workspaceId}:${repoId}:${filePath}`}
            fallbackId={f.fallbackId}
            duplicateId={f.duplicateId}
            canSave={canAnswer}
            currentUserId={props.currentUserId}
            currentUserEmail={props.currentUserEmail}
            currentUserAvatarUrl={props.currentUserAvatarUrl}
            onDiscuss={
              props.onDiscussQuestion
                ? (q) => props.onDiscussQuestion?.(q, r?.title)
                : undefined
            }
            onApplyAnswer={(q, input) =>
              Promise.resolve(
                rewrite(q.id, f.body, (live) => applyAnswer(live, input)),
              )
            }
            onClearAnswer={(q) =>
              Promise.resolve(rewrite(q.id, f.body, clearAnswer))
            }
            onApplyEdit={(q) => Promise.resolve(rewrite(q.id, f.body, () => q))}
            onRepairPayload={(body) =>
              applyRewrite(repairQuestionInMarkdown(content, f, body))
            }
          />
        );
      }}
    />
  );
  // A findings file is large and its decisions re-render the file view
  // several times while one line is written: its document renders again
  // only when what it shows changes (`g5.documents.findings`).
  const findingsDocument = useMemo(
    () => renderedDocument,
    // eslint-disable-next-line react-hooks/exhaustive-deps -- the inputs below decide a findings file's document
    findingsFile
      ? [
          content,
          findingsPlugin,
          markup,
          groups,
          filePath,
          workspaceId,
          repoId,
          canAnswer,
          props.onOpenWorkspaceFile,
          props.onNavigate,
          props.onDiscussQuestion,
          props.currentUserId,
          props.currentUserEmail,
          props.currentUserAvatarUrl,
          props.sessionKey,
        ]
      : [renderedDocument],
  );
  const documentBody = findingsFile ? findingsDocument : renderedDocument;
  return (
    <PreviewContext.Provider
      value={{
        record: renderRecord,
        review: renderReview,
        recordId: (node) => records[Number(node.properties.recordIndex)]?.id,
        statementState,
        revealPending,
      }}
    >
      <div
        ref={root}
        className="g5-document-links flex min-h-0 flex-col"
        data-testid="document-preview"
      >
        <div
          ref={row}
          className="flex min-h-0"
          data-testid="document-preview-row"
        >
          <DocumentOutline
            rootRef={root}
            blocks={blocks.map((b, i) => {
              // A statement's heading is listed by kind, never its title.
              const r = recordByHeading.get(i);
              if (r && b.type === "heading")
                return { ...b, content: RECORD_KIND_META[r.kind].label };
              return markup.hidden.has(i)
                ? { type: "paragraph", content: "" }
                : b;
            })}
            questionRecords={ontology ? [] : questions}
          />
          <div
            // Its own height, not the row's: the margin measures the document
            // by it, and a stretched column would hide an image loading below.
            className={`g5-doc-column min-w-0 flex-1 self-start ${ontology ? "ontology-statement-list" : ""}`}
            data-testid="document-preview-content"
            ref={contentRoot}
            // A click into the document gives it focus, so `e` reaches FileView.
            tabIndex={props.onRequestEdit ? -1 : undefined}
            onMouseUp={
              props.onCommentSelection
                ? () => {
                    // The browser settles the selection after mouse-up.
                    setTimeout(() => setSelectionBubble(findSelection()), 10);
                  }
                : undefined
            }
            onDoubleClick={
              props.onRequestEdit
                ? (event) => {
                    const target = event.target as HTMLElement;
                    if (target.closest(OWN_DOUBLE_CLICK)) return;
                    // Only the document's own text starts editing.
                    const host =
                      target.closest<HTMLElement>("[data-block-index]");
                    if (!host) return;
                    const index = Number(host.getAttribute("data-block-index"));
                    const selection = window.getSelection();
                    const word = selection?.toString().trim() ?? "";
                    selection?.removeAllRanges();
                    props.onRequestEdit?.({
                      offset: Number.isInteger(index)
                        ? (blocks[index]?.sourceRange?.start ?? null)
                        : null,
                      word:
                        word && word.length <= 80 && !/\s/.test(word)
                          ? word
                          : null,
                    });
                  }
                : undefined
            }
          >
            {!props.toolbarSlot && toolbar}
            {ontology &&
              currentOntologyDiagnostics(blocks).map((d, i) => (
                <p role="alert" key={i}>
                  {d}
                </p>
              ))}
            {missing && <p role="alert">Missing target: #{missing}</p>}
            <FindingsWrap host={findingsHost}>{documentBody}</FindingsWrap>
            {changes?.deletedAtEnd.map((s) => (
              <OntologyBeforeStatement
                key={s.record.id}
                statement={s}
                deleted
              />
            ))}
            {selectionBubble && props.onCommentSelection && (
              <SelectionCommentBubble
                position={selectionBubble.position}
                testId="document-preview-selection-comment-button"
                onComment={() => {
                  props.onCommentSelection?.({
                    line: selectionBubble.line,
                    quote: selectionBubble.quote,
                  });
                  // The draft holds the quote now; the selection, and with it
                  // the bubble, is done.
                  window.getSelection()?.removeAllRanges();
                  setSelectionBubble(null);
                }}
              />
            )}
          </div>
          {!stacked && margin}
          {ontology && props.onNavigate && (
            <CurrentOntologyReference
              workspaceId={workspaceId}
              repoId={repoId}
              path={filePath}
              content={content}
              rootRef={root}
              onNavigate={props.onNavigate}
              onOpenWorkspaceFile={props.onOpenWorkspaceFile}
            />
          )}
        </div>
        {stacked && margin}
        {props.toolbarSlot &&
          toolbar &&
          createPortal(toolbar, props.toolbarSlot)}
      </div>
    </PreviewContext.Provider>
  );
}
