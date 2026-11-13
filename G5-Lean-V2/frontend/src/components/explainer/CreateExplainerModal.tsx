import React, {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { ChevronDown, Search, X } from "lucide-react";
import type { AgentLaunchSelectionV1, SyncAgentConfig } from "../../lib/types";
import { classNames } from "../../lib/utils";
import { useResolveAgentLaunch } from "../../hooks/useAgentLaunch";
import { useStartChatMessage } from "../../hooks/useChat";
import { explainerChatPrompt } from "./explainerChatPrompt";
import {
  EXPLAINER_FILE_NAME_MAX_CHARS,
  EXPLAINER_INSTRUCTION_MAX_CHARS,
  EXPLAINER_PROMPT_MAX_CHARS,
  EXPLAINER_TEMPLATE_NAME_MAX_CHARS,
  EXPLAINER_TEMPLATE_SLUG_MAX_CHARS,
  explainerCharCount,
  explainerTemplateSlug,
  isExplainerTemplateConflict,
  readRecentExplainerTemplates,
  recordRecentExplainerTemplate,
  useExplainerTemplates,
  useSaveExplainerTemplate,
  type ExplainerFormat,
  type ExplainerTemplate,
} from "../../hooks/useExplainers";
import {
  AgentLaunchConfigControls,
  agentLabel,
  completeAgentConfig,
  initialAgentConfig,
  launchEffortLabel,
  reconcileAgentConfig,
} from "../agent/AgentLaunchConfigControls";
import { Modal } from "../ui/UI";
import {
  GatedControl,
  useLiveGateReason,
  useWorkspaceLiveGate,
} from "../../lib/workspaceLiveGate";

export interface CreateExplainerModalProps {
  open: boolean;
  workspaceId: string;
  /**
   * Workspace docs-branch id for branch-scoped explainer-template calls.
   * WorkspacePage computes `docsBranchId` internally; it is not derivable
   * from other modal props, so the launcher passes it in (spec D1).
   */
  branchId: string;
  /** Node-context launches pre-add these ids as focus chips (spec D3/D4). */
  agentConfig: SyncAgentConfig;
  agentConfigOptions?: SyncAgentConfig[];
  onCreated?: (result: { sessionId: string }) => void;
  onClose: () => void;
}

/** Sentinel select value for a prompt that matches no template verbatim. */
const CUSTOM_TEMPLATE_VALUE = "__custom__";

/**
 * How many characters over its limit a value is (0 while within it).
 * Counts Unicode code points via `explainerCharCount`, never UTF-16
 * `String.length`, so the client agrees with the server's `chars().count()`
 * on non-BMP input (spec D12; PR #2299 review finding).
 */
function charsOverLimit(value: string, max: number): number {
  return Math.max(0, explainerCharCount(value) - max);
}

/**
 * Live length feedback for a limited field (spec D12): a quiet
 * `<count> / <max>` counter from 80% of the limit, flipping to a loud
 * over-limit error message past it. No native `maxLength` anywhere — the
 * user keeps typing and fixes the text in place.
 */
function FieldLengthHint({
  noun,
  value,
  max,
  testId,
}: {
  noun: string;
  value: string;
  max: number;
  testId: string;
}) {
  const count = explainerCharCount(value);
  if (count < Math.ceil(max * 0.8)) return null;
  const over = charsOverLimit(value, max);
  return (
    <p
      data-testid={testId}
      className={classNames(
        "mt-1 text-xs",
        over > 0
          ? "font-medium text-[var(--g5-blocked)]"
          : "text-[var(--g5-text2)]",
      )}
    >
      {over > 0
        ? `${noun} is ${over.toLocaleString()} character${
            over === 1 ? "" : "s"
          } over the ${max.toLocaleString()}-character limit (${count.toLocaleString()} / ${max.toLocaleString()})`
        : `${count.toLocaleString()} / ${max.toLocaleString()}`}
    </p>
  );
}

export const CreateExplainerModal = React.memo(function CreateExplainerModal({
  open,
  workspaceId,
  branchId,
  agentConfig,
  agentConfigOptions,
  onCreated,
  onClose,
}: CreateExplainerModalProps) {
  const [topic, setTopic] = useState("");
  const topicTouchedRef = useRef(false);
  // Split from the subject (D3/D4): a sentence-length subject makes an
  // unwieldy filename, so the user names the file separately. When blank,
  // the agent chooses a short name from the instruction.
  const [fileName, setFileName] = useState("");
  const fileNameTouchedRef = useRef(false);
  const [prompt, setPrompt] = useState("");
  // The last prompt text the user actually typed (spec D5): explicitly
  // selecting "Custom (write my own)" restores it, so trying a template is
  // reversible without hand-deleting the template text.
  const lastCustomPromptRef = useRef("");
  // The last template the user explicitly chose; wins the verbatim re-match
  // when several templates share identical prompt text (spec D5).
  const [chosenTemplateSlug, setChosenTemplateSlug] = useState<string | null>(
    null,
  );
  const [recentSlugs, setRecentSlugs] = useState<string[]>([]);
  const [format, setFormat] = useState<ExplainerFormat>("page");
  // Last HTML layout choice (spec D7): toggling Markdown→HTML restores it.
  const lastHtmlLayoutRef = useRef<"page" | "slides">("page");
  // Whether the user has made the required template choice — a template or
  // the explicit Custom path (spec D5).
  const [templateChosen, setTemplateChosen] = useState(false);
  const [selectedAgentConfig, setSelectedAgentConfig] =
    useState<SyncAgentConfig>(() =>
      initialAgentConfig(agentConfig, agentConfigOptions),
    );
  const [confirmedAgentLaunch, setConfirmedAgentLaunch] =
    useState<AgentLaunchSelectionV1 | null>(null);
  const resolveAgentLaunch = useResolveAgentLaunch(workspaceId);
  // The dialog can outlive the VM: Generate reads the current gate so a
  // stopped workspace never queues the explainer chat
  // (`g5.conversation.composer.stopped`); the draft stays in the dialog.
  // Start and generate (g5.chat.start-and-send): the dialog starts the chat
  // itself, so while the owner's VM is stopped or failed the same request
  // records Start consent and queues the prompt bound to it.
  const liveGate = useWorkspaceLiveGate();
  const startAndGenerate = !!liveGate && liveGate.canStart;
  const liveGateBlockedReason = useLiveGateReason("generate an explainer");
  const liveBlockedReason = startAndGenerate ? null : liveGateBlockedReason;
  // Agent controls sit in a disclosure row that STARTS EXPANDED (spec D8:
  // agent-launch contract I9 keeps launch values visible at open); collapsing
  // is a per-open space-saving action.
  const [agentExpanded, setAgentExpanded] = useState(true);
  const userPickedAgentConfigRef = useRef(false);
  const handleAgentConfigChange = useCallback((config: SyncAgentConfig) => {
    userPickedAgentConfigRef.current = true;
    setSelectedAgentConfig(config);
  }, []);
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [submitError, setSubmitError] = useState<string | null>(null);
  // Single-flight submit (spec I1): the ref is the synchronous concurrency
  // guard; the state only drives the disabled button / "Generating…" label
  // (experience note react_query_ispending_not_a_double_submit_guard).
  const [isSubmitting, setIsSubmitting] = useState(false);
  const submitLockRef = useRef(false);

  // Save-as-template state (spec D6).
  const [showSaveTemplate, setShowSaveTemplate] = useState(false);
  const [templateName, setTemplateName] = useState("");
  const [saveTemplateError, setSaveTemplateError] = useState<string | null>(
    null,
  );
  const [conflictSlug, setConflictSlug] = useState<string | null>(null);
  const [isSavingTemplate, setIsSavingTemplate] = useState(false);
  const saveLockRef = useRef(false);

  const firstFocusableRef = useRef<HTMLButtonElement>(null);
  const wasOpenRef = useRef(false);

  const startChatMessage = useStartChatMessage();
  const openRef = useRef(open);
  useEffect(() => {
    openRef.current = open;
    return () => {
      openRef.current = false;
    };
  }, [open]);
  const templatesState = useExplainerTemplates(workspaceId, branchId);
  const saveTemplate = useSaveExplainerTemplate(workspaceId, branchId);

  // Reset all local state on the closed→open transition only.
  useEffect(() => {
    const wasOpen = wasOpenRef.current;
    if (open && !wasOpen) {
      setTopic("");
      topicTouchedRef.current = false;
      setFileName("");
      fileNameTouchedRef.current = false;
      setPrompt("");
      lastCustomPromptRef.current = "";
      setChosenTemplateSlug(null);
      setRecentSlugs(readRecentExplainerTemplates(workspaceId));
      setFormat("page");
      lastHtmlLayoutRef.current = "page";
      setTemplateChosen(false);
      userPickedAgentConfigRef.current = false;
      setSelectedAgentConfig(
        initialAgentConfig(agentConfig, agentConfigOptions),
      );
      setAgentExpanded(true);
      setErrors({});
      setSubmitError(null);
      setShowSaveTemplate(false);
      setTemplateName("");
      setSaveTemplateError(null);
      setConflictSlug(null);
    }
    wasOpenRef.current = open;
  }, [agentConfig, agentConfigOptions, open, workspaceId]);

  // Untouched selections follow the recomputed preferred config when the
  // runtime agent gate hydrates mid-open (spec D8).
  useEffect(() => {
    if (!open) return;
    setSelectedAgentConfig((previous) =>
      reconcileAgentConfig(
        previous,
        userPickedAgentConfigRef.current,
        agentConfig,
        agentConfigOptions,
      ),
    );
  }, [agentConfig, agentConfigOptions, open]);

  const templates: ExplainerTemplate[] = useMemo(
    () => (templatesState.status === "ok" ? templatesState.data : []),
    [templatesState],
  );

  const templatesBySlug = useMemo(() => {
    const map = new Map<string, ExplainerTemplate>();
    for (const template of templates) map.set(template.slug, template);
    return map;
  }, [templates]);

  // Derived verbatim selection (spec D5): a template is selected exactly when
  // the prompt equals its text; the last explicitly chosen slug wins ties.
  const selectedTemplate = useMemo(() => {
    const matching = templates.filter((template) => template.prompt === prompt);
    if (matching.length === 0) return null;
    return (
      matching.find((template) => template.slug === chosenTemplateSlug) ??
      matching[0]
    );
  }, [chosenTemplateSlug, prompt, templates]);

  const recentTemplates = useMemo(
    () =>
      recentSlugs
        .map((slug) => templatesBySlug.get(slug))
        .filter((template): template is ExplainerTemplate => Boolean(template)),
    [recentSlugs, templatesBySlug],
  );
  const builtinTemplates = templates.filter(
    (template) => template.source === "builtin",
  );
  const savedTemplates = templates.filter(
    (template) => template.source === "saved",
  );

  const clearError = useCallback((key: string) => {
    setErrors((prev) => {
      if (!prev[key]) return prev;
      const next = { ...prev };
      delete next[key];
      return next;
    });
  }, []);

  const handleTemplateSelect = useCallback(
    (value: string) => {
      if (value === "") return; // the disabled placeholder
      if (value === CUSTOM_TEMPLATE_VALUE) {
        // Explicitly choosing Custom restores the user's own remembered
        // prompt text (empty when they never typed one) — spec D5, user
        // report 2026-08-10: a no-op here made a template choice one-way.
        setTemplateChosen(true);
        setChosenTemplateSlug(null);
        setPrompt(lastCustomPromptRef.current);
        return;
      }
      const template = templatesBySlug.get(value);
      if (!template) return;
      setTemplateChosen(true);
      setChosenTemplateSlug(template.slug);
      setPrompt(template.prompt);
      // A verbatim-selected template hides the save affordance (spec D6);
      // close an open save panel rather than stranding it.
      setShowSaveTemplate(false);
      clearError("prompt");
    },
    [clearError, templatesBySlug],
  );

  // Preview of the managed filename: ONLY what the user typed in the
  // file-name field — typing the topic must not visibly rewrite the file
  // name (user feedback 2026-08-07). A blank field shows the <name>
  // placeholder; the agent chooses that name from the instruction.
  const fileNamePreviewSlug = useMemo(
    () => explainerTemplateSlug(fileName.trim()),
    [fileName],
  );

  // Realtime over-limit state (spec D12): derived each render so the loud
  // counter/error styling tracks every keystroke, and submit re-checks the
  // same values.
  const instructionOver = charsOverLimit(
    topic,
    EXPLAINER_INSTRUCTION_MAX_CHARS,
  );
  const fileNameOver = charsOverLimit(fileName, EXPLAINER_FILE_NAME_MAX_CHARS);
  const promptOver = charsOverLimit(prompt, EXPLAINER_PROMPT_MAX_CHARS);
  const templateNameOver = charsOverLimit(
    templateName,
    EXPLAINER_TEMPLATE_NAME_MAX_CHARS,
  );
  // The DERIVED slug has its own server-side 64-char rule: a 65-80 character
  // ASCII name passes the name counter but its slug would only fail after
  // Save with a 422, so the slug limit is validated in realtime too
  // (spec D12; PR #2299 review finding).
  const templateNameSlug = explainerTemplateSlug(templateName);
  const templateSlugOver = Math.max(
    0,
    templateNameSlug.length - EXPLAINER_TEMPLATE_SLUG_MAX_CHARS,
  );

  const validate = useCallback((): boolean => {
    const newErrors: Record<string, string> = {};
    if (!topic.trim()) {
      newErrors.topic = "Instruction is required";
    } else if (charsOverLimit(topic, EXPLAINER_INSTRUCTION_MAX_CHARS) > 0) {
      newErrors.topic = `Instruction exceeds the ${EXPLAINER_INSTRUCTION_MAX_CHARS.toLocaleString()}-character limit — shorten it to continue`;
    }
    // An explicit filename must produce a usable filename stem.
    if (fileName.trim() && !explainerTemplateSlug(fileName.trim())) {
      newErrors.fileName =
        "File name must contain at least one letter or number";
    } else if (charsOverLimit(fileName, EXPLAINER_FILE_NAME_MAX_CHARS) > 0) {
      newErrors.fileName = `File name exceeds the ${EXPLAINER_FILE_NAME_MAX_CHARS.toLocaleString()}-character limit — shorten it to continue`;
    }
    if (!prompt.trim()) {
      newErrors.prompt = "Prompt is required";
    } else if (charsOverLimit(prompt, EXPLAINER_PROMPT_MAX_CHARS) > 0) {
      newErrors.prompt = `Prompt exceeds the ${EXPLAINER_PROMPT_MAX_CHARS.toLocaleString()}-character limit — shorten it to continue`;
    }
    setErrors(newErrors);
    return Object.keys(newErrors).length === 0;
  }, [fileName, prompt, topic]);

  const handleSubmit = useCallback(async () => {
    // Single-flight guard (spec I1): synchronous ref lock before any await.
    if (submitLockRef.current) return;
    if (liveBlockedReason !== null) return;
    if (!validate()) return;

    submitLockRef.current = true;
    setIsSubmitting(true);
    setSubmitError(null);

    try {
      const agentLaunch = confirmedAgentLaunch;
      if (!agentLaunch) {
        throw new Error("The exact launch values are still resolving.");
      }
      const result = await startChatMessage.mutateAsync({
        workspaceId,
        content: explainerChatPrompt({
          workspaceId,
          topic,
          fileName,
          prompt,
          format,
        }),
        scopeType: "workspace",
        agentLaunch,
        ...(startAndGenerate ? { startCompute: true } : {}),
      });
      if (selectedTemplate) {
        setRecentSlugs(
          recordRecentExplainerTemplate(workspaceId, selectedTemplate.slug),
        );
      }
      // A launch can finish after navigation unmounts this workspace's modal.
      // The session remains available in its workspace, but must not steal focus.
      if (!openRef.current) return;
      onCreated?.({ sessionId: result.session.id });
      onClose();
    } catch (err: unknown) {
      if (!openRef.current) return;
      const message =
        err instanceof Error ? err.message : "Failed to start explainer chat";
      setSubmitError(message);
    } finally {
      submitLockRef.current = false;
      setIsSubmitting(false);
    }
  }, [
    startChatMessage,
    fileName,
    format,
    liveBlockedReason,
    startAndGenerate,
    onClose,
    onCreated,
    prompt,
    confirmedAgentLaunch,
    selectedTemplate,
    topic,
    validate,
    workspaceId,
  ]);

  const handleSaveTemplate = useCallback(
    async (overwrite: boolean) => {
      // Separate synchronous lock: save-as-template is its own direct entry
      // point and must not share the submit lock (experience note variant
      // 2026-08-04 — direct entry points need their own guard).
      if (saveLockRef.current) return;
      const name = templateName.trim();
      if (!name) {
        setSaveTemplateError("Template name is required");
        return;
      }
      const slug = explainerTemplateSlug(name);
      if (!slug) {
        setSaveTemplateError("Template name must contain letters or numbers");
        return;
      }
      if (charsOverLimit(templateName, EXPLAINER_TEMPLATE_NAME_MAX_CHARS) > 0) {
        setSaveTemplateError(
          `Template name exceeds the ${EXPLAINER_TEMPLATE_NAME_MAX_CHARS.toLocaleString()}-character limit — shorten it to continue`,
        );
        return;
      }
      // The server rejects slugs over 64 chars; without this gate a 65-80
      // character ASCII name would pass the name counter and only fail
      // after Save with a 422 (PR #2299 review finding).
      if (slug.length > EXPLAINER_TEMPLATE_SLUG_MAX_CHARS) {
        setSaveTemplateError(
          `Template name saves as a ${slug.length}-character slug — over the ${EXPLAINER_TEMPLATE_SLUG_MAX_CHARS}-character slug limit; shorten the name to continue`,
        );
        return;
      }
      if (!prompt.trim()) {
        setSaveTemplateError("Write a prompt before saving it as a template");
        return;
      }

      saveLockRef.current = true;
      setIsSavingTemplate(true);
      setSaveTemplateError(null);

      try {
        await saveTemplate.mutateAsync({
          name,
          slug,
          prompt,
          ...(overwrite ? { overwrite: true } : {}),
        });
        // The saved template's prompt equals the current prompt, so the D5
        // verbatim re-match keeps it selected under its slug.
        setChosenTemplateSlug(slug);
        setConflictSlug(null);
        setShowSaveTemplate(false);
        setTemplateName("");
      } catch (err: unknown) {
        if (isExplainerTemplateConflict(err)) {
          setConflictSlug(slug);
        } else {
          setSaveTemplateError(
            err instanceof Error ? err.message : "Failed to save template",
          );
        }
      } finally {
        saveLockRef.current = false;
        setIsSavingTemplate(false);
      }
    },
    [prompt, saveTemplate, templateName],
  );

  if (!open) return null;

  // Template choice is the required first step (spec D5): before the user
  // picks a template or explicit Custom, the select shows the placeholder
  // and the prompt stays locked.
  const templateSelectValue = !templateChosen
    ? ""
    : selectedTemplate
      ? selectedTemplate.slug
      : CUSTOM_TEMPLATE_VALUE;

  // Collapsed-row summary (spec D8): derived from the selected config the
  // expanded controls edit; resolution warnings still render inside them.
  const summaryConfig = completeAgentConfig(selectedAgentConfig);
  const agentSummary = [
    agentLabel(summaryConfig.agentProvider),
    summaryConfig.modelLabel,
    summaryConfig.effort
      ? `${launchEffortLabel(summaryConfig.effort)} effort`
      : null,
    summaryConfig.permissionLabel,
  ]
    .filter(Boolean)
    .join(" · ");

  return (
    // Two-column dialog with full-width output and agent rows (spec D11):
    // the widest shell size, so every section fits without body scrolling on
    // a laptop viewport.
    <Modal
      onClose={onClose}
      labelledBy="create-explainer-modal-title"
      size="xl"
      closeDisabled={
        isSubmitting || startChatMessage.isLoading || isSavingTemplate
      }
      initialFocusRef={firstFocusableRef}
      className="flex flex-col !overflow-hidden p-0"
    >
      <div className="flex shrink-0 items-center justify-between border-b border-[var(--g5-stroke)] px-5 py-4">
        <div>
          <h2
            id="create-explainer-modal-title"
            className="text-lg font-semibold text-[var(--g5-text0)]"
          >
            Create Explainer
          </h2>
          <p className="mt-0.5 text-xs text-[var(--g5-text2)]">
            A readable doc (HTML or Markdown) for a topic, scoped to chosen
            nodes or the whole workspace.
          </p>
        </div>
        <button
          ref={firstFocusableRef}
          data-testid="create-explainer-close"
          onClick={onClose}
          disabled={
            isSubmitting || startChatMessage.isLoading || isSavingTemplate
          }
          className="p-1 rounded-[var(--g5-radius-sm)] text-[var(--g5-text2)] hover:text-[var(--g5-text0)] hover:bg-[var(--g5-bg2)]"
          aria-label="Close"
        >
          <X size={18} />
        </button>
      </div>

      <div
        data-testid="create-explainer-modal-body"
        className="grid grid-cols-1 md:grid-cols-2 gap-x-6 gap-y-4 px-5 py-4 flex-1 min-h-0 overflow-y-auto"
      >
        <div className="space-y-4 min-w-0">
          <p className="text-[10px] font-semibold uppercase tracking-[0.08em] text-[var(--g5-text2)]">
            What to explain
          </p>
          {/* Instructions routinely run past one line. */}
          <div>
            <label className="block text-sm font-medium text-[var(--g5-text1)] mb-1">
              Instruction <span className="text-[var(--g5-blocked)]">*</span>
            </label>
            <textarea
              rows={2}
              data-testid="create-explainer-topic"
              value={topic}
              onChange={(e) => {
                topicTouchedRef.current = true;
                setTopic(e.target.value);
                if (
                  e.target.value.trim() &&
                  charsOverLimit(
                    e.target.value,
                    EXPLAINER_INSTRUCTION_MAX_CHARS,
                  ) === 0
                ) {
                  clearError("topic");
                }
              }}
              placeholder="e.g., What APIs does the server accept, and what do they return?"
              className={classNames(
                "w-full px-3 py-2 text-sm rounded-[var(--g5-radius-md)] bg-[var(--g5-bg1)] border border-[var(--g5-stroke)] resize-y",
                "text-[var(--g5-text0)] placeholder:text-[var(--g5-text2)] focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-[var(--g5-active)]",
                errors.topic || instructionOver > 0
                  ? "ring-1 ring-[var(--g5-blocked)]"
                  : "",
              )}
            />
            <FieldLengthHint
              noun="Instruction"
              value={topic}
              max={EXPLAINER_INSTRUCTION_MAX_CHARS}
              testId="create-explainer-topic-length"
            />
            {errors.topic && (
              <p className="mt-1 text-xs text-[var(--g5-blocked)]">
                {errors.topic}
              </p>
            )}
          </div>
        </div>

        <div className="space-y-4 min-w-0">
          <p className="text-[10px] font-semibold uppercase tracking-[0.08em] text-[var(--g5-text2)]">
            How to write it
          </p>
          <div>
            <div className="flex items-center justify-between mb-1">
              <label className="block text-sm font-medium text-[var(--g5-text1)]">
                Template <span className="text-[var(--g5-blocked)]">*</span>
              </label>
              {/* Saving only makes sense for a custom prompt — a
                  verbatim-selected template already exists (spec D6). */}
              {templateChosen && !selectedTemplate && (
                <button
                  type="button"
                  data-testid="create-explainer-save-template-toggle"
                  onClick={() => {
                    setShowSaveTemplate((prev) => !prev);
                    setSaveTemplateError(null);
                    setConflictSlug(null);
                  }}
                  className="text-xs text-[var(--g5-active)] hover:underline"
                >
                  Save as template…
                </button>
              )}
            </div>
            {/* Single template surface (spec D5, user decision 2026-08-28):
                the grouped select is the only selection control — the former
                quick-pick chip row duplicated it. */}
            <select
              data-testid="create-explainer-template-select"
              value={templateSelectValue}
              onChange={(e) => handleTemplateSelect(e.target.value)}
              className="w-full px-3 py-2 text-sm rounded-[var(--g5-radius-md)] bg-[var(--g5-bg1)] border border-[var(--g5-stroke)] text-[var(--g5-text0)] focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-[var(--g5-active)]"
            >
              <option value="" disabled>
                Choose a template…
              </option>
              <option value={CUSTOM_TEMPLATE_VALUE}>
                Custom (write my own)
              </option>
              {recentTemplates.length > 0 && (
                <optgroup label="Recent">
                  {recentTemplates.map((template) => (
                    <option
                      key={`recent-${template.slug}`}
                      value={template.slug}
                    >
                      {template.name}
                    </option>
                  ))}
                </optgroup>
              )}
              {builtinTemplates.length > 0 && (
                <optgroup label="Built-in">
                  {builtinTemplates.map((template) => (
                    <option key={template.slug} value={template.slug}>
                      {template.name}
                    </option>
                  ))}
                </optgroup>
              )}
              {savedTemplates.length > 0 && (
                <optgroup label="Saved">
                  {savedTemplates.map((template) => (
                    <option key={template.slug} value={template.slug}>
                      {template.name}
                    </option>
                  ))}
                </optgroup>
              )}
            </select>

            {showSaveTemplate && !selectedTemplate && (
              <div className="mt-2 rounded-[var(--g5-radius-md)] border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] p-3 space-y-2">
                <label className="block text-xs font-medium text-[var(--g5-text1)]">
                  Template name
                </label>
                <div className="flex items-center gap-2">
                  <input
                    type="text"
                    data-testid="create-explainer-template-name"
                    value={templateName}
                    onChange={(e) => {
                      setTemplateName(e.target.value);
                      setSaveTemplateError(null);
                      setConflictSlug(null);
                    }}
                    placeholder="e.g., Team onboarding"
                    className={classNames(
                      "flex-1 px-3 py-2 text-sm rounded-[var(--g5-radius-md)] bg-[var(--g5-bg0)] border border-[var(--g5-stroke)] text-[var(--g5-text0)] placeholder:text-[var(--g5-text2)] focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-[var(--g5-active)]",
                      templateNameOver > 0 || templateSlugOver > 0
                        ? "ring-1 ring-[var(--g5-blocked)]"
                        : "",
                    )}
                  />
                  <button
                    type="button"
                    data-testid="create-explainer-save-template"
                    onClick={() => void handleSaveTemplate(false)}
                    disabled={isSavingTemplate}
                    className="px-3 py-2 text-sm font-medium rounded-[var(--g5-radius-md)] border border-[var(--g5-stroke)] text-[var(--g5-text1)] hover:bg-[var(--g5-bg2)] disabled:cursor-not-allowed disabled:opacity-50"
                  >
                    {isSavingTemplate ? "Saving…" : "Save"}
                  </button>
                </div>
                <FieldLengthHint
                  noun="Template name"
                  value={templateName}
                  max={EXPLAINER_TEMPLATE_NAME_MAX_CHARS}
                  testId="create-explainer-template-name-length"
                />
                {templateSlugOver > 0 && (
                  <p
                    data-testid="create-explainer-template-slug-length"
                    className="mt-1 text-xs font-medium text-[var(--g5-blocked)]"
                  >
                    Template name saves as a {templateNameSlug.length}
                    -character slug — over the{" "}
                    {EXPLAINER_TEMPLATE_SLUG_MAX_CHARS}-character slug limit;
                    shorten the name to continue
                  </p>
                )}
                {conflictSlug && (
                  <div className="flex items-center justify-between gap-2">
                    <p className="text-xs text-[var(--g5-blocked)]">
                      A template named “{conflictSlug}” already exists.
                      Overwrite it?
                    </p>
                    <button
                      type="button"
                      data-testid="create-explainer-overwrite-confirm"
                      onClick={() => void handleSaveTemplate(true)}
                      disabled={isSavingTemplate}
                      className="shrink-0 px-3 py-1 text-xs font-medium rounded-[var(--g5-radius-md)] border border-[var(--g5-blocked)] text-[var(--g5-blocked)] hover:bg-[var(--g5-bg2)] disabled:cursor-not-allowed disabled:opacity-50"
                    >
                      Overwrite
                    </button>
                  </div>
                )}
                {saveTemplateError && (
                  <p className="text-xs text-[var(--g5-blocked)]">
                    {saveTemplateError}
                  </p>
                )}
              </div>
            )}
          </div>

          <div>
            <label className="block text-sm font-medium text-[var(--g5-text1)] mb-1">
              Prompt <span className="text-[var(--g5-blocked)]">*</span>
            </label>
            <textarea
              data-testid="create-explainer-prompt"
              value={prompt}
              disabled={!templateChosen}
              onChange={(e) => {
                setPrompt(e.target.value);
                // Remember the user's own brief so an explicit Custom
                // selection can restore it after a template was tried —
                // but ONLY text that matches no template verbatim (spec
                // D5): typing a template's exact text re-selects that
                // template, and restoring it as "custom" later would lose
                // the earlier genuinely-custom brief.
                if (
                  !templates.some(
                    (template) => template.prompt === e.target.value,
                  )
                ) {
                  lastCustomPromptRef.current = e.target.value;
                }
                if (
                  e.target.value.trim() &&
                  charsOverLimit(e.target.value, EXPLAINER_PROMPT_MAX_CHARS) ===
                    0
                ) {
                  clearError("prompt");
                }
              }}
              placeholder={
                templateChosen
                  ? "Describe what the explainer should cover and how it should present it."
                  : "Choose a template above first — or pick Custom (write my own)."
              }
              rows={7}
              className={classNames(
                "w-full px-3 py-2 text-sm rounded-[var(--g5-radius-md)] bg-[var(--g5-bg1)] border border-[var(--g5-stroke)]",
                "text-[var(--g5-text0)] placeholder:text-[var(--g5-text2)] focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-[var(--g5-active)] resize-y min-h-[150px] max-h-[320px]",
                errors.prompt || promptOver > 0
                  ? "ring-1 ring-[var(--g5-blocked)]"
                  : "",
              )}
            />
            <FieldLengthHint
              noun="Prompt"
              value={prompt}
              max={EXPLAINER_PROMPT_MAX_CHARS}
              testId="create-explainer-prompt-length"
            />
            {errors.prompt && (
              <p className="mt-1 text-xs text-[var(--g5-blocked)]">
                {errors.prompt}
              </p>
            )}
          </div>
        </div>

        {/* Output bar (spec D11/D7): file name and format sit together —
            the format decides the extension in the managed-path preview. */}
        <div className="md:col-span-2 grid grid-cols-1 sm:grid-cols-[1fr_auto] gap-x-8 gap-y-3 items-start rounded-[var(--g5-radius-md)] border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-4 py-3">
          <div>
            <label className="block text-sm font-medium text-[var(--g5-text1)] mb-1">
              File name{" "}
              <span className="font-normal text-[var(--g5-text2)]">
                (optional — defaults to the start of the instruction)
              </span>
            </label>
            <input
              type="text"
              data-testid="create-explainer-file-name"
              value={fileName}
              onChange={(e) => {
                fileNameTouchedRef.current = true;
                setFileName(e.target.value);
                // Clear only when EVERY fileName failure mode is resolved:
                // within the char limit AND (blank or slugifiable) — typing
                // more symbols must not hide a still-valid "must contain a
                // letter or number" error (PR #2299 review finding).
                const trimmedFileName = e.target.value.trim();
                if (
                  (!trimmedFileName ||
                    explainerTemplateSlug(trimmedFileName)) &&
                  charsOverLimit(
                    e.target.value,
                    EXPLAINER_FILE_NAME_MAX_CHARS,
                  ) === 0
                ) {
                  clearError("fileName");
                }
              }}
              placeholder="e.g., workspace-merge"
              className={classNames(
                "w-full px-3 py-2 text-sm rounded-[var(--g5-radius-md)] bg-[var(--g5-bg0)] border border-[var(--g5-stroke)]",
                "text-[var(--g5-text0)] placeholder:text-[var(--g5-text2)] focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-[var(--g5-active)]",
                errors.fileName || fileNameOver > 0
                  ? "ring-1 ring-[var(--g5-blocked)]"
                  : "",
              )}
            />
            <p className="mt-1 text-xs text-[var(--g5-text2)]">
              Requested file:{" "}
              <code>
                docs/&lt;workspace&gt;/explainers/
                {fileNamePreviewSlug || "<name>"}-&lt;date&gt;.
                {format === "markdown" ? "md" : "html"}
              </code>
            </p>
            <FieldLengthHint
              noun="File name"
              value={fileName}
              max={EXPLAINER_FILE_NAME_MAX_CHARS}
              testId="create-explainer-file-name-length"
            />
            {errors.fileName && (
              <p className="mt-1 text-xs text-[var(--g5-blocked)]">
                {errors.fileName}
              </p>
            )}
          </div>

          {/* Two-step format choice (spec D7): document kind first — making
              the HTML/Markdown split explicit — then an HTML-only layout
              follow-up for Single page vs Slides. */}
          <div className="flex flex-col items-start sm:items-end gap-2">
            <div className="flex items-center gap-3">
              <label className="text-sm font-medium text-[var(--g5-text1)]">
                Format
              </label>
              <div
                role="radiogroup"
                aria-label="Document kind"
                className="inline-flex rounded-[var(--g5-radius-md)] border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] p-0.5"
              >
                <button
                  type="button"
                  role="radio"
                  aria-checked={format !== "markdown"}
                  data-testid="create-explainer-format-html"
                  onClick={() => setFormat(lastHtmlLayoutRef.current)}
                  title="A self-contained HTML document (single page or slides)"
                  className={classNames(
                    "px-3 py-1.5 text-xs font-medium rounded-[var(--g5-radius-sm)] transition-colors",
                    format !== "markdown"
                      ? "bg-[var(--g5-bg0)] text-[var(--g5-text0)] shadow-sm"
                      : "text-[var(--g5-text2)] hover:text-[var(--g5-text0)]",
                  )}
                >
                  HTML
                </button>
                <button
                  type="button"
                  role="radio"
                  aria-checked={format === "markdown"}
                  data-testid="create-explainer-format-markdown"
                  onClick={() => {
                    if (format !== "markdown") {
                      lastHtmlLayoutRef.current = format;
                    }
                    setFormat("markdown");
                  }}
                  title="A plain Markdown document"
                  className={classNames(
                    "px-3 py-1.5 text-xs font-medium rounded-[var(--g5-radius-sm)] transition-colors",
                    format === "markdown"
                      ? "bg-[var(--g5-bg0)] text-[var(--g5-text0)] shadow-sm"
                      : "text-[var(--g5-text2)] hover:text-[var(--g5-text0)]",
                  )}
                >
                  Markdown
                </button>
              </div>
            </div>
            {format !== "markdown" && (
              <div className="flex items-center gap-3">
                <span className="text-xs text-[var(--g5-text2)]">
                  HTML layout
                </span>
                <div
                  role="radiogroup"
                  aria-label="HTML layout"
                  className="inline-flex rounded-[var(--g5-radius-md)] border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] p-0.5"
                >
                  <button
                    type="button"
                    role="radio"
                    aria-checked={format === "page"}
                    data-testid="create-explainer-format-page"
                    onClick={() => {
                      lastHtmlLayoutRef.current = "page";
                      setFormat("page");
                    }}
                    title="One scrollable explainer page"
                    className={classNames(
                      "px-3 py-1.5 text-xs font-medium rounded-[var(--g5-radius-sm)] transition-colors",
                      format === "page"
                        ? "bg-[var(--g5-bg0)] text-[var(--g5-text0)] shadow-sm"
                        : "text-[var(--g5-text2)] hover:text-[var(--g5-text0)]",
                    )}
                  >
                    Single page
                  </button>
                  <button
                    type="button"
                    role="radio"
                    aria-checked={format === "slides"}
                    data-testid="create-explainer-format-slides"
                    onClick={() => {
                      lastHtmlLayoutRef.current = "slides";
                      setFormat("slides");
                    }}
                    title="A stepped slide deck"
                    className={classNames(
                      "px-3 py-1.5 text-xs font-medium rounded-[var(--g5-radius-sm)] transition-colors",
                      format === "slides"
                        ? "bg-[var(--g5-bg0)] text-[var(--g5-text0)] shadow-sm"
                        : "text-[var(--g5-text2)] hover:text-[var(--g5-text0)]",
                    )}
                  >
                    Slides
                  </button>
                </div>
              </div>
            )}
          </div>
        </div>

        {/* Agent disclosure row (spec D8/D11): starts expanded so launch
            values are visible at open (agent-launch I9); when collapsed the
            shared controls stay mounted so resolution still completes. */}
        <div className="md:col-span-2 rounded-[var(--g5-radius-md)] border border-[var(--g5-stroke)] bg-[var(--g5-bg1)]">
          <button
            type="button"
            data-testid="create-explainer-agent-toggle"
            aria-expanded={agentExpanded}
            onClick={() => setAgentExpanded((prev) => !prev)}
            className="flex w-full items-center gap-3 rounded-[var(--g5-radius-md)] px-4 py-2.5 text-left hover:bg-[var(--g5-bg2)]"
          >
            <span className="text-sm font-medium text-[var(--g5-text1)]">
              Agent
            </span>
            <span
              data-testid="create-explainer-agent-summary"
              className="flex-1 truncate text-right text-xs text-[var(--g5-text2)]"
            >
              {agentSummary}
              {confirmedAgentLaunch ? "" : " · resolving defaults…"}
            </span>
            <ChevronDown
              size={15}
              className={classNames(
                "shrink-0 text-[var(--g5-text2)] transition-transform",
                agentExpanded ? "rotate-180" : "",
              )}
            />
          </button>
          <div hidden={!agentExpanded} className="px-4 pb-4">
            <AgentLaunchConfigControls
              value={selectedAgentConfig}
              options={agentConfigOptions}
              onChange={handleAgentConfigChange}
              resolvePreferences={resolveAgentLaunch.mutateAsync}
              onResolutionSelectionChange={setConfirmedAgentLaunch}
              disabled={isSubmitting}
              testIdPrefix="create-explainer"
              selectVariant="themed"
              // 2×2 keeps the selects readable at this width and matches the
              // approved mockup (shared-components D37).
              columns={2}
            />
          </div>
        </div>
      </div>

      <div
        data-testid="create-explainer-modal-actions"
        className="flex shrink-0 items-center justify-between border-t border-[var(--g5-stroke)] px-5 py-4"
      >
        <div className="space-y-1">
          {submitError && (
            <p
              data-testid="create-explainer-submit-error"
              className="text-xs text-[var(--g5-blocked)]"
            >
              {submitError}
            </p>
          )}
        </div>
        <div className="flex items-center gap-3">
          <button
            type="button"
            data-testid="create-explainer-cancel"
            onClick={onClose}
            disabled={
              isSubmitting || startChatMessage.isLoading || isSavingTemplate
            }
            className="px-4 py-2 text-sm font-medium rounded-[var(--g5-radius-md)] text-[var(--g5-text1)] hover:bg-[var(--g5-bg2)]"
          >
            Cancel
          </button>
          <GatedControl
            verb="generate an explainer"
            reason={liveBlockedReason}
            testId="create-explainer-start-hint"
          >
            <button
              type="button"
              data-testid="create-explainer-submit"
              onClick={handleSubmit}
              title={liveBlockedReason ?? undefined}
              disabled={
                isSubmitting ||
                startChatMessage.isLoading ||
                !confirmedAgentLaunch ||
                liveBlockedReason !== null
              }
              className="px-4 py-2 text-sm font-medium rounded-[var(--g5-radius-md)] bg-[var(--g5-active)] text-white hover:opacity-90 disabled:opacity-50 disabled:cursor-not-allowed"
            >
              {isSubmitting || startChatMessage.isLoading
                ? startAndGenerate
                  ? "Starting…"
                  : "Generating…"
                : startAndGenerate
                  ? "Start and generate"
                  : "Generate"}
            </button>
          </GatedControl>
        </div>
      </div>
    </Modal>
  );
});
