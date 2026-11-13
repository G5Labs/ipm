// =============================================================================
// G5 Files — fileBytes
// =============================================================================
// The bytes a workspace read carries and the object URLs viewers show them
// through (`g5.documents.viewers`). A read serves text as UTF-8 and anything
// else as base64; an object URL lives exactly as long as the component
// showing it needs the blob, and is revoked when the blob is replaced or the
// component unmounts.
// =============================================================================

import { useEffect, useState } from "react";
import type { WorkspaceFileContent } from "../../hooks/useFiles";

export type FileRead = Pick<WorkspaceFileContent, "content" | "encoding">;

/** Decodes a base64 read into bytes. */
export function base64Bytes(content: string): Uint8Array<ArrayBuffer> {
  const binary = atob(content);
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index);
  }
  return bytes;
}

/** The bytes a read carries, whichever encoding served them. */
export function fileBytes(file: FileRead): Uint8Array<ArrayBuffer> {
  if (file.encoding === "base64") return base64Bytes(file.content);
  const encoded = new TextEncoder().encode(file.content);
  return new Uint8Array(encoded);
}

/**
 * A Blob of the read's bytes under `type`; without one, binary bytes are
 * octet-stream and text is UTF-8 plain text, as Download sends them.
 */
export function fileBlob(file: FileRead, type?: string): Blob {
  if (file.encoding === "base64") {
    return new Blob([base64Bytes(file.content)], {
      type: type ?? "application/octet-stream",
    });
  }
  return new Blob([file.content], {
    type: type ?? "text/plain;charset=utf-8",
  });
}

export function formatByteSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/**
 * An object URL for `blob`, created when the blob changes and revoked when
 * it is replaced or the component unmounts (`g5.documents.viewers`). The URL
 * is created in an effect, not during render, so a strict-mode double render
 * never leaks one; the first render after a change therefore shows no URL.
 */
export function useObjectUrl(blob: Blob | null): string | null {
  const [url, setUrl] = useState<{ blob: Blob; url: string } | null>(null);
  useEffect(() => {
    if (!blob) return;
    const next = URL.createObjectURL(blob);
    // eslint-disable-next-line react-hooks/set-state-in-effect -- the URL exists only once the effect created it; see above.
    setUrl({ blob, url: next });
    return () => {
      URL.revokeObjectURL(next);
    };
  }, [blob]);
  return url && url.blob === blob ? url.url : null;
}
