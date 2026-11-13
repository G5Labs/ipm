// =============================================================================
// G5 issue image attachments in the browser (g5.issue.native.attachments)
// =============================================================================
// Text shows an attachment by its address under the issue's page; that
// address is never loaded as an image. A G5 issue's rendered Markdown hands
// such images to `NativeAttachmentImage`, which reads the bytes through the
// authenticated client for the route's organization and shows them through
// an object URL, or says the attachment was removed. Composers on a G5 issue
// take images by paste, drop or the Attach image button, upload each under
// its own idempotency key (a retry of a failed upload reuses it) and insert
// the returned Markdown on its own line at the caret, or at the end of a box
// the person has not clicked or typed in. A description takes no image
// chosen with the button before then: it is only attached to the issue. The
// issue page lists the attachments; their uploader or an administrator
// removes one.
// =============================================================================

import {
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type ClipboardEvent,
  type DragEvent,
  type ReactNode,
  type RefObject,
} from "react";
import { ImagePlus, LoaderCircle } from "lucide-react";

import { Button } from "../ui/UI";
import { useRouteOrgMembership } from "../../hooks/useExternalIssues";
import {
  useNativeAttachmentImage,
  useNativeIssueAttachments,
  useRemoveNativeAttachment,
  useUploadNativeAttachment,
} from "../../hooks/useNativeAttachments";
import { describeIssueWriteError } from "../../lib/issueCommentErrors";
import { issueUserLabel } from "../../lib/nativeIssues";
import {
  NATIVE_ATTACHMENT_ACCEPT,
  attachmentFileName,
  describeUploadError,
  formatAttachmentSize,
  imageFileRefusal,
  insertAtCaret,
  pendingImagePlaceholder,
  originOf,
  parseNativeAttachmentUrl,
} from "../../lib/nativeAttachments";
import { randomUuid } from "../../lib/randomUuid";
import type { NativeAttachmentListItemDTO } from "../../lib/types";
import type { MarkdownPreviewProps } from "../files/MarkdownPreview";

const PLACEHOLDER_CLASSES =
  "inline-flex items-center rounded border border-dashed border-[var(--g5-stroke)] px-2 py-1 text-xs text-[var(--g5-text2)]";

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/** An attachment's image, read with the reader's own authority. */
export function NativeAttachmentImage({
  orgId,
  number,
  attachmentId,
  alt,
  title,
  className,
}: {
  orgId: string;
  number: number;
  attachmentId: string;
  alt: string;
  title?: string;
  className?: string;
}) {
  const image = useNativeAttachmentImage(orgId, number, attachmentId);
  const blob = image.status === "ok" ? image.blob : null;
  // The bytes are shown through an object URL, set on the element and
  // revoked when the image goes away or its bytes change.
  const imageRef = useRef<HTMLImageElement>(null);
  useEffect(() => {
    const element = imageRef.current;
    if (!blob || !element) return;
    const url = URL.createObjectURL(blob);
    element.src = url;
    return () => {
      element.removeAttribute("src");
      URL.revokeObjectURL(url);
    };
  }, [blob]);
  if (image.status === "removed") {
    return (
      <span
        data-testid="native-attachment-removed"
        title={alt || undefined}
        className={PLACEHOLDER_CLASSES}
      >
        Attachment removed
      </span>
    );
  }
  if (image.status === "error") {
    return (
      <span
        data-testid="native-attachment-unavailable"
        title={alt || undefined}
        className={PLACEHOLDER_CLASSES}
      >
        Image could not be loaded
      </span>
    );
  }
  if (!blob) {
    return (
      <span
        data-testid="native-attachment-loading"
        className={PLACEHOLDER_CLASSES}
      >
        Loading image…
      </span>
    );
  }
  return (
    <img
      ref={imageRef}
      data-testid="native-attachment-image"
      alt={alt}
      title={title}
      className={className}
    />
  );
}

/**
 * The image rendering for a G5 issue's text: attachment addresses of the
 * route's organization load through the authenticated client; another
 * organization's are not fetched; other images keep the default rendering.
 * `issueUrl` is an issue page URL the server built, which carries the
 * deployment's public address.
 */
