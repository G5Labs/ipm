/**
 * OrgApprovalsPageContent — Org approval-definition library (mock E++).
 *
 * Center-panel tab (`?view=approval-definitions`) for the org-wide library
 * of approval sign-offs backed by `approval_definitions` (migration 186, B6
 * governance). Workspace attachment (`approval_configs`) has shipped; the
 * merge-blocking finalizer gate is still pending, and the library copy
 * carries no enforcement-status caveat (user decision 2026-08-13).
 *
 * Naming: "org approvals" everywhere; bare "approvals" identifiers mean
 * in-chat agent tool-approvals (lib/agentApprovals.ts).
 *
 * Views (full-page subviews, no modals — user decision):
 *   - library: search + composition filter chips + card grid; per card a
 *     composition badge and the configured human quorum checklist with
 *     an expandable "Agent check" prompt; v1 "Created by" foot; no version
 *     chrome.
 *   - create: sectioned editor publishing v1 (no invalidation control).
 *   - edit: prefilled editor publishing the next append-only version, with
 *     the "Existing decisions" keep/invalidate choice and 409 handling.
 *
 * Write affordances: per-target-org role Admin, or active-org
 * `org:admin` scope (`hasScope` reflects only the active org). Members see
 * the library read-only.
 *
 * @module org/OrgApprovalsPageContent
 */

import { useMemo, useState } from "react";
import { useParams } from "react-router-dom";
import {
  AlertCircle,
  Bot,
  ChevronDown,
  ChevronRight,
  Plus,
  Search,
  ShieldCheck,
} from "lucide-react";

import { Button } from "../ui/UI";
import {
  AgentLaunchConfigControls,
  completeAgentConfig,
} from "../agent/AgentLaunchConfigControls";
import { useAuth } from "../../hooks/useAuth";
import { useOrgMembers } from "../../hooks/useOrg";
import { ExternalChecksMergeSwitch } from "./ExternalChecksMergeSwitch";
import {
  useCreateOrgApprovalDefinition,
  useOrgApprovalDefinitions,
  usePublishOrgApprovalDefinitionVersion,
} from "../../hooks/useOrgApprovals";
import { ApiError } from "../../lib/api";
import { APPROVAL_PHASES, definitionPhases } from "../../lib/approvalPhases";
import { WORKBENCH_PHASES } from "../../lib/workbenchDefaults";
import { CLAUDE_DIRECT_CHAT_DEFAULT_MODEL } from "../../lib/claudeModels";
import { catalogDefaultChoiceFor } from "../../lib/harnessPicker";
import type {
  OrgApprovalDefinitionDTO,
  OrgApprovalHumanDTO,
  OrgMemberDTO,
  PickerHarness,
  SyncAgentConfig,
  WorkspacePhase,
} from "../../lib/types";

/* ------------------------------------------------------------------ */
/*  Small presentation helpers                                         */
/* ------------------------------------------------------------------ */

const AVATAR_PALETTE = [
  "rgb(103 164 255)",
  "rgb(113 201 136)",
  "rgb(242 162 73)",
  "rgb(192 132 252)",
  "rgb(103 232 249)",
];

function initialsOf(displayName: string): string {
  const parts = displayName.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return "?";
  const first = parts[0][0] ?? "";
  const last = parts.length > 1 ? (parts[parts.length - 1][0] ?? "") : "";
  return (first + last).toUpperCase() || "?";
}

function avatarColor(userId: string): string {
  let hash = 0;
  for (const ch of userId) hash = (hash * 31 + ch.charCodeAt(0)) >>> 0;
  return AVATAR_PALETTE[hash % AVATAR_PALETTE.length];
}

/**
 * Deterministic initials avatar. Exported for reuse by the status band's
 * Ship Approvals roster (workspace-page-shell D50); `title` defaults to the
 * display name and can be suppressed (empty string) when a wrapper carries a
 * richer tooltip.
 */
export function InitialsAvatar({
  userId,
  displayName,
  size = "w-[22px] h-[22px] text-[9.5px]",
  title = displayName,
}: {
  userId: string;
  displayName: string;
  size?: string;
  title?: string;
}) {
  return (
    <span
      className={`inline-flex items-center justify-center rounded-full font-bold text-[rgb(8,12,24)] ${size}`}
      style={{ backgroundColor: avatarColor(userId) }}
      title={title || undefined}
      aria-hidden="true"
    >
      {initialsOf(displayName)}
    </span>
  );
}

