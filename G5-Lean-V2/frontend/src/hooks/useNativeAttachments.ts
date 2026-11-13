// =============================================================================
// G5 Client — G5 issue image attachments (g5.issue.native.attachments)
// =============================================================================
// Backend contract, ATTACHMENTS = /api/v1/orgs/:orgId/issues/g5/:number/attachments:
//   POST   ATTACHMENTS { fileName, contentBase64, idempotencyKey }
//            → 201 NativeAttachmentDTO (200 for a replay of the key)
//   GET    ATTACHMENTS                → NativeAttachmentListItemDTO[]
//   GET    ATTACHMENTS/:attachmentId  → the image bytes (404 once removed)
//   DELETE ATTACHMENTS/:attachmentId  → 204 (uploader or administrator)
// The bytes need the bearer token, so text never loads an attachment's
// address as an image: it renders the Blob read here through an object URL.
// =============================================================================

import {
  useMutation,
  useQuery,
  useQueryClient,
  type QueryClient,
} from "@tanstack/react-query";

import { ApiError, api } from "../lib/api";
import { readFileAsBase64 } from "../lib/nativeAttachments";
import { routes } from "../lib/routes";
import type {
  AsyncState,
  MutationHook,
  NativeAttachmentDTO,
  NativeAttachmentListItemDTO,
} from "../lib/types";
import { externalIssueKeys } from "./useExternalIssues";
import { toAsyncState, toMutationHook } from "./hookUtils";

export const nativeAttachmentKeys = {
  // Under the issue's detail key, so every change of the issue's text
  // (which moves where attachments appear) reads the list again.
  list: (orgId: string, number: number) =>
    [...externalIssueKeys.detail(orgId, "g5", number), "attachments"] as const,
  imageAll: ["native-attachment-image"] as const,
  image: (orgId: string, number: number, attachmentId: string) =>
    ["native-attachment-image", orgId, number, attachmentId] as const,
};

/** One file to upload, under the key every retry of it reuses. */
export interface NativeAttachmentUpload {
  file: File;
  fileName: string;
  idempotencyKey: string;
}

/** Upload one image to a G5 issue; the issue's text is not changed. */
export async function uploadNativeAttachment(
  orgId: string,
  number: number,
  upload: NativeAttachmentUpload,
): Promise<NativeAttachmentDTO> {
  return api.post<NativeAttachmentDTO>(
    routes.orgs.nativeIssueAttachments(orgId, number),
    {
      fileName: upload.fileName,
      contentBase64: await readFileAsBase64(upload.file),
      idempotencyKey: upload.idempotencyKey,
    },
  );
}

function invalidateAttachmentList(
  queryClient: QueryClient,
  orgId: string,
  number: number,
) {
  void queryClient.invalidateQueries({
    queryKey: nativeAttachmentKeys.list(orgId, number),
  });
}

/** The issue's attachments with where each first appears. */
export function useNativeIssueAttachments(
  orgId: string,
  number: number,
  enabled = true,
): AsyncState<NativeAttachmentListItemDTO[]> & { refetch: () => void } {
  const query = useQuery<NativeAttachmentListItemDTO[], Error>({
    queryKey: nativeAttachmentKeys.list(orgId, number),
    queryFn: ({ signal }) =>
      api.get<NativeAttachmentListItemDTO[]>(
        routes.orgs.nativeIssueAttachments(orgId, number),
        { signal },
      ),
    enabled: !!orgId && number > 0 && enabled,
    retry: false,
  });
  return { ...toAsyncState(query), refetch: () => void query.refetch() };
}

/** Upload an image to the issue from a composer. */
export function useUploadNativeAttachment(
  orgId: string,
  number: number,
): MutationHook<NativeAttachmentUpload, NativeAttachmentDTO> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    NativeAttachmentDTO,
    Error,
    NativeAttachmentUpload
  >({
    mutationFn: (upload) => uploadNativeAttachment(orgId, number, upload),
    onSuccess: () => invalidateAttachmentList(queryClient, orgId, number),
  });
  return toMutationHook(mutation);
}

/** Remove an attachment: the caller's own, or any for an administrator. */
export function useRemoveNativeAttachment(
  orgId: string,
  number: number,
): MutationHook<string, void> {
  const queryClient = useQueryClient();
  const mutation = useMutation<void, Error, string>({
    mutationFn: (attachmentId) =>
      api.delete<void>(
        routes.orgs.nativeIssueAttachment(orgId, number, attachmentId),
      ),
    onSettled: (_data, _error, attachmentId) => {
      invalidateAttachmentList(queryClient, orgId, number);
      // Text that shows it reads it again, and shows it as removed.
      void queryClient.invalidateQueries({
        queryKey: nativeAttachmentKeys.image(orgId, number, attachmentId),
      });
    },
  });
  return toMutationHook(mutation);
}

/** An attachment's bytes could not be shown: `removed` once it is gone. */
export type NativeAttachmentImageState =
  | { status: "loading" }
  | { status: "ok"; blob: Blob }
  | { status: "removed" }
  | { status: "error" };

/**
 * An attachment's image bytes, read through the authenticated client. An
 * id's bytes never change, so they are read once per page unless removed.
 */
export function useNativeAttachmentImage(
  orgId: string,
  number: number,
  attachmentId: string,
): NativeAttachmentImageState {
  const query = useQuery<Blob, Error>({
    queryKey: nativeAttachmentKeys.image(orgId, number, attachmentId),
    queryFn: async ({ signal }) => {
      const response = await api.getRawResponse(
        routes.orgs.nativeIssueAttachment(orgId, number, attachmentId),
        { signal },
      );
      return response.blob();
    },
    enabled: !!orgId,
    retry: false,
    staleTime: Infinity,
  });
  if (query.isError) {
    return query.error instanceof ApiError && query.error.status === 404
      ? { status: "removed" }
      : { status: "error" };
  }
  if (query.data) return { status: "ok", blob: query.data };
  return { status: "loading" };
}
