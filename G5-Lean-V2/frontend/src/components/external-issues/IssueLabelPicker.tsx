// =============================================================================
// Label picker — select labels from one issue domain's catalog
// =============================================================================
// The one picker for issue creation and the issue page
// (g5.issue.labels.surfaces): the selection as coloured chips, a search box
// and the domain's catalog as a checkable list with colour swatches. Only
// catalog labels can be selected: a typed name the catalog lacks is never
// added (labels are created in the catalog editor, which "Manage labels"
// opens). Names compare without regard to case. A selected name the catalog
// no longer has (or did not list) stays shown so it can be removed, never
// re-added.
//
// The picker is bound to the domain it was opened for: callers key it, and
// the selection it reports, by that domain (and issue), so a change of
// default domain or a navigation never carries a selection elsewhere.
// =============================================================================

import { useId, useState } from "react";
import { Tags } from "lucide-react";

import { Button, classNames } from "../ui/UI";
import { useIssueLabelCatalog } from "../../hooks/useExternalIssues";
import { describeIssueRequestError } from "../../lib/externalIssueErrors";
import { findLabel, hasLabelName, sameLabelName } from "../../lib/issueLabels";
import type { ExternalIssueLabelDTO } from "../../lib/types";
import { IssueErrorNotice, IssueLabels } from "./ExternalIssueParts";
import {
  IssueLabelCatalogDialog,
  LabelSwatch,
  type IssueLabelDomain,
} from "./IssueLabelCatalogEditor";

export interface IssueLabelPickerProps {
  orgId: string;
  domain: IssueLabelDomain;
  /** Selected label names. */
  selected: string[];
  onChange: (names: string[]) => void;
  /** Labels the issue carries, to show selected names the catalog lacks. */
  known?: ExternalIssueLabelDTO[];
  /** Most labels that may be selected. */
  limit?: number;
  disabled?: boolean;
  /** Why the labels cannot be changed now, shown while `disabled`. */
  disabledReason?: string | null;
  /** The person may change the catalog (members with write permission). */
  canManage: boolean;
  /** Shown beside the legend, e.g. "(optional, at most 20)". */
  hint?: string;
}