export function useNativeAttachmentImages(
  issueUrl: string | null | undefined,
): NonNullable<MarkdownPreviewProps["renderImage"]> {
  const membership = useRouteOrgMembership();
  const orgId = membership?.orgId ?? "";
  const orgSlug = membership?.orgSlug ?? "";
  const deploymentOrigin = originOf(issueUrl);
  return useCallback(
    ({ src, alt, title }) => {
      const origins = [window.location.origin];
      if (deploymentOrigin) origins.push(deploymentOrigin);
      const reference = parseNativeAttachmentUrl(src, origins);
      if (!reference) return null;
      if (!orgId || reference.orgSlug !== orgSlug) {
        return (
          <span
            data-testid="native-attachment-unavailable"
            title={alt || undefined}
            className={PLACEHOLDER_CLASSES}
          >
            Image from another organization
          </span>
        );
      }
      return (
        <NativeAttachmentImage
          orgId={orgId}
          number={reference.number}
          attachmentId={reference.attachmentId}
          alt={alt}
          title={title}
        />
      );
    },
    [deploymentOrigin, orgId, orgSlug],
  );
}

// ---------------------------------------------------------------------------
// Taking images in a text box
// ---------------------------------------------------------------------------

/** The files a paste or drop carries. */
function transferFiles(data: DataTransfer | null): File[] {
  if (!data) return [];
  const files: File[] = [];
  for (const item of Array.from(data.items ?? [])) {
    if (item.kind !== "file") continue;
    const file = item.getAsFile();
    if (file) files.push(file);
  }
  if (files.length === 0) files.push(...Array.from(data.files ?? []));
  return files;
}

/**
 * Paste and drop handlers for a text box that takes images. A paste
 * without files stays an ordinary text paste.
 */
export function useImageTransferHandlers(
  onFiles: (files: File[]) => void,
  disabled = false,
) {
  return {
    onPaste: (event: ClipboardEvent<HTMLTextAreaElement>) => {
      if (disabled) return;
      const files = transferFiles(event.clipboardData);
      if (files.length === 0) return;
      event.preventDefault();
      onFiles(files);
    },
    onDragOver: (event: DragEvent<HTMLTextAreaElement>) => {
      if (disabled) return;
      if (Array.from(event.dataTransfer?.types ?? []).includes("Files")) {
        event.preventDefault();
      }
    },
    onDrop: (event: DragEvent<HTMLTextAreaElement>) => {
      if (disabled) return;
      const files = transferFiles(event.dataTransfer);
      if (files.length === 0) return;
      event.preventDefault();
      onFiles(files);
    },
  };
}

/**
 * Whether the person has clicked or typed in a text box since it opened.
 * Until then its caret is the browser's default at the start, which says
 * nothing about where they want an image; focus alone (Tab, autofocus)
 * does not count.
 */
export function useCaretPlacement() {
  const placed = useRef(false);
  const mark = () => {
    placed.current = true;
  };
  return { placed, handlers: { onPointerDown: mark, onKeyDown: mark } };
}

/** The Attach image button and its file chooser. */
export function AttachImageButton({
  onFiles,
  disabled = false,
  testId = "attach-image",
}: {
  onFiles: (files: File[]) => void;
  disabled?: boolean;
  testId?: string;
}) {
  const inputRef = useRef<HTMLInputElement>(null);
  const inputId = useId();
  return (
    <>
      <Button
        kind="ghost"
        data-testid={`${testId}-button`}
        disabled={disabled}
        onClick={() => inputRef.current?.click()}
        className="px-2 py-1 text-xs"
      >
        <ImagePlus aria-hidden="true" className="h-3.5 w-3.5" />
        Attach image
      </Button>
      <input
        ref={inputRef}
        id={inputId}
        type="file"
        multiple
        accept={NATIVE_ATTACHMENT_ACCEPT}
        aria-label="Attach image"
        tabIndex={-1}
        data-testid={`${testId}-input`}
        disabled={disabled}
        className="hidden"
        onChange={(event) => {
          const files = Array.from(event.target.files ?? []);
          // Choosing the same file again is a new choice.
          event.target.value = "";
          if (files.length) onFiles(files);
        }}
      />
    </>
  );
}

interface FailedUpload {
  id: string;
  file: File;
  fileName: string;
  idempotencyKey: string;
  /** Whether the text shows it, or it is only attached to the issue. */
  shown: boolean;
  message: string;
}

