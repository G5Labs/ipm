// =============================================================================
// G5 Workbench — ribbon action icons
// =============================================================================
// The glyph beside or above a ribbon action (workbench
// g5.workbench.header-styles). Icons are client presentation: the built-in
// action ids map to a lucide glyph, and anything else — a committed custom
// action, a renamed id — falls back by `target`, then by `kind`, so every
// action renders a glyph without an icon field in `actions.yaml`.
// =============================================================================

import {
  BadgeCheck,
  BarChart3,
  CircleHelp,
  ClipboardList,
  Code2,
  Download,
  ExternalLink,
  Eye,
  FilePlus2,
  GitCompareArrows,
  GitMerge,
  GitPullRequest,
  Hammer,
  Lightbulb,
  List,
  ListChecks,
  MessageSquare,
  PanelTop,
  RefreshCw,
  SearchCheck,
  ShieldCheck,
  Stamp,
  Upload,
  Users,
} from "lucide-react";
import type { WorkbenchAction } from "../../lib/types";
import type { PhaseIcon } from "./phaseIcons";

// Maps, not plain objects: a committed action id such as `constructor` must
// miss the lookup instead of reading an inherited property.
const BY_ID: ReadonlyMap<string, PhaseIcon> = new Map<string, PhaseIcon>([
  ["new-doc", FilePlus2],
  ["import", Download],
  ["upload-file", Upload],
  ["import-user-stories", ListChecks],
  ["review-meeting", Users],
  ["upload-transcript", Upload],
  ["extract-decisions", ListChecks],
  ["build-mock", Hammer],
  ["generate-mock", Hammer],
  ["preview", Eye],
  ["approvals", Stamp],
  ["approval", Stamp],
  ["create-implementation-plan", ClipboardList],
  ["generate-code", Code2],
  ["create-pr", GitPullRequest],
  ["get-approvals", BadgeCheck],
  ["rebase", RefreshCw],
  ["merge", GitMerge],
  ["open-questions", CircleHelp],
  ["view-diff-graph", GitCompareArrows],
  ["explainer", Lightbulb],
  ["code-review", SearchCheck],
  ["rerun-verification", ShieldCheck],
]);

const BY_TARGET: ReadonlyMap<string, PhaseIcon> = new Map<string, PhaseIcon>([
  ["new-doc", FilePlus2],
  ["upload", Upload],
  ["approvals", Stamp],
  ["ship", GitPullRequest],
  ["rebase", RefreshCw],
  ["merge", GitMerge],
  ["preview", Eye],
  ["meeting", Users],
  ["view", PanelTop],
  ["explainer", Lightbulb],
  ["review", SearchCheck],
  ["verification", ShieldCheck],
  ["mock", Hammer],
]);

/** The glyph for an action: by built-in id, else by target, else by kind. */
export function workbenchActionIcon(action: WorkbenchAction): PhaseIcon {
  const byId = BY_ID.get(action.id);
  if (byId) return byId;
  const byTarget =
    action.kind === "open" && action.target
      ? BY_TARGET.get(action.target)
      : undefined;
  if (byTarget) return byTarget;
  if (action.kind === "chat") return MessageSquare;
  if (action.kind === "menu") return List;
  return ExternalLink;
}
