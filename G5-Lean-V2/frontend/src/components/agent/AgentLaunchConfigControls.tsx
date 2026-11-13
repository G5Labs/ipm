import React, { useEffect, useMemo, useRef, useState } from "react";
import {
  catalogDefaultChoiceFor,
  effortSelectionForModel,
  launchEffortForModel,
  launcherModelOptionsForHarness,
  launchModeForHarness,
  launchModelLabelFor,
} from "../../lib/harnessPicker";
import type {
  AgentAccountSelectionV1,
  AgentLaunchPreferencesV1,
  AgentLaunchSelectionV1,
  ChatAgentProvider,
  ResolvedAgentExecutionV1,
  SyncAgentConfig,
} from "../../lib/types";
import {
  canonicalAgentLaunchPreferences,
  configuredAgentLaunchPreferences,
} from "../../lib/types";
import { classNames, ThemedSelect } from "../ui/UI";
import type { ThemedSelectOption } from "../ui/UI";
import { accountSelectionFromValue } from "../../lib/providerAccountSelection";

export function normalizeLaunchProvider(
  provider: ChatAgentProvider,
): ChatAgentProvider {
  return provider === "codex" ? "codex" : "claude_code";
}

export function agentLabel(provider: ChatAgentProvider): string {
  return provider === "codex" ? "Codex" : "Claude Code";
}

export function normalizeAgentConfig(config: SyncAgentConfig): SyncAgentConfig {
  return {
    ...config,
    agentProvider: normalizeLaunchProvider(config.agentProvider),
  };
}

// Launcher model options: the harness catalog lineup behind the `""`
// configured-default/inherit sentinel, plus an append for an out-of-catalog
// persisted pin so it stays selectable (uniform across harnesses — D37).
export function modelOptionsFor(config: SyncAgentConfig): Array<{
  value: string;
  label: string;
}> {
  // The "" configured-default sentinel is labeled honestly (#1370, chat
  // intent 2bca6571): "Default (<resolved default model label>)" instead of
  // borrowing the model's bare label, which read as a concrete selection.
  const options = launcherModelOptionsForHarness(config.agentProvider).map(
    (option) =>
      option.value === ""
        ? { ...option, label: `Default (${option.label})` }
        : option,
  );
  if (
    config.model &&
    !options.some((option) => option.value === config.model)
  ) {
    options.push({
      value: config.model,
      label:
        config.modelLabel ??
        launchModelLabelFor(config.agentProvider, config.model),
    });
  }
  return options;
}

export function defaultModelValueFor(config: SyncAgentConfig): string {
  return config.model ?? "";
}

export function modelLabelFor(
  config: SyncAgentConfig,
  model: string,
): string | undefined {
  // The "" sentinel's payload label stays the bare resolved default-model
  // label — the "Default (…)" wording is dropdown display only, and this
  // label travels into task summaries.
  if (!model) {
    return launchModelLabelFor(config.agentProvider, "");
  }
  return (
    modelOptionsFor(config).find((option) => option.value === model)?.label ??
    config.modelLabel ??
    (model || undefined)
  );
}

export function launchEffortLabel(effort: string): string {
  switch (effort) {
    case "none":
      return "Not supported";
    case "low":
      return "Low";
    case "medium":
      return "Medium";
    case "high":
      return "High";
    case "xhigh":
      return "XHigh";
    case "max":
      return "Max";
    case "ultra":
      return "Ultra";
    default:
      return effort;
  }
}

export function effortOptionsForModel(
  agentProvider: ChatAgentProvider,
  model: string,
): Array<{ value: string; label: string }> {
  const selection = effortSelectionForModel(agentProvider, model);
  return selection.efforts.map((effort) => ({
    value: effort,
    label: launchEffortLabel(effort),
  }));
}

// Effort options for the selected model: the model's catalog ladder subset;
// the `""` configured-default/inherit model shows the single inherit entry.
export function effortOptionsFor(config: SyncAgentConfig): Array<{
  value: string;
  label: string;
}> {
  const model =
    defaultModelValueFor(config) ||
    catalogDefaultChoiceFor(config.agentProvider).model;
  return effortOptionsForModel(config.agentProvider, model);
}