/** The G5 issue a text box's images are uploaded to. */
export interface ImageUploadTarget {
  orgId: string;
  number: number;
}

/**
 * Images for a text box on an existing G5 issue: each is uploaded at once
 * and its Markdown inserted at the caret when the upload succeeds. Before
 * the person has clicked or typed in the box, a pasted or dropped image
 * goes at the end, and one chosen with Attach image does too unless
 * `unplacedChoice` is `"attach"` (a description, which an image need not
 * appear in): then it is only attached to the issue, which lists it.
 * Without a `target` (a GitHub issue, which takes no uploads) there are no
 * controls and pastes and drops are left alone.
 */
export function useIssueImageUploads({
  target,
  textareaRef,
  value,
  onChange,
  disabled = false,
  unplacedChoice = "end",
  testId = "issue-attach-image",
}: {
  target: ImageUploadTarget | null;
  textareaRef: RefObject<HTMLTextAreaElement | null>;
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
  unplacedChoice?: "end" | "attach";
  testId?: string;
}): {
  handlers: Partial<ReturnType<typeof useImageTransferHandlers>>;
  controls: ReactNode;
  uploading: boolean;
} {
  const upload = useUploadNativeAttachment(
    target?.orgId ?? "",
    target?.number ?? 0,
  );
  const [uploading, setUploading] = useState<{ id: string; name: string }[]>(
    [],
  );
  const [refusals, setRefusals] = useState<string[]>([]);
  const [failed, setFailed] = useState<FailedUpload[]>([]);
  // Images only attached to the issue, by name.
  const [attached, setAttached] = useState<{ id: string; name: string }[]>([]);
  const placement = useCaretPlacement();
  // The text as last rendered, for an upload that finishes later.
  const latest = useRef({ value, onChange });
  useLayoutEffect(() => {
    latest.current = { value, onChange };
  });

  const send = async (attempt: Omit<FailedUpload, "message">) => {
    setUploading((current) => [
      ...current,
      { id: attempt.id, name: attempt.fileName },
    ]);
    setFailed((current) => current.filter((item) => item.id !== attempt.id));
    // Another upload may finish before this text renders; build on it.
    const replacePlaceholder = (snippet: string): boolean => {
      const marker = pendingImagePlaceholder(attempt.id, attempt.fileName);
      const text = latest.current.value;
      const at = text.indexOf(marker);
      if (at < 0) return false;
      // A stand-in taken out takes its own line's break with it.
      const end =
        at +
        marker.length +
        (snippet === "" && text[at + marker.length] === "\n" ? 1 : 0);
      const next = `${text.slice(0, at)}${snippet}${text.slice(end)}`;
      latest.current.value = next;
      latest.current.onChange(next);
      return true;
    };
    try {
      const attachment = await upload.mutateAsync(attempt);
      // The image goes where it was pasted, dropped or chosen; a retry, or
      // one whose stand-in was deleted, goes at the caret.
      if (!attempt.shown) {
        setAttached((current) => [
          ...current,
          { id: attempt.id, name: attempt.fileName },
        ]);
      } else if (!replacePlaceholder(attachment.markdown)) {
        latest.current.value = insertAtCaret(
          textareaRef.current,
          latest.current.value,
          attachment.markdown,
          latest.current.onChange,
          placement.placed.current,
        );
      }
    } catch (error) {
      replacePlaceholder("");
      setFailed((current) => [
        ...current,
        { ...attempt, message: describeUploadError(error, attempt.fileName) },
      ]);
    } finally {
      setUploading((current) =>
        current.filter((item) => item.id !== attempt.id),
      );
    }
  };

  const take = (files: File[], chosen: boolean) => {
    const shown = !(
      chosen &&
      unplacedChoice === "attach" &&
      !placement.placed.current
    );
    const refused: string[] = [];
    const attempts: Omit<FailedUpload, "message">[] = [];
    for (const file of files) {
      const refusal = imageFileRefusal(file);
      if (refusal) {
        refused.push(refusal);
        continue;
      }
      attempts.push({
        id: randomUuid(),
        file,
        fileName: attachmentFileName(file),
        idempotencyKey: randomUuid(),
        shown,
      });
    }
    setRefusals(refused);
    if (attempts.length === 0) return;
    // A stand-in at the caret now, so that typing on while the upload runs
    // never moves where the image lands.
    if (shown) {
      latest.current.value = insertAtCaret(
        textareaRef.current,
        latest.current.value,
        attempts
          .map((attempt) =>
            pendingImagePlaceholder(attempt.id, attempt.fileName),
          )
          .join("\n"),
        latest.current.onChange,
        placement.placed.current,
      );
    }
    for (const attempt of attempts) void send(attempt);
  };

  const transfer = useImageTransferHandlers(
    (files) => take(files, false),
    disabled,
  );
  const handlers = { ...transfer, ...placement.handlers };
  const controls = (
    <div data-testid={`${testId}-controls`} className="mt-1.5 space-y-1">
      <div className="flex flex-wrap items-center gap-2">
        <AttachImageButton
          onFiles={(files) => take(files, true)}
          disabled={disabled}
          testId={testId}
        />
        <span className="text-xs text-[var(--g5-text2)]">
          Paste or drop PNG, JPEG, GIF or WebP images (10 MiB each) into the
          text box above.
        </span>
      </div>
      {uploading.map((item) => (
        <p
          key={item.id}
          role="status"
          data-testid={`${testId}-uploading`}
          className="flex items-center gap-1.5 text-xs text-[var(--g5-text1)]"
        >
          <LoaderCircle aria-hidden="true" className="h-3 w-3 animate-spin" />
          Uploading {item.name}…
        </p>
      ))}
      {attached.map((item) => (
        <p
          key={item.id}
          role="status"
          data-testid={`${testId}-attached`}
          className="text-xs text-[var(--g5-text1)]"
        >
          Attached {item.name} to the issue; it is listed with its attachments
          rather than shown in the description.
        </p>
      ))}
      {refusals.map((message) => (
        <p
          key={message}
          role="alert"
          data-testid={`${testId}-refused`}
          className="text-xs text-[var(--g5-blocked)]"
        >
          {message}
        </p>
      ))}
      {failed.map((item) => (
        <div
          key={item.id}
          role="alert"
          data-testid={`${testId}-failed`}
          className="flex flex-wrap items-center gap-2 text-xs text-[var(--g5-blocked)]"
        >
          <p className="min-w-0 flex-1">{item.message}</p>
          <Button
            kind="secondary"
            data-testid={`${testId}-retry`}
            disabled={disabled}
            onClick={() => void send(item)}
            className="px-2 py-0.5 text-xs"
          >
            Retry
          </Button>
          <Button
            kind="ghost"
            data-testid={`${testId}-dismiss`}
            onClick={() =>
              setFailed((current) =>
                current.filter((other) => other.id !== item.id),
              )
            }
            className="px-2 py-0.5 text-xs"
          >
            Dismiss
          </Button>
        </div>
      ))}
    </div>
  );
  if (!target) return { handlers: {}, controls: null, uploading: false };
  return {
    handlers,
    controls,
    uploading: uploading.length > 0,
  };
}

