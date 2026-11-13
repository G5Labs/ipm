// =============================================================================
// G5 editor — text measure
// =============================================================================
// Editors read and edit text in a column at most 8 inches wide and wrap there
// (`g5.documents.text-measure`). Rendered Markdown and Rich text take the
// measure from the `--g5-text-measure` CSS variable; plain text (Monaco)
// wraps at the column count its font gives for the same width. HTML and
// JSONL keep unwrapped lines.
// =============================================================================

import { isHtmlPath } from "../files/fileText";

/** The measure in CSS pixels (96 per inch); keep `--g5-text-measure` in step. */
export const TEXT_MEASURE_PX = 8 * 96;

const UNWRAPPED = new Set(["html", "htm", "jsonl"]);

/** Whether this file's plain text wraps at the measure. */
export function wrapsAtTextMeasure(
  language: string,
  filePath: string,
): boolean {
  if (isHtmlPath(filePath) || UNWRAPPED.has(language.toLowerCase()))
    return false;
  const name = filePath.split("/").pop() ?? "";
  const dot = name.lastIndexOf(".");
  return !(dot > 0 && UNWRAPPED.has(name.slice(dot + 1).toLowerCase()));
}

/**
 * How many typical characters of a font fit in the measure; null when the
 * width is not known yet. Never fewer than one, and never more than fit, so
 * the wrapped text stays within the measure at any font size.
 */
export function textMeasureColumns(
  typicalCharacterWidth: number | undefined,
): number | null {
  if (!typicalCharacterWidth || !(typicalCharacterWidth > 0)) return null;
  return Math.max(1, Math.floor(TEXT_MEASURE_PX / typicalCharacterWidth));
}