// Stamp the harness-forced launch mode (types/core D15/D19) and the model's
// clamped effort (plan D-6) so a submitted config can never carry a stale
// mode or an out-of-range effort.
export function completeAgentConfig(config: SyncAgentConfig): SyncAgentConfig {
  const normalized = normalizeAgentConfig(config);
  const model = defaultModelValueFor(normalized);
  const { mode, label } = launchModeForHarness(normalized.agentProvider);
  return {
    ...normalized,
    model: model || undefined,
    modelLabel: modelLabelFor(normalized, model),
    permissionMode: mode,
    permissionLabel: label,
    effort: launchEffortForModel(
      normalized.agentProvider,
      model,
      normalized.effort,
    ),
  };
}

export interface AgentLaunchPreferenceExtras {
  /**
   * Visible Account choice (contract D11/D14; right-panel
   * session_provider_controls D30). Once a host displays the field it passes
   * the exact shown value and it travels `source: explicit`; hosts that do
   * not render the field omit it so the resolver applies the effective
   * assignment (`inherit_configured`).
   */
  account?: AgentAccountSelectionV1;
}

/** Convert the uniform UX source choice into the versioned API contract. */
export function agentLaunchPreferencesForConfig(
  config: SyncAgentConfig,
  extras: AgentLaunchPreferenceExtras = {},
): AgentLaunchPreferencesV1 {
  const source = config.launchSource ?? "inherit_configured";
  const accountPreference = extras.account
    ? { account: { source: "explicit" as const, value: extras.account } }
    : {};
  if (source === "inherit_configured")
    return { ...configuredAgentLaunchPreferences(), ...accountPreference };
  if (source === "canonical_default")
    return { ...canonicalAgentLaunchPreferences(), ...accountPreference };

  const complete = completeAgentConfig(config);
  const canonical = catalogDefaultChoiceFor(complete.agentProvider);
  return {
    schemaVersion: 1,
    ...accountPreference,
    provider: { source: "explicit", value: complete.agentProvider },
    model: {
      source: "explicit",
      value: complete.model ?? canonical.model,
    },
    permissionMode: {
      source: "explicit",
      value:
        complete.permissionMode ??
        launchModeForHarness(complete.agentProvider).mode,
    },
    effort: {
      source: "explicit",
      value: complete.effort ?? canonical.effort,
    },
  };
}

export function dedupeAgentConfigs(
  configs: SyncAgentConfig[],
): SyncAgentConfig[] {
  const byProvider = new Map<ChatAgentProvider, SyncAgentConfig>();
  for (const config of configs) {
    const normalized = completeAgentConfig(config);
    if (!byProvider.has(normalized.agentProvider)) {
      byProvider.set(normalized.agentProvider, normalized);
    }
  }
  return Array.from(byProvider.values());
}

export function initialAgentConfig(
  agentConfig: SyncAgentConfig,
  agentConfigOptions?: SyncAgentConfig[],
): SyncAgentConfig {
  const options = dedupeAgentConfigs(
    agentConfigOptions?.length ? agentConfigOptions : [agentConfig],
  );
  const preferred = normalizeLaunchProvider(agentConfig.agentProvider);
  return (
    options.find((option) => option.agentProvider === preferred) ??
    options[0] ??
    completeAgentConfig(agentConfig)
  );
}

/**
 * Shallow equality over the launch-config fields — used by launch modals to
 * avoid re-render churn when re-seeding an untouched selection from a
 * recomputed (but content-identical) preferred config.
 */
export function agentConfigsEqual(
  a: SyncAgentConfig,
  b: SyncAgentConfig,
): boolean {
  return (
    a.agentProvider === b.agentProvider &&
    a.launchSource === b.launchSource &&
    a.model === b.model &&
    a.modelLabel === b.modelLabel &&
    a.permissionMode === b.permissionMode &&
    a.permissionLabel === b.permissionLabel &&
    a.effort === b.effort
  );
}

/**
 * Reconcile a modal's selected launch config after the preferred config or
 * option set changes while the modal is open. An untouched selection follows
 * the recomputed preferred config; an explicit user pick is kept for as long
 * as its provider remains offered.
 */