// ---------------------------------------------------------------------------
// The issue page's list
// ---------------------------------------------------------------------------

function placeLabel(attachment: NativeAttachmentListItemDTO): string {
  switch (attachment.place) {
    case "description":
      return "In the description";
    case "comment":
      return "In a comment";
    default:
      return "Not shown in the text";
  }
}

function AttachmentRow({
  orgId,
  number,
  attachment,
  removable,
}: {
  orgId: string;
  number: number;
  attachment: NativeAttachmentListItemDTO;
  removable: boolean;
}) {
  const remove = useRemoveNativeAttachment(orgId, number);
  const [confirming, setConfirming] = useState(false);
  const uploader = issueUserLabel(attachment.uploader) ?? "Unknown";
  return (
    <li
      data-testid={`native-attachment-${attachment.id}`}
      className="flex flex-wrap items-center gap-3 rounded-lg border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-3 py-2"
    >
      <span className="flex h-12 w-16 flex-none items-center justify-center overflow-hidden rounded bg-[var(--g5-bg0)]">
        <NativeAttachmentImage
          orgId={orgId}
          number={number}
          attachmentId={attachment.id}
          alt={attachment.fileName}
          className="max-h-12 max-w-16 object-contain"
        />
      </span>
      <div className="min-w-0 flex-1 text-xs">
        <p className="truncate font-medium text-[var(--g5-text0)]">
          {attachment.fileName}
        </p>
        <p className="text-[var(--g5-text2)]">
          {formatAttachmentSize(attachment.sizeBytes)} · {uploader}
          {attachment.origin === "agent" && " (agent)"} ·{" "}
          {placeLabel(attachment)}
        </p>
      </div>
      {removable && !confirming && (
        <Button
          kind="ghost"
          data-testid={`native-attachment-remove-${attachment.id}`}
          aria-label={`Remove ${attachment.fileName}`}
          onClick={() => {
            remove.reset();
            setConfirming(true);
          }}
          className="px-2 py-1 text-xs"
        >
          Remove
        </Button>
      )}
      {confirming && (
        <div
          data-testid={`native-attachment-remove-confirm-${attachment.id}`}
          className="flex w-full flex-wrap items-center gap-2"
        >
          <p className="min-w-0 flex-1 text-xs text-[var(--g5-text1)]">
            Remove {attachment.fileName}? Its image is deleted, and text that
            shows it will say it was removed.
          </p>
          <Button
            kind="secondary"
            data-testid={`native-attachment-remove-cancel-${attachment.id}`}
            disabled={remove.isLoading}
            onClick={() => setConfirming(false)}
            className="px-3 py-1 text-xs"
          >
            Cancel
          </Button>
          <Button
            kind="primary"
            data-testid={`native-attachment-remove-confirm-button-${attachment.id}`}
            aria-label={`Remove ${attachment.fileName}`}
            disabled={remove.isLoading}
            onClick={() => {
              remove
                .mutateAsync(attachment.id)
                .then(() => setConfirming(false))
                .catch(() => {
                  // Explained below.
                });
            }}
            className="px-3 py-1 text-xs"
          >
            {remove.isLoading && (
              <LoaderCircle className="h-3 w-3 animate-spin" />
            )}
            Remove
          </Button>
        </div>
      )}
      {remove.error && (
        <p
          role="alert"
          data-testid={`native-attachment-remove-error-${attachment.id}`}
          className="w-full text-xs text-[var(--g5-blocked)]"
        >
          {describeIssueWriteError(
            remove.error,
            "The attachment could not be removed.",
          )}
        </p>
      )}
    </li>
  );
}