function formatDate(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

/* ------------------------------------------------------------------ */
/*  Editor form state                                                  */
/* ------------------------------------------------------------------ */

interface EditorState {
  name: string;
  description: string;
  humanUserIds: string[];
  agentEnabled: boolean;
  agentPrompt: string;
  agentConfig: SyncAgentConfig;
  invalidatesPrevious: boolean;
  requireAllReviewers: boolean;
  allowSelfApproval: boolean;
  humanApprovalsOverrideAgent: boolean;
  // Earliest sensible request timing; target merge policy is workspace-wide.
  recommendedPhase: WorkspacePhase;
}

function approvalLaunchDefaultFor(agentProvider: PickerHarness) {
  const choice = catalogDefaultChoiceFor(agentProvider);
  return agentProvider === "claude_code"
    ? { ...choice, model: CLAUDE_DIRECT_CHAT_DEFAULT_MODEL }
    : choice;
}

function editorStateFrom(def: OrgApprovalDefinitionDTO | null): EditorState {
  const agentProvider: PickerHarness =
    def?.agentHarness === "codex" ? "codex" : "claude_code";
  const launchDefault = approvalLaunchDefaultFor(agentProvider);
  return {
    name: def?.name ?? "",
    description: def?.description ?? "",
    // Departed members are dropped from the SELECTION only by the user's
    // explicit act of publishing; they remain visible in the departed note.
    humanUserIds:
      def?.humans
        .filter((h) => h.stillMember && !h.deactivatedAt)
        .map((h) => h.userId) ?? [],
    agentEnabled: def?.agentType === "simple",
    agentPrompt: def?.agentPrompt ?? "",
    agentConfig: completeAgentConfig({
      agentProvider,
      launchSource: "explicit",
      model: def?.agentModel ?? launchDefault.model,
      effort: def?.agentEffort ?? launchDefault.effort,
    }),
    invalidatesPrevious: false,
    requireAllReviewers: def ? (def.requireAllReviewers ?? true) : false,
    allowSelfApproval: def?.allowSelfApproval ?? false,
    humanApprovalsOverrideAgent: def?.humanApprovalsOverrideAgent ?? true,
    recommendedPhase: def ? (definitionPhases(def)[0] ?? "ie") : "ie",
  };
}

/** Number of explicit non-default settings in Advanced. */
function advancedChangedCount(state: EditorState): number {
  const defaultLaunch = approvalLaunchDefaultFor("claude_code");
  const policyCount =
    Number(state.requireAllReviewers) +
    Number(!state.humanApprovalsOverrideAgent) +
    Number(state.allowSelfApproval);
  return (
    policyCount +
    (state.agentEnabled
      ? Number(state.agentConfig.agentProvider !== "claude_code") +
        Number(state.agentConfig.model !== defaultLaunch.model) +
        Number(state.agentConfig.effort !== defaultLaunch.effort)
      : 0)
  );
}

/** Mirror of the API/DB composition rules — gates the submit button. */
function approvalEditorSubmittable(state: EditorState): boolean {
  if (state.name.trim() === "") return false;
  if (state.agentEnabled && state.agentPrompt.trim() === "") return false;
  if (state.humanUserIds.length === 0 && !state.agentEnabled) return false;
  return true;
}

/* ------------------------------------------------------------------ */
/*  Definition card (library view)                                     */
/* ------------------------------------------------------------------ */

/** Composition summary shown as the card badge ("2 people + agent"). */
function compositionBadge(definition: OrgApprovalDefinitionDTO): {
  label: string;
  agent: boolean;
} {
  const humanCount = definition.humans.length;
  const hasAgent = definition.agentType === "simple";
  if (humanCount === 0 && hasAgent) return { label: "Agent only", agent: true };
  const people = `${humanCount} ${humanCount === 1 ? "person" : "people"}`;
  return hasAgent
    ? { label: `${people} + agent`, agent: true }
    : { label: people, agent: false };
}

/**
 * Show saved phase recommendations, including legacy multiple/empty lists.
 * Recommendations never restrict workspace-wide merge requirements.
 */
export function PhaseChips({
  definition,
  testIdPrefix = "org-approvals-card",
}: {
  definition: Pick<OrgApprovalDefinitionDTO, "phases">;
  testIdPrefix?: string;
}) {
  const phases = definitionPhases(definition);
  return (
    <div className="flex flex-col gap-1">
      <div
        data-testid={`${testIdPrefix}-phase-chips`}
        className="flex flex-wrap items-center gap-1.5"
      >
        {phases.map((phase) => (
          <span
            key={phase}
            data-testid={`${testIdPrefix}-phase-${phase}`}
            className="inline-flex items-center gap-1 rounded-full border border-dashed px-2 py-0.5 text-[10px] font-bold"
            style={{
              color: `var(${WORKBENCH_PHASES[phase].token})`,
              borderColor: `var(${WORKBENCH_PHASES[phase].token})`,
              backgroundColor: `var(${WORKBENCH_PHASES[phase].softToken})`,
            }}
            title={WORKBENCH_PHASES[phase].name}
          >
            Recommended for {WORKBENCH_PHASES[phase].name}
          </span>
        ))}
        {phases.length === 0 && (
          <span
            data-testid={`${testIdPrefix}-phase-none`}
            className="inline-flex items-center gap-1 rounded-full border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-2 py-0.5 text-[10px] font-bold text-[var(--g5-text2)]"
          >
            No phase recommendation
          </span>
        )}
      </div>
    </div>
  );
}

function DefinitionCard({
  definition,
  canWrite,
  showAllPrompts,
  onEdit,
}: {
  definition: OrgApprovalDefinitionDTO;
  canWrite: boolean;
  showAllPrompts: boolean;
  onEdit: () => void;
}) {
  // The per-card toggle overrides the toolbar-level "Show all prompts" state
  // until the next all-prompts flip resets the override (adjust-during-render,
  // not an effect, so the reset lands in the same commit as the flip).
  const [promptOverride, setPromptOverride] = useState<boolean | null>(null);
  const [prevShowAll, setPrevShowAll] = useState(showAllPrompts);
  if (prevShowAll !== showAllPrompts) {
    setPrevShowAll(showAllPrompts);
    setPromptOverride(null);
  }
  const showPrompt = promptOverride ?? showAllPrompts;
  const badge = compositionBadge(definition);
  const hasAgent = definition.agentType === "simple";
  // Whitespace-only descriptions must not render an empty strip (D3).
  const description = definition.description?.trim() ?? "";

  return (
    <div
      data-testid="org-approvals-card"
      className="flex flex-col gap-3 rounded-xl border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] p-4 transition-colors hover:border-[rgb(103,164,255,0.4)]"
    >
      <div className="flex items-center justify-between gap-2.5">
        <h2 className="text-sm font-semibold text-[var(--g5-text0)]">
          {definition.name}
        </h2>
        <span
          className={`whitespace-nowrap rounded-full border px-2 py-0.5 text-[10.5px] font-semibold ${
            badge.agent
              ? "border-[rgb(192,132,252,0.3)] bg-[rgb(192,132,252,0.12)] text-[rgb(206,160,252)]"
              : "border-[rgb(113,201,136,0.3)] bg-[rgb(113,201,136,0.12)] text-[rgb(150,220,170)]"
          }`}
        >
          {badge.label}
        </span>
      </div>
      {description && (
        <div className="text-xs leading-relaxed text-[var(--g5-text2)]">
          {description}
        </div>
      )}
      <PhaseChips definition={definition} />
      <p className="text-xs text-[var(--g5-text2)]">
        Any human denial blocks approval.{" "}
        {definition.agentType &&
          definition.humans.length > 0 &&
          ((definition.requireAllReviewers ?? true)
            ? "The agent must also approve. "
            : (definition.humanApprovalsOverrideAgent ?? true)
              ? "A human approval may override an agent rejection. "
              : "An agent rejection blocks approval. ")}
        Self approval {definition.allowSelfApproval ? "allowed" : "disallowed"}.
      </p>
      <div className="flex flex-col overflow-hidden rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg0)]">
        <div className="border-b border-[var(--g5-stroke)] bg-[rgb(255,255,255,0.04)] px-3 py-1.5 text-[10px] font-semibold uppercase tracking-wider text-[var(--g5-text2)]">
          {definition.humans.length === 0
            ? "Agent must approve"
            : (definition.requireAllReviewers ?? true)
              ? "All reviewers must approve"
              : "At least one reviewer must approve"}
        </div>
        <div className="flex flex-col divide-y divide-[rgb(255,255,255,0.04)]">
          {definition.humans.map((h) => (
            <div
              key={h.userId}
              className="flex items-center gap-2.5 px-3 py-2 text-xs text-[var(--g5-text1)]"
            >
              <span
                aria-hidden="true"
                className="h-[15px] w-[15px] shrink-0 rounded-full border-[1.5px] border-[var(--g5-text2)]"
              />
              <InitialsAvatar userId={h.userId} displayName={h.displayName} />
              <span className="font-medium text-[var(--g5-text0)]">
                {h.displayName}
              </span>
              {!h.stillMember && (
                <span className="rounded-full border border-[rgb(242,162,73,0.3)] bg-[rgb(242,162,73,0.12)] px-2 text-[10px] text-[var(--g5-warn)]">
                  left org
                </span>
              )}
              {h.stillMember && h.deactivatedAt && (
                <span className="text-[var(--g5-warn)]">inactive</span>
              )}
            </div>
          ))}
          {hasAgent && (
            <div className="flex items-center gap-2.5 px-3 py-2 text-xs text-[var(--g5-text1)]">
              <span
                aria-hidden="true"
                className="h-[15px] w-[15px] shrink-0 rounded-full border-[1.5px] border-[var(--g5-text2)]"
              />
              <span className="inline-flex h-[22px] w-[22px] shrink-0 items-center justify-center rounded-full border border-[rgb(192,132,252,0.3)] bg-[rgb(192,132,252,0.15)]">
                <Bot className="h-3.5 w-3.5 text-[rgb(206,160,252)]" />
              </span>
              <span className="font-medium text-[var(--g5-text0)]">
                Agent check
              </span>
              {definition.agentPrompt && (
                <button
                  type="button"
                  data-testid="org-approvals-toggle-prompt"
                  className="ml-auto cursor-pointer whitespace-nowrap text-[11px] text-[var(--g5-info)] hover:underline"
                  onClick={() => setPromptOverride(!showPrompt)}
                >
                  {showPrompt ? "Hide prompt" : "Show prompt"}
                </button>
              )}
            </div>
          )}
        </div>
        {showPrompt && definition.agentPrompt && (
          <div className="border-t border-[rgb(192,132,252,0.2)] bg-[rgb(192,132,252,0.06)] py-2 pl-[43px] pr-3 font-mono text-[11px] leading-relaxed text-[var(--g5-text1)]">
            {definition.agentPrompt}
          </div>
        )}
      </div>
      <div className="flex items-center justify-between text-[11px] text-[var(--g5-text2)]">
        <span>
          Created by {definition.firstCreatedBy.displayName} ·{" "}
          {formatDate(definition.firstCreatedAt)}
        </span>
        {canWrite && (
          <button
            type="button"
            data-testid="org-approvals-edit"
            className="cursor-pointer rounded-md border border-[var(--g5-stroke)] px-3 py-1 text-xs text-[var(--g5-text1)] hover:bg-[rgb(255,255,255,0.05)] hover:text-[var(--g5-text0)]"
            onClick={onEdit}
          >
            Edit
          </button>
        )}
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Editor (create + edit full-page subviews)                          */
/* ------------------------------------------------------------------ */

function ApprovalDefinitionEditor({
  mode,
  orgName,
  definition,
  members,
  submitting,
  submitError,
  onCancel,
  onSubmit,
}: {
  mode: "create" | "edit";
  orgName: string;
  definition: OrgApprovalDefinitionDTO | null;
  members: OrgMemberDTO[];
  submitting: boolean;
  submitError: Error | null;
  onCancel: () => void;
  onSubmit: (state: EditorState) => void;
}) {
  const [state, setState] = useState<EditorState>(() =>
    editorStateFrom(definition),
  );
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const patch = (p: Partial<EditorState>) =>
    setState((prev) => ({ ...prev, ...p }));
  const changedCount = advancedChangedCount(state);
  const launchOptions = useMemo(() => {
    const providers: PickerHarness[] = ["claude_code", "codex"];
    const selectedProvider = state.agentConfig.agentProvider as PickerHarness;
    // An existing Codex-backed definition stays faithfully editable even if
    // the environment UI gate is later disabled; it is not silently changed.
    if (!providers.includes(selectedProvider)) providers.push(selectedProvider);
    return providers.map((provider) => {
      const choice = approvalLaunchDefaultFor(provider);
      return completeAgentConfig({
        agentProvider: provider,
        launchSource: "explicit",
        model: choice.model,
        effort: choice.effort,
      });
    });
  }, [state.agentConfig.agentProvider]);

  const departedHumans: OrgApprovalHumanDTO[] =
    definition?.humans.filter((h) => !h.stillMember) ?? [];
  const inactiveHumans =
    definition?.humans.filter((h) => h.stillMember && h.deactivatedAt) ?? [];
  const isConflict =
    submitError instanceof ApiError && submitError.status === 409;
  // A 409 permanently disables Save for this editor instance: the form state
  // is stale against the winner's version, so publishing it would overwrite
  // unreviewed content. The only way forward is reloading the latest.
  const submittable =
    approvalEditorSubmittable(state) &&
    !submitting &&
    !isConflict &&
    state.humanUserIds.every((id) =>
      members.some((m) => m.userId === id && !m.deactivatedAt),
    );

  const toggleHuman = (userId: string) =>
    patch({
      humanUserIds: state.humanUserIds.includes(userId)
        ? state.humanUserIds.filter((id) => id !== userId)
        : [...state.humanUserIds, userId],
    });

  return (
    <div className="mx-auto w-full max-w-2xl">
      {/* Breadcrumb back to the library */}
      <nav className="mb-3 text-xs text-[var(--g5-text2)]">
        <button
          type="button"
          data-testid="org-approvals-breadcrumb-back"
          className="cursor-pointer text-[var(--g5-info)] hover:underline"
          onClick={onCancel}
        >
          {orgName} / Approvals
        </button>{" "}
        /{" "}
        <span className="text-[var(--g5-text0)]">
          {mode === "create" ? "New definition" : (definition?.name ?? "Edit")}
        </span>
      </nav>
      <h1 className="text-xl font-semibold text-[var(--g5-text0)]">
        {mode === "create"
          ? "New approval definition"
          : `Edit — ${definition?.name ?? ""}`}
      </h1>
      <p className="mb-5 mt-1 text-xs leading-relaxed text-[var(--g5-text2)]">
        {mode === "create"
          ? "A definition declares who must sign off. Where it applies is decided per target workspace, when workspace attachment ships."
          : `Created by ${definition?.firstCreatedBy.displayName ?? ""} · ${formatDate(
              definition?.firstCreatedAt ?? "",
            )} · History is preserved; once workspace attachment ships, changes will take effect immediately for every workspace requiring this definition.`}
      </p>

      {/* Details */}
      <section className="mb-4 rounded-lg border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] p-5">
        <h2 className="text-[13px] font-semibold text-[var(--g5-text0)]">
          Details
        </h2>
        <p className="mb-3 mt-1 text-xs text-[var(--g5-text2)]">
          Names don’t have to be unique — the definition id disambiguates.
        </p>
        <label className="mb-1 block text-xs font-semibold text-[var(--g5-text1)]">
          Name
          <input
            type="text"
            data-testid="org-approvals-name-input"
            className="mt-1 w-full rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-2.5 py-2 text-[13px] font-normal text-[var(--g5-text0)] focus:border-[var(--g5-info)] focus-visible:outline-none"
            placeholder="e.g. Production merge sign-off"
            value={state.name}
            onChange={(e) => patch({ name: e.target.value })}
          />
        </label>
        <label className="mt-3 block text-xs font-semibold text-[var(--g5-text1)]">
          Description{" "}
          <span className="font-normal text-[var(--g5-text2)]">(optional)</span>
          <textarea
            data-testid="org-approvals-description-input"
            className="mt-1 min-h-[56px] w-full resize-y rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-2.5 py-2 text-[13px] font-normal text-[var(--g5-text0)] focus:border-[var(--g5-info)] focus-visible:outline-none"
            placeholder="What does this sign-off attest?"
            value={state.description}
            onChange={(e) => patch({ description: e.target.value })}
          />
        </label>
        <label className="mt-3 block text-xs font-semibold text-[var(--g5-text1)]">
          Earliest relevant phase
          <select
            data-testid="org-approvals-earliest-phase"
            className="mt-1 block w-full rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-2.5 py-2 text-[13px] font-normal text-[var(--g5-text0)]"
            disabled={submitting}
            value={state.recommendedPhase}
            onChange={(e) =>
              patch({ recommendedPhase: e.target.value as WorkspacePhase })
            }
          >
            {APPROVAL_PHASES.map((phase) => (
              <option key={phase} value={phase}>
                {WORKBENCH_PHASES[phase].name}
              </option>
            ))}
          </select>
        </label>
        <p className="mt-1 text-xs text-[var(--g5-text2)]">
          Recommended timing for requesting this review. Approvals only gate
          merging into the target, never moving between phases.
        </p>
      </section>

      {/* Human approvers */}
      <section className="mb-4 rounded-lg border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] p-5">
        <h2 className="text-[13px] font-semibold text-[var(--g5-text0)]">
          Human approvers
        </h2>
        <p className="mb-3 mt-1 text-xs text-[var(--g5-text2)]">
          Choose who may review. Any human denial blocks approval. At least one
          human approver or an agent approver is required.
        </p>
        <div className="flex flex-wrap gap-1.5">
          {members.map((m) => {
            const on = state.humanUserIds.includes(m.userId);
            return (
              <button
                key={m.userId}
                type="button"
                data-testid={`org-approvals-member-pill-${m.userId}`}
                disabled={!!m.deactivatedAt && !on}
                aria-pressed={on}
                className={`inline-flex cursor-pointer items-center gap-1.5 rounded-full border px-2.5 py-1.5 text-xs select-none disabled:cursor-not-allowed disabled:opacity-50 ${
                  on
                    ? "border-[var(--g5-info)] bg-[rgb(103,164,255,0.12)] text-[var(--g5-text0)]"
                    : "border-[var(--g5-stroke)] bg-[var(--g5-bg0)] text-[var(--g5-text1)]"
                }`}
                onClick={() => toggleHuman(m.userId)}
              >
                <InitialsAvatar
                  userId={m.userId}
                  displayName={m.displayName}
                  size="w-[18px] h-[18px] text-[8.5px]"
                />
                {m.displayName}
                {m.deactivatedAt && " (inactive)"}
              </button>
            );
          })}
        </div>
        {inactiveHumans.length > 0 && (
          <p className="mt-3 text-xs text-[var(--g5-text2)]">
            {inactiveHumans.map((h) => h.displayName).join(", ")} — inactive
            approvers remain in history and are excluded from the new version.
          </p>
        )}
        {departedHumans.length > 0 && (
          <p
            data-testid="org-approvals-departed-note"
            className="mt-3 text-xs text-[var(--g5-text2)]"
          >
            {departedHumans.map((h) => h.displayName).join(", ")}{" "}
            {departedHumans.length === 1 ? "remains" : "remain"} on the current
            version but left the org — departed members can’t be selected for
            the new version; history keeps them.
          </p>
        )}
      </section>

      {/* Agent approver */}
      <section className="mb-4 rounded-lg border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] p-5">
        <h2 className="text-[13px] font-semibold text-[var(--g5-text0)]">
          Agent approver
        </h2>
        <p className="mb-3 mt-1 text-xs text-[var(--g5-text2)]">
          The agent participates in the same approval requirement as the humans.
          Configure its review instructions below.
        </p>
        <div className="flex w-fit overflow-hidden rounded-md border border-[var(--g5-stroke)]">
          <button
            type="button"
            data-testid="org-approvals-agent-none"
            className={`px-4 py-1.5 text-xs ${
              state.agentEnabled
                ? "bg-[var(--g5-bg0)] text-[var(--g5-text2)]"
                : "bg-[rgb(192,132,252,0.15)] text-[rgb(206,160,252)]"
            }`}
            onClick={() => patch({ agentEnabled: false })}
          >
            None
          </button>
          <button
            type="button"
            data-testid="org-approvals-agent-simple"
            className={`px-4 py-1.5 text-xs ${
              state.agentEnabled
                ? "bg-[rgb(192,132,252,0.15)] text-[rgb(206,160,252)]"
                : "bg-[var(--g5-bg0)] text-[var(--g5-text2)]"
            }`}
            onClick={() => patch({ agentEnabled: true })}
          >
            ◆ simple
          </button>
        </div>
        {state.agentEnabled && (
          <textarea
            data-testid="org-approvals-agent-prompt"
            className="mt-3 min-h-[96px] w-full resize-y rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-2.5 py-2 font-mono text-xs text-[var(--g5-text0)] focus:border-[var(--g5-info)] focus-visible:outline-none"
            placeholder="Agent approval prompt — what must hold for the agent to approve?"
            value={state.agentPrompt}
            onChange={(e) => patch({ agentPrompt: e.target.value })}
          />
        )}
      </section>

      <>
        <button
          type="button"
          data-testid="org-approvals-advanced-toggle"
          aria-expanded={advancedOpen}
          className="mt-1 flex w-full cursor-pointer items-center gap-2 py-1.5 text-[12.5px] font-semibold text-[var(--g5-text0)]"
          onClick={() => setAdvancedOpen((v) => !v)}
        >
          {advancedOpen ? (
            <ChevronDown className="h-3.5 w-3.5 text-[var(--g5-text2)]" />
          ) : (
            <ChevronRight className="h-3.5 w-3.5 text-[var(--g5-text2)]" />
          )}
          Advanced
          {changedCount > 0 && (
            <span
              data-testid="org-approvals-advanced-badge"
              className="inline-flex items-center rounded-full bg-[rgb(103,164,255,0.1)] px-2 py-0.5 text-[10.5px] font-semibold text-[var(--g5-info)]"
            >
              {changedCount} changed
            </span>
          )}
          <span
            aria-hidden="true"
            className="h-px flex-1 bg-[var(--g5-stroke)]"
          />
        </button>
        <div
          hidden={!advancedOpen}
          data-testid="org-approvals-advanced-content"
        >
          <section className="mb-4 mt-3 rounded-lg border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] p-5">
            <h2 className="mb-3 text-[13px] font-semibold text-[var(--g5-text0)]">
              Approval policy
            </h2>
            <label className="mb-3 block text-xs">
              Reviewer approval requirement
              <select
                data-testid="org-approvals-quorum"
                aria-label="Reviewer approval requirement"
                className="ml-2 rounded border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] p-1"
                disabled={submitting}
                value={state.requireAllReviewers ? "all" : "any"}
                onChange={(e) =>
                  patch({ requireAllReviewers: e.target.value === "all" })
                }
              >
                <option value="any">
                  Any-of — at least one approver must approve
                </option>
                <option value="all">
                  All — every human and agent must approve
                </option>
              </select>
            </label>
            <label className="mb-3 flex items-center gap-2 text-xs">
              <input
                data-testid="org-approvals-self-approval"
                type="checkbox"
                disabled={submitting}
                checked={state.allowSelfApproval}
                onChange={(e) => patch({ allowSelfApproval: e.target.checked })}
              />
              Allow self approval
            </label>
            <label className="mb-1 flex items-center gap-2 text-xs">
              <input
                data-testid="org-approvals-human-override"
                type="checkbox"
                disabled={submitting}
                checked={state.humanApprovalsOverrideAgent}
                onChange={(e) =>
                  patch({ humanApprovalsOverrideAgent: e.target.checked })
                }
              />
              Human approvals may override agents
            </label>
            <p className="text-xs text-[var(--g5-text2)]">
              Applies to Any-of: a human approval can override an agent
              rejection. A human rejection always blocks. All requires every
              approver to approve.
            </p>
          </section>
          {state.agentEnabled && (
            <section className="mb-4 mt-3 rounded-lg border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] p-5">
              <h2 className="text-[13px] font-semibold text-[var(--g5-text0)]">
                Agent execution
              </h2>
              <p className="mb-3 mt-1 text-xs text-[var(--g5-text2)]">
                The exact Agent, Model, and Effort saved on this version. Mode
                is fixed by the selected Agent and cannot be changed.
              </p>
              <AgentLaunchConfigControls
                value={state.agentConfig}
                options={launchOptions}
                onChange={(agentConfig) => patch({ agentConfig })}
                disabled={submitting}
                testIdPrefix="org-approvals-launch"
                selectVariant="themed"
                columns={2}
              />
            </section>
          )}
        </div>
      </>

      {/* Existing decisions — edit only */}
      {mode === "edit" && (
        <section className="mb-4 rounded-lg border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] p-5">
          <h2 className="text-[13px] font-semibold text-[var(--g5-text0)]">
            Existing decisions
          </h2>
          <p className="mb-3 mt-1 text-xs text-[var(--g5-text2)]">
            What happens to approvals already recorded against this definition?
          </p>
          <div className="flex flex-col gap-2">
            <label className="flex cursor-pointer items-start gap-2.5 rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-3 py-2.5 text-xs leading-relaxed text-[var(--g5-text1)]">
              <input
                type="radio"
                name="org-approvals-invalidate"
                data-testid="org-approvals-keep-decisions"
                className="mt-0.5 accent-[var(--g5-info)]"
                checked={!state.invalidatesPrevious}
                onChange={() => patch({ invalidatesPrevious: false })}
              />
              <span>
                <b className="text-[var(--g5-text0)]">Keep them usable</b>{" "}
                (default). Existing approvals still satisfy this definition.
              </span>
            </label>
            <label className="flex cursor-pointer items-start gap-2.5 rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-3 py-2.5 text-xs leading-relaxed text-[var(--g5-text1)]">
              <input
                type="radio"
                name="org-approvals-invalidate"
                data-testid="org-approvals-invalidate-decisions"
                className="mt-0.5 accent-[var(--g5-blocked)]"
                checked={state.invalidatesPrevious}
                onChange={() => patch({ invalidatesPrevious: true })}
              />
              <span>
                <b className="text-[var(--g5-text0)]">Invalidate them.</b> Every
                approver must re-approve. Typically only needed when the agent
                prompt materially changes.
              </span>
            </label>
          </div>
        </section>
      )}

      {/* Submit errors */}
      {submitError && (
        <div
          data-testid={
            isConflict ? "org-approvals-conflict" : "org-approvals-submit-error"
          }
          className="mb-4 flex items-start gap-2 rounded-md border border-[rgb(255,107,107,0.3)] bg-[rgb(255,107,107,0.08)] px-3 py-2.5 text-xs leading-relaxed text-[var(--g5-text1)]"
        >
          <AlertCircle className="mt-0.5 h-3.5 w-3.5 shrink-0 text-[var(--g5-blocked)]" />
          <span>
            {isConflict ? (
              <>
                Someone published a new version of this definition while you
                were editing. Your changes were not saved and Save is disabled —
                return to the library and re-apply your edit against the latest
                version.{" "}
                <button
                  type="button"
                  data-testid="org-approvals-conflict-reload"
                  className="cursor-pointer font-semibold text-[var(--g5-info)] hover:underline"
                  onClick={onCancel}
                >
                  Reload latest
                </button>
              </>
            ) : (
              submitError.message
            )}
          </span>
        </div>
      )}

      {/* Footer */}
      <div className="flex items-center justify-between gap-4 border-t border-[var(--g5-stroke)] py-3.5">
        <span className="max-w-md text-[11px] leading-relaxed text-[var(--g5-text2)]">
          {mode === "create"
            ? "Definitions are append-only — later edits never rewrite what you create here."
            : "Saving publishes a new revision of this definition — the full history stays auditable and is never rewritten."}
        </span>
        <div className="flex gap-2.5">
          <Button kind="secondary" onClick={onCancel}>
            Cancel
          </Button>
          <Button
            kind="primary"
            data-testid="org-approvals-submit"
            disabled={!submittable}
            onClick={() => onSubmit(state)}
          >
            {mode === "create" ? "Create definition" : "Save changes"}
          </Button>
        </div>
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Main content component                                             */
/* ------------------------------------------------------------------ */

type ApprovalsView =
  | { kind: "library" }
  | { kind: "create" }
  | { kind: "edit"; definition: OrgApprovalDefinitionDTO };

type CompositionFilter = "all" | "people" | "includes-agent" | "agents-only";

const COMPOSITION_FILTERS: Array<{ key: CompositionFilter; label: string }> = [
  { key: "all", label: "All" },
  { key: "people", label: "People only" },
  { key: "includes-agent", label: "Includes agent" },
  { key: "agents-only", label: "Agents only" },
];

export function OrgApprovalsPageContent() {
  const { org } = useParams<{ org: string }>();
  const { user, hasScope } = useAuth();

  // Derive org identity from the route slug (works on both the standalone
  // redirect route and the in-workspace `/:org/:workspace` route).
  const currentOrg = useMemo(
    () => user?.orgs.find((o) => o.orgSlug === org),
    [user, org],
  );
  const orgId = currentOrg?.orgId ?? "";
  const orgName = currentOrg?.orgName ?? org ?? "";
  // Per-target-org authority: `hasScope` reflects only the ACTIVE org, so the
  // per-target-org signal is the profile role (worker-host-management D19).
  const isActiveOrg = !!currentOrg && currentOrg.orgId === user?.activeOrgId;
  const canWrite =
    currentOrg?.role === "admin" || (isActiveOrg && hasScope("org:admin"));

  const [view, setView] = useState<ApprovalsView>({ kind: "library" });
  const [search, setSearch] = useState("");
  const [filter, setFilter] = useState<CompositionFilter>("all");
  const [showAllPrompts, setShowAllPrompts] = useState(false);

  const definitionsState = useOrgApprovalDefinitions(orgId);
  const membersState = useOrgMembers(orgId);
  const members = membersState.status === "ok" ? membersState.data : [];

  const createDefinition = useCreateOrgApprovalDefinition(orgId);
  const publishVersion = usePublishOrgApprovalDefinitionVersion(orgId);

  const filtered = useMemo(() => {
    if (definitionsState.status !== "ok") return [];
    const needle = search.trim().toLowerCase();
    return definitionsState.data.filter((d) => {
      const hasAgent = d.agentType === "simple";
      if (filter === "people" && hasAgent) return false;
      if (filter === "includes-agent" && !hasAgent) return false;
      if (filter === "agents-only" && (!hasAgent || d.humans.length > 0))
        return false;
      if (!needle) return true;
      return (
        d.name.toLowerCase().includes(needle) ||
        (d.description ?? "").toLowerCase().includes(needle)
      );
    });
  }, [definitionsState, search, filter]);

  const backToLibrary = () => {
    createDefinition.reset();
    publishVersion.reset();
    setView({ kind: "library" });
  };

  /* ---- editor subviews ---- */
  if (view.kind === "create" || view.kind === "edit") {
    const editing = view.kind === "edit" ? view.definition : null;
    const mutation = view.kind === "create" ? createDefinition : publishVersion;
    return (
      <div className="mx-auto w-full max-w-4xl px-6 py-8">
        <ApprovalDefinitionEditor
          mode={view.kind}
          orgName={orgName}
          definition={editing}
          members={members}
          submitting={mutation.isLoading}
          submitError={mutation.error}
          onCancel={backToLibrary}
          onSubmit={(state) => {
            const base = {
              name: state.name.trim(),
              description: state.description.trim() || undefined,
              humanUserIds: state.humanUserIds,
              requireAllReviewers: state.requireAllReviewers,
              allowSelfApproval: state.allowSelfApproval,
              humanApprovalsOverrideAgent: state.humanApprovalsOverrideAgent,
              agentType: state.agentEnabled ? ("simple" as const) : undefined,
              agentPrompt: state.agentEnabled
                ? state.agentPrompt.trim()
                : undefined,
              agentHarness: state.agentEnabled
                ? (state.agentConfig.agentProvider as PickerHarness)
                : undefined,
              agentModel: state.agentEnabled
                ? state.agentConfig.model
                : undefined,
              agentEffort: state.agentEnabled
                ? state.agentConfig.effort
                : undefined,
              // Retain the versioned wire format with one selected recommendation.
              phases: [state.recommendedPhase],
            };
            if (view.kind === "create") {
              createDefinition
                .mutateAsync(base)
                .then(backToLibrary)
                .catch(() => {
                  /* surfaced via createDefinition.error */
                });
            } else {
              publishVersion
                .mutateAsync({
                  definitionId: view.definition.id,
                  ...base,
                  invalidatesPrevious: state.invalidatesPrevious,
                })
                .then(backToLibrary)
                .catch(() => {
                  /* surfaced via publishVersion.error (incl. 409 conflict) */
                });
            }
          }}
        />
      </div>
    );
  }

  /* ---- library view ---- */
  return (
    <div className="mx-auto w-full max-w-4xl px-6 py-8">
      {/* Header */}
      <div className="mb-1 flex items-center justify-between gap-4">
        <div className="flex items-center gap-2.5">
          <ShieldCheck className="h-5 w-5 text-[var(--g5-info)]" />
          <h1 className="text-xl font-semibold text-[var(--g5-text0)]">
            Approval definitions
          </h1>
          <span className="rounded-full border border-[var(--g5-stroke)] bg-[rgb(255,255,255,0.05)] px-2.5 py-0.5 text-[10.5px] font-semibold uppercase tracking-wide text-[var(--g5-text2)]">
            {orgName}
          </span>
        </div>
        {canWrite && (
          <Button
            kind="primary"
            data-testid="org-approvals-new"
            onClick={() => setView({ kind: "create" })}
          >
            <Plus className="mr-1 h-3.5 w-3.5" />
            New definition
          </Button>
        )}
      </div>
      <p
        className={`${canWrite ? "mb-5" : "mb-1"} max-w-2xl text-[13px] leading-relaxed text-[var(--g5-text1)]`}
      >
        Library of approvals that can be attached to multiple workspaces.
      </p>
      {!canWrite && (
        <p className="mb-5 text-[11px] text-[var(--g5-text2)]">
          Read-only — publishing definitions requires an org admin.
        </p>
      )}

      {/* Org merge policy: require passing external PR checks before merge
          (governance g5.governance.merge.external-policy). */}
      {orgId && <ExternalChecksMergeSwitch orgId={orgId} canEdit={canWrite} />}

      {/* Toolbar */}
      <div className="mb-4 flex items-center gap-3">
        <div className="flex w-72 items-center gap-2 rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-2.5 py-1.5">
          <Search className="h-3.5 w-3.5 text-[var(--g5-text2)]" />
          <input
            data-testid="org-approvals-search"
            className="w-full bg-transparent text-xs text-[var(--g5-text0)]"
            placeholder="Search definitions…"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
        </div>
        <div className="flex items-center gap-1.5">
          {COMPOSITION_FILTERS.map((f) => (
            <button
              key={f.key}
              type="button"
              data-testid={`org-approvals-filter-${f.key}`}
              aria-pressed={filter === f.key}
              className={`cursor-pointer rounded-full border px-3 py-1 text-[11px] font-medium ${
                filter === f.key
                  ? "border-[var(--g5-info)] bg-[rgb(103,164,255,0.12)] text-[var(--g5-text0)]"
                  : "border-[var(--g5-stroke)] bg-transparent text-[var(--g5-text2)] hover:bg-[rgb(255,255,255,0.05)] hover:text-[var(--g5-text0)]"
              }`}
              onClick={() => setFilter(f.key)}
            >
              {f.label}
            </button>
          ))}
        </div>
        {definitionsState.status === "ok" &&
          definitionsState.data.some((d) => d.agentPrompt) && (
            <button
              type="button"
              data-testid="org-approvals-show-all-prompts"
              aria-pressed={showAllPrompts}
              className="cursor-pointer whitespace-nowrap rounded-md border border-[var(--g5-stroke)] px-3 py-1 text-[11px] font-medium text-[var(--g5-text2)] hover:bg-[rgb(255,255,255,0.05)] hover:text-[var(--g5-text0)]"
              onClick={() => setShowAllPrompts((v) => !v)}
            >
              {showAllPrompts ? "Hide all prompts" : "Show all prompts"}
            </button>
          )}
        {definitionsState.status === "ok" && (
          <span
            data-testid="org-approvals-count"
            className="text-[11px] text-[var(--g5-text2)]"
          >
            {filtered.length}{" "}
            {filtered.length === 1 ? "definition" : "definitions"}
          </span>
        )}
      </div>

      {/* States */}
      {definitionsState.status === "loading" && (
        <p
          data-testid="org-approvals-loading"
          className="text-sm text-[var(--g5-text2)]"
        >
          Loading approval definitions…
        </p>
      )}
      {definitionsState.status === "error" && (
        <div
          data-testid="org-approvals-error"
          className="flex items-center gap-2 text-sm text-[var(--g5-blocked)]"
        >
          <AlertCircle className="h-4 w-4" />
          Failed to load approval definitions.
        </div>
      )}

      {definitionsState.status === "ok" &&
        definitionsState.data.length === 0 && (
          <div
            data-testid="org-approvals-empty"
            className="rounded-lg border border-dashed border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-6 py-11 text-center"
          >
            <h2 className="mb-2 text-[15px] font-semibold text-[var(--g5-text0)]">
              No approval definitions yet
            </h2>
            <p className="mx-auto mb-4 max-w-md text-xs leading-relaxed text-[var(--g5-text2)]">
              Definitions declare who must sign off on code entering a governed
              workspace — human approvers with either one or all approvals
              required, an agent approver, or both. Create the first one, then
              require it from a workspace's Approvals tab.
            </p>
            {canWrite && (
              <Button
                kind="primary"
                onClick={() => setView({ kind: "create" })}
              >
                <Plus className="mr-1 h-3.5 w-3.5" />
                New definition
              </Button>
            )}
          </div>
        )}

      {definitionsState.status === "ok" && definitionsState.data.length > 0 && (
        <div className="grid gap-4 sm:grid-cols-2">
          {filtered.map((definition) => (
            <DefinitionCard
              key={definition.id}
              definition={definition}
              canWrite={canWrite}
              showAllPrompts={showAllPrompts}
              onEdit={() => setView({ kind: "edit", definition })}
            />
          ))}
          {canWrite && (
            <button
              type="button"
              data-testid="org-approvals-new-card"
              className="flex min-h-[180px] cursor-pointer flex-col items-center justify-center gap-1.5 rounded-xl border border-dashed border-[var(--g5-stroke)] px-4 text-center text-[var(--g5-text2)] transition-colors hover:border-[rgb(103,164,255,0.4)] hover:bg-[rgb(255,255,255,0.05)] hover:text-[var(--g5-info)]"
              onClick={() => setView({ kind: "create" })}
            >
              <span className="text-[22px] leading-none">+</span>
              <span className="text-xs font-medium">New definition</span>
              <span className="max-w-[220px] text-[11px] leading-snug">
                Pick the people (and optionally an agent) whose sign-off code
                needs.
              </span>
            </button>
          )}
        </div>
      )}
    </div>
  );
}