export function reconcileAgentConfig(
  previous: SyncAgentConfig,
  userPicked: boolean,
  agentConfig: SyncAgentConfig,
  agentConfigOptions?: SyncAgentConfig[],
): SyncAgentConfig {
  const seeded = initialAgentConfig(agentConfig, agentConfigOptions);
  if (!userPicked) {
    return agentConfigsEqual(previous, seeded) ? previous : seeded;
  }
  const options = dedupeAgentConfigs(
    agentConfigOptions?.length ? agentConfigOptions : [agentConfig],
  );
  const selectedProvider = normalizeLaunchProvider(previous.agentProvider);
  return options.some((option) => option.agentProvider === selectedProvider)
    ? previous
    : seeded;
}

export interface AgentLaunchConfigControlsProps {
  value: SyncAgentConfig;
  options?: SyncAgentConfig[];
  onChange: (config: SyncAgentConfig) => void;
  disabled?: boolean;
  testIdPrefix: string;
  className?: string;
  selectVariant?: "native" | "themed";
  resolvePreferences?: ResolveAgentLaunchPreferences;
  onResolutionSelectionChange?: (
    selection: AgentLaunchSelectionV1 | null,
  ) => void;
  /** Grid shape: four-across (default) or 2×2 for narrow hosts such as the
   * Create Explainer modal's half-width column (shared-components D37). */
  columns?: 2 | 4;
  /**
   * Optional Account field (g5.chat.configuration), rendered between Agent
   * and Model only when `accountOptions` is supplied. The host owns the
   * value (an account id or the Runtime-variables sentinel); once shown, the
   * value travels as an explicit account preference so the resolution and
   * the confirmed selection name the exact visible combination.
   */
  accountOptions?: ThemedSelectOption[];
  account?: string;
  onAccountChange?: (account: string) => void;
  accountDisabled?: boolean;
}

export type ResolveAgentLaunchPreferences = (
  preferences: AgentLaunchPreferencesV1,
) => Promise<ResolvedAgentExecutionV1>;

export interface AgentLaunchResolutionStatusProps {
  preferences: AgentLaunchPreferencesV1;
  resolvePreferences: ResolveAgentLaunchPreferences;
  testIdPrefix: string;
  className?: string;
  onSelectionChange?: (selection: AgentLaunchSelectionV1 | null) => void;
  onResolutionChange?: (resolution: ResolvedAgentExecutionV1 | null) => void;
  onPendingChange?: (pending: boolean) => void;
  onError?: (message: string) => void;
  showError?: boolean;
}

function resolvedModeLabel(resolved: ResolvedAgentExecutionV1): string {
  const harnessMode = launchModeForHarness(resolved.provider.value);
  return resolved.permissionMode.value === harnessMode.mode
    ? harnessMode.label
    : resolved.permissionMode.value;
}

/** Resolve a launch draft and render only transient or actionable status. */
export function AgentLaunchResolutionStatus({
  preferences,
  resolvePreferences,
  testIdPrefix,
  className,
  onSelectionChange,
  onResolutionChange,
  onPendingChange,
  onError,
  showError = true,
}: AgentLaunchResolutionStatusProps): React.JSX.Element | null {
  const preferenceKey = JSON.stringify(preferences);
  const requestIdRef = useRef(0);
  const [result, setResult] = useState<{
    preferenceKey: string;
    resolution: ResolvedAgentExecutionV1 | null;
    error: string | null;
  } | null>(null);

  useEffect(() => {
    const requestId = ++requestIdRef.current;
    onPendingChange?.(true);
    onSelectionChange?.(null);
    onResolutionChange?.(null);
    const requestPreferences = JSON.parse(
      preferenceKey,
    ) as AgentLaunchPreferencesV1;
    void resolvePreferences(requestPreferences).then(
      (next) => {
        if (requestId !== requestIdRef.current) return;
        setResult({ preferenceKey, resolution: next, error: null });
        onPendingChange?.(false);
        onResolutionChange?.(next);
        onSelectionChange?.({
          preferences: requestPreferences,
          confirmedResolutionId: next.resolutionId,
        });
      },
      (reason: unknown) => {
        if (requestId !== requestIdRef.current) return;
        const message =
          reason instanceof Error
            ? reason.message
            : "The launch defaults could not be resolved.";
        setResult({
          preferenceKey,
          resolution: null,
          error: message,
        });
        onPendingChange?.(false);
        onResolutionChange?.(null);
        onSelectionChange?.(null);
        onError?.(message);
      },
    );
    return () => {
      requestIdRef.current += 1;
    };
  }, [
    onPendingChange,
    onError,
    onResolutionChange,
    onSelectionChange,
    preferenceKey,
    resolvePreferences,
  ]);

  const currentResult = result?.preferenceKey === preferenceKey ? result : null;
  const loading = currentResult === null;
  const resolution = currentResult?.resolution ?? null;
  const error = currentResult?.error ?? null;

  if (loading) {
    return (
      <div
        data-testid={`${testIdPrefix}-resolution-loading`}
        className={classNames(
          "text-[10px] leading-snug text-[var(--g5-text2)]",
          className,
        )}
      >
        Loading launch defaults…
      </div>
    );
  }

  if (error || !resolution) {
    if (!showError) return null;
    return (
      <div
        data-testid={`${testIdPrefix}-resolution-error`}
        role="alert"
        className={classNames(
          "text-[10px] leading-snug text-[var(--g5-blocked)]",
          className,
        )}
      >
        Could not load the launch defaults: {error}
      </div>
    );
  }

  if (resolution.requiresInteractiveAuth) {
    return (
      <p
        data-testid={`${testIdPrefix}-resolution-auth-warning`}
        role="status"
        className={classNames(
          "text-[10px] leading-snug text-[var(--g5-warn)]",
          className,
        )}
      >
        Sign-in will be requested in the chat when Codex needs it.
      </p>
    );
  }
  if (!resolution.credentialReady) {
    return (
      <p
        data-testid={`${testIdPrefix}-resolution-auth-warning`}
        role="status"
        className={classNames(
          "text-[10px] leading-snug text-[var(--g5-warn)]",
          className,
        )}
      >
        No account can authenticate this agent. Add one in Settings → Accounts.
      </p>
    );
  }
  return null;
}

