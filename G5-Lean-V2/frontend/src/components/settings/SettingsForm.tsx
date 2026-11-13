// =============================================================================
// G5 Settings Panel — SettingsForm
// =============================================================================
// Workspace-level VM lifecycle and agent-permission form. Tracks dirty state
// and enables Save only when changes exist.
// =============================================================================

import React, { useCallback, useEffect, useMemo, useState } from "react";
import { Loader2 } from "lucide-react";
import { Button, classNames } from "../ui/UI";
import type { WorkspaceSettings } from "../../lib/types";

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface SettingsFormProps {
  /** Current workspace settings */
  settings: WorkspaceSettings;
  /** Save handler — receives partial update */
  onSave: (patch: Partial<WorkspaceSettings>) => Promise<void>;
  /** Whether a save is in progress */
  saving: boolean;
  /** Validation errors keyed by field name */
  errors: Record<string, string>;
}

// ---------------------------------------------------------------------------
// Helper: input field with label and optional error
// ---------------------------------------------------------------------------

function FormField({
  label,
  error,
  children,
}: {
  label: string;
  error?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex flex-col gap-1">
      <label className="text-xs font-medium text-[var(--g5-text1)]">
        {label}
      </label>
      {children}
      {error && (
        <span className="text-xs text-[var(--g5-blocked)]">{error}</span>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// SettingsForm Component
// ---------------------------------------------------------------------------

export const SettingsForm: React.FC<SettingsFormProps> = ({
  settings,
  onSave,
  saving,
  errors,
}) => {
  // Local form state
  const [formState, setFormState] = useState<Partial<WorkspaceSettings>>({});

  // Reset form state when settings change from server
  useEffect(() => {
    setFormState({});
  }, [settings]);

  // Merged view: form state overlaid on server settings
  const merged = useMemo(
    () => ({ ...settings, ...formState }),
    [settings, formState],
  );

  // Dirty check
  const dirty = useMemo(() => {
    return Object.keys(formState).some((key) => {
      const k = key as keyof WorkspaceSettings;
      return formState[k] !== settings[k];
    });
  }, [formState, settings]);

  // Field updater
  const setField = useCallback(
    <K extends keyof WorkspaceSettings>(
      key: K,
      value: WorkspaceSettings[K],
    ) => {
      setFormState((prev) => ({ ...prev, [key]: value }));
    },
    [],
  );

  // Handle save
  const handleSave = useCallback(async () => {
    if (!dirty || saving) return;
    await onSave(formState);
  }, [dirty, saving, formState, onSave]);

  // Input class helper
  const inputCls = (fieldName: string) =>
    classNames(
      "w-full px-3 py-2 text-sm",
      "rounded-[var(--g5-radius-sm)]",
      "bg-[var(--g5-bg2)] text-[var(--g5-text0)]",
      "border",
      errors[fieldName]
        ? "border-[var(--g5-blocked)]"
        : "border-[var(--g5-stroke)]",
      "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]",
    );

  return (
    <div className="flex flex-col gap-4">
      <FormField
        label="Workspace VM idle timeout (minutes)"
        error={errors.idleTimeoutSecs}
      >
        <input
          type="number"
          min={1}
          max={1440}
          className={inputCls("idleTimeoutSecs")}
          value={Math.round(merged.idleTimeoutSecs / 60)}
          onChange={(e) =>
            setField(
              "idleTimeoutSecs",
              Math.min(1440, Math.max(1, parseInt(e.target.value) || 1)) * 60,
            )
          }
          aria-label="Workspace VM idle timeout in minutes"
        />
      </FormField>

      <div className="flex items-start gap-2">
        <input
          id="settings-agent-other-workspaces"
          type="checkbox"
          checked={merged.agentMayModifyOtherWorkspaces}
          data-testid="settings-agent-other-workspaces"
          aria-describedby="settings-agent-other-workspaces-help"
          className="mt-0.5 h-3.5 w-3.5 flex-shrink-0"
          style={{ accentColor: "var(--g5-info)" }}
          onChange={(e) =>
            setField("agentMayModifyOtherWorkspaces", e.target.checked)
          }
        />
        <div className="flex flex-col gap-0.5">
          <label
            htmlFor="settings-agent-other-workspaces"
            className="text-xs font-medium text-[var(--g5-text1)]"
          >
            Agent may modify other workspaces
          </label>
          <span
            id="settings-agent-other-workspaces-help"
            className="text-xs text-[var(--g5-text2)]"
          >
            On by default. Lets this workspace&apos;s agent fork child
            workspaces and start, stop, chat in, delete or publish pull requests
            from them. Forks you own keep this setting; transferring the
            workspace turns it off.
          </span>
        </div>
      </div>

      {/* Save button */}
      <div className="flex items-center justify-between pt-2">
        <Button
          data-testid="settings-form-save"
          kind="primary"
          disabled={!dirty || saving}
          onClick={handleSave}
        >
          {saving ? (
            <>
              <Loader2 className="w-4 h-4 animate-spin" />
              Saving…
            </>
          ) : (
            "Save"
          )}
        </Button>
        {settings.updatedBy && (
          <span className="text-xs text-[var(--g5-text2)]">
            Last updated by {settings.updatedBy} at{" "}
            {new Date(settings.updatedAt).toLocaleString()}
          </span>
        )}
      </div>
    </div>
  );
};
