import { OntologyTree } from "./OntologyTree";
import { WorkspaceReviewSidebar } from "../workspace-review/WorkspaceReviewContent";
import { useSourceControlMenuRequest } from "../source-control/SourceControlBrowser";
import {
  TaskStatusIndicator,
  taskStatusTitle,
} from "../shared/TaskStatusIndicator";
import {
  AlertCircle,
  BookOpen,
  Building2,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
  Compass,
  Database,
  Eye,
  FileText,
  Folder,
  FolderTree,
  GitCompare,
  GitMerge,
  Layers,
  Stamp,
  Loader,
  Map as MapIcon,
  MoreHorizontal,
  Pin,
  Plus,
  SquareTerminal,
  Trash2,
} from "lucide-react";
import React, {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import {
  getWorkspaceUiCapabilities,
  type WorkspaceUiCapabilities,
} from "../../lib/capabilities";
import type {
  DocNode,
  BranchSummary,
  TaskListEntry,
  ProjectDoc,
  TaskSummary,
  CommitHistoryEntry,
  SidebarDisplayMode,
} from "../../lib/types";
import {
  fileNameFromPath,
  formatLargeNumber,
  labelWorkspaceFolderInPath,
} from "../../lib/utils";
import {
  floatingSurfaceOpen,
  withinFloatingSurface,
} from "../../lib/floatingSurface";
import { GatedControl, useLiveGateReason } from "../../lib/workspaceLiveGate";
import { useWorkspaceNames } from "../../hooks/useWorkspaceNames";
import type { ReviewSuggestionPathCount } from "../../lib/types";
import { classNames } from "../ui/UI";
import {
  CompactContextMenu,
  CompactContextMenuItem,
} from "./CompactContextMenu";
import { WorkspaceIssuesSection } from "../external-issues/WorkspaceIssuesSection";
import type { WorkspaceIssuesPanelProps } from "../external-issues/WorkspaceIssuesPanel";
import {
  useExternalIssuesFeature,
  useRouteOrgMembership,
  useWorkspaceIssueLinks,
} from "../../hooks/useExternalIssues";

// ---------------------------------------------------------------------------
// Type exports
// ---------------------------------------------------------------------------

export type SidebarSectionId =
  | "ontology"
  | "approvals"
  | "guided"
  | "issues"
  | "project-docs"
  | "all-docs"
  | "source-control"
  | "tasks"
  | "files";

// Approvals is a header-only destination; the Workspace row (`activity`) is a
// panel (ui g5.ui.workbench-shell).
function isWorkspaceDestination(id: SidebarSectionId) {
  return id === "approvals";
}

/** Which canonical default section order applies (workbench g5.workbench.phases). */
export type SidebarSectionOrderPreset = "default" | "workbench";

export interface SourceControlDirtyFile {
  repoId: string;
  repoName: string;
  file: string;
}

/**
 * Internal workspace docs use a `#!internal-workspace-<slug>` folder prefix.
 * Docs with this prefix always render with the plain project-doc treatment.
 */
export const INTERNAL_WS_PREFIX = "#!internal-workspace-";

function pathHasInternalWorkspaceMarker(
  path: string | null | undefined,
): boolean {
  if (!path) return false;
  return path
    .split("/")
    .some((segment) => segment.startsWith(INTERNAL_WS_PREFIX));
}

export function isInternalWorkspaceDoc(doc: ProjectDoc | DocNode): boolean {
  const docNode = doc as DocNode;
  return (
    pathHasInternalWorkspaceMarker(docNode.folder) ||
    pathHasInternalWorkspaceMarker(docNode.filePath)
  );
}

const VIEW_DOC_TYPES = new Set([
  "view",
  "pm_view",
  "arch_view",
  "security_view",
  "performance_view",
  "test_view",
  "ui_view",
  "customized_view",
]);

export function isViewDoc(doc: ProjectDoc | DocNode): boolean {
  const docNode = doc as DocNode;
  return (
    VIEW_DOC_TYPES.has(docNode.docType) ||
    (!!docNode.category && VIEW_DOC_TYPES.has(docNode.category))
  );
}

/**
 * Whether a doc is explainer-typed: its wire `doc_type` (left-sidebar
 * D33/D34) or the legacy `category`. Pair it with the managed path through
 * {@link isWorkspaceExplainerDoc}; the type alone spans every workspace.
 */
export function isExplainerDoc(doc: ProjectDoc | DocNode): boolean {
  const docNode = doc as DocNode;
  return docNode.docType === "explainer" || docNode.category === "explainer";
}

/**
 * This workspace's own explainer: explainer-typed AND under the CURRENT
 * workspace's managed `<docsFolderPath>/explainers` subfolder (core D25 /
 * `is_managed_explainer_path`). The Guided journey reads it as the explainer
 * step's evidence (g5.workbench.journey.steps). Since 2026-10-30 explainers
 * are listed as ordinary Project Docs under `explainers/`, not in a rail
 * section of their own; a `docs/<other_workspace_id>/explainers/…` document
 * and a legacy top-level `explainers/…` document stay where their folders put
 * them (All Docs) and are not this workspace's evidence.
 */
export function isWorkspaceExplainerDoc(
  doc: ProjectDoc | DocNode,
  docsFolderPath: string | null | undefined,
): boolean {
  if (!isExplainerDoc(doc)) return false;
  const workspaceFolder = sanitizePathForDisplay(docsFolderPath ?? null);
  if (!workspaceFolder) return false;
  return docIsUnderFolder(doc, `${workspaceFolder}/explainers`);
}

export function sanitizePathForDisplay(
  path: string | null | undefined,
): string | null {
  if (!path) return null;
  let cleaned = path
    .split("/")
    .filter((segment) => !segment.startsWith(INTERNAL_WS_PREFIX))
    .join("/");

  // For internal workspace docs, hide legacy "designs/" prefixes so Project Docs
  // read as workspace-native files instead of nested design paths.
  if (pathHasInternalWorkspaceMarker(path)) {
    if (cleaned === "designs") cleaned = "";
    else if (cleaned.startsWith("designs/"))
      cleaned = cleaned.slice("designs/".length);
  }

  return cleaned.length > 0 ? cleaned : null;
}

// ---------------------------------------------------------------------------
// TreeNode — shared primitive
// ---------------------------------------------------------------------------

export interface TreeNodeProps {
  /** Display label */
  label: string;
  /** Nesting depth for indentation */
  depth: number;
  /** Whether this node is currently selected */
  isSelected: boolean;
  /** Whether this node is expanded (has visible children) */
  isExpanded: boolean;
  /** Whether this node can be expanded (has children or childCount > 0) */
  isExpandable: boolean;
  /** Icon element to render before the label */
  icon?: React.ReactNode;
  /** Optional label styling override */
  labelClassName?: string;
  /** Optional right-side decorations (status dots, action buttons) */
  trailing?: React.ReactNode;
  /** Stable domain identity for E2E selectors and diagnostics. */
  dataNodeId?: string;
  /** Click handler for selecting the node */
  onClick: () => void;
  /** Click handler for the expand/collapse chevron */
  onToggleExpand: () => void;
  /** Optional right-click handler for context menus */
  onContextMenu?: React.MouseEventHandler<HTMLDivElement>;
  /** Children nodes (rendered below when expanded) */
  children?: React.ReactNode;
}

export const TreeNode = React.memo(function TreeNode({
  label,
  depth,
  isSelected,
  isExpanded,
  isExpandable,
  icon,
  labelClassName,
  trailing,
  dataNodeId,
  onClick,
  onToggleExpand,
  onContextMenu,
  children,
}: TreeNodeProps) {
  const paddingLeft = depth * 16 + 12;

  return (
    <div>
      <div
        role="treeitem"
        data-testid={isExpandable ? "doc-entry" : "file-tree-entry"}
        data-g5-node-id={dataNodeId}
        aria-selected={isSelected}
        aria-expanded={isExpandable ? isExpanded : undefined}
        className={classNames(
          "group/treerow flex items-center gap-1.5 h-7 cursor-pointer select-none text-sm",
          "transition-colors duration-[120ms] ease-out",
          isSelected
            ? "bg-[var(--g5-bg2)] border-l-2 border-[var(--g5-info)]"
            : "hover:bg-[var(--g5-bg2)]/50 border-l-2 border-transparent",
        )}
        style={{ paddingLeft }}
        onClick={onClick}
        onContextMenu={onContextMenu}
      >
        {/* Expand/collapse chevron */}
        <button
          data-testid="tree-node-expand-toggle"
          className={classNames(
            "flex-shrink-0 w-4 h-4 flex items-center justify-center rounded-sm",
            "transition-transform duration-[120ms] ease-out",
            isExpandable
              ? "text-[var(--g5-text2)] hover:text-[var(--g5-text0)]"
              : "invisible",
          )}
          onClick={(e) => {
            e.stopPropagation();
            onToggleExpand();
          }}
          aria-label={isExpanded ? "Collapse" : "Expand"}
          tabIndex={-1}
        >
          <ChevronRight
            size={14}
            className={classNames(
              "transition-transform duration-[120ms] ease-out",
              isExpanded ? "rotate-90" : "",
            )}
          />
        </button>

        {/* Icon slot */}
        {icon ? (
          <span className="flex-shrink-0 w-4 h-4 flex items-center justify-center text-[var(--g5-text2)]">
            {icon}
          </span>
        ) : null}

        {/* Label text */}
        <span
          className={classNames(
            "flex-1 truncate",
            labelClassName ?? "text-[var(--g5-text0)]",
          )}
        >
          {label}
        </span>

        {/* Trailing slot */}
        {trailing && (
          <span className="flex-shrink-0 flex items-center gap-1 pr-2">
            {trailing}
          </span>
        )}
      </div>

      {/* Children rendered below when expanded */}
      {isExpanded && children && (
        <div className="transition-all duration-[120ms] ease-out">
          {children}
        </div>
      )}
    </div>
  );
});

// ---------------------------------------------------------------------------
// SidebarSection — collapsible section container
// ---------------------------------------------------------------------------

export interface SidebarSectionProps {
  id: SidebarSectionId;
  title: string;
  icon: React.ReactNode;
  badge?: number | string;
  /** Optional always-visible direct action rendered in the section header. */
  headerAction?: {
    label: string;
    icon: React.ReactNode;
    showLabel?: boolean;
    onActivate: () => void;
    /** Kept visible, disabled, with `title` saying why (`g5.workspace.ui.unavailable`). */
    disabled?: boolean;
    title?: string;
    /**
     * Why the action cannot run now: trying the disabled control reveals it
     * with Start when that is the valid action (`g5.ui.unavailable-actions`).
     */
    blockedReason?: string | null;
    /** The reason answers to the VM state only (terminal). */
    policyFree?: boolean;
  };
  /** Optional file-drop target applied to this section's existing header. */
  headerFileDropTarget?: {
    folderPath: string;
    onDropFiles: (files: File[]) => void;
  };
  /** Optional shared highlight spanning this header and its rendered body. */
  unifiedFileDropHighlight?: {
    bodyActive: boolean;
  };
  /** Opens the section-level contextual action menu. */
  onOpenContextMenu?: (position: { x: number; y: number }) => void;
  /** Whether to render the visible ellipsis for an available context menu. */
  showContextMenuButton?: boolean;
  headerOnly?: boolean;
  /** A single rail section owns a bounded scroller; stacked sections size naturally. */
  fillHeight?: boolean;
  isCollapsed: boolean;
  onToggleCollapse: () => void;
  children: React.ReactNode;
}

export function SidebarSection({
  id,
  title,
  icon,
  badge,
  headerAction,
  headerFileDropTarget,
  unifiedFileDropHighlight,
  onOpenContextMenu,
  showContextMenuButton = true,
  headerOnly = false,
  fillHeight = false,
  isCollapsed,
  onToggleCollapse,
  children,
}: SidebarSectionProps) {
  const [activeHeaderFileDropPath, setActiveHeaderFileDropPath] = useState<
    string | null
  >(null);
  const isHeaderFileDragActive =
    !!headerFileDropTarget &&
    activeHeaderFileDropPath === headerFileDropTarget.folderPath;
  const isUnifiedFileDragActive =
    !!unifiedFileDropHighlight &&
    (isHeaderFileDragActive || unifiedFileDropHighlight.bodyActive);

  const openKeyboardMenu = useCallback(
    (event: React.KeyboardEvent<HTMLDivElement>) => {
      if (!onOpenContextMenu) return;
      const rect = event.currentTarget.getBoundingClientRect();
      onOpenContextMenu({
        x: rect.left + Math.min(rect.width - 12, 220),
        y: rect.bottom + 4,
      });
    },
    [onOpenContextMenu],
  );

  const handleHeaderFileDrag = (event: React.DragEvent<HTMLDivElement>) => {
    const hasFiles =
      Array.from(event.dataTransfer.types).includes("Files") ||
      event.dataTransfer.files.length > 0;
    if (!headerFileDropTarget || !hasFiles) return;
    event.preventDefault();
    event.stopPropagation();
    event.dataTransfer.dropEffect = "copy";
    setActiveHeaderFileDropPath(headerFileDropTarget.folderPath);
  };

  const handleHeaderFileDragLeave = (
    event: React.DragEvent<HTMLDivElement>,
  ) => {
    if (!headerFileDropTarget) return;
    event.preventDefault();
    event.stopPropagation();
    const relatedTarget = event.relatedTarget;
    if (
      relatedTarget instanceof Node &&
      event.currentTarget.contains(relatedTarget)
    ) {
      return;
    }
    setActiveHeaderFileDropPath(null);
  };

  const handleHeaderFileDrop = (event: React.DragEvent<HTMLDivElement>) => {
    const files = Array.from(event.dataTransfer.files);
    const isFileDrop =
      files.length > 0 ||
      Array.from(event.dataTransfer.types).includes("Files");
    if (!headerFileDropTarget || !isFileDrop) return;
    event.preventDefault();
    event.stopPropagation();
    setActiveHeaderFileDropPath(null);
    if (files.length > 0) headerFileDropTarget.onDropFiles(files);
  };

  return (
    <div
      className={classNames(
        "flex flex-col",
        fillHeight && !isCollapsed && "min-h-0 flex-1",
        isUnifiedFileDragActive &&
          "overflow-hidden rounded-[var(--g5-radius-sm)] bg-[var(--g5-bg2)] ring-1 ring-inset ring-[var(--g5-active)]",
      )}
      data-section-id={id}
      data-testid={`sidebar-${sectionTestIdSegment(id)}-tab`}
    >
      <div
        data-surface="section-header"
        data-drop-state={
          isUnifiedFileDragActive
            ? "section"
            : isHeaderFileDragActive
              ? "header"
              : undefined
        }
        className={classNames(
          "group/sidebar-section flex h-8 w-full items-center gap-2 px-3",
          "border-b border-[var(--g5-stroke)]",
          "select-none transition-colors duration-[120ms] ease-out",
          isUnifiedFileDragActive
            ? "bg-transparent"
            : isHeaderFileDragActive
              ? "bg-[var(--g5-bg2)] ring-1 ring-inset ring-[var(--g5-active)]"
              : "bg-[var(--g5-bg1)] hover:bg-[var(--g5-bg2)]/50",
        )}
        role="button"
        tabIndex={0}
        onContextMenu={(event) => {
          if (!onOpenContextMenu) return;
          event.preventDefault();
          event.stopPropagation();
          onOpenContextMenu({ x: event.clientX, y: event.clientY });
        }}
        onClick={onToggleCollapse}
        onDragEnter={handleHeaderFileDrag}
        onDragOver={handleHeaderFileDrag}
        onDragLeave={handleHeaderFileDragLeave}
        onDrop={handleHeaderFileDrop}
        onKeyDown={(event) => {
          if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            onToggleCollapse();
            return;
          }
          if (
            onOpenContextMenu &&
            (event.key === "ContextMenu" ||
              (event.shiftKey && event.key === "F10"))
          ) {
            event.preventDefault();
            event.stopPropagation();
            openKeyboardMenu(event);
          }
        }}
        aria-label={title}
        aria-expanded={headerOnly ? undefined : !isCollapsed}
      >
        <div className="flex h-full flex-1 cursor-pointer items-center gap-2">
          {headerOnly ? (
            // Reserve the chevron's width so the icon and label line up with
            // the section rows (a leaf row in a tree keeps the expander's slot).
            <span
              aria-hidden="true"
              data-testid="sidebar-section-chevron-spacer"
              className="h-3 w-3 shrink-0"
            />
          ) : (
            <ChevronRight
              size={12}
              className={classNames(
                "shrink-0 text-[var(--g5-text2)] transition-transform duration-[120ms] ease-out",
                !isCollapsed && "rotate-90",
              )}
            />
          )}
          <span className="flex h-4 w-4 shrink-0 items-center justify-center text-[var(--g5-text2)]">
            {icon}
          </span>
          <span className="flex-1 text-left text-xs uppercase tracking-wide text-[var(--g5-text2)]">
            {title}
          </span>
          {badge !== undefined && badge !== null && (
            <span className="min-w-5 rounded bg-[var(--g5-bg2)] px-1.5 py-0.5 text-center text-xs text-[var(--g5-text2)]">
              {typeof badge === "number" ? formatLargeNumber(badge) : badge}
            </span>
          )}
        </div>
        {headerAction && (
          // The wrapper keeps a tried disabled control's click and keys
          // inside the header action, as the button does: bubbling to the
          // header would toggle the section and, in rail mode, close the
          // overlay before the reason can be read.
          <span
            className="contents"
            onClick={(event) => event.stopPropagation()}
            onMouseDown={(event) => event.stopPropagation()}
            onPointerDown={(event) => event.stopPropagation()}
            onKeyDown={(event) => {
              // Escape still reaches the document, which closes the hint.
              if (event.key !== "Escape") event.stopPropagation();
            }}
          >
            <GatedControl
              verb={headerAction.label.toLowerCase()}
              policyFree={headerAction.policyFree}
              reason={headerAction.blockedReason ?? null}
              testId="tree-node-header-action-hint"
              align="end"
              placement="fixed"
            >
              <button
                type="button"
                data-testid="tree-node-header-action-button"
                data-workspace-activity={
                  headerAction.label === "Terminal" ? "ignore" : undefined
                }
                className={classNames(
                  "flex h-6 shrink-0 items-center justify-center rounded opacity-100",
                  headerAction.showLabel ? "gap-1 px-1 text-xs" : "w-6",
                  "text-[var(--g5-text2)] transition-colors duration-[120ms]",
                  "hover:bg-[var(--g5-bg2)] hover:text-[var(--g5-text0)]",
                  "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-[var(--g5-info)]",
                  "disabled:cursor-not-allowed disabled:opacity-40",
                )}
                aria-label={headerAction.label}
                title={headerAction.title ?? headerAction.label}
                disabled={headerAction.disabled}
                onClick={(event) => {
                  event.stopPropagation();
                  headerAction.onActivate();
                }}
                onMouseDown={(event) => event.stopPropagation()}
                onPointerDown={(event) => event.stopPropagation()}
                onKeyDown={(event) => event.stopPropagation()}
              >
                {headerAction.icon}
                {headerAction.showLabel && headerAction.label}
              </button>
            </GatedControl>
          </span>
        )}
        {onOpenContextMenu && showContextMenuButton && (
          <button
            type="button"
            data-testid="tree-node-context-menu-button"
            className={classNames(
              "flex h-6 w-6 shrink-0 items-center justify-center rounded",
              "text-[var(--g5-text2)] opacity-0 transition-all duration-[120ms]",
              "hover:bg-[var(--g5-bg2)] hover:text-[var(--g5-text0)]",
              "focus:opacity-100 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-[var(--g5-info)]",
              "group-hover/sidebar-section:opacity-100 group-focus-within/sidebar-section:opacity-100",
            )}
            aria-label={`${title} actions`}
            title={`${title} actions`}
            onClick={(event) => {
              event.stopPropagation();
              const rect = event.currentTarget.getBoundingClientRect();
              onOpenContextMenu({
                x: rect.right - 4,
                y: rect.bottom + 4,
              });
            }}
            onMouseDown={(event) => event.stopPropagation()}
            onPointerDown={(event) => event.stopPropagation()}
            onKeyDown={(event) => {
              event.stopPropagation();
            }}
          >
            <MoreHorizontal size={14} />
          </button>
        )}
      </div>

      {headerFileDropTarget && (
        <p
          className="sr-only"
          role="status"
          aria-live="polite"
          data-testid="sidebar-section-header-drop-status"
        >
          {isHeaderFileDragActive
            ? `Drop files into ${headerFileDropTarget.folderPath}`
            : ""}
        </p>
      )}

      {!isCollapsed && (
        <div
          className={
            fillHeight
              ? "flex min-h-0 flex-1 flex-col overflow-hidden"
              : id === "source-control"
                ? "flex max-h-[70dvh] min-h-0 flex-col overflow-hidden"
                : "flex-1 overflow-y-auto scrollbar-thin"
          }
        >
          {children}
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// DocItem — row for Docs section
// ---------------------------------------------------------------------------

export interface DocItemProps {
  doc: ProjectDoc | DocNode;
  isSelected?: boolean;
  onClick: () => void;
  showDocumentSource?: boolean;
  onContextMenu?: (
    event: React.MouseEvent<HTMLDivElement>,
    doc: ProjectDoc | DocNode,
  ) => void;
}

const DOC_CATEGORY_ICONS: Record<string, React.ReactNode> = {
  architecture: <Building2 size={14} />,
  plan: <MapIcon size={14} />,
  guide: <BookOpen size={14} />,
  spec: <Database size={14} />,
  design: <Building2 size={14} />,
  api: <Layers size={14} />,
  adr: <BookOpen size={14} />,
  note: <BookOpen size={14} />,
};

export function DocItem({
  doc,
  isSelected,
  onClick,
  onContextMenu,
  showDocumentSource = false,
}: DocItemProps) {
  // Normalize: API returns name/docType, display expects title/category.
  // Show the file name (excluding directories) as the primary label and the
  // full path underneath — the backend's content-derived name/title (first
  // heading) is only used as a fallback when there is no file path.
  const docNode = doc as DocNode;
  const workspaceNamesById = useWorkspaceNames();
  const displayTitle =
    (docNode.filePath ? fileNameFromPath(docNode.filePath) : null) ||
    doc.title ||
    docNode.name ||
    "Untitled";
  // Relabel the leading `docs/<workspace_id>` segment with the workspace's
  // display name (pm_view D8) — display-only, the stored filePath is unchanged.
  const sanitizedPath = sanitizePathForDisplay(docNode.filePath);
  const displayPreview =
    (sanitizedPath
      ? labelWorkspaceFolderInPath(sanitizedPath, workspaceNamesById)
      : undefined) ||
    doc.preview ||
    (isInternalWorkspaceDoc(doc) ? "Project document" : undefined);
  const docCategory = doc.category || docNode.category || docNode.docType;
  const documentSource =
    docNode.documentSource === "user" || docNode.documentSource === "auxiliary"
      ? docNode.documentSource
      : null;
  const documentSourceTooltipId = React.useId();
  const documentSourceLabel =
    documentSource === "user"
      ? "User"
      : documentSource === "auxiliary"
        ? "Auxiliary"
        : null;
  const documentSourceDescription =
    documentSource === "user"
      ? "Uploaded or created by a user"
      : documentSource === "auxiliary"
        ? "Generated support artifact"
        : null;
  const docIcon =
    (docCategory && DOC_CATEGORY_ICONS[docCategory]) ||
    (isViewDoc(doc) ? <Eye size={14} /> : <BookOpen size={14} />);
  return (
    <div
      data-testid="doc-item"
      data-project-doc-drop-excluded="true"
      className={classNames(
        "group/doc flex items-start gap-2 px-3 py-2 cursor-pointer select-none",
        "transition-colors duration-[120ms] ease-out rounded-md",
        isSelected ? "bg-[var(--g5-active)]/15" : "hover:bg-white/[0.04]",
      )}
      onClick={onClick}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          onClick();
        }
      }}
      onContextMenu={(event) => onContextMenu?.(event, doc)}
      role="button"
      tabIndex={0}
      aria-describedby={
        showDocumentSource && documentSource
          ? documentSourceTooltipId
          : undefined
      }
    >
      <span
        className={classNames(
          "mt-0.5 shrink-0 text-[var(--g5-text2)]",
          isSelected && "text-[var(--g5-text1)]",
        )}
      >
        {docIcon}
      </span>
      <div className="flex-1 min-w-0">
        <div
          className={classNames(
            "text-sm truncate",
            isSelected
              ? "text-[var(--g5-text0)] font-medium"
              : "text-[var(--g5-text1)] group-hover/doc:text-[var(--g5-text0)]",
          )}
        >
          {displayTitle}
        </div>
        {displayPreview && displayPreview !== displayTitle && (
          <div className="text-xs text-[var(--g5-text2)] truncate">
            {displayPreview}
          </div>
        )}
      </div>
      {showDocumentSource && documentSource && documentSourceLabel && (
        <span
          className="group/source relative mt-0.5 shrink-0 rounded border border-white/10 px-1.5 py-0.5 text-[10px] leading-none text-[var(--g5-text2)]"
          aria-label={`Source: ${documentSourceLabel}`}
        >
          {documentSourceLabel}
          <span
            id={documentSourceTooltipId}
            role="tooltip"
            className="pointer-events-none absolute right-0 top-full z-20 mt-1 w-max max-w-48 rounded border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-2 py-1.5 text-left text-xs font-normal leading-snug text-[var(--g5-text1)] opacity-0 shadow-lg transition-opacity group-hover/source:opacity-100 group-focus-within/doc:opacity-100"
          >
            {documentSourceDescription}
          </span>
        </span>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// DocsSection — container rendering DocItem list
// ---------------------------------------------------------------------------

export interface DocsSectionProps {
  docs: (ProjectDoc | DocNode)[];
  selectedDocId?: string | null;
  onSelectDoc: (docId: string) => void;
  onDeleteDoc?: (doc: ProjectDoc | DocNode) => void;
  flat?: boolean;
  /** Show optional document-source markers on rows in this section. */
  showDocumentSource?: boolean;
  /**
   * Folder paths to render even when no doc lives under them (e.g. the
   * workspace's dedicated `docs/<workspace_id>` folder). Ignored in flat mode.
   */
  extraFolders?: string[];
  /**
   * Display-label overrides keyed by full folder path (e.g. mapping a
   * `docs/<workspace_id>` segment to the workspace's display name).
   */
  folderLabels?: Record<string, string>;
  /**
   * Folder path to hide as the visual root in tree mode. Docs under
   * `docs/...` can render as the contents of `docs` without a wrapper node.
   */
  folderDisplayRoot?: string;
  /** Controlled folder expansion state for tree-mode consumers. */
  expandedFolderPaths?: ReadonlySet<string>;
  /** Receives a cloned Set whenever tree-mode folder expansion changes. */
  onExpandedFolderPathsChange?: (next: Set<string>) => void;
  /**
   * When true, folder ancestors auto-expand as docs appear. Defaults false so
   * folder trees start collapsed unless a caller explicitly opts in.
   */
  autoExpandFolderAncestors?: boolean;
  /** Optional Project Docs-only file drop callback for rendered folders. */
  onDropProjectDocFiles?: (drop: ProjectDocsFileDrop) => void;
  /** Canonical folder path currently highlighted as the exact drop target. */
  activeDropFolderPath?: string | null;
  /** Updates the exact Project Docs drop target highlighted by the owner. */
  onActiveDropFolderPathChange?: (folderPath: string | null) => void;
}

export interface ProjectDocsFileDrop {
  files: File[];
  folderPath: string;
}

interface DocFolderTreeNode {
  path: string;
  label: string;
  docs: (ProjectDoc | DocNode)[];
  children: Map<string, DocFolderTreeNode>;
}

function folderPathSegments(path: string): string[] {
  return path.split("/").filter(Boolean);
}

function folderPathAncestors(path: string): string[] {
  const segments = folderPathSegments(path);
  return segments.map((_, index) => segments.slice(0, index + 1).join("/"));
}

function docFolderPath(doc: ProjectDoc | DocNode): string | null {
  const docNode = doc as DocNode;

  if (docNode.folder) {
    return sanitizePathForDisplay(docNode.folder);
  }

  if (!docNode.filePath) return null;

  const displayPath = sanitizePathForDisplay(docNode.filePath);
  if (!displayPath) return null;

  const segments = folderPathSegments(displayPath);
  if (segments.length <= 1) return null;
  return segments.slice(0, -1).join("/");
}

function docFolderPathForTree(
  doc: ProjectDoc | DocNode,
  folderDisplayRoot?: string,
): string | null {
  return stripFolderDisplayRoot(docFolderPath(doc), folderDisplayRoot);
}

function stripFolderDisplayRoot(
  path: string | null | undefined,
  folderDisplayRoot?: string,
): string | null {
  const folder = sanitizePathForDisplay(path);
  const displayRoot = sanitizePathForDisplay(folderDisplayRoot);
  if (!folder) return null;
  if (!displayRoot) return folder;
  if (folder === displayRoot) return null;
  if (folder.startsWith(`${displayRoot}/`)) {
    const stripped = folder.slice(displayRoot.length + 1);
    return stripped.length > 0 ? stripped : null;
  }
  return folder;
}

export function docIsUnderFolder(
  doc: ProjectDoc | DocNode,
  folderPath: string | null,
): boolean {
  if (!folderPath) return false;
  const folder = docFolderPath(doc);
  return folder === folderPath || !!folder?.startsWith(`${folderPath}/`);
}

function folderSegmentDisplayName(path: string): string {
  const sanitized = sanitizePathForDisplay(path);
  if (!sanitized) return "Project Docs";

  const segments = folderPathSegments(sanitized);
  return segments.length > 0 ? segments[segments.length - 1] : "Project Docs";
}

function buildDocFolderTree(
  folders: Map<string, (ProjectDoc | DocNode)[]>,
): DocFolderTreeNode {
  const root: DocFolderTreeNode = {
    path: "",
    label: "",
    docs: [],
    children: new Map(),
  };

  for (const [folder, folderDocs] of folders) {
    let current = root;
    let currentPath = "";

    for (const segment of folderPathSegments(folder)) {
      currentPath = currentPath ? `${currentPath}/${segment}` : segment;
      let child = current.children.get(segment);
      if (!child) {
        child = {
          path: currentPath,
          label: folderSegmentDisplayName(currentPath),
          docs: [],
          children: new Map(),
        };
        current.children.set(segment, child);
      }
      current = child;
    }

    current.docs.push(...folderDocs);
  }

  return root;
}

function docFolderNodeCount(node: DocFolderTreeNode): number {
  let count = node.docs.length;
  for (const child of node.children.values()) {
    count += docFolderNodeCount(child);
  }
  return count;
}

function collectDocFolderAncestors(
  docs: (ProjectDoc | DocNode)[],
  extraFolders?: string[],
  folderDisplayRoot?: string,
): Set<string> {
  const folders = new Set<string>();
  for (const doc of docs) {
    const folder = docFolderPathForTree(doc, folderDisplayRoot);
    if (folder) {
      for (const ancestor of folderPathAncestors(folder)) {
        folders.add(ancestor);
      }
    }
  }
  for (const extra of extraFolders ?? []) {
    const sanitized = stripFolderDisplayRoot(extra, folderDisplayRoot);
    if (!sanitized) continue;
    for (const ancestor of folderPathAncestors(sanitized)) {
      folders.add(ancestor);
    }
  }
  return folders;
}

export function DocsSection({
  docs,
  selectedDocId,
  onSelectDoc,
  onDeleteDoc,
  flat = false,
  showDocumentSource = false,
  extraFolders,
  folderLabels,
  folderDisplayRoot,
  expandedFolderPaths,
  onExpandedFolderPathsChange,
  autoExpandFolderAncestors = false,
  onDropProjectDocFiles,
  activeDropFolderPath,
  onActiveDropFolderPathChange,
}: DocsSectionProps) {
  const [contextMenu, setContextMenu] = useState<{
    doc: ProjectDoc | DocNode;
    x: number;
    y: number;
  } | null>(null);

  const [internalExpandedFolders, setInternalExpandedFolders] = useState<
    Set<string>
  >(() =>
    autoExpandFolderAncestors
      ? collectDocFolderAncestors(docs, extraFolders, folderDisplayRoot)
      : new Set(),
  );
  const knownFoldersRef = useRef<Set<string> | null>(null);
  const effectiveExpandedFolders =
    expandedFolderPaths ?? internalExpandedFolders;

  const updateExpandedFolders = useCallback(
    (updater: (prev: ReadonlySet<string>) => Set<string>) => {
      if (expandedFolderPaths && onExpandedFolderPathsChange) {
        onExpandedFolderPathsChange(updater(expandedFolderPaths));
        return;
      }
      setInternalExpandedFolders((prev) => updater(prev));
    },
    [expandedFolderPaths, onExpandedFolderPathsChange],
  );

  useEffect(() => {
    const currentFolders = collectDocFolderAncestors(
      docs,
      extraFolders,
      folderDisplayRoot,
    );
    if (!autoExpandFolderAncestors) {
      knownFoldersRef.current = currentFolders;
      return;
    }
    const knownFolders = knownFoldersRef.current;
    if (!knownFolders) {
      knownFoldersRef.current = currentFolders;
      return;
    }

    const newlyDiscoveredFolders = Array.from(currentFolders).filter(
      (folder) => !knownFolders.has(folder),
    );
    knownFoldersRef.current = currentFolders;
    if (newlyDiscoveredFolders.length === 0) return;

    updateExpandedFolders((prev) => {
      let changed = false;
      const next = new Set(prev);
      for (const folder of newlyDiscoveredFolders) {
        if (!next.has(folder)) {
          next.add(folder);
          changed = true;
        }
      }
      return changed ? next : new Set(prev);
    });
  }, [
    docs,
    extraFolders,
    folderDisplayRoot,
    autoExpandFolderAncestors,
    updateExpandedFolders,
  ]);

  const toggleFolder = useCallback(
    (folder: string) => {
      updateExpandedFolders((prev) => {
        const next = new Set(prev);
        if (next.has(folder)) next.delete(folder);
        else next.add(folder);
        return next;
      });
    },
    [updateExpandedFolders],
  );

  useEffect(() => {
    if (!contextMenu) return;

    const close = () => setContextMenu(null);
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") close();
    };

    document.addEventListener("pointerdown", close);
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("pointerdown", close);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [contextMenu]);

  const openContextMenu = useCallback(
    (event: React.MouseEvent<HTMLDivElement>, doc: ProjectDoc | DocNode) => {
      // View docs keep the context menu: with the Views sidebar section folded
      // into All Docs (left-sidebar D35), this menu is the only remaining
      // delete affordance for a retained view. The page routes view deletes to
      // the doc-scoped view route.
      if (!onDeleteDoc) {
        return;
      }
      event.preventDefault();
      event.stopPropagation();
      setContextMenu({
        doc,
        x: event.clientX,
        y: event.clientY,
      });
    },
    [onDeleteDoc],
  );

  const requestDeleteContextDoc = useCallback(() => {
    if (!contextMenu || !onDeleteDoc) return;
    const doc = contextMenu.doc;
    setContextMenu(null);
    onDeleteDoc(doc);
  }, [contextMenu, onDeleteDoc]);
  // Deleting a doc writes the running workspace's docs checkout.
  const deleteBlocked = useLiveGateReason("delete docs");

  const renderContextMenu = () =>
    contextMenu && onDeleteDoc ? (
      <CompactContextMenu
        x={contextMenu.x}
        y={contextMenu.y}
        label="Project Doc actions"
      >
        <CompactContextMenuItem
          icon={<Trash2 size={13} />}
          label="Delete"
          tone="danger"
          onClick={requestDeleteContextDoc}
          blockedReason={deleteBlocked}
        />
      </CompactContextMenu>
    ) : null;

  const renderDocItem = (doc: ProjectDoc | DocNode) => (
    <DocItem
      key={doc.id}
      doc={doc}
      isSelected={selectedDocId === doc.id}
      onClick={() => onSelectDoc(doc.id)}
      onContextMenu={openContextMenu}
      showDocumentSource={showDocumentSource}
    />
  );

  const sanitizedExtraFolders = (extraFolders ?? [])
    .map((folder) => stripFolderDisplayRoot(folder, folderDisplayRoot))
    .filter((folder): folder is string => !!folder);

  if (docs.length === 0 && (flat || sanitizedExtraFolders.length === 0)) {
    return (
      <div className="px-3 py-4 text-xs text-[var(--g5-text2)] text-center">
        No documents found
      </div>
    );
  }

  // Sort helper — alphabetical by display name (case-insensitive).
  // DocNode uses `name`, ProjectDoc uses `title`.
  const label = (d: ProjectDoc | DocNode): string =>
    "name" in d ? (d as DocNode).name : (d as ProjectDoc).title;
  const sortByName = (a: ProjectDoc | DocNode, b: ProjectDoc | DocNode) =>
    label(a).localeCompare(label(b), undefined, { sensitivity: "base" });

  // Group docs by folder (if folder field exists), pinned docs go first
  const pinned: (ProjectDoc | DocNode)[] = [];
  const folders = new Map<string, (ProjectDoc | DocNode)[]>();
  const ungrouped: (ProjectDoc | DocNode)[] = [];

  for (const doc of docs) {
    const docNode = doc as DocNode;
    const folder = docFolderPathForTree(doc, folderDisplayRoot);
    if (docNode.pinned) {
      pinned.push(doc);
    } else if (folder) {
      const list = folders.get(folder) || [];
      list.push(doc);
      folders.set(folder, list);
    } else {
      ungrouped.push(doc);
    }
  }

  // Synthesize empty folders (e.g. the workspace's dedicated docs folder) so
  // they render in the tree even before any doc lives under them.
  if (!flat) {
    for (const folder of sanitizedExtraFolders) {
      if (!folders.has(folder)) folders.set(folder, []);
    }
  }

  // Sort each group alphabetically so order is stable across renders
  pinned.sort(sortByName);
  for (const [, folderDocs] of folders) folderDocs.sort(sortByName);
  ungrouped.sort(sortByName);
  const folderTree = buildDocFolderTree(folders);
  const sortedFolderChildren = (node: DocFolderTreeNode) =>
    Array.from(node.children.values()).sort((a, b) =>
      a.label.localeCompare(b.label, undefined, { sensitivity: "base" }),
    );

  // Explicit flat mode for sections that should never render folder groups.
  if (flat) {
    const sorted = [...docs].sort(sortByName);
    return (
      <>
        <div role="list">{sorted.map(renderDocItem)}</div>
        {renderContextMenu()}
      </>
    );
  }

  // If no folders exist, render flat list
  if (folders.size === 0 && pinned.length === 0) {
    const sorted = [...docs].sort(sortByName);
    return (
      <>
        <div role="list">{sorted.map(renderDocItem)}</div>
        {renderContextMenu()}
      </>
    );
  }

  const renderFolderNode = (
    folderNode: DocFolderTreeNode,
    depth: number,
  ): React.ReactNode => {
    const isExpanded = effectiveExpandedFolders.has(folderNode.path);
    const childNodes = sortedFolderChildren(folderNode);
    const dropFolderPath = sanitizePathForDisplay(
      folderDisplayRoot
        ? `${folderDisplayRoot}/${folderNode.path}`
        : folderNode.path,
    );
    const isDropActive =
      !!dropFolderPath && activeDropFolderPath === dropFolderPath;

    const handleFolderDragEnter = (
      event: React.DragEvent<HTMLButtonElement>,
    ) => {
      if (
        !onDropProjectDocFiles ||
        !dropFolderPath ||
        (!Array.from(event.dataTransfer.types).includes("Files") &&
          event.dataTransfer.files.length === 0)
      ) {
        return;
      }
      event.preventDefault();
      event.stopPropagation();
      event.dataTransfer.dropEffect = "copy";
      onActiveDropFolderPathChange?.(dropFolderPath);
    };

    const handleFolderDragLeave = (
      event: React.DragEvent<HTMLButtonElement>,
    ) => {
      if (!onDropProjectDocFiles || !dropFolderPath) return;
      event.preventDefault();
      event.stopPropagation();
      const relatedTarget = event.relatedTarget;
      if (
        relatedTarget instanceof Node &&
        event.currentTarget.contains(relatedTarget)
      ) {
        return;
      }
      if (activeDropFolderPath === dropFolderPath) {
        onActiveDropFolderPathChange?.(null);
      }
    };

    const handleFolderDrop = (event: React.DragEvent<HTMLButtonElement>) => {
      const files = Array.from(event.dataTransfer.files);
      const isFileDrop =
        files.length > 0 ||
        Array.from(event.dataTransfer.types).includes("Files");
      if (!onDropProjectDocFiles || !dropFolderPath || !isFileDrop) return;
      event.preventDefault();
      event.stopPropagation();
      onActiveDropFolderPathChange?.(null);
      if (files.length > 0) {
        onDropProjectDocFiles({ files, folderPath: dropFolderPath });
      }
    };

    return (
      <div key={folderNode.path}>
        <button
          data-testid="doc-folder"
          type="button"
          aria-expanded={isExpanded}
          className={classNames(
            "w-full flex items-center gap-1.5 py-1.5 cursor-pointer",
            "text-xs text-[var(--g5-text1)] hover:bg-[var(--g5-bg2)]/50 transition-colors",
            isDropActive &&
              "bg-[var(--g5-bg2)] ring-1 ring-inset ring-[var(--g5-active)]",
          )}
          style={{ paddingLeft: 12 + depth * 12, paddingRight: 12 }}
          onClick={() => toggleFolder(folderNode.path)}
          onDragEnter={handleFolderDragEnter}
          onDragOver={handleFolderDragEnter}
          onDragLeave={handleFolderDragLeave}
          onDrop={handleFolderDrop}
        >
          <ChevronRight
            size={12}
            className={classNames(
              "shrink-0 transition-transform duration-150",
              isExpanded && "rotate-90",
            )}
          />
          <Folder size={12} className="shrink-0 text-[var(--g5-text2)]" />
          <span className="truncate">
            {folderLabels?.[folderNode.path] ?? folderNode.label}
          </span>
          <span className="ml-auto text-[10px] text-[var(--g5-text2)]">
            {docFolderNodeCount(folderNode)}
          </span>
        </button>
        {isExpanded && (
          <div role="group">
            {childNodes.map((child) => renderFolderNode(child, depth + 1))}
            {folderNode.docs.map((doc) => (
              <div key={doc.id} style={{ paddingLeft: (depth + 1) * 12 }}>
                {renderDocItem(doc)}
              </div>
            ))}
          </div>
        )}
      </div>
    );
  };

  return (
    <>
      <div role="list">
        {/* Pinned section */}
        {pinned.length > 0 && (
          <div>
            <div className="flex items-center gap-1.5 px-3 py-1.5 text-[10px] uppercase tracking-wide text-[var(--g5-text2)]">
              <Pin size={10} />
              <span>Pinned</span>
            </div>
            {pinned.map(renderDocItem)}
          </div>
        )}

        {/* Folder groups */}
        {sortedFolderChildren(folderTree).map((folderNode) =>
          renderFolderNode(folderNode, 0),
        )}

        {/* Ungrouped docs */}
        {ungrouped.map(renderDocItem)}
      </div>
      {renderContextMenu()}
    </>
  );
}

// ---------------------------------------------------------------------------
// First-class docs sections — Project Docs and All Docs
// ---------------------------------------------------------------------------

interface ProjectDocsSectionProps {
  /** The whole docs pool; the section keeps this workspace's folder. */
  docs: (ProjectDoc | DocNode)[];
  docsFolderPath?: string | null;
  selectedDocId?: string | null;
  onSelectDoc: (docId: string) => void;
  onDeleteDoc?: (doc: ProjectDoc | DocNode) => void;
  expandedFolderPaths?: ReadonlySet<string>;
  onExpandedFolderPathsChange?: (next: Set<string>) => void;
  onDropProjectDocFiles?: (drop: ProjectDocsFileDrop) => void;
  unifyEmptyRootDropHighlight?: boolean;
  onRootDropActivePathChange?: (folderPath: string | null) => void;
}

const EMPTY_EXPANDED_FOLDER_PATHS: ReadonlySet<string> = new Set();

export function ProjectDocsSection({
  docs,
  docsFolderPath,
  selectedDocId,
  onSelectDoc,
  onDeleteDoc,
  expandedFolderPaths = EMPTY_EXPANDED_FOLDER_PATHS,
  onExpandedFolderPathsChange,
  onDropProjectDocFiles,
  unifyEmptyRootDropHighlight = false,
  onRootDropActivePathChange,
}: ProjectDocsSectionProps) {
  const workspaceFolder = sanitizePathForDisplay(docsFolderPath ?? null);
  const [activeDropFolderPath, setActiveDropFolderPath] = useState<
    string | null
  >(null);

  // Every document of this workspace's folder, explainers and mocks included
  // (owner instruction, 2026-10-30: one folder, no categories above it); the
  // rows keep their source markers. Same folder derivation All Docs uses
  // (doc.folder, falling back to the filePath dirname), so Project Docs and
  // Archive agree.
  const workspaceDocs = useMemo(() => {
    if (!workspaceFolder) return [];
    return docs.filter((doc) => docIsUnderFolder(doc, workspaceFolder));
  }, [docs, workspaceFolder]);

  if (!workspaceFolder) return null;

  const handleRootFileDrag = (event: React.DragEvent<HTMLDivElement>) => {
    const hasFiles =
      Array.from(event.dataTransfer.types).includes("Files") ||
      event.dataTransfer.files.length > 0;
    if (!onDropProjectDocFiles || !hasFiles) return;
    event.preventDefault();
    event.stopPropagation();

    const target = event.target;
    if (
      target instanceof Element &&
      target.closest('[data-project-doc-drop-excluded="true"]')
    ) {
      event.dataTransfer.dropEffect = "none";
      setActiveDropFolderPath(null);
      onRootDropActivePathChange?.(null);
      return;
    }

    event.dataTransfer.dropEffect = "copy";
    setActiveDropFolderPath(workspaceFolder);
    onRootDropActivePathChange?.(workspaceFolder);
  };

  const handleRootDragLeave = (event: React.DragEvent<HTMLDivElement>) => {
    if (!onDropProjectDocFiles) return;
    event.preventDefault();
    event.stopPropagation();
    const relatedTarget = event.relatedTarget;
    if (
      relatedTarget instanceof Node &&
      event.currentTarget.contains(relatedTarget)
    ) {
      return;
    }
    setActiveDropFolderPath(null);
    onRootDropActivePathChange?.(null);
  };

  const handleRootDrop = (event: React.DragEvent<HTMLDivElement>) => {
    const files = Array.from(event.dataTransfer.files);
    const isFileDrop =
      files.length > 0 ||
      Array.from(event.dataTransfer.types).includes("Files");
    if (!onDropProjectDocFiles || !isFileDrop) return;
    event.preventDefault();
    event.stopPropagation();
    setActiveDropFolderPath(null);
    onRootDropActivePathChange?.(null);

    const target = event.target;
    if (
      files.length === 0 ||
      (target instanceof Element &&
        target.closest('[data-project-doc-drop-excluded="true"]'))
    ) {
      return;
    }
    onDropProjectDocFiles({ files, folderPath: workspaceFolder });
  };

  return (
    <div
      data-testid="project-docs-drop-zone"
      className={classNames(
        "relative min-h-12 rounded-[var(--g5-radius-sm)] transition-colors",
        activeDropFolderPath === workspaceFolder &&
          !unifyEmptyRootDropHighlight &&
          "bg-[var(--g5-bg2)] ring-1 ring-inset ring-[var(--g5-active)]",
      )}
      onDragEnter={handleRootFileDrag}
      onDragOver={handleRootFileDrag}
      onDragLeave={handleRootDragLeave}
      onDrop={handleRootDrop}
    >
      {workspaceDocs.length === 0 ? (
        <div className="px-3 py-4 text-xs text-[var(--g5-text2)] text-center">
          No project docs yet
        </div>
      ) : (
        <DocsSection
          docs={workspaceDocs}
          selectedDocId={selectedDocId}
          onSelectDoc={onSelectDoc}
          onDeleteDoc={onDeleteDoc}
          folderDisplayRoot={workspaceFolder}
          expandedFolderPaths={expandedFolderPaths}
          onExpandedFolderPathsChange={onExpandedFolderPathsChange}
          onDropProjectDocFiles={onDropProjectDocFiles}
          activeDropFolderPath={activeDropFolderPath}
          onActiveDropFolderPathChange={setActiveDropFolderPath}
          showDocumentSource
        />
      )}
      <p className="sr-only" role="status" aria-live="polite">
        {activeDropFolderPath ? `Drop files into ${activeDropFolderPath}` : ""}
      </p>
    </div>
  );
}

interface AllDocsSectionProps {
  /** The whole docs pool; the section drops this workspace's folder. */
  docs: (ProjectDoc | DocNode)[];
  docsFolderPath?: string | null;
  workspaceNamesById?: Record<string, string>;
  selectedDocId?: string | null;
  onSelectDoc: (docId: string) => void;
  expandedFolderPaths?: ReadonlySet<string>;
  onExpandedFolderPathsChange?: (next: Set<string>) => void;
}

export function AllDocsSection({
  docs,
  docsFolderPath,
  workspaceNamesById,
  selectedDocId,
  onSelectDoc,
  expandedFolderPaths = EMPTY_EXPANDED_FOLDER_PATHS,
  onExpandedFolderPathsChange,
}: AllDocsSectionProps) {
  const workspaceFolder = sanitizePathForDisplay(docsFolderPath ?? null);

  const allDocs = useMemo(() => {
    if (!workspaceFolder) return docs;
    return docs.filter((doc) => !docIsUnderFolder(doc, workspaceFolder));
  }, [docs, workspaceFolder]);

  // Label UUID folder segments after the docs/ display root is stripped.
  const folderLabels = useMemo(() => {
    const labels: Record<string, string> = {};
    for (const [id, name] of Object.entries(workspaceNamesById ?? {})) {
      labels[`docs/${id}`] = name;
      labels[id] = name;
    }
    return labels;
  }, [workspaceNamesById]);

  return (
    <DocsSection
      docs={allDocs}
      selectedDocId={selectedDocId}
      onSelectDoc={onSelectDoc}
      folderLabels={folderLabels}
      folderDisplayRoot="docs"
      expandedFolderPaths={expandedFolderPaths}
      onExpandedFolderPathsChange={onExpandedFolderPathsChange}
    />
  );
}

// ---------------------------------------------------------------------------
// TaskRow — one workspace task
// ---------------------------------------------------------------------------

export interface TaskRowProps {
  task: Pick<TaskSummary, "id" | "name" | "status" | "costUsd"> &
    Pick<TaskListEntry, "failure">;
  isSelected?: boolean;
  onSelectTask: (taskId: string) => void;
}

export function TaskRow({ task, isSelected, onSelectTask }: TaskRowProps) {
  return (
    <div
      className={classNames(
        "flex items-center gap-2 pl-8 pr-3 h-6 cursor-pointer select-none text-xs",
        "transition-colors duration-[120ms] ease-out",
        isSelected ? "bg-[var(--g5-bg2)]" : "hover:bg-[var(--g5-bg2)]/50",
      )}
      onClick={() => onSelectTask(task.id)}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          onSelectTask(task.id);
        }
      }}
      role="button"
      data-testid="tree-task-row"
      title={
        task.failure
          ? `Failed: ${task.failure.detail ?? task.failure.reason}`
          : undefined
      }
      tabIndex={0}
    >
      <span
        className="flex-shrink-0 w-3 h-3 flex items-center justify-center"
        title={taskStatusTitle(task.status)}
        aria-label={taskStatusTitle(task.status)}
      >
        <TaskStatusIndicator status={task.status} />
      </span>
      <span
        className={classNames(
          "flex-1 truncate",
          task.status === "succeeded"
            ? "text-[var(--g5-text1)]"
            : "text-[var(--g5-text2)]/70",
        )}
      >
        {task.name}
      </span>
    </div>
  );
}

// ---------------------------------------------------------------------------
// LeftSidebar — top-level container
// ---------------------------------------------------------------------------

export interface LeftSidebarProps {
  onSelectOntologyFile?: (repoId: string, path: string) => void;
  onOpenOntologyChanges?: () => void;
  ontologyChangesSelected?: boolean;
  onExplainOntologyFile?: (repoId: string, path: string) => void;
  onOpenApprovals?: () => void;
  /**
   * Collapse this sidebar from its own top row (the Activity row's trailing
   * action under the workbench preset, g5.workbench.issues); the page shows
   * an expand control at the collapsed edge. Omitted → no control here.
   */
  onCollapse?: () => void;
  /**
   * Expand the collapsed sidebar (ui g5.ui.sidebar-default, the
   * `collapse-to-rail` mode): the collapsed icon rail carries an expand
   * control, and pressing a section's icon expands the stacked panel on that
   * section. Omitted → the collapsed rail is not rendered.
   */
  onExpand?: () => void;
  /**
   * Terminal on the Files header for the owner under the alignment layout
   * (workbench g5.workbench.shell-chrome): the title bar's shortcut is gone,
   * so the rail keeps the Terminal center tab one click away. Kept visible,
   * disabled with the reason, while the VM is not running.
   */
  terminalAction?: {
    onActivate: () => void;
    disabled?: boolean;
    title?: string;
    /** The reason the terminal cannot open now; `null` when it can. */
    blockedReason?: string | null;
  };
  /** The Guided journey section's body (workbench g5.workbench.journey);
   * absent → no Guided section (classic layouts, Base, feature off). */
  guidedSection?: React.ReactNode;
  issuesSection?: Omit<WorkspaceIssuesPanelProps, "workspaceId"> & {
    onImportTask: () => void;
  };
  isOpen: boolean;
  /**
   * Default section order preset (workbench g5.workbench.phases): `workbench`
   * leads with Activity under the `alignment` layout. Only the DEFAULT
   * changes — a user's own reordering is kept either way.
   */
  sectionOrderPreset?: SidebarSectionOrderPreset;
  /**
   * User-scoped display mode (left-sidebar D38): `expanded` (default) renders
   * the stacked panel in flow; `collapse-to-rail` is the same panel whose
   * collapsed state (`isOpen` false) is the icon rail with an expand control
   * instead of nothing; `expand-on-hover` and `rail` render a compact icon
   * activity rail whose stacked panel opens as an overlay beside it — on
   * hover/focus for `expand-on-hover`, only on icon activation for `rail`.
   */
  displayMode?: SidebarDisplayMode;
  /**
   * Deprecated compatibility prop from the former admin-surface handoff. The
   * current sidebar always remains a stacked accordion.
   */
  railOnly?: boolean;
  /**
   * Deprecated compatibility prop. Worker/settings controls are owned by the
   * consolidated top-right controls, not the left sidebar.
   */
  activeRailAction?: "workers" | "settings" | "intent_log" | null;
  /**
   * Deprecated compatibility prop. Section selection now only expands/collapses
   * accordion bodies.
   */
  onSelectContentSection?: () => void;
  width: number;
  workspaceId: string;
  organization: string;
  branches: BranchSummary[];
  /** Open review suggestion counts per file, for the Files section badges. */
  suggestionCountPaths?: ReviewSuggestionPathCount[];
  capabilities?: WorkspaceUiCapabilities;

  tasks: TaskListEntry[];
  /** Workspace-wide activity, independent of loaded history pages. */
  hasActiveTask?: boolean;
  taskListStatus?: "loading" | "ok" | "error";
  taskListError?: Error | null;
  onRetryTasks?: () => void;
  hasMoreTasks?: boolean;
  isLoadingMoreTasks?: boolean;
  onLoadMoreTasks?: () => void;
  /** Project docs for the current workspace */
  docs: (ProjectDoc | DocNode)[];
  /**
   * Dedicated workspace docs folder convention ("docs/<workspace_id>"), from
   * `WorkspaceDetail.docsFolderPath`. Drives Project Docs filtering and All
   * Docs current-folder exclusion.
   */
  docsFolderPath?: string | null;
  /**
   * Workspace display names by id, used to label UUID folder segments under
   * `docs/` in the All Docs section (unknown ids fall back to the raw segment).
   */
  workspaceNamesById?: Record<string, string>;
  /** Currently selected doc ID (from active doc tab) */
  selectedDocId: string | null;
  selectedTaskId: string | null;
  /**
   * Open a document by identity; the shared file session derives current authority.
   */
  onSelectDoc: (docId: string) => void;
  /** Workspace-wide dirty draft files shown in Source Control. */
  sourceControlDirtyFiles?: SourceControlDirtyFile[];
  /** Open the singleton Source Control diff tab for a dirty file. */
  onOpenSourceControlDiff?: (target?: SourceControlDirtyFile) => void;
  /** Canonical workspace repositories available to Source Control History. */
  sourceControlBranches?: BranchSummary[];
  /** Current workspace branch used only for History's first-open default. */
  sourceControlActiveBranchId?: string;
  /** Open or replace the singleton read-only commit-diff tab. */
  onOpenSourceControlCommit?: (
    branch: BranchSummary,
    commit: CommitHistoryEntry,
  ) => void;
  selectedFilePath?: string | null;
  selectedFileBranchId?: string | null;
  onSelectFile: (branchId: string, filePath: string) => void;
  onSelectTask: (taskId: string) => void;
  /** Project doc creation callback */
  onCreateProjectDoc?: () => void;
  /** Open Project Docs upload for dropped files at a canonical destination. */
  onDropProjectDocFiles?: (drop: ProjectDocsFileDrop) => void;
  /** Project doc deletion callback */
  onDeleteProjectDoc?: (doc: ProjectDoc | DocNode) => void;
  /** File tree creation/deletion callbacks */
  onCreateFileEntry?: (branchId: string, parentPath: string | null) => void;
  onCreateFolderEntry?: (branchId: string, parentPath: string | null) => void;
  onDeleteFileEntry?: (branchId: string, entry: SidebarFileTreeEntry) => void;
  /** Optional externally controlled ontology filter. */
  searchQuery?: string;
  /** External request to expand a sidebar section (e.g. from the top-bar task
   * progress indicator opening the Task Queue). The `nonce` makes repeated
   * requests for the same section re-fire. */
  activateSectionRequest?: { section: SidebarSectionId; nonce: number } | null;
  onSectionRequestApplied?: (nonce: number) => void;
  onActiveSectionChange?: (section: SidebarSectionId | null) => void;
  /** Open the worker-host admin page. Retained for shell compatibility; this
   * sidebar no longer renders global admin action buttons. */
  onOpenWorkerAdmin?: () => void;
  /** Open the workspace settings page (`/:org/settings`). Retained for shell
   * compatibility; the top bar owns the settings affordance. */
  onOpenWorkspaceSettings?: () => void;
  /** Open the read-only All-intent-logs audit tab in the center panel.
   * Retained for shell compatibility; global actions are not rendered here. */
  onOpenIntentLog?: () => void;
  /**
   * Optional content rendered at the bottom of the sidebar below the scroll
   * area. Use this slot for workspace-level controls (e.g. PreviewControl)
   * that need a fixed footer position inside the sidebar boundary.
   */
  bottomSlot?: React.ReactNode;
}

// Tree component imports live here to keep the sidebar section wiring together.
import type { FileTreeEntry as SidebarFileTreeEntry } from "./FileTree";
import { WorkspaceFileTree } from "./WorkspaceFileTree";

// Core Ontology leads the stacked sidebar — it is the primary navigation surface.
const DEFAULT_SECTION_ORDER: SidebarSectionId[] = [
  "ontology",
  "project-docs",
  "all-docs",
  "source-control",
  "tasks",
  "files",
  "issues",
  "guided",
];

// Same membership in both presets preserves customised orders across layouts.
// Guided leads the workbench order (workbench g5.workbench.issues,
// g5.workbench.journey); the Overview control lives in the ribbon.
const WORKBENCH_SECTION_ORDER: SidebarSectionId[] = [
  "guided",
  "ontology",
  "project-docs",
  "files",
  "issues",
  "tasks",
  "all-docs",
  "source-control",
];

const CANONICAL_SECTION_ORDERS: Record<
  SidebarSectionOrderPreset,
  SidebarSectionId[]
> = {
  default: DEFAULT_SECTION_ORDER,
  workbench: WORKBENCH_SECTION_ORDER,
};

/**
 * Previous canonical defaults (including old merged-docs defaults). The
 * section order is auto-persisted on first mount, so a
 * stored order identical to one of these was never customised by the user and
 * migrates to the current {@link DEFAULT_SECTION_ORDER} instead of pinning the
 * old layout forever.
 */
const LEGACY_DEFAULT_SECTION_ORDERS: string[][] = [
  // Outgoing canonical orders that carried the rail's Explainers section
  // (explainers are Project Docs under `explainers/`, 2026-10-30).
  [
    "ontology",
    "project-docs",
    "explainers",
    "all-docs",
    "source-control",
    "tasks",
    "files",
    "issues",
    "guided",
  ],
  [
    "guided",
    "ontology",
    "project-docs",
    "files",
    "issues",
    "tasks",
    "explainers",
    "all-docs",
    "source-control",
  ],
  // Outgoing canonical orders that carried the rail's Activity section (the
  // Overview control moved into the ribbon, 2026-10-29).
  [
    "ontology",
    "project-docs",
    "explainers",
    "all-docs",
    "source-control",
    "tasks",
    "files",
    "activity",
    "issues",
    "guided",
  ],
  [
    "activity",
    "guided",
    "ontology",
    "project-docs",
    "files",
    "issues",
    "tasks",
    "explainers",
    "all-docs",
    "source-control",
  ],
  DEFAULT_SECTION_ORDER.filter((id) => id !== "ontology"),
  WORKBENCH_SECTION_ORDER.filter((id) => id !== "ontology"),
  // Outgoing canonical orders before the Guided section (both presets).
  // Previous production and review-branch defaults migrate together.
  [
    "ontology",
    "project-docs",
    "explainers",
    "all-docs",
    "source-control",
    "tasks",
    "files",
    "activity",
    "issues",
    "approvals",
  ],
  [
    "activity",
    "project-docs",
    "ontology",
    "files",
    "issues",
    "tasks",
    "explainers",
    "all-docs",
    "source-control",
    "approvals",
  ],
  [
    "ontology",
    "project-docs",
    "explainers",
    "all-docs",
    "source-control",
    "tasks",
    "files",
    "activity",
    "issues",
    "guided",
  ],
  [
    "activity",
    "guided",
    "project-docs",
    "ontology",
    "files",
    "issues",
    "tasks",
    "explainers",
    "all-docs",
    "source-control",
  ],
  [
    "ontology",
    "project-docs",
    "explainers",
    "all-docs",
    "source-control",
    "tasks",
    "files",
    "activity",
    "issues",
  ],
  [
    "activity",
    "project-docs",
    "ontology",
    "files",
    "issues",
    "tasks",
    "explainers",
    "all-docs",
    "source-control",
  ],
  [
    "ontology",
    "project-docs",
    "explainers",
    "all-docs",
    "source-control",
    "tasks",
    "files",
  ],
  [
    "project-docs",
    "ontology",
    "files",
    "tasks",
    "explainers",
    "all-docs",
    "source-control",
  ],
  ["project-docs", "ontology", "tasks", "files"],
  ["project-docs", "views", "ontology", "tasks", "files"],
  ["project-docs", "views", "issues", "ontology", "tasks", "files"],
  ["ontology", "project-docs", "views", "issues", "tasks", "files"],
  ["ontology", "project-docs", "issues", "tasks", "files"],
  ["ontology", "project-docs", "views", "all-docs", "issues", "tasks", "files"],
  // Outgoing canonical order containing the retired `views` id (replaced by
  // `explainers`, itself retired). Never-customised persisted orders that
  // match it snap to the current default; customised orders drop retired ids
  // via membership filtering and gain new ones through `mergeWithMissing`.
  [
    "ontology",
    "project-docs",
    "views",
    "all-docs",
    "source-control",
    "issues",
    "tasks",
    "files",
  ],
  // Outgoing canonical order containing the retired local `issues` section.
  [
    "ontology",
    "project-docs",
    "explainers",
    "all-docs",
    "source-control",
    "issues",
    "tasks",
    "files",
  ],
];

/**
 * A persisted order equal to a previous canonical default, or to either
 * current canonical default (the order is auto-persisted on first mount), was
 * never customised by the user and follows the active preset.
 */
function isUncustomisedOrder(order: string[]): boolean {
  return [
    ...LEGACY_DEFAULT_SECTION_ORDERS,
    ...LEGACY_DEFAULT_SECTION_ORDERS.map((order) =>
      order.filter((id) => id !== "ontology"),
    ),
    ...Object.values(CANONICAL_SECTION_ORDERS),
  ].some(
    (canonical) =>
      canonical.length === order.length &&
      canonical.every((id, i) => id === order[i]),
  );
}

function loadSectionOrder(
  workspaceId: string,
  preset: SidebarSectionOrderPreset,
): SidebarSectionId[] {
  const canonical = CANONICAL_SECTION_ORDERS[preset];
  try {
    const stored = localStorage.getItem(`g5-sidebar-order-${workspaceId}`);
    if (stored) {
      const parsed = JSON.parse(stored);
      if (Array.isArray(parsed)) {
        // A persisted order equal to a canonical default (previous or
        // current) was never customised — replace it with the active
        // preset's default. Compare against the raw persisted value so legacy
        // defaults still match even if their section membership differs from
        // the current canonical order.
        const retained = parsed.filter((id) => id !== "approvals");
        if (isUncustomisedOrder(parsed) || isUncustomisedOrder(retained)) {
          return canonical;
        }
        // Generic path: filter to valid sections, then insert any missing
        // canonical sections at their natural position.
        const filtered = retained.filter((id): id is SidebarSectionId =>
          canonical.includes(id as SidebarSectionId),
        );
        return mergeWithMissing(filtered, canonical);
      }
    }
  } catch (err) {
    // ignore
    console.warn("[TreeNode] Failed to read sidebar section order:", err);
  }
  return canonical;
}

/**
 * Append any sections from `canonical` that are missing from `filtered`,
 * preserving the user's customised positions for sections they already have.
 * New defaults land at the canonical position (using the order of `canonical`
 * as the rebalancing key) so that an added section appears in its "natural"
 * slot rather than tacked on at the end.
 */
function mergeWithMissing(
  filtered: SidebarSectionId[],
  canonical: SidebarSectionId[],
): SidebarSectionId[] {
  const result = [...filtered];
  for (const id of canonical) {
    if (result.includes(id)) continue;
    // Find the nearest already-present section that should sit immediately
    // before `id` in the canonical order, and insert after it.
    const canonicalIdx = canonical.indexOf(id);
    let insertAfter = -1;
    for (let i = canonicalIdx - 1; i >= 0; i--) {
      const prev = canonical[i];
      const idx = result.indexOf(prev);
      if (idx >= 0) {
        insertAfter = idx;
        break;
      }
    }
    result.splice(insertAfter + 1, 0, id);
  }
  return result;
}

function saveSectionOrder(workspaceId: string, order: SidebarSectionId[]) {
  try {
    localStorage.setItem(
      `g5-sidebar-order-${workspaceId}`,
      JSON.stringify(order),
    );
  } catch (err) {
    // ignore
    console.warn("[TreeNode] Failed to save sidebar section order:", err);
  }
}

/** Map a SidebarSectionId to its stable test-id segment (preserves legacy ids). */
function sectionTestIdSegment(id: SidebarSectionId): string {
  if (id === "project-docs") return "docs";
  if (id === "tasks") return "tasks";
  return id;
}

/**
 * Every section starts collapsed, Guided included, when a workspace is
 * opened or forked (ui g5.ui.guided-collapsed); nothing is remembered.
 */
function defaultCollapsedSections(): Set<SidebarSectionId> {
  return new Set<SidebarSectionId>(DEFAULT_SECTION_ORDER);
}

/**
 * Issues & tasks rail badge (issues g5.workbench.issues.arrival): the linked
 * count from the same query the section reads, pulsing once when it rises
 * after the first read so an arrival is noticed while the overlay is closed.
 * Nothing on first load; nothing when the count falls. Rendered only under
 * the workbench preset with an issues section, so its reads cost nothing
 * elsewhere.
 */
function IssuesRailBadge({ workspaceId }: { workspaceId: string }) {
  const membership = useRouteOrgMembership();
  const feature = useExternalIssuesFeature(membership?.orgId);
  const links = useWorkspaceIssueLinks(
    workspaceId,
    feature.status === "enabled",
  );
  const count = links.status === "ok" ? links.data.links.length : null;
  const seenCount = useRef<number | null>(null);
  const [pulse, setPulse] = useState(false);
  useEffect(() => {
    if (count === null) return;
    const seen = seenCount.current;
    seenCount.current = count;
    if (seen === null || count <= seen) return;
    setPulse(true);
    const timer = window.setTimeout(() => setPulse(false), 700);
    return () => window.clearTimeout(timer);
  }, [count]);
  if (!count) return null;
  return (
    <span
      data-testid="sidebar-rail-badge-issues"
      data-pulse={pulse ? "true" : undefined}
      className={classNames(
        "absolute -right-0.5 -top-0.5 min-w-4 rounded-full bg-[var(--g5-bg2)] px-1 text-center text-[9px] leading-4 text-[var(--g5-text2)]",
        pulse && "g5-badge-pulse-once",
      )}
    >
      {formatLargeNumber(count)}
    </span>
  );
}

export const LeftSidebar = React.memo(function LeftSidebar({
  onSelectOntologyFile,
  onOpenOntologyChanges,
  ontologyChangesSelected = false,
  onExplainOntologyFile,
  onOpenApprovals,
  onCollapse,
  onExpand,
  terminalAction,
  guidedSection,
  issuesSection,
  isOpen,
  displayMode = "expanded",
  sectionOrderPreset = "default",
  railOnly = false,
  activeRailAction = null,
  onSelectContentSection,
  width,
  workspaceId,
  suggestionCountPaths,
  organization,
  branches,
  capabilities = getWorkspaceUiCapabilities(),
  tasks,
  hasActiveTask = false,
  taskListStatus,
  taskListError,
  onRetryTasks,
  hasMoreTasks,
  isLoadingMoreTasks,
  onLoadMoreTasks,
  docs,
  docsFolderPath,
  workspaceNamesById,
  selectedDocId,
  selectedTaskId,
  selectedFilePath = null,
  selectedFileBranchId = null,
  onSelectDoc,
  onSelectFile,
  onSelectTask,
  onCreateProjectDoc,
  onDropProjectDocFiles,
  onDeleteProjectDoc,
  onCreateFileEntry,
  onCreateFolderEntry,
  onDeleteFileEntry,
  searchQuery: externalSearchQuery,
  activateSectionRequest,
  onSectionRequestApplied,
  onActiveSectionChange,
  onOpenWorkerAdmin,
  onOpenWorkspaceSettings,
  onOpenIntentLog,
  bottomSlot,
}: LeftSidebarProps) {
  void railOnly;
  void activeRailAction;
  void onSelectContentSection;
  void onOpenWorkerAdmin;
  // Creating a doc writes the running workspace: the owner
  // keeps the buttons while the VM is stopped, disabled with the reason
  // (`g5.workspace.ui.unavailable`).
  const docWriteBlocked = useLiveGateReason("create docs");
  // Dropping files creates docs too: no drop target while blocked.
  const dropProjectDocFiles =
    docWriteBlocked === null ? onDropProjectDocFiles : undefined;
  void onOpenWorkspaceSettings;
  void onOpenIntentLog;

  const [collapsedSectionState, setCollapsedSectionState] = useState<{
    workspaceId: string;
    sections: Set<SidebarSectionId>;
  }>(() => ({
    workspaceId,
    sections: defaultCollapsedSections(),
  }));
  const collapsedSections =
    collapsedSectionState.workspaceId === workspaceId
      ? collapsedSectionState.sections
      : defaultCollapsedSections();
  const setCollapsedSections = useCallback(
    (update: React.SetStateAction<Set<SidebarSectionId>>) => {
      setCollapsedSectionState((prev) => {
        const currentSections =
          prev.workspaceId === workspaceId
            ? prev.sections
            : defaultCollapsedSections();
        const nextSections =
          typeof update === "function"
            ? (
                update as (prev: Set<SidebarSectionId>) => Set<SidebarSectionId>
              )(currentSections)
            : update;
        return { workspaceId, sections: nextSections };
      });
    },
    [workspaceId],
  );
  // Rail display modes (left-sidebar D38): in `expand-on-hover` and `rail`
  // the persistent footprint is a compact icon rail and the stacked panel
  // renders as an overlay beside it. `expanded` (default) is untouched.
  const isRailMode =
    displayMode === "expand-on-hover" || displayMode === "rail";
  // `collapse-to-rail` (ui g5.ui.sidebar-default): open, it is the full
  // sidebar; collapsed, the icon rail with an expand control stands in for
  // the bare edge, and an icon press expands the panel on that section.
  const isCollapsedRail = displayMode === "collapse-to-rail" && !isOpen;
  const [revealSectionAfterExpand, setRevealSectionAfterExpand] =
    useState<SidebarSectionId | null>(null);
  // Both rail modes share one single-active-section overlay (D38, chats
  // 4e4a6f09 + 008903e9). `railActiveSection` is sticky across overlay
  // closes; `railOverlayVisible` is the open flag. The pinned bottom Task
  // Queue keeps its own collapse flag.
  const [railOverlayVisible, setRailOverlayVisible] = useState(false);
  const [railActiveSection, setRailActiveSection] =
    useState<SidebarSectionId | null>(null);
  const [railTasksCollapsed, setRailTasksCollapsed] = useState(false);
  // Close the overlay when the display mode changes so returning to a rail
  // mode never reopens a stale overlay without a fresh activation (PR #1956
  // review). React's adjust-state-during-render pattern, matching the
  // workspace-keyed section state above.
  const [railOverlayDisplayMode, setRailOverlayDisplayMode] =
    useState(displayMode);
  if (railOverlayDisplayMode !== displayMode) {
    setRailOverlayDisplayMode(displayMode);
    setRailOverlayVisible(false);
  }
  const railOverlayOpen =
    isRailMode && railOverlayVisible && railActiveSection !== null;
  const sourceControlMenuRequest = useSourceControlMenuRequest();
  const handledSourceControlMenuRequest = useRef(0);
  useEffect(() => {
    if (sourceControlMenuRequest === handledSourceControlMenuRequest.current)
      return;
    handledSourceControlMenuRequest.current = sourceControlMenuRequest;
    setCollapsedSections((previous) => {
      const next = new Set(previous);
      next.delete("source-control");
      return next;
    });
    if (isRailMode) {
      setRailActiveSection("source-control");
      setRailOverlayVisible(true);
    }
  }, [sourceControlMenuRequest, isRailMode, setCollapsedSections]);
  const asideRef = useRef<HTMLElement>(null);
  // Once the panel has expanded from the collapsed rail, bring the pressed
  // section into view (it was uncollapsed when pressed).
  useEffect(() => {
    if (!isOpen || revealSectionAfterExpand === null) return;
    const target = asideRef.current?.querySelector(
      `[data-section-id="${revealSectionAfterExpand}"]`,
    );
    if (
      target instanceof HTMLElement &&
      typeof target.scrollIntoView === "function"
    ) {
      target.scrollIntoView({ block: "nearest" });
    }
    setRevealSectionAfterExpand(null);
  }, [isOpen, revealSectionAfterExpand]);
  // Whether a pointer press is in progress (hover mode). A press on
  // non-focusable overlay content (a file row, a repository root) blurs the
  // focused control with no related target before the click lands, and
  // closing the overlay then would unmount the row under the pointer; the
  // pointerdown rule below decides those presses instead.
  const pointerPressRef = useRef(false);
  useEffect(() => {
    if (displayMode !== "expand-on-hover") return;
    const handlePointerDown = () => {
      pointerPressRef.current = true;
    };
    const handlePointerUp = () => {
      pointerPressRef.current = false;
    };
    document.addEventListener("pointerdown", handlePointerDown, true);
    document.addEventListener("pointerup", handlePointerUp, true);
    document.addEventListener("pointercancel", handlePointerUp, true);
    return () => {
      pointerPressRef.current = false;
      document.removeEventListener("pointerdown", handlePointerDown, true);
      document.removeEventListener("pointerup", handlePointerUp, true);
      document.removeEventListener("pointercancel", handlePointerUp, true);
    };
  }, [displayMode]);

  // Rail overlay dismissal: Escape and clicks outside the sidebar.
  useEffect(() => {
    if (!isRailMode || !railOverlayVisible) return;
    const handlePointerDown = (event: PointerEvent) => {
      if (
        asideRef.current &&
        event.target instanceof Node &&
        asideRef.current.contains(event.target)
      ) {
        return;
      }
      // A popover a section anchored (the roster popover) is portaled to the
      // body: a click in it is inside the overlay (lib/floatingSurface.ts).
      if (withinFloatingSurface(event.target)) return;
      setRailOverlayVisible(false);
    };
    const handleKeyDown = (event: KeyboardEvent) => {
      // A floating surface a section opened (a preview dialog) takes Escape
      // first; the overlay under it stays so focus returns into it.
      if (event.key === "Escape" && !floatingSurfaceOpen()) {
        setRailOverlayVisible(false);
      }
    };
    document.addEventListener("pointerdown", handlePointerDown);
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("pointerdown", handlePointerDown);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [isRailMode, railOverlayVisible]);

  // Section order
  // Reload the persisted order when the workspace or the layout preset
  // changes; an uncustomised order then follows the new preset's default.
  const sectionOrderKey = `${workspaceId}|${sectionOrderPreset}`;
  const [sectionOrder, setSectionOrder] = useState<SidebarSectionId[]>(() =>
    loadSectionOrder(workspaceId, sectionOrderPreset),
  );
  const [sectionOrderLoadedKey, setSectionOrderLoadedKey] =
    useState(sectionOrderKey);
  if (sectionOrderLoadedKey !== sectionOrderKey) {
    setSectionOrderLoadedKey(sectionOrderKey);
    setSectionOrder(loadSectionOrder(workspaceId, sectionOrderPreset));
  }
  const searchQuery = externalSearchQuery ?? "";
  const showCloudWorkspaceWorkflows = capabilities.cloudWorkspaceWorkflows;
  const [sectionContextMenu, setSectionContextMenu] = useState<{
    sectionId: "project-docs";
    x: number;
    y: number;
  } | null>(null);
  const docsFolderStateKey = `${workspaceId}:${
    sanitizePathForDisplay(docsFolderPath ?? null) ?? ""
  }`;
  const [projectDocsFolderState, setProjectDocsFolderState] = useState<{
    key: string;
    expanded: Set<string>;
  }>(() => ({ key: docsFolderStateKey, expanded: new Set() }));
  const projectDocsExpandedFolderPaths =
    projectDocsFolderState.key === docsFolderStateKey
      ? projectDocsFolderState.expanded
      : EMPTY_EXPANDED_FOLDER_PATHS;
  const setProjectDocsExpandedFolderPaths = useCallback(
    (next: Set<string>) => {
      setProjectDocsFolderState({
        key: docsFolderStateKey,
        expanded: new Set(next),
      });
    },
    [docsFolderStateKey],
  );
  const [allDocsFolderState, setAllDocsFolderState] = useState<{
    key: string;
    expanded: Set<string>;
  }>(() => ({ key: docsFolderStateKey, expanded: new Set() }));
  const allDocsExpandedFolderPaths =
    allDocsFolderState.key === docsFolderStateKey
      ? allDocsFolderState.expanded
      : EMPTY_EXPANDED_FOLDER_PATHS;
  const setAllDocsExpandedFolderPaths = useCallback(
    (next: Set<string>) => {
      setAllDocsFolderState({
        key: docsFolderStateKey,
        expanded: new Set(next),
      });
    },
    [docsFolderStateKey],
  );

  // Persist section order on change
  useEffect(() => {
    saveSectionOrder(workspaceId, sectionOrder);
  }, [workspaceId, sectionOrder]);

  useEffect(() => {
    if (!sectionContextMenu) return;

    const close = () => setSectionContextMenu(null);
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") close();
    };

    document.addEventListener("pointerdown", close);
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("pointerdown", close);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [sectionContextMenu]);

  const canOpenSectionContextMenu = useCallback(
    (sectionId: SidebarSectionId) =>
      sectionId === "project-docs" && !!onCreateProjectDoc,
    [onCreateProjectDoc],
  );

  const openSectionContextMenu = useCallback(
    (sectionId: SidebarSectionId, position: { x: number; y: number }) => {
      if (sectionId !== "project-docs") return;
      if (!canOpenSectionContextMenu(sectionId)) return;
      setSectionContextMenu({ sectionId, ...position });
    },
    [canOpenSectionContextMenu],
  );

  const createFromSectionMenu = useCallback(() => {
    if (!sectionContextMenu) return;
    setSectionContextMenu(null);
    onCreateProjectDoc?.();
  }, [onCreateProjectDoc, sectionContextMenu]);

  const sectionContextMenuLabel = sectionContextMenu
    ? "Project Docs actions"
    : "";

  const sectionContextMenuItemLabel = sectionContextMenu
    ? "Create project doc"
    : "";

  const renderSectionContextMenu = () =>
    sectionContextMenu ? (
      <CompactContextMenu
        x={sectionContextMenu.x}
        y={sectionContextMenu.y}
        label={sectionContextMenuLabel}
      >
        <CompactContextMenuItem
          icon={<Plus size={13} />}
          label={sectionContextMenuItemLabel}
          onClick={createFromSectionMenu}
          blockedReason={docWriteBlocked}
        />
      </CompactContextMenu>
    ) : null;

  const toggleCollapse = useCallback(
    (sectionId: SidebarSectionId) => {
      if (isWorkspaceDestination(sectionId)) {
        onOpenApprovals?.();
        return;
      }
      setCollapsedSections((prev) => {
        const next = new Set(prev);
        if (next.has(sectionId)) {
          next.delete(sectionId);
        } else {
          next.add(sectionId);
        }
        return next;
      });
    },
    [setCollapsedSections, onOpenApprovals],
  );

  // Rail icon activation, identical in both rail modes (left-sidebar D38):
  // activating an icon opens/switches the single-section overlay to that
  // section; activating the active section's own icon closes the overlay
  // (chats d4afa88c, 4e4a6f09, 008903e9).
  const handleRailSectionActivate = useCallback(
    (sectionId: SidebarSectionId) => {
      if (isWorkspaceDestination(sectionId)) {
        setRailOverlayVisible(false);
        onOpenApprovals?.();
        return;
      }
      if (railOverlayVisible && railActiveSection === sectionId) {
        setRailOverlayVisible(false);
        return;
      }
      setRailActiveSection(sectionId);
      setRailOverlayVisible(true);
    },
    [railOverlayVisible, railActiveSection, onOpenApprovals],
  );

  // Collapsed rail (`collapse-to-rail`): a section icon expands the full
  // sidebar on that section; Approvals stays a workspace destination.
  const handleCollapsedRailActivate = useCallback(
    (sectionId: SidebarSectionId) => {
      if (isWorkspaceDestination(sectionId)) {
        onOpenApprovals?.();
        return;
      }
      setCollapsedSections((prev) => {
        if (!prev.has(sectionId)) return prev;
        const next = new Set(prev);
        next.delete(sectionId);
        return next;
      });
      setRevealSectionAfterExpand(sectionId);
      onExpand?.();
    },
    [onOpenApprovals, onExpand, setCollapsedSections],
  );

  // Apply an external request to reveal a section (e.g. the top-bar task
  // progress indicator opening the Task Queue). The processed-nonce ref makes
  // re-runs from other dependency changes no-ops while repeated requests for
  // the same section (new nonce) still re-fire. In rail modes the stacked
  // panel is not in flow, so the request also opens the single-section
  // overlay on the target (PR #1956 review).
  const activateSectionNonce = activateSectionRequest?.nonce;
  const activateSectionTarget = activateSectionRequest?.section;
  const processedActivateNonceRef = useRef<number | null>(null);
  useEffect(() => {
    if (!activateSectionTarget || activateSectionNonce === undefined) return;
    if (processedActivateNonceRef.current === activateSectionNonce) return;
    processedActivateNonceRef.current = activateSectionNonce;
    onSectionRequestApplied?.(activateSectionNonce);
    if (isWorkspaceDestination(activateSectionTarget)) {
      setRailOverlayVisible(false);
      onOpenApprovals?.();
      return;
    }
    setCollapsedSections((prev) => {
      if (!prev.has(activateSectionTarget)) return prev;
      const next = new Set(prev);
      next.delete(activateSectionTarget);
      return next;
    });
    if (isRailMode) {
      setRailActiveSection(activateSectionTarget);
      setRailOverlayVisible(true);
    }
  }, [
    activateSectionNonce,
    activateSectionTarget,
    onSectionRequestApplied,
    onOpenApprovals,
    setCollapsedSections,
    isRailMode,
  ]);

  // One docs pool for Project Docs and All Docs (left-sidebar D36): view docs
  // are included (they fold into All Docs) and so are explainers and mocks,
  // which are Project Docs under their `explainers/` and `mocks/` subfolders
  // (owner instruction, 2026-10-30: no Explainers rail section). Each doc
  // appears in exactly one docs section, decided by its folder alone.
  const projectDocs = docs;
  const projectDocsRootFolderPath = sanitizePathForDisplay(
    docsFolderPath ?? null,
  );
  const projectDocsRootIsEmpty = useMemo(
    () =>
      !!projectDocsRootFolderPath &&
      !projectDocs.some((doc) =>
        docIsUnderFolder(doc, projectDocsRootFolderPath),
      ),
    [projectDocs, projectDocsRootFolderPath],
  );
  const [activeProjectDocsBodyDropPath, setActiveProjectDocsBodyDropPath] =
    useState<string | null>(null);

  // Check if pm-paper theme is active (hides Files section)
  const [isPmPaper, setIsPmPaper] = useState(false);
  useEffect(() => {
    const checkTheme = () => {
      const theme = document.documentElement.getAttribute("data-theme");
      setIsPmPaper(theme === "pm-paper");
    };
    checkTheme();
    const observer = new MutationObserver(checkTheme);
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["data-theme"],
    });
    return () => observer.disconnect();
  }, []);

  // Section renderers
  const sectionConfig: Record<
    SidebarSectionId,
    {
      title: string;
      icon: React.ReactNode;
      badge?: number | string;
      /** A badge that owns its own reads, rendered instead of `badge`. */
      badgeNode?: React.ReactNode;
      hidden?: boolean;
      render: () => React.ReactNode;
    }
  > = useMemo(
    () => ({
      ontology: {
        title: "Core Ontology",
        icon: <Database size={14} />,
        render: () => (
          <OntologyTree
            workspaceId={workspaceId}
            branches={branches}
            onSelectFile={onSelectOntologyFile ?? onSelectFile}
            onOpenChanges={onOpenOntologyChanges}
            changesSelected={ontologyChangesSelected}
            onExplainFile={onExplainOntologyFile}
            searchQuery={externalSearchQuery}
            selected={
              selectedFileBranchId && selectedFilePath
                ? { repoId: selectedFileBranchId, path: selectedFilePath }
                : null
            }
          />
        ),
      },
      approvals: {
        title: "Approvals",
        icon: <Stamp size={14} />,
        hidden: sectionOrderPreset !== "workbench" || !onOpenApprovals,
        render: () => null,
      },
      guided: {
        title: "Guided",
        icon: <Compass size={14} />,
        hidden: sectionOrderPreset !== "workbench" || guidedSection == null,
        render: () => guidedSection ?? null,
      },
      issues: {
        title: "Issues & tasks",
        icon: <AlertCircle size={14} />,
        hidden: sectionOrderPreset !== "workbench",
        badgeNode: issuesSection ? (
          <IssuesRailBadge workspaceId={workspaceId} />
        ) : undefined,
        render: () =>
          issuesSection ? (
            <WorkspaceIssuesSection
              workspaceId={workspaceId}
              onOpenIssue={issuesSection.onOpenIssue}
              viewerOwnsWorkspace={issuesSection.viewerOwnsWorkspace}
              writeBlockedReason={issuesSection.writeBlockedReason}
              linkBlockedReason={issuesSection.linkBlockedReason}
            />
          ) : null,
      },
      "project-docs": {
        title: "Project Docs",
        icon: <BookOpen size={14} />,
        render: () => (
          <WorkspaceReviewSidebar
            enabled={sectionOrderPreset === "workbench"}
            workspaceId={workspaceId}
            section="docs"
          >
            <ProjectDocsSection
              docs={projectDocs}
              docsFolderPath={docsFolderPath}
              selectedDocId={selectedDocId}
              onSelectDoc={onSelectDoc}
              onDeleteDoc={onDeleteProjectDoc}
              expandedFolderPaths={projectDocsExpandedFolderPaths}
              onExpandedFolderPathsChange={setProjectDocsExpandedFolderPaths}
              onDropProjectDocFiles={dropProjectDocFiles}
              unifyEmptyRootDropHighlight={projectDocsRootIsEmpty}
              onRootDropActivePathChange={setActiveProjectDocsBodyDropPath}
            />
          </WorkspaceReviewSidebar>
        ),
      },
      "all-docs": {
        // Display title "Archive" (renamed from "All Docs (read-only)",
        // left-sidebar D19); the all-docs id and test id are unchanged.
        title: "Archive",
        icon: <FolderTree size={14} />,
        render: () => (
          <AllDocsSection
            docs={projectDocs}
            docsFolderPath={docsFolderPath}
            workspaceNamesById={workspaceNamesById}
            selectedDocId={selectedDocId}
            onSelectDoc={onSelectDoc}
            expandedFolderPaths={allDocsExpandedFolderPaths}
            onExpandedFolderPathsChange={setAllDocsExpandedFolderPaths}
          />
        ),
      },
      "source-control": {
        title: "Source Control",
        icon: <GitCompare size={14} />,
        render: () => (
          <WorkspaceReviewSidebar workspaceId={workspaceId} section="code" />
        ),
      },
      tasks: {
        title: "Tasks",
        icon: <Layers size={14} />,
        hidden: !showCloudWorkspaceWorkflows,
        badge: hasActiveTask ? "Active" : undefined,
        render: () => (
          <TasksPanel
            status={taskListStatus}
            error={taskListError}
            onRetry={onRetryTasks}
            tasks={tasks}
            hasMoreTasks={hasMoreTasks}
            isLoadingMoreTasks={isLoadingMoreTasks}
            onLoadMoreTasks={onLoadMoreTasks}
            selectedTaskId={selectedTaskId}
            onSelectTask={onSelectTask}
          />
        ),
      },
      files: {
        title: "Files",
        icon: <FolderTree size={14} />,
        hidden: isPmPaper,
        render: () => (
          <WorkspaceReviewSidebar
            enabled={sectionOrderPreset === "workbench"}
            workspaceId={workspaceId}
            section="files"
          >
            <WorkspaceFileTree
              branches={branches}
              organization={organization}
              selectedFile={
                selectedFileBranchId && selectedFilePath
                  ? {
                      branchId: selectedFileBranchId,
                      path: selectedFilePath,
                    }
                  : null
              }
              onSelectFile={onSelectFile}
              onCreateFile={onCreateFileEntry}
              onCreateFolder={onCreateFolderEntry}
              onDeleteEntry={onDeleteFileEntry}
              suggestionCounts={suggestionCountPaths}
            />
          </WorkspaceReviewSidebar>
        ),
      },
    }),
    [
      onSelectOntologyFile,
      onOpenOntologyChanges,
      ontologyChangesSelected,
      onExplainOntologyFile,
      externalSearchQuery,
      sectionOrderPreset,
      onOpenApprovals,
      guidedSection,
      issuesSection,
      workspaceId,
      suggestionCountPaths,
      projectDocs,
      projectDocsRootIsEmpty,
      projectDocsExpandedFolderPaths,
      setProjectDocsExpandedFolderPaths,
      docsFolderPath,
      workspaceNamesById,
      allDocsExpandedFolderPaths,
      setAllDocsExpandedFolderPaths,

      tasks,
      taskListStatus,
      taskListError,
      onRetryTasks,
      hasMoreTasks,
      isLoadingMoreTasks,
      onLoadMoreTasks,
      branches,
      organization,

      selectedDocId,
      selectedTaskId,
      selectedFilePath,
      selectedFileBranchId,

      searchQuery,
      hasActiveTask,
      isPmPaper,
      showCloudWorkspaceWorkflows,

      onSelectDoc,
      onSelectFile,
      onSelectTask,

      onDeleteProjectDoc,
      dropProjectDocFiles,
      onCreateFileEntry,
      onCreateFolderEntry,
      onDeleteFileEntry,
    ],
  );

  // Visible sections (accordion order, capability/theme gating applied).
  const visibleSectionIds = useMemo(
    () =>
      sectionOrder.filter(
        (id) => sectionConfig[id] && !sectionConfig[id].hidden,
      ),
    [sectionOrder, sectionConfig],
  );

  // A sticky rail target that a workspace switch hid (Guided on Base, a
  // capability-gated section) would reopen as an empty panel: drop it so the
  // next hover or activation starts from the first visible section.
  if (
    railActiveSection !== null &&
    !visibleSectionIds.includes(railActiveSection)
  ) {
    setRailActiveSection(null);
    setRailOverlayVisible(false);
  }

  const effectiveActiveSection = collapsedSections.has("files")
    ? null
    : "files";
  useEffect(() => {
    onActiveSectionChange?.(effectiveActiveSection);
  }, [effectiveActiveSection, onActiveSectionChange]);

  if (!isOpen && !(isCollapsedRail && onExpand)) {
    return null;
  }

  // One shared section renderer keeps every per-section affordance (create
  // actions, context menus, drop targets) identical across the stacked panel
  // and the rail-only single-section overlay (left-sidebar D18/D38).
  const renderSection = (
    sectionId: SidebarSectionId,
    isCollapsed: boolean,
    onToggleCollapse: () => void,
  ) => {
    const config = sectionConfig[sectionId];
    if (!config) return null;

    return (
      <SidebarSection
        key={sectionId}
        id={sectionId}
        fillHeight={isRailMode && sectionId === "source-control"}
        title={config.title}
        icon={config.icon}
        badge={config.badge}
        headerAction={
          sectionId === "issues" && issuesSection?.viewerOwnsWorkspace
            ? {
                label: "Import task",
                icon: <Plus size={14} />,
                showLabel: true,
                onActivate: issuesSection.onImportTask,
                disabled: !!(
                  issuesSection.writeBlockedReason ??
                  issuesSection.linkBlockedReason
                ),
                title:
                  issuesSection.writeBlockedReason ??
                  issuesSection.linkBlockedReason ??
                  undefined,
              }
            : sectionId === "files" && terminalAction
              ? {
                  label: "Terminal",
                  icon: <SquareTerminal size={14} />,
                  onActivate: terminalAction.onActivate,
                  disabled: terminalAction.disabled,
                  title: terminalAction.title,
                  blockedReason: terminalAction.blockedReason ?? null,
                  policyFree: true,
                }
              : sectionId === "project-docs" && onCreateProjectDoc
                ? {
                    label: "Create project doc",
                    icon: <Plus size={14} />,
                    onActivate: onCreateProjectDoc,
                    disabled: docWriteBlocked !== null,
                    blockedReason: docWriteBlocked,
                    title: docWriteBlocked ?? undefined,
                  }
                : undefined
        }
        showContextMenuButton={sectionId !== "project-docs"}
        headerFileDropTarget={
          sectionId === "project-docs" &&
          dropProjectDocFiles &&
          projectDocsRootFolderPath
            ? {
                folderPath: projectDocsRootFolderPath,
                onDropFiles: (files) =>
                  dropProjectDocFiles({
                    files,
                    folderPath: projectDocsRootFolderPath,
                  }),
              }
            : undefined
        }
        unifiedFileDropHighlight={
          sectionId === "project-docs" &&
          dropProjectDocFiles &&
          projectDocsRootFolderPath &&
          projectDocsRootIsEmpty
            ? {
                bodyActive:
                  activeProjectDocsBodyDropPath === projectDocsRootFolderPath,
              }
            : undefined
        }
        onOpenContextMenu={
          canOpenSectionContextMenu(sectionId)
            ? (position) => openSectionContextMenu(sectionId, position)
            : undefined
        }
        headerOnly={isWorkspaceDestination(sectionId)}
        isCollapsed={isWorkspaceDestination(sectionId) || isCollapsed}
        onToggleCollapse={
          isWorkspaceDestination(sectionId)
            ? () => onOpenApprovals?.()
            : onToggleCollapse
        }
      >
        {config.render()}
      </SidebarSection>
    );
  };

  const sidebarBottomSlot = bottomSlot ? (
    <div
      className="shrink-0 border-t border-[var(--g5-stroke)] bg-[var(--g5-bg1)]"
      data-testid="sidebar-bottom-slot"
    >
      {bottomSlot}
    </div>
  ) : null;

  // The stacked section panel — the `expanded` display mode's in-flow body
  // (left-sidebar D18); rail-mode overlays use the single-section layout.
  // The full sidebar's collapse control (ui g5.ui.workbench-shell), at the
  // top of the stacked panel; the icon rails have none (owner instruction,
  // 2026-10-29).
  const collapseControl =
    onCollapse && !isRailMode ? (
      <button
        type="button"
        data-testid="sidebar-collapse"
        aria-label="Collapse sidebar"
        title="Collapse sidebar"
        onClick={onCollapse}
        className="flex h-7 w-7 shrink-0 items-center justify-center rounded text-[var(--g5-text2)] hover:bg-[var(--g5-bg2)] hover:text-[var(--g5-text0)] focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-[var(--g5-info)]"
      >
        <ChevronsLeft size={14} />
      </button>
    ) : null;
  const stackedPanel = (
    <>
      {collapseControl && (
        <div className="flex shrink-0 justify-end border-b border-[var(--g5-stroke)] px-1 py-0.5">
          {collapseControl}
        </div>
      )}
      <div className="flex flex-1 flex-col overflow-y-auto scrollbar-thin">
        {visibleSectionIds.map((sectionId) =>
          renderSection(sectionId, collapsedSections.has(sectionId), () =>
            toggleCollapse(sectionId),
          ),
        )}
        {renderSectionContextMenu()}
      </div>
      {sidebarBottomSlot}
    </>
  );

  // A rail icon's body: the section icon with its badges, shared by the
  // activity rails and the collapsed rail.
  const railIconBody = (
    config: NonNullable<(typeof sectionConfig)[SidebarSectionId]>,
  ) => (
    <>
      {config.icon}
      {config.badgeNode}
      {config.badge !== undefined && config.badge !== null && (
        <span className="absolute -right-0.5 -top-0.5 min-w-4 rounded-full bg-[var(--g5-bg2)] px-1 text-center text-[9px] leading-4 text-[var(--g5-text2)]">
          {typeof config.badge === "number"
            ? formatLargeNumber(config.badge)
            : config.badge}
        </span>
      )}
    </>
  );
  const railIconClass =
    "relative flex h-8 w-8 shrink-0 items-center justify-center rounded text-[var(--g5-text2)] hover:bg-[var(--g5-bg2)] hover:text-[var(--g5-text0)] focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-[var(--g5-info)]";

  if (isCollapsedRail) {
    return (
      <aside
        ref={asideRef}
        data-testid="left-sidebar"
        data-surface="sidebar"
        data-collapsed="true"
        className="relative h-full shrink-0 border-r border-[var(--g5-stroke)] bg-[var(--g5-bg1)]"
        style={{ width: SIDEBAR_RAIL_WIDTH }}
      >
        <div
          data-testid="sidebar-rail"
          className="flex h-full w-full flex-col items-center gap-1 overflow-y-auto py-1"
        >
          <button
            type="button"
            data-testid="sidebar-expand"
            className="inline-flex h-6 w-6 shrink-0 items-center justify-center rounded text-[var(--g5-text2)] hover:bg-[var(--g5-bg2)] hover:text-[var(--g5-text0)] focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-[var(--g5-info)]"
            onClick={onExpand}
            title="Expand sidebar"
            aria-label="Expand sidebar"
          >
            <ChevronsRight size={14} />
          </button>
          {visibleSectionIds.map((sectionId) => {
            const config = sectionConfig[sectionId];
            if (!config) return null;
            return (
              <button
                key={sectionId}
                type="button"
                data-testid={`sidebar-rail-${sectionTestIdSegment(sectionId)}`}
                aria-label={config.title}
                title={config.title}
                onClick={() => handleCollapsedRailActivate(sectionId)}
                className={railIconClass}
              >
                {railIconBody(config)}
              </button>
            );
          })}
        </div>
      </aside>
    );
  }

  if (!isRailMode) {
    return (
      <aside
        ref={asideRef}
        data-testid="left-sidebar"
        data-surface="sidebar"
        className="flex h-full flex-col overflow-hidden border-r border-[var(--g5-stroke)] bg-[var(--g5-bg1)]"
        style={{ width }}
      >
        {stackedPanel}
      </aside>
    );
  }

  // Icon activity rail (left-sidebar D38): fixed-width footprint; the
  // stacked panel opens as an overlay positioned beside the rail at the
  // persisted expanded width. No overflow clipping on the aside so the
  // overlay can extend past the rail.
  const openHoverOverlay = (event: React.SyntheticEvent<HTMLElement>) => {
    if (
      event.target instanceof Element &&
      event.target.closest('[data-testid="sidebar-rail-approvals"]')
    ) {
      setRailOverlayVisible(false);
      return;
    }
    // First hover before any activation defaults to the first visible
    // section; afterwards the sticky last-active section reopens (D38).
    setRailActiveSection(
      (prev) =>
        prev ??
        visibleSectionIds.find((id) => !isWorkspaceDestination(id)) ??
        null,
    );
    setRailOverlayVisible(true);
  };
  const hoverHandlers =
    displayMode === "expand-on-hover"
      ? {
          onMouseEnter: openHoverOverlay,
          // Leaving into a section's portaled popover keeps the overlay; the
          // popover's own outside click then closes both.
          onMouseLeave: (event: React.MouseEvent<HTMLElement>) => {
            if (withinFloatingSurface(event.relatedTarget)) return;
            setRailOverlayVisible(false);
          },
          onFocusCapture: openHoverOverlay,
          onBlurCapture: (event: React.FocusEvent<HTMLElement>) => {
            if (
              (event.relatedTarget instanceof Node &&
                event.currentTarget.contains(event.relatedTarget)) ||
              withinFloatingSurface(event.relatedTarget) ||
              // Pressing non-focusable content (a file row, a repository
              // root, text in a popover) blurs the focused control with no
              // related target; the pointerdown rule above decides.
              (event.relatedTarget === null &&
                (pointerPressRef.current || floatingSurfaceOpen()))
            ) {
              return;
            }
            setRailOverlayVisible(false);
          },
        }
      : {};

  return (
    <aside
      ref={asideRef}
      data-testid="left-sidebar"
      data-surface="sidebar"
      className="relative h-full shrink-0 border-r border-[var(--g5-stroke)] bg-[var(--g5-bg1)]"
      style={{ width: SIDEBAR_RAIL_WIDTH }}
      {...hoverHandlers}
    >
      <div
        data-testid="sidebar-rail"
        className="flex h-full w-full flex-col items-center gap-1 overflow-y-auto py-2"
      >
        {visibleSectionIds.map((sectionId) => {
          const config = sectionConfig[sectionId];
          if (!config) return null;
          return (
            <button
              key={sectionId}
              type="button"
              data-testid={`sidebar-rail-${sectionTestIdSegment(sectionId)}`}
              aria-label={config.title}
              title={config.title}
              aria-expanded={railOverlayOpen && railActiveSection === sectionId}
              onClick={() => handleRailSectionActivate(sectionId)}
              // Hover mode: hovering (or focusing) an icon opens/switches the
              // overlay to that icon's section (chat ad99320b).
              onMouseEnter={
                displayMode === "expand-on-hover"
                  ? () => {
                      if (isWorkspaceDestination(sectionId)) {
                        setRailOverlayVisible(false);
                        return;
                      }
                      setRailActiveSection(sectionId);
                      setRailOverlayVisible(true);
                    }
                  : undefined
              }
              onFocus={
                displayMode === "expand-on-hover"
                  ? () => {
                      if (isWorkspaceDestination(sectionId)) {
                        setRailOverlayVisible(false);
                        return;
                      }
                      setRailActiveSection(sectionId);
                      setRailOverlayVisible(true);
                    }
                  : undefined
              }
              className={railIconClass}
            >
              {railIconBody(config)}
            </button>
          );
        })}
      </div>
      {railOverlayOpen && (
        <div
          data-testid="sidebar-rail-overlay"
          className="absolute left-full top-0 z-30 flex h-full flex-col overflow-hidden border-r border-[var(--g5-stroke)] bg-[var(--g5-bg1)] shadow-xl"
          style={{ width }}
        >
          {/* Single-section overlay, shared by both rail modes (D38, chats
              4e4a6f09 + 008903e9): exactly the active section, expanded,
              with the Task Queue pinned at the bottom (own bounded scroll,
              individually collapsible). */}
          {railActiveSection && (
            <>
              <div className="flex min-h-0 flex-1 flex-col overflow-y-auto scrollbar-thin">
                {/* A header click closes the overlay, Guided's included
                    (ui g5.ui.guided-collapsed). */}
                {renderSection(railActiveSection, false, () =>
                  setRailOverlayVisible(false),
                )}
              </div>
              {railActiveSection !== "tasks" &&
                visibleSectionIds.includes("tasks") && (
                  <div
                    data-testid="sidebar-rail-pinned-tasks"
                    className="flex max-h-[40%] min-h-0 shrink-0 flex-col overflow-y-auto scrollbar-thin border-t border-[var(--g5-stroke)]"
                  >
                    {renderSection("tasks", railTasksCollapsed, () =>
                      setRailTasksCollapsed((prev) => !prev),
                    )}
                  </div>
                )}
              {renderSectionContextMenu()}
              {sidebarBottomSlot}
            </>
          )}
        </div>
      )}
    </aside>
  );
});

/** Fixed footprint of the icon activity rail in rail display modes (D38). */
export const SIDEBAR_RAIL_WIDTH = 44;

export function TasksPanel({
  status,
  error,
  onRetry,
  tasks,
  selectedTaskId,
  onSelectTask,
  hasMoreTasks,
  isLoadingMoreTasks,
  onLoadMoreTasks,
}: {
  status?: "loading" | "ok" | "error";
  error?: Error | null;
  onRetry?: () => void;
  tasks: TaskListEntry[];
  selectedTaskId: string | null;
  onSelectTask: (id: string) => void;
  hasMoreTasks?: boolean;
  isLoadingMoreTasks?: boolean;
  onLoadMoreTasks?: () => void;
}) {
  return (
    <div>
      {status === "loading" && <p role="status">Loading tasks…</p>}
      {error && (
        <p role="alert">
          Tasks could not be loaded.{" "}
          <button
            type="button"
            data-testid="sidebar-tasks-retry"
            onClick={onRetry}
          >
            Retry
          </button>
        </p>
      )}
      {status !== "loading" && !error && tasks.length === 0 && (
        <p className="px-3 py-2 text-xs text-[var(--g5-text2)]">
          No tasks yet.
        </p>
      )}
      {tasks.map((task) => (
        <TaskRow
          key={task.id}
          task={task}
          isSelected={selectedTaskId === task.id}
          onSelectTask={onSelectTask}
        />
      ))}
      {hasMoreTasks && (
        <button
          data-testid="sidebar-tasks-load-more"
          type="button"
          className="px-3 py-2 text-xs text-[var(--g5-info)]"
          disabled={isLoadingMoreTasks}
          onClick={onLoadMoreTasks}
        >
          {isLoadingMoreTasks ? "Loading…" : "Load more tasks"}
        </button>
      )}
    </div>
  );
}