interface LaunchSelectProps {
  label: string;
  testId: string;
  value: string;
  options: ThemedSelectOption[];
  onChange: (value: string) => void;
  disabled: boolean;
  themed: boolean;
}

// One select renderer for Agent, Model, and Effort so the three choices stay
// visually and behaviorally identical across harnesses (D37 uniformity).
function LaunchSelect({
  label,
  testId,
  value,
  options,
  onChange,
  disabled,
  themed,
}: LaunchSelectProps): React.JSX.Element {
  // The custom listbox is a SIBLING of the label, never its descendant: an
  // option button inside a wrapping <label> is removed by React during its
  // own click, after which Chromium's label activation no longer sees an
  // interactive target and re-dispatches the click onto the labelled control,
  // reopening the listbox over the field below (the Model select once the
  // Account slot moved it under Agent). `htmlFor` keeps the label association.
  const selectId = React.useId();
  return (
    <div className="flex flex-col gap-1 text-xs text-[var(--g5-text1)]">
      <label htmlFor={selectId}>
        <span>{label}</span>
      </label>
      {themed ? (
        <ThemedSelect
          id={selectId}
          value={value}
          onChange={onChange}
          disabled={disabled}
          testId={testId}
          size="sm"
          options={options}
        />
      ) : (
        <select
          id={selectId}
          value={value}
          onChange={(event) => onChange(event.target.value)}
          disabled={disabled}
          data-testid={testId}
          title={options.find((option) => option.value === value)?.label}
          className={classNames(
            "rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)]",
            "bg-[var(--g5-bg2)] px-2 py-1.5 text-xs text-[var(--g5-text0)]",
            "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]",
            "disabled:opacity-70",
          )}
        >
          {options.map((option) => (
            <option
              key={option.value || "default"}
              value={option.value}
              title={option.label}
            >
              {option.label}
            </option>
          ))}
        </select>
      )}
    </div>
  );
}

export interface AgentLaunchFieldControlsProps {
  provider: string;
  providerOptions: ThemedSelectOption[];
  model: string;
  modelOptions: ThemedSelectOption[];
  effort: string;
  effortOptions: ThemedSelectOption[];
  modeLabel: string;
  onProviderChange: (provider: string) => void;
  onModelChange: (model: string) => void;
  onEffortChange: (effort: string) => void;
  disabled?: boolean;
  agentDisabled?: boolean;
  /**
   * Optional fifth Account field (shared-components D37; right-panel
   * session_provider_controls D30): rendered between Agent and Model only when
   * `accountOptions` is supplied. Hosts without account selection omit it.
   */
  accountOptions?: ThemedSelectOption[];
  account?: string;
  onAccountChange?: (account: string) => void;
  accountDisabled?: boolean;
  accountTestId?: string;
  testIdPrefix: string;
  testId?: string;
  modeTestId?: string;
  className?: string;
  selectVariant?: "native" | "themed";
  columns?: 2 | 4;
  status?: React.ReactNode;
}

