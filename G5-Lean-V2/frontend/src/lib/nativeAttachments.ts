// =============================================================================
// G5 issue image attachments (g5.issue.native.attachments)
// =============================================================================
// A G5 issue's images are uploaded to it and shown in its text by their
// address under the issue's page (`/<org>/issues/g5/<n>/attachments/<id>`).
// That address is never loaded as an image: readers fetch the bytes through
// the authenticated API route. The server recognizes the type from the
// content and enforces the bounds; the checks here only explain a refusal
// before anything is sent.
// =============================================================================

import { ApiError } from "./api";
import { describeIssueWriteError } from "./issueCommentErrors";

/** The image types a G5 issue takes. */
export const NATIVE_ATTACHMENT_TYPES: Record<string, string> = {
  "image/png": "png",
  "image/jpeg": "jpg",
  "image/gif": "gif",
  "image/webp": "webp",
};

/** The file chooser's `accept`. */
export const NATIVE_ATTACHMENT_ACCEPT =
  "image/png,image/jpeg,image/gif,image/webp,.png,.jpg,.jpeg,.gif,.webp";

/** Largest image a G5 issue takes (10 MiB). */
export const MAX_NATIVE_ATTACHMENT_BYTES = 10 * 1024 * 1024;

const IMAGE_EXTENSION = /\.(png|jpe?g|gif|webp)$/i;

/** Why `file` cannot be attached, or `null` when it may be sent. */
export function imageFileRefusal(file: File): string | null {
  const name = file.name || "The file";
  const typed = file.type
    ? file.type in NATIVE_ATTACHMENT_TYPES
    : IMAGE_EXTENSION.test(file.name);
  if (!typed) return `${name} is not a PNG, JPEG, GIF or WebP image.`;
  if (file.size === 0) return `${name} is empty.`;
  if (file.size > MAX_NATIVE_ATTACHMENT_BYTES) {
    return `${name} is larger than 10 MiB.`;
  }
  return null;
}

/** A name for a file the clipboard handed over unnamed. */
export function attachmentFileName(file: File): string {
  if (file.name.trim()) return file.name.trim();
  return `pasted-image.${NATIVE_ATTACHMENT_TYPES[file.type] ?? "png"}`;
}

/** Why an upload was refused, naming the file. */
export function describeUploadError(error: unknown, fileName: string): string {
  if (error instanceof ApiError) {
    if (error.details.code === "attachment_limit") {
      return `${fileName} was not added: this issue already holds 50 attachments.`;
    }
    if (error.status === 422 && typeof error.details.error === "string") {
      return `${fileName} was not added: ${error.details.error}.`;
    }
  }
  return `${fileName} was not added. ${describeIssueWriteError(
    error,
    "The upload failed.",
  )}`;
}

/** Raw base64 of a file (no `data:` prefix), the upload's wire form. */
export function readFileAsBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const result = String(reader.result ?? "");
      const comma = result.indexOf(",");
      resolve(comma >= 0 ? result.slice(comma + 1) : result);
    };
    reader.onerror = () =>
      reject(reader.error ?? new Error("The file could not be read."));
    reader.readAsDataURL(file);
  });
}

/** `1.2 MB`-style size for the attachments list. */
export function formatAttachmentSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/** An attachment as its address names it. */
export interface NativeAttachmentRef {
  orgSlug: string;
  number: number;
  attachmentId: string;
}

const ATTACHMENT_PATH = /^\/([^/]+)\/issues\/g5\/(\d+)\/attachments\/(\d+)$/;

/**
 * The attachment an image source names when it is a G5 attachment address
 * of this deployment: a path, or absolute at one of `origins` (the
 * deployment's public address and this page's). `null` for any other
 * image. The caller checks the organization.
 */
export function parseNativeAttachmentUrl(
  source: string,
  origins: readonly string[],
): NativeAttachmentRef | null {
  let path: string;
  if (source.startsWith("/") && !source.startsWith("//")) {
    path = source.split(/[?#]/, 1)[0];
  } else {
    let url: URL;
    try {
      url = new URL(source);
    } catch {
      return null;
    }
    if (!origins.includes(url.origin)) return null;
    path = url.pathname;
  }
  const match = ATTACHMENT_PATH.exec(path);
  if (!match) return null;
  return {
    orgSlug: match[1],
    number: Number(match[2]),
    attachmentId: match[3],
  };
}

/** The origin of an absolute URL, or `null`. */
export function originOf(url: string | null | undefined): string | null {
  if (!url) return null;
  try {
    return new URL(url).origin;
  } catch {
    return null;
  }
}

/**
 * `text` with `snippet` inserted on its own line in place of the selection
 * `start`–`end`, and the caret after it.
 */
export function insertOnOwnLine(
  text: string,
  start: number,
  end: number,
  snippet: string,
): { text: string; caret: number } {
  const before = text.slice(0, start);
  const after = text.slice(end);
  const lead = before === "" || before.endsWith("\n") ? "" : "\n";
  const trail = after.startsWith("\n") ? "" : "\n";
  const inserted = `${lead}${snippet}${trail}`;
  return {
    text: `${before}${inserted}${after}`,
    caret: before.length + inserted.length,
  };
}

/**
 * Insert `snippet` on its own line at the text box's caret, after any
 * selection, which it keeps, and put the caret after it. Returns the new
 * text. A box the person has not `placed` the caret in, or one not shown,
 * takes it at the end: its caret is only the browser's default, the start.
 */
export function insertAtCaret(
  textarea: HTMLTextAreaElement | null,
  value: string,
  snippet: string,
  onChange: (value: string) => void,
  placed = true,
): string {
  const caret = placed ? textarea?.selectionEnd : undefined;
  const at = Math.min(caret ?? value.length, value.length);
  const next = insertOnOwnLine(value, at, at, snippet);
  onChange(next.text);
  requestAnimationFrame(() => {
    if (textarea && document.activeElement === textarea) {
      textarea.setSelectionRange(next.caret, next.caret);
    }
  });
  return next.text;
}

/** Markdown image alt text G5 writes for a file name. */
function altText(fileName: string): string {
  return fileName.replace(/[[\]\r\n]/g, "");
}

/**
 * The creation form's stand-in for an image that is uploaded once the issue
 * exists; unique per image, and visible in the text until then.
 */
export function pendingImagePlaceholder(id: string, fileName: string): string {
  return `![Uploading ${altText(fileName)}…](pending-image:${id})`;
}

/** What replaces a placeholder whose image could not be added. */
export function imageNotAddedNote(fileName: string): string {
  return `_(Image ${altText(fileName)} was not added.)_`;
}