/**
 * A G5 issue's attachments on its page. `removerId` is the signed-in user
 * when they can write; `administrator` lets them remove anyone's.
 */
export function NativeIssueAttachments({
  orgId,
  number,
  removerId,
  administrator,
}: {
  orgId: string;
  number: number;
  removerId: string | null;
  administrator: boolean;
}) {
  const attachments = useNativeIssueAttachments(orgId, number);
  if (attachments.status === "loading") return null;
  return (
    <section
      data-testid="native-issue-attachments"
      aria-labelledby="native-issue-attachments-title"
      className="mt-6"
    >
      <h2
        id="native-issue-attachments-title"
        className="text-xs font-semibold uppercase tracking-[0.14em] text-[var(--g5-text1)]"
      >
        Attachments
        {attachments.status === "ok" && ` (${attachments.data.length})`}
      </h2>
      {attachments.status === "error" ? (
        <div
          role="alert"
          className="mt-2 flex flex-wrap items-center gap-2 text-xs text-[var(--g5-blocked)]"
        >
          <p className="min-w-0 flex-1">
            The attachments could not be listed: {attachments.error.message}
          </p>
          <Button
            kind="secondary"
            data-testid="native-issue-attachments-retry"
            onClick={attachments.refetch}
            className="px-2.5 py-1 text-xs"
          >
            Retry
          </Button>
        </div>
      ) : attachments.data.length === 0 ? (
        <p className="mt-2 text-sm text-[var(--g5-text2)]">No attachments.</p>
      ) : (
        <ul className="mt-2 space-y-1.5">
          {attachments.data.map((attachment) => (
            <AttachmentRow
              key={attachment.id}
              orgId={orgId}
              number={number}
              attachment={attachment}
              removable={
                removerId !== null &&
                (administrator || attachment.uploader.userId === removerId)
              }
            />
          ))}
        </ul>
      )}
    </section>
  );
}