/**
 * One data-driven renderer for the four provider fields. Chat and every task
 * modal reuse this component so the harness-resolved Mode remains the same
 * read-only display before and after a Chat session is created.
 */
export function AgentLaunchFieldControls({
  provider,
  providerOptions,
  model,
  modelOptions,
  effort,
  effortOptions,
  modeLabel,
  onProviderChange,
  onModelChange,
  onEffortChange,
  disabled = false,
  agentDisabled = false,
  accountOptions,
  account = "",
  onAccountChange,
  accountDisabled = false,
  accountTestId,
  testIdPrefix,
  testId,
  modeTestId,
  className,
  selectVariant = "themed",
  columns = 4,
  status,
}: AgentLaunchFieldControlsProps): React.JSX.Element {
  const themed = selectVariant === "themed";
  return (
    <div
      data-testid={testId}
      className={classNames(
        "grid gap-3",
        // The wide layout keeps every field on one row: five columns once
        // the Account field joins Agent, Model, Effort and Mode, so Mode is
        // never left alone on a second row.
        columns === 2
          ? "sm:grid-cols-2"
          : accountOptions
            ? "sm:grid-cols-5"
            : "sm:grid-cols-4",
        className,
      )}
    >
      {status}
      <LaunchSelect
        label="Agent"
        testId={`${testIdPrefix}-agent-select`}
        value={provider}
        options={providerOptions}
        onChange={onProviderChange}
        disabled={disabled || agentDisabled}
        themed={themed}
      />
      {accountOptions ? (
        <LaunchSelect
          label="Account"
          testId={accountTestId ?? `${testIdPrefix}-account-select`}
          value={account}
          options={accountOptions}
          onChange={(value) => onAccountChange?.(value)}
          disabled={disabled || accountDisabled}
          themed={themed}
        />
      ) : null}
      <LaunchSelect
        label="Model"
        testId={`${testIdPrefix}-model-select`}
        value={model}
        options={modelOptions}
        onChange={onModelChange}
        disabled={disabled}
        themed={themed}
      />
      <LaunchSelect
        label="Effort"
        testId={`${testIdPrefix}-effort-select`}
        value={effort}
        options={effortOptions}
        onChange={onEffortChange}
        disabled={
          disabled ||
          (effortOptions.length === 1 && effortOptions[0].value === "none")
        }
        themed={themed}
      />
      <label className="flex flex-col gap-1 text-xs text-[var(--g5-text1)]">
        <span>Mode</span>
        <input
          value={modeLabel}
          readOnly
          aria-readonly="true"
          disabled={disabled}
          data-testid={modeTestId ?? `${testIdPrefix}-mode-display`}
          className={classNames(
            "rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)]",
            "bg-[var(--g5-bg2)] px-2 py-1.5 text-xs text-[var(--g5-text0)]",
          )}
        />
      </label>
    </div>
  );
}