export function IssueLabelPicker({
  orgId,
  domain,
  selected,
  onChange,
  known = [],
  limit,
  disabled = false,
  disabledReason = null,
  canManage,
  hint,
}: IssueLabelPickerProps) {
  const searchId = useId();
  const listId = useId();
  const catalog = useIssueLabelCatalog(orgId, domain.domainId);
  const [query, setQuery] = useState("");
  const [managing, setManaging] = useState(false);

  const labels = catalog.status === "ok" ? catalog.data.labels : [];
  const displayKey =
    catalog.status === "ok" && catalog.data.displayKey
      ? catalog.data.displayKey
      : domain.displayKey;
  const needle = query.trim().toLowerCase();
  const matches = needle
    ? labels.filter(
        (label) =>
          label.name.toLowerCase().includes(needle) ||
          (label.description ?? "").toLowerCase().includes(needle),
      )
    : labels;
  const exact = needle ? findLabel(labels, query.trim()) : undefined;
  const full = limit !== undefined && selected.length >= limit;

  // Selected names with the catalog's colour, else the issue's.
  const chips: ExternalIssueLabelDTO[] = selected.map(
    (name) =>
      findLabel(labels, name) ??
      findLabel(known, name) ?? { name, color: null },
  );

  const toggle = (label: ExternalIssueLabelDTO) => {
    if (hasLabelName(selected, label.name)) {
      onChange(selected.filter((name) => !sameLabelName(name, label.name)));
    } else if (!full) {
      onChange([...selected, label.name]);
    }
  };

  return (
    <fieldset data-testid="label-picker" disabled={disabled}>
      <legend className="mb-1 text-sm font-medium text-[var(--g5-text1)]">
        Labels{" "}
        {hint && (
          <span className="font-normal text-[var(--g5-text2)]">{hint}</span>
        )}
      </legend>
      <div className="mb-1.5 min-h-[1.25rem]">
        {chips.length > 0 ? (
          <IssueLabels
            labels={chips}
            testId="label-picker-selected"
            onRemove={(name) =>
              onChange(selected.filter((value) => !sameLabelName(value, name)))
            }
            removeDisabled={disabled}
          />
        ) : (
          <p className="text-xs text-[var(--g5-text2)]">No labels selected.</p>
        )}
      </div>
      {disabled && disabledReason && (
        <p
          data-testid="label-picker-disabled"
          className="mb-1.5 text-xs text-[var(--g5-text1)]"
        >
          {disabledReason}
        </p>
      )}
      <div className="rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg0)]">
        <div className="flex items-center gap-2 border-b border-[var(--g5-stroke)] px-2 py-1.5">
          <label htmlFor={searchId} className="sr-only">
            Filter labels
          </label>
          <input
            id={searchId}
            type="search"
            data-testid="label-picker-search"
            aria-controls={listId}
            placeholder="Filter labels"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={(event) => {
              // Enter picks the label named exactly, never a new name, and
              // never submits the surrounding form.
              if (event.key !== "Enter") return;
              event.preventDefault();
              if (exact) toggle(exact);
            }}
            className="min-w-0 flex-1 rounded bg-transparent px-1 text-sm text-[var(--g5-text0)] placeholder:text-[var(--g5-text2)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]"
          />
          <Button
            kind="ghost"
            data-testid="label-picker-manage"
            onClick={() => setManaging(true)}
            className="flex-none px-2 py-0.5 text-xs"
          >
            <Tags aria-hidden="true" className="h-3.5 w-3.5" />
            {canManage ? "Manage labels" : "View labels"}
          </Button>
        </div>
        {catalog.status === "loading" && (
          <p role="status" className="px-3 py-2 text-xs text-[var(--g5-text2)]">
            Loading labels…
          </p>
        )}
        {catalog.status === "error" && (
          <div className="p-2">
            <IssueErrorNotice
              compact
              testId="label-picker-error"
              presentation={describeIssueRequestError(catalog.error)}
              onRetry={catalog.refetch}
            />
          </div>
        )}
        {catalog.status === "ok" && labels.length === 0 && (
          <p
            data-testid="label-picker-empty"
            className="px-3 py-2 text-xs text-[var(--g5-text2)]"
          >
            {displayKey} has no labels yet.{" "}
            {canManage
              ? "Create them in Manage labels."
              : "A member with write permission can create them."}
          </p>
        )}
        {catalog.status === "ok" && labels.length > 0 && (
          <ul
            id={listId}
            data-testid="label-picker-options"
            aria-label={`Labels in ${displayKey}`}
            className="max-h-48 space-y-0.5 overflow-y-auto px-2 py-1.5"
          >
            {matches.map((label) => {
              const checked = hasLabelName(selected, label.name);
              return (
                <li key={label.id ?? label.name}>
                  <label
                    className={classNames(
                      "flex cursor-pointer items-start gap-2 rounded px-1 py-0.5 text-sm text-[var(--g5-text0)] hover:bg-[var(--g5-bg2)]",
                      !checked && full && "cursor-not-allowed opacity-60",
                    )}
                  >
                    <input
                      type="checkbox"
                      data-testid={`label-picker-option-${label.name}`}
                      checked={checked}
                      disabled={!checked && full}
                      onChange={() => toggle(label)}
                      className="mt-0.5 h-4 w-4 flex-none accent-[var(--g5-info)]"
                    />
                    <LabelSwatch color={label.color} className="mt-1" />
                    <span className="min-w-0">
                      <span className="block">{label.name}</span>
                      {label.description && (
                        <span className="block text-xs text-[var(--g5-text2)]">
                          {label.description}
                        </span>
                      )}
                    </span>
                  </label>
                </li>
              );
            })}
            {matches.length === 0 && (
              <li
                data-testid="label-picker-no-match"
                className="px-1 py-0.5 text-xs text-[var(--g5-text2)]"
              >
                No label in {displayKey} matches “{query.trim()}”. Only existing
                labels can be selected
                {canManage ? "; create new ones in Manage labels." : "."}
              </li>
            )}
          </ul>
        )}
      </div>
      {catalog.status === "ok" && catalog.data.truncated && (
        <p
          data-testid="label-picker-truncated"
          className="mt-1 text-xs text-[var(--g5-text2)]"
        >
          Only the first {labels.length} labels of {displayKey} are listed.
        </p>
      )}
      {full && (
        <p
          data-testid="label-picker-full"
          className="mt-1 text-xs text-[var(--g5-text2)]"
        >
          At most {limit} labels can be selected.
        </p>
      )}
      {managing && (
        <IssueLabelCatalogDialog
          orgId={orgId}
          domain={{ ...domain, displayKey }}
          canEdit={canManage}
          onClose={() => setManaging(false)}
        />
      )}
    </fieldset>
  );
}
