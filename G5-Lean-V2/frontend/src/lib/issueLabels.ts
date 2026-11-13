// =============================================================================
// Issue labels (g5.issue.labels)
// =============================================================================
// Limits, name matching, deltas and refusal messages shared by the label
// picker, the catalog editor, issue creation and the issue page. Labels are
// named by name and matched without regard to case; an issue's labels change
// by deltas (names to add and to remove), never by a replacement set, so a
// concurrent change to another label survives (g5.issue.labels.contract).
// The server validates everything again.
// =============================================================================

import { ApiError } from "./api";
import { describeIssueWriteError } from "./issueCommentErrors";
import type { ExternalIssueLabelDTO } from "./types";

/** Longest label name, as on GitHub. */
export const MAX_LABEL_NAME_CHARS = 50;
/** Longest label description, as on GitHub. */
export const MAX_LABEL_DESCRIPTION_CHARS = 100;
/** Most labels one change names (added and removed), or one creation selects. */
export const MAX_LABELS_PER_CHANGE = 20;
/** Most catalog pages the picker reads (100 labels each). */
export const MAX_LABEL_CATALOG_PAGES = 10;

const HEX_COLOR_RE = /^#?[0-9a-f]{6}$/i;

export function sameLabelName(left: string, right: string): boolean {
  return left.toLowerCase() === right.toLowerCase();
}

export function hasLabelName(names: readonly string[], name: string): boolean {
  return names.some((candidate) => sameLabelName(candidate, name));
}

/** The catalog label a name names, without regard to case. */
export function findLabel(
  labels: readonly ExternalIssueLabelDTO[],
  name: string,
): ExternalIssueLabelDTO | undefined {
  return labels.find((label) => sameLabelName(label.name, name));
}

/**
 * The change from `current` to `selected`: names to add and to remove,
 * compared without regard to case.
 */
export function labelDelta(
  current: readonly string[],
  selected: readonly string[],
): { addLabels: string[]; removeLabels: string[] } {
  return {
    addLabels: selected.filter((name) => !hasLabelName(current, name)),
    removeLabels: current.filter((name) => !hasLabelName(selected, name)),
  };
}

/** A colour as the API takes it, or `null` when it is not six hex digits. */
export function normalizeLabelColor(value: string): string | null {
  const trimmed = value.trim();
  return HEX_COLOR_RE.test(trimmed)
    ? trimmed.replace(/^#/, "").toLowerCase()
    : null;
}

/** A CSS colour for a label's swatch, or `null` for none. */
export function labelSwatchColor(color: string | null | undefined) {
  const normalized = color ? normalizeLabelColor(color) : null;
  return normalized ? `#${normalized}` : null;
}

/** Why a label name cannot be saved, or `null` when it can. */
export function labelNameProblem(name: string): string | null {
  const trimmed = name.trim();
  if (trimmed === "") return "Enter a name.";
  if (/[\r\n]/.test(trimmed)) return "A label name is one line.";
  if ([...trimmed].length > MAX_LABEL_NAME_CHARS) {
    return `Label names are at most ${MAX_LABEL_NAME_CHARS} characters.`;
  }
  return null;
}

/** Why a label description cannot be saved, or `null` when it can. */
export function labelDescriptionProblem(description: string): string | null {
  return [...description.trim()].length > MAX_LABEL_DESCRIPTION_CHARS
    ? `Descriptions are at most ${MAX_LABEL_DESCRIPTION_CHARS} characters.`
    : null;
}

/** The existing label a `label_exists` refusal carries. */
export function existingLabelOf(error: unknown): ExternalIssueLabelDTO | null {
  if (
    !(error instanceof ApiError) ||
    error.status !== 409 ||
    error.details.code !== "label_exists"
  ) {
    return null;
  }
  const label = error.details.label as ExternalIssueLabelDTO | undefined;
  return label && typeof label.name === "string" ? label : null;
}

/** The server's validation text as a sentence; `upTo` cuts it there. */
function detailText(error: ApiError, upTo?: string): string | null {
  const raw = error.details.error;
  if (typeof raw !== "string") return null;
  const detail = (upTo ? raw.split(upTo)[0] : raw).trim();
  if (!detail) return null;
  return `${detail.charAt(0).toUpperCase()}${detail.slice(1)}${/[.!?]$/.test(detail) ? "" : "."}`;
}

/** The secret kinds a refusal names; never the matched text. */
function secretKinds(error: ApiError): string {
  return Array.isArray(error.details.kinds) && error.details.kinds.length > 0
    ? (error.details.kinds as string[]).join(", ")
    : "a secret";
}

/**
 * Why a label write (a catalog create, edit or delete, or an issue's label
 * change) was refused, from the API's typed error.
 */
export function describeLabelError(error: unknown, fallback: string): string {
  if (!(error instanceof ApiError)) return fallback;
  switch (error.details.code) {
    case "label_exists": {
      const existing = existingLabelOf(error);
      return existing
        ? `A label named “${existing.name}” already exists. Choose another name, or edit that label.`
        : "A label with this name already exists.";
    }
    case "unknown_label":
      // The UI names its own way to add a label after the server's fact.
      return `${detailText(error, ";") ?? "A label is not in this tracker's labels."} Only existing labels can be added; create labels in Manage labels.`;
    case "invalid_label":
    case "invalid_color":
    case "duplicate_label":
    case "label_conflict":
    case "too_many_labels":
    case "label_scan_limit":
      return detailText(error) ?? fallback;
    case "no_change":
      return "Nothing to change: no label was added or removed.";
    case "secret_detected": {
      const field =
        error.details.field === "description" ? "description" : "name";
      return `The label ${field} appears to contain a secret (${secretKinds(error)}). Remove it before saving.`;
    }
  }
  if (error.status === 404) {
    return "This label no longer exists. Reload the labels to see the current catalog.";
  }
  if (error.status === 403) {
    return "You do not have permission to change labels here.";
  }
  return describeIssueWriteError(error, fallback);
}