// Launch modals offer THREE choices — Agent (harness), Model, Effort — and
// display the harness-forced mode read-only (#1370; shared-components D37;
// types/core D15/D19). Mode is never a launch choice. A host that starts a
// chat (the fork dialog, #987) adds the Account choice.
export function AgentLaunchConfigControls({
  value,
  options,
  onChange,
  disabled = false,
  testIdPrefix,
  className,
  selectVariant = "themed",
  resolvePreferences,
  onResolutionSelectionChange,
  columns = 4,
  accountOptions,
  account,
  onAccountChange,
  accountDisabled = false,
}: AgentLaunchConfigControlsProps): React.JSX.Element {
  const normalizedOptions = useMemo(
    () => dedupeAgentConfigs(options?.length ? options : [value]),
    [options, value],
  );
  // Direct launch fields are the exact confirmation contract (agent UX D7):
  // even an untouched settings-derived draft resolves the values on screen as
  // explicit, so background Runtime Config resolution cannot replace Haiku
  // with Opus or one selected harness with another.
  const selectedValue = completeAgentConfig({
    ...value,
    launchSource: "explicit",
  });
  const selectedConfig =
    normalizedOptions.find(
      (option) => option.agentProvider === selectedValue.agentProvider,
    ) ?? selectedValue;
  const [resolution, setResolution] = useState<ResolvedAgentExecutionV1 | null>(
    null,
  );
  const [resolutionPending, setResolutionPending] = useState(
    Boolean(resolvePreferences),
  );
  const resolvedProvider =
    resolution?.provider.value ?? selectedValue.agentProvider;
  const resolvedOption =
    normalizedOptions.find(
      (option) => option.agentProvider === resolvedProvider,
    ) ?? selectedConfig;
  const canonical = catalogDefaultChoiceFor(resolvedProvider);
  const resolvedModel =
    resolution?.model.value ?? selectedValue.model ?? canonical.model;
  const resolvedEffort =
    resolution?.effort.value ??
    selectedValue.effort ??
    effortSelectionForModel(resolvedProvider, resolvedModel).defaultEffort;
  const resolvedPermissionMode =
    resolution?.permissionMode.value ??
    launchModeForHarness(resolvedProvider).mode;
  const resolvedConfig = completeAgentConfig({
    ...resolvedOption,
    agentProvider: resolvedProvider,
    launchSource: "explicit",
    model: resolvedModel,
    modelLabel: launchModelLabelFor(resolvedProvider, resolvedModel),
    permissionMode: resolvedPermissionMode,
    permissionLabel: resolution
      ? resolvedModeLabel(resolution)
      : launchModeForHarness(resolvedProvider).label,
    effort: resolvedEffort,
  });
  const directModelOptions = modelOptionsFor(resolvedConfig).filter(
    (option) => option.value !== "",
  );
  const controlsDisabled =
    disabled || (!!resolvePreferences && resolutionPending);
  // A shown Account field is part of the confirmation contract: its value is
  // resolved and confirmed with the other choices, never left to the
  // resolver's effective assignment.
  const accountSelection =
    accountOptions && account ? accountSelectionFromValue(account) : undefined;

  return (
    <AgentLaunchFieldControls
      provider={resolvedProvider}
      providerOptions={normalizedOptions.map((option) => ({
        value: option.agentProvider,
        label: agentLabel(option.agentProvider),
      }))}
      model={resolvedModel}
      modelOptions={directModelOptions}
      effort={resolvedEffort}
      effortOptions={effortOptionsFor(resolvedConfig)}
      modeLabel={resolvedConfig.permissionLabel ?? ""}
      onProviderChange={(nextValue) => {
        const provider = nextValue as ChatAgentProvider;
        const next =
          normalizedOptions.find(
            (option) => option.agentProvider === provider,
          ) ?? resolvedOption;
        const nextCanonical = catalogDefaultChoiceFor(provider);
        onChange(
          completeAgentConfig({
            ...next,
            launchSource: "explicit",
            model: next.model ?? nextCanonical.model,
            modelLabel: launchModelLabelFor(
              provider,
              next.model ?? nextCanonical.model,
            ),
            effort: next.effort ?? nextCanonical.effort,
          }),
        );
      }}
      onModelChange={(model) => {
        onChange(
          completeAgentConfig({
            ...resolvedConfig,
            launchSource: "explicit",
            model,
            modelLabel: modelLabelFor(resolvedConfig, model),
          }),
        );
      }}
      onEffortChange={(effort) => {
        onChange(
          completeAgentConfig({
            ...resolvedConfig,
            launchSource: "explicit",
            effort,
          }),
        );
      }}
      disabled={controlsDisabled}
      agentDisabled={normalizedOptions.length <= 1}
      accountOptions={accountOptions}
      account={account}
      onAccountChange={onAccountChange}
      accountDisabled={accountDisabled}
      testIdPrefix={testIdPrefix}
      className={className}
      selectVariant={selectVariant}
      columns={columns}
      status={
        resolvePreferences ? (
          <AgentLaunchResolutionStatus
            preferences={agentLaunchPreferencesForConfig(selectedValue, {
              account: accountSelection,
            })}
            resolvePreferences={resolvePreferences}
            testIdPrefix={testIdPrefix}
            className="sm:col-span-full"
            onSelectionChange={onResolutionSelectionChange}
            onResolutionChange={setResolution}
            onPendingChange={setResolutionPending}
          />
        ) : null
      }
    />
  );
}
