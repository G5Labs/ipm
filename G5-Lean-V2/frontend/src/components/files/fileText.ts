// =============================================================================
// G5 Files — fileText
// =============================================================================
// Where a FILES read opens (`g5.documents.viewers`): the PDF viewer, the
// image viewer, the text editor (Markdown rendered, `g5.documents.editor.modes`)
// or the Download card.
// =============================================================================

import type { WorkspaceFileContent } from "../../hooks/useFiles";

/** Matches shared server read admission; previews may defer expensive rendering. */
export const MAX_EDITABLE_TEXT_BYTES = 32 * 1024 * 1024;

/** Matches workspace_review_service::MAX_FILE_SUGGESTION_TEXT_BYTES. */
export const MAX_FILE_SUGGESTION_TEXT_BYTES = 2 * 1024 * 1024;

export function fileSuggestionFits(content: string): boolean {
  return (
    new TextEncoder().encode(content).length <= MAX_FILE_SUGGESTION_TEXT_BYTES
  );
}

export type FileTextEligibility = "text" | "binary" | "too-large";

/**
 * Whether a read can open in the editor: text means valid UTF-8 without a
 * NUL byte, whichever source served it. The read routes label any
 * UTF-8-decodable bytes as text, so the NUL check is made here.
 */
export function fileTextEligibility(
  file: Pick<WorkspaceFileContent, "content" | "encoding" | "size">,
): FileTextEligibility {
  if (file.encoding !== "utf-8" || file.content.includes("\u0000")) {
    return "binary";
  }
  if (file.size > MAX_EDITABLE_TEXT_BYTES) return "too-large";
  return "text";
}

/** The lower-cased extension of a path, or "" when its name has none. */
export function fileExtension(path: string): string {
  const name = path.split("/").pop() ?? "";
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : "";
}

const MARKDOWN_EXTENSIONS = new Set(["md", "markdown", "mdown", "mkd", "mkdn"]);

export function isMarkdownPath(path: string): boolean {
  return MARKDOWN_EXTENSIONS.has(fileExtension(path));
}

const HTML_EXTENSIONS = new Set(["html", "htm", "xhtml", "shtml"]);

/** HTML preview routing and its size-deferral exception share this rule. */
export function isHtmlPath(path: string): boolean {
  return HTML_EXTENSIONS.has(fileExtension(path));
}

/**
 * The raster formats a browser decodes in an `<img>`, by extension, and
 * SVG, which renders in an image context only
 * (`g5.documents.viewers.images`). Markdown images and the standalone
 * viewer share this table; HEIC/HEIF, TIFF, PSD, RAW and JPEG XL are
 * deliberately absent and keep the Download card.
 */
export const IMAGE_MIME_TYPES: Readonly<Record<string, string>> = {
  png: "image/png",
  apng: "image/apng",
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  jfif: "image/jpeg",
  gif: "image/gif",
  webp: "image/webp",
  avif: "image/avif",
  bmp: "image/bmp",
  ico: "image/x-icon",
  svg: "image/svg+xml",
};

export function imageMimeForPath(path: string): string | null {
  const extension = fileExtension(path);
  // An own key only: `x.constructor` is not an image.
  return Object.hasOwn(IMAGE_MIME_TYPES, extension)
    ? IMAGE_MIME_TYPES[extension]
    : null;
}

export function isSvgPath(path: string): boolean {
  return fileExtension(path) === "svg";
}

export function isPdfPath(path: string): boolean {
  return fileExtension(path) === "pdf";
}

export type FileViewerKind = "pdf" | "image" | FileTextEligibility;

/**
 * Where a read opens (`g5.documents.viewers`). The extension alone sends a
 * PDF to the PDF viewer and a raster image to the image viewer, whatever
 * their bytes hold, so a corrupt or misnamed file lands on the viewer's
 * failure card with Download; an SVG that is text keeps the text view, with
 * its image as the rendered mode; everything else follows text eligibility.
 */
export function fileViewerKind(
  path: string,
  file: Pick<WorkspaceFileContent, "content" | "encoding" | "size">,
): FileViewerKind {
  if (isPdfPath(path)) return "pdf";
  const eligibility = fileTextEligibility(file);
  if (imageMimeForPath(path) && !(isSvgPath(path) && eligibility === "text"))
    return "image";
  return eligibility;
}
