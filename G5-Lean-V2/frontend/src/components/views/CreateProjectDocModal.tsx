import React, {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { X, Upload } from "lucide-react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "../../lib/api";
import type { WorkspaceUiCapabilities } from "../../lib/capabilities";
import type { DocNode } from "../../lib/types";
import { classNames, labelWorkspaceFolderInPath } from "../../lib/utils";
import { repositoryDocHandle } from "../../hooks/useDocs";
import { useWorkspaceNames } from "../../hooks/useWorkspaceNames";
import { GatedControl, useLiveGateReason } from "../../lib/workspaceLiveGate";
import { Modal } from "../ui/UI";

export interface CreateProjectDocModalProps {
  open: boolean;
  workspaceId: string;
  title?: string;
  /**
   * Default destination folder for plain/uploaded docs — the workspace's
   * dedicated docs folder ("docs/<workspace_id>"). Falls back to "docs" when
   * absent.
   */
  defaultFolderPath?: string | null;
  /** Optional files supplied by a Project Docs drag-and-drop launch. */
  initialUploadFiles?: readonly File[];
  /**
   * Mode the modal opens in; the workbench "Upload file" action opens
   * `upload` and "Upload transcript" opens `transcript` (workbench
   * g5.workbench.meeting): a `.txt`, `.md`, `.vtt` or `.srt` meeting
   * transcript stored as a Markdown Project Doc under `meetings/`.
   */
  initialMode?: "plain" | "upload" | "transcript";
  capabilities?: WorkspaceUiCapabilities;
  onCreated?: (doc: DocNode) => void;
  /**
   * Transcript mode's second source (workbench g5.workbench.meeting): the
   * transcripts already stored under `meetings/`. Choosing one skips the
   * upload; `onUseExisting` receives it as `onCreated` would a stored one.
   */
  existingTranscripts?: readonly DocNode[];
  onUseExisting?: (doc: DocNode) => void;
  /**
   * The decisions documents extracted earlier (`meetings/*.decisions.md`),
   * listed so a person can return to one; `onOpenDecisions` opens it.
   */
  decisionDocuments?: readonly DocNode[];
  onOpenDecisions?: (doc: DocNode) => void;
  onClose: () => void;
}

type DocMode = "plain" | "upload" | "transcript";
type UploadStatus = "queued" | "uploading" | "succeeded" | "failed";

interface UploadQueueItem {
  id: string;
  file: File;
  normalizedName: string;
  destination: string;
  status: UploadStatus;
  error?: string;
  createdDoc?: DocNode;
}

function sanitizeFileName(name: string): string {
  return name
    .trim()
    .toLowerCase()
    .replace(/\s+/g, "-")
    .replace(/[^a-z0-9._-]/g, "");
}

function normalizePlainDocBaseName(name: string): string {
  return sanitizeFileName(name).replace(/(?:\.md)+$/g, "");
}

const UPLOAD_DOC_EXTENSIONS = [".md", ".html", ".htm", ".jsonl"];
/** Meeting transcripts (workbench g5.workbench.meeting): text exports and
 * subtitle files; every one is stored as Markdown under `meetings/`. */
export const TRANSCRIPT_EXTENSIONS = [".md", ".txt", ".vtt", ".srt"];
export const TRANSCRIPT_FOLDER = "meetings";

// Maximum size for an uploaded project doc. Kept comfortably below the docs
// route's 32 MiB server-side body cap (g5-api routes/docs spec D21) so JSON
// string escaping and the name/filePath fields cannot push a client-accepted
// file past the server limit. Oversized files are rejected in `validate()` with
// a clear message before any request is issued, so the upload never stalls the
// server body-limit layer. See create-project-doc-modal spec D10.
const MAX_UPLOAD_DOC_BYTES = 25 * 1024 * 1024;
const MAX_UPLOAD_DOC_FILES = 20;
const MAX_UPLOAD_BATCH_BYTES = 100 * 1024 * 1024;

// Serialized-request ceiling for uploads: the docs route accepts 32 MiB
// (g5-api routes/docs spec D21), but `api.post` transmits
// `JSON.stringify(body)`, and JSON escaping can roughly double newline-,
// quote-, or backslash-heavy content — so the raw `File.size` gate above
// cannot bound the transmitted body on its own. Submit measures the exact
// stringified request body against this cap (1 MiB headroom under the server
// limit) and rejects before issuing the request. See create-project-doc-modal
// spec D10.
const MAX_UPLOAD_DOC_REQUEST_BYTES = 31 * 1024 * 1024;

function formatFileSizeMb(bytes: number): string {
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function formatUploadFileSize(bytes: number): string {
  if (bytes < 1024 * 1024) {
    return `${Math.max(1, Math.ceil(bytes / 1024))} KB`;
  }
  return formatFileSizeMb(bytes);
}

function normalizeUploadedDocFileName(
  name: string,
  extensions: readonly string[] = UPLOAD_DOC_EXTENSIONS,
): string {
  const sanitized = sanitizeFileName(name);
  if (
    !sanitized ||
    !extensions.some((extension) => sanitized.endsWith(extension))
  ) {
    return "";
  }
  return sanitized;
}

/** The Markdown document name a transcript file is stored under. */
export function transcriptDocName(normalizedName: string): string {
  const stem = normalizedName.replace(/\.(md|txt|vtt|srt)$/i, "");
  return `${stem}.md`;
}

/**
 * Transcript bytes as a Markdown document: text and Markdown exports verbatim;
 * subtitle files (`.vtt`, `.srt`) inside one fenced block so cues and
 * timestamps render as typed instead of as paragraphs.
 */
export function transcriptMarkdown(
  normalizedName: string,
  text: string,
): string {
  if (!/\.(vtt|srt)$/i.test(normalizedName)) return text;
  const body = text.endsWith("\n") ? text : `${text}\n`;
  // A fence longer than every backtick run in the content, so a cue that
  // holds three backticks cannot close it.
  let longestRun = 0;
  for (const run of body.matchAll(/`+/g)) {
    if (run[0].length > longestRun) longestRun = run[0].length;
  }
  const fence = "`".repeat(Math.max(3, longestRun + 1));
  return `${fence}text\n${body}${fence}\n`;
}

async function readFileAsText(file: File): Promise<string> {
  if (typeof file.text === "function") {
    return file.text();
  }
  if (typeof FileReader !== "undefined") {
    return new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(String(reader.result ?? ""));
      reader.onerror = () =>
        reject(reader.error ?? new Error("Failed to read file"));
      reader.readAsText(file);
    });
  }
  if (typeof Response !== "undefined") {
    return new Response(file).text();
  }
  throw new Error("File upload is not supported in this browser");
}

function withDerivedFolder(doc: DocNode): DocNode {
  if (doc.folder) return doc;
  const folder = doc.filePath
    ? doc.filePath.split("/").slice(0, -1).join("/") || undefined
    : undefined;
  return folder ? { ...doc, folder } : doc;
}

export const CreateProjectDocModal = React.memo(function CreateProjectDocModal({
  open,
  workspaceId,
  title = "Create Doc",
  defaultFolderPath,
  initialUploadFiles,
  initialMode,
  onCreated,
  existingTranscripts = [],
  onUseExisting,
  decisionDocuments = [],
  onOpenDecisions,
  onClose,
}: CreateProjectDocModalProps) {
  // Plain/uploaded docs default into the workspace's dedicated folder.
  const docFolder = defaultFolderPath?.replace(/\/+$/, "") || "docs";
  const [mode, setMode] = useState<DocMode>("plain");
  // Transcript mode is the upload flow with its own extensions, destination
  // folder and copy (workbench g5.workbench.meeting).
  const transcriptMode = mode === "transcript";
  const isUploadMode = mode === "upload" || transcriptMode;
  // Transcript mode's two sources (workbench g5.workbench.meeting): a file to
  // upload, or a transcript already under meetings/ which is extracted as is.
  const [transcriptSource, setTranscriptSource] = useState<
    "upload" | "existing"
  >("upload");
  const [selectedTranscriptId, setSelectedTranscriptId] = useState<
    string | null
  >(null);
  const useExistingTranscript =
    transcriptMode && transcriptSource === "existing";
  const selectedTranscript =
    useExistingTranscript && selectedTranscriptId
      ? (existingTranscripts.find((doc) => doc.id === selectedTranscriptId) ??
        null)
      : null;
  const handleUseExistingTranscript = () => {
    if (!selectedTranscript) return;
    onUseExisting?.(selectedTranscript);
    onClose();
  };
  const [fileName, setFileName] = useState("");
  const [uploadQueue, setUploadQueue] = useState<UploadQueueItem[]>([]);
  const uploadQueueRef = useRef<UploadQueueItem[]>([]);
  const uploadIdRef = useRef(0);
  const stopRequestedRef = useRef(false);
  const uploadRunLockRef = useRef(false);
  const [isUploadRunning, setIsUploadRunning] = useState(false);
  const [isUploadDropActive, setIsUploadDropActive] = useState(false);
  const [stopRequested, setStopRequested] = useState(false);
  const [uploadProgress, setUploadProgress] = useState<{
    current: number;
    total: number;
  } | null>(null);
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [submitError, setSubmitError] = useState<string | null>(null);
  // Single-flight submit (spec D11): the ref is the synchronous concurrency
  // guard; the state only drives the disabled button / "Creating…" label.
  const [isSubmitting, setIsSubmitting] = useState(false);
  const submitLockRef = useRef(false);
  // The dialog can outlive the VM: creation and uploads read the current
  // gate at submission (`g5.workspace.ui.unavailable`); the queue and name
  // stay in the dialog.
  const liveBlockedReason = useLiveGateReason("create documents");

  const firstFocusableRef = useRef<HTMLButtonElement>(null);
  const hasSeededInitialUploadFilesRef = useRef(false);
  const uploadDragDepthRef = useRef(0);
  const wasOpenRef = useRef(false);

  const queryClient = useQueryClient();
  const createDoc = useMutation<
    DocNode,
    Error,
    { name: string; docType: string; filePath: string; content: string }
  >({
    mutationFn: async (input) => {
      const created = await api.post<DocNode>(
        `/api/v1/workspaces/${workspaceId}/docs`,
        input,
      );
      const displayId = repositoryDocHandle(created);
      if (!displayId) {
        throw new Error(
          "Created document response is missing its repository path identity",
        );
      }
      return { ...created, id: displayId, isVirtual: true };
    },
  });
  const cacheCreatedDocs = useCallback(
    (createdDocs: DocNode[]) => {
      if (createdDocs.length === 0) return;
      const cachedDocs = createdDocs.map(withDerivedFolder);
      queryClient.setQueryData<DocNode[]>(
        ["docs", workspaceId],
        (previous = []) => {
          const createdIds = new Set(cachedDocs.map((doc) => doc.id));
          const withoutExisting = previous.filter(
            (doc) => !createdIds.has(doc.id),
          );
          return [...withoutExisting, ...cachedDocs];
        },
      );
      queryClient.invalidateQueries({ queryKey: ["docs", workspaceId] });
      // The new files are live, uncommitted changes in the docs checkout.
      queryClient.invalidateQueries({
        queryKey: ["workspace-changes", workspaceId],
      });
      if (cachedDocs.some((doc) => doc.docType === "view")) {
        queryClient.invalidateQueries({ queryKey: ["views", workspaceId] });
      }
    },
    [queryClient, workspaceId],
  );
  useEffect(() => {
    const wasOpen = wasOpenRef.current;
    if (open && !wasOpen) {
      const hasInitialUploadFiles =
        !!initialUploadFiles && initialUploadFiles.length > 0;
      setMode(hasInitialUploadFiles ? "upload" : (initialMode ?? "plain"));
      setTranscriptSource("upload");
      setSelectedTranscriptId(null);
      setFileName("");
      uploadQueueRef.current = [];
      setUploadQueue([]);
      stopRequestedRef.current = false;
      setStopRequested(false);
      setIsUploadRunning(false);
      setIsUploadDropActive(false);
      uploadDragDepthRef.current = 0;
      setUploadProgress(null);
      setErrors({});
      setSubmitError(null);
      hasSeededInitialUploadFilesRef.current = !hasInitialUploadFiles;
    }
    wasOpenRef.current = open;
  }, [initialMode, initialUploadFiles, open]);

  const previewName = useMemo(() => {
    if (isUploadMode) return null;
    const plainBase = normalizePlainDocBaseName(fileName);
    if (!plainBase) return null;
    return `${docFolder}/${plainBase}.md`;
  }, [docFolder, fileName, isUploadMode]);

  // Display-only: show the workspace name in place of the docs/<workspace_id>
  // UUID in the preview (pm_view D9). The submitted filePath is unchanged.
  const workspaceNamesById = useWorkspaceNames();
  const previewDisplay = useMemo(
    () =>
      previewName
        ? labelWorkspaceFolderInPath(previewName, workspaceNamesById)
        : previewName,
    [previewName, workspaceNamesById],
  );

  const clearError = useCallback((key: string) => {
    setErrors((prev) => {
      if (!prev[key]) return prev;
      const next = { ...prev };
      delete next[key];
      return next;
    });
  }, []);

  const replaceUploadQueue = useCallback(
    (updater: (items: UploadQueueItem[]) => UploadQueueItem[]) => {
      const next = updater(uploadQueueRef.current);
      uploadQueueRef.current = next;
      setUploadQueue(next);
    },
    [],
  );

  const handleUploadSelection = useCallback(
    (files: File[], selectionMode: DocMode = mode) => {
      if (files.length === 0) return;
      // The rules follow the mode the selection is for: a sidebar launch
      // seeds its files in the same commit that switches the mode, so the
      // caller names it instead of relying on this closure.
      const forTranscript = selectionMode === "transcript";

      const current = uploadQueueRef.current;
      if (forTranscript && current.length + files.length > 1) {
        // One transcript at a time: the stored document starts the
        // extraction, which needs exactly one.
        setErrors((previous) => ({
          ...previous,
          uploadFile: "Upload one transcript at a time.",
        }));
        return;
      }
      const selectedNames = files.map((file) => file.name).join(", ");
      const selectedBytes = files.reduce((total, file) => total + file.size, 0);
      const currentBytes = current.reduce(
        (total, item) => total + item.file.size,
        0,
      );
      if (
        current.length + files.length > MAX_UPLOAD_DOC_FILES ||
        currentBytes + selectedBytes > MAX_UPLOAD_BATCH_BYTES
      ) {
        setErrors((previous) => ({
          ...previous,
          uploadFile: `Selection rejected (${selectedNames}). A batch can contain at most ${MAX_UPLOAD_DOC_FILES} files and ${formatFileSizeMb(MAX_UPLOAD_BATCH_BYTES)} total.`,
        }));
        return;
      }

      const knownPaths = new Set(
        (
          queryClient.getQueryData<DocNode[]>(["docs", workspaceId]) ?? []
        ).flatMap((doc) => (doc.filePath ? [doc.filePath] : [])),
      );
      const queuedPaths = new Set(current.map((item) => item.destination));
      const invalid: string[] = [];
      const oversized: string[] = [];
      const duplicates: string[] = [];
      const collisions: string[] = [];
      const additions: UploadQueueItem[] = [];

      for (const file of files) {
        const normalizedName = normalizeUploadedDocFileName(
          file.name,
          forTranscript ? TRANSCRIPT_EXTENSIONS : UPLOAD_DOC_EXTENSIONS,
        );
        if (!normalizedName) {
          invalid.push(file.name);
          continue;
        }
        if (file.size > MAX_UPLOAD_DOC_BYTES) {
          oversized.push(file.name);
          continue;
        }
        const destination = forTranscript
          ? `${docFolder}/${TRANSCRIPT_FOLDER}/${transcriptDocName(normalizedName)}`
          : `${docFolder}/${normalizedName}`;
        if (queuedPaths.has(destination)) {
          duplicates.push(file.name);
          continue;
        }
        if (knownPaths.has(destination)) {
          collisions.push(file.name);
          continue;
        }
        queuedPaths.add(destination);
        uploadIdRef.current += 1;
        additions.push({
          id: `upload-${uploadIdRef.current}`,
          file,
          normalizedName,
          destination,
          status: "queued",
        });
      }

      if (additions.length > 0) {
        replaceUploadQueue((items) => [...items, ...additions]);
      }
      const messages = [
        invalid.length > 0 ? `Unsupported files: ${invalid.join(", ")}` : null,
        oversized.length > 0
          ? `Files rejected because the maximum upload size is ${formatFileSizeMb(MAX_UPLOAD_DOC_BYTES)}: ${oversized.join(", ")}`
          : null,
        duplicates.length > 0
          ? `Duplicate destinations: ${duplicates.join(", ")}`
          : null,
        collisions.length > 0
          ? `Already in Project Docs: ${collisions.join(", ")}`
          : null,
      ].filter((message): message is string => Boolean(message));

      if (messages.length > 0) {
        setErrors((previous) => ({
          ...previous,
          uploadFile: messages.join(" "),
        }));
      } else {
        clearError("uploadFile");
      }
    },
    [clearError, docFolder, mode, queryClient, replaceUploadQueue, workspaceId],
  );

  useEffect(() => {
    if (
      !open ||
      hasSeededInitialUploadFilesRef.current ||
      !initialUploadFiles ||
      initialUploadFiles.length === 0
    ) {
      return;
    }

    hasSeededInitialUploadFilesRef.current = true;
    // A sidebar launch is always an ordinary upload (the reset effect set
    // that mode in this same commit).
    handleUploadSelection(Array.from(initialUploadFiles), "upload");
  }, [handleUploadSelection, initialUploadFiles, open]);

  const handleUploadDragEnter = useCallback(
    (event: React.DragEvent<HTMLLabelElement>) => {
      if (!Array.from(event.dataTransfer.types).includes("Files")) return;
      event.preventDefault();
      event.stopPropagation();
      if (isUploadRunning) return;
      uploadDragDepthRef.current += 1;
      setIsUploadDropActive(true);
    },
    [isUploadRunning],
  );

  const handleUploadDragOver = useCallback(
    (event: React.DragEvent<HTMLLabelElement>) => {
      if (!Array.from(event.dataTransfer.types).includes("Files")) return;
      event.preventDefault();
      event.stopPropagation();
      event.dataTransfer.dropEffect = isUploadRunning ? "none" : "copy";
      if (!isUploadRunning) setIsUploadDropActive(true);
    },
    [isUploadRunning],
  );

  const handleUploadDragLeave = useCallback(
    (event: React.DragEvent<HTMLLabelElement>) => {
      if (!Array.from(event.dataTransfer.types).includes("Files")) return;
      event.preventDefault();
      event.stopPropagation();
      if (isUploadRunning) return;
      uploadDragDepthRef.current = Math.max(0, uploadDragDepthRef.current - 1);
      if (uploadDragDepthRef.current === 0) setIsUploadDropActive(false);
    },
    [isUploadRunning],
  );

  const handleUploadDrop = useCallback(
    (event: React.DragEvent<HTMLLabelElement>) => {
      const files = Array.from(event.dataTransfer.files);
      const isFileDrop =
        files.length > 0 ||
        Array.from(event.dataTransfer.types).includes("Files");
      if (!isFileDrop) return;
      event.preventDefault();
      event.stopPropagation();
      uploadDragDepthRef.current = 0;
      setIsUploadDropActive(false);
      if (isUploadRunning || files.length === 0) return;
      handleUploadSelection(files);
    },
    [handleUploadSelection, isUploadRunning],
  );

  const removeUploadItem = useCallback(
    (itemId: string) => {
      if (isUploadRunning) return;
      replaceUploadQueue((items) =>
        items.filter(
          (item) => item.id !== itemId || item.status === "succeeded",
        ),
      );
    },
    [isUploadRunning, replaceUploadQueue],
  );

  const requestUploadStop = useCallback(() => {
    stopRequestedRef.current = true;
    setStopRequested(true);
  }, []);

  const runUploadQueue = useCallback(
    async (includeFailed: boolean) => {
      // Queue runs have a direct retry entry point that bypasses handleSubmit.
      // This ref is the synchronous concurrency guard; isUploadRunning only
      // drives rendering and cannot close a same-tick retry window (spec D14).
      if (uploadRunLockRef.current) return;
      if (liveBlockedReason !== null) return;
      uploadRunLockRef.current = true;
      let runStarted = false;

      try {
        if (includeFailed) {
          replaceUploadQueue((items) =>
            items.map((item) =>
              item.status === "failed"
                ? { ...item, status: "queued", error: undefined }
                : item,
            ),
          );
        }
        const targets = uploadQueueRef.current.filter(
          (item) => item.status === "queued",
        );
        if (targets.length === 0) {
          setErrors((previous) => ({
            ...previous,
            uploadFile: transcriptMode
              ? "Choose at least one queued transcript file"
              : "Choose at least one queued Markdown, HTML, or JSONL file",
          }));
          return;
        }

        runStarted = true;
        stopRequestedRef.current = false;
        setStopRequested(false);
        setSubmitError(null);
        setIsUploadRunning(true);
        const createdThisRun: DocNode[] = [];

        for (let index = 0; index < targets.length; index += 1) {
          if (stopRequestedRef.current) break;
          const target = targets[index];
          setUploadProgress({ current: index + 1, total: targets.length });
          replaceUploadQueue((items) =>
            items.map((item) =>
              item.id === target.id
                ? { ...item, status: "uploading", error: undefined }
                : item,
            ),
          );

          try {
            const rawContent = await readFileAsText(target.file);
            const uploadedContent = transcriptMode
              ? transcriptMarkdown(target.normalizedName, rawContent)
              : rawContent;
            const requestBody = {
              name: transcriptMode
                ? transcriptDocName(target.normalizedName)
                : target.normalizedName,
              docType: "design",
              filePath: target.destination,
              content: uploadedContent,
            };
            const serializedBytes = new TextEncoder().encode(
              JSON.stringify(requestBody),
            ).length;
            if (serializedBytes > MAX_UPLOAD_DOC_REQUEST_BYTES) {
              const message = `This file serializes to ${formatFileSizeMb(
                serializedBytes,
              )} of JSON request body once escaped. The maximum upload request size is ${formatFileSizeMb(
                MAX_UPLOAD_DOC_REQUEST_BYTES,
              )}.`;
              setErrors((previous) => ({
                ...previous,
                uploadFile: message,
              }));
              throw new Error(message);
            }
            const createdDoc = await createDoc.mutateAsync(requestBody);
            const cachedDoc = withDerivedFolder({
              ...createdDoc,
              content: createdDoc.content ?? uploadedContent,
            });
            createdThisRun.push(cachedDoc);
            replaceUploadQueue((items) =>
              items.map((item) =>
                item.id === target.id
                  ? { ...item, status: "succeeded", createdDoc: cachedDoc }
                  : item,
              ),
            );
          } catch (err: unknown) {
            const message =
              err instanceof Error ? err.message : "Failed to upload document";
            replaceUploadQueue((items) =>
              items.map((item) =>
                item.id === target.id
                  ? { ...item, status: "failed", error: message }
                  : item,
              ),
            );
          }
        }

        const wasStopped = stopRequestedRef.current;
        cacheCreatedDocs(createdThisRun);

        const finalQueue = uploadQueueRef.current;
        const allSucceeded =
          finalQueue.length > 0 &&
          finalQueue.every((item) => item.status === "succeeded");
        if (allSucceeded) {
          if (finalQueue.length === 1 && finalQueue[0].createdDoc) {
            onCreated?.(finalQueue[0].createdDoc);
          }
          onClose();
        } else if (
          wasStopped &&
          finalQueue.some((item) => item.status === "queued")
        ) {
          setErrors((previous) => ({
            ...previous,
            uploadFile:
              "Upload stopped after the current file. Remaining files are still queued.",
          }));
        }
      } finally {
        if (runStarted) {
          setUploadProgress(null);
          setIsUploadRunning(false);
          stopRequestedRef.current = false;
          setStopRequested(false);
        }
        uploadRunLockRef.current = false;
      }
    },
    [
      cacheCreatedDocs,
      createDoc,
      liveBlockedReason,
      onClose,
      onCreated,
      replaceUploadQueue,
      transcriptMode,
    ],
  );

  const validate = useCallback((): boolean => {
    const newErrors: Record<string, string> = {};
    if (isUploadMode) {
      if (!uploadQueue.some((item) => item.status === "queued")) {
        newErrors.uploadFile = transcriptMode
          ? "Choose at least one queued transcript file"
          : "Choose at least one queued Markdown, HTML, or JSONL file";
      }
      setErrors(newErrors);
      return Object.keys(newErrors).length === 0;
    }
    if (!fileName.trim()) {
      newErrors.fileName = "File name is required";
    } else if (normalizePlainDocBaseName(fileName).length === 0) {
      newErrors.fileName = "File name contains only invalid characters";
    }
    setErrors(newErrors);
    return Object.keys(newErrors).length === 0;
  }, [fileName, isUploadMode, transcriptMode, uploadQueue]);

  const handleSubmit = useCallback(async () => {
    // Single-flight guard (spec D11): a synchronous ref lock closes the
    // double-click window before the first await — react-query isPending is
    // render-async and cannot guard it (experience note
    // react_query_ispending_not_a_double_submit_guard).
    if (submitLockRef.current) return;
    if (liveBlockedReason !== null) return;
    if (!validate()) return;

    submitLockRef.current = true;
    setIsSubmitting(true);
    setSubmitError(null);

    try {
      if (isUploadMode) {
        await runUploadQueue(false);
        return;
      } else if (mode === "plain") {
        const plainBase = normalizePlainDocBaseName(fileName);
        const fullName = `${plainBase}.md`;
        const createdDoc = await createDoc.mutateAsync({
          name: fullName,
          docType: "design",
          filePath: `${docFolder}/${fullName}`,
          content: "",
        });
        const cachedDoc = withDerivedFolder(createdDoc);
        cacheCreatedDocs([cachedDoc]);
        onCreated?.(cachedDoc);
      }
      onClose();
    } catch (err: unknown) {
      const message =
        err instanceof Error ? err.message : "Failed to create document";
      setSubmitError(message);
    } finally {
      submitLockRef.current = false;
      setIsSubmitting(false);
    }
  }, [
    cacheCreatedDocs,
    createDoc,
    docFolder,
    fileName,
    isUploadMode,
    liveBlockedReason,
    mode,
    onCreated,
    onClose,
    runUploadQueue,
    validate,
  ]);

  const hasFailedUploads = uploadQueue.some((item) => item.status === "failed");
  const hasQueuedUploads = uploadQueue.some((item) => item.status === "queued");

  if (!open) return null;

  const closeBlocked = isSubmitting || isUploadRunning;

  return (
    <Modal
      onClose={onClose}
      labelledBy="create-project-doc-modal-title"
      closeDisabled={closeBlocked}
      initialFocusRef={firstFocusableRef}
      className="p-0 flex flex-col"
    >
      <div className="flex items-center justify-between px-5 py-4 border-b border-[var(--g5-stroke)]">
        <div>
          <h2
            id="create-project-doc-modal-title"
            className="text-lg font-semibold text-[var(--g5-text0)]"
          >
            {title}
          </h2>
          <p className="mt-1 text-xs text-[var(--g5-text2)]">
            {transcriptMode
              ? "Upload a meeting transcript (.txt, .md, .vtt or .srt) or pick one already in Project Docs. The Business Engineering agent extracts its decisions into cards you decide from."
              : "Create a plain markdown document or upload existing Markdown, HTML, or JSONL files."}
          </p>
        </div>
        <button
          ref={firstFocusableRef}
          data-testid="create-doc-close"
          onClick={onClose}
          disabled={closeBlocked}
          className="p-1 rounded-[var(--g5-radius-sm)] text-[var(--g5-text2)] hover:text-[var(--g5-text0)] hover:bg-[var(--g5-bg2)] disabled:cursor-not-allowed disabled:opacity-50"
          aria-label="Close"
        >
          <X size={18} />
        </button>
      </div>

      <div className="px-5 py-4 space-y-5 flex-1 min-h-0 overflow-y-auto">
        {transcriptMode ? null : (
          <div>
            <label className="block text-sm font-medium text-[var(--g5-text1)] mb-2">
              Doc Kind
            </label>
            <div className="grid grid-cols-2 gap-3">
              <button
                type="button"
                data-testid="create-doc-mode-plain"
                onClick={() => setMode("plain")}
                disabled={isUploadRunning}
                className={classNames(
                  "rounded-[var(--g5-radius-md)] border p-3 text-left transition-all",
                  mode === "plain"
                    ? "ring-2 ring-[var(--g5-active)] bg-[var(--g5-bg2)] border-transparent"
                    : "border-[var(--g5-stroke)] bg-[var(--g5-bg1)] hover:bg-[var(--g5-bg2)]",
                )}
              >
                <div className="text-sm font-medium text-[var(--g5-text0)]">
                  Plain doc
                </div>
                <div className="mt-1 text-xs text-[var(--g5-text2)]">
                  Create a normal markdown document with no initial ontology
                  links.
                </div>
              </button>
              <button
                type="button"
                data-testid="create-doc-mode-upload"
                onClick={() => setMode("upload")}
                disabled={isUploadRunning}
                className={classNames(
                  "rounded-[var(--g5-radius-md)] border p-3 text-left transition-all",
                  mode === "upload"
                    ? "ring-2 ring-[var(--g5-active)] bg-[var(--g5-bg2)] border-transparent"
                    : "border-[var(--g5-stroke)] bg-[var(--g5-bg1)] hover:bg-[var(--g5-bg2)]",
                )}
              >
                <div className="flex items-center gap-2 text-sm font-medium text-[var(--g5-text0)]">
                  <Upload size={15} />
                  <span>Upload a doc</span>
                </div>
                <div className="mt-1 text-xs text-[var(--g5-text2)]">
                  Add existing Markdown, HTML, or JSONL files from your
                  computer.
                </div>
              </button>
            </div>
          </div>
        )}

        {transcriptMode && (
          <div>
            <label className="block text-sm font-medium text-[var(--g5-text1)] mb-2">
              Transcript
            </label>
            <div className="grid grid-cols-2 gap-3">
              <button
                type="button"
                data-testid="transcript-source-upload"
                aria-pressed={transcriptSource === "upload"}
                onClick={() => setTranscriptSource("upload")}
                disabled={isUploadRunning}
                className={classNames(
                  "rounded-[var(--g5-radius-md)] border p-3 text-left transition-all",
                  transcriptSource === "upload"
                    ? "ring-2 ring-[var(--g5-active)] bg-[var(--g5-bg2)] border-transparent"
                    : "border-[var(--g5-stroke)] bg-[var(--g5-bg1)] hover:bg-[var(--g5-bg2)]",
                )}
              >
                <div className="flex items-center gap-2 text-sm font-medium text-[var(--g5-text0)]">
                  <Upload size={15} />
                  <span>Upload a transcript</span>
                </div>
                <div className="mt-1 text-xs text-[var(--g5-text2)]">
                  Stored as Markdown under meetings/, then extracted.
                </div>
              </button>
              <button
                type="button"
                data-testid="transcript-source-existing"
                aria-pressed={transcriptSource === "existing"}
                onClick={() => setTranscriptSource("existing")}
                disabled={isUploadRunning}
                className={classNames(
                  "rounded-[var(--g5-radius-md)] border p-3 text-left transition-all",
                  transcriptSource === "existing"
                    ? "ring-2 ring-[var(--g5-active)] bg-[var(--g5-bg2)] border-transparent"
                    : "border-[var(--g5-stroke)] bg-[var(--g5-bg1)] hover:bg-[var(--g5-bg2)]",
                )}
              >
                <div className="text-sm font-medium text-[var(--g5-text0)]">
                  Use a transcript in Project Docs
                </div>
                <div className="mt-1 text-xs text-[var(--g5-text2)]">
                  {existingTranscripts.length === 0
                    ? "None stored under meetings/ yet."
                    : `${existingTranscripts.length} under meetings/; extracted as it is.`}
                </div>
              </button>
            </div>
            {decisionDocuments.length > 0 && (
              <div className="mt-4">
                <p className="mb-1 text-sm font-medium text-[var(--g5-text1)]">
                  Extracted decisions
                </p>
                <ul
                  data-testid="transcript-decisions-list"
                  aria-label="Decisions documents"
                  className="max-h-40 space-y-1 overflow-y-auto"
                >
                  {decisionDocuments.map((doc) => (
                    <li key={doc.id}>
                      <button
                        type="button"
                        data-testid={`transcript-decisions-${doc.id}`}
                        onClick={() => {
                          onOpenDecisions?.(doc);
                          onClose();
                        }}
                        className="flex w-full items-center gap-2 rounded-[var(--g5-radius-sm)] px-2 py-1 text-left text-sm text-[var(--g5-info)] hover:bg-[var(--g5-bg2)]"
                      >
                        <span className="min-w-0 truncate">
                          {doc.name || doc.filePath}
                        </span>
                      </button>
                    </li>
                  ))}
                </ul>
                <p className="mt-1 text-xs text-[var(--g5-text2)]">
                  Opens the document; its cards carry the decisions already
                  made.
                </p>
              </div>
            )}
            {transcriptSource === "existing" && (
              <div className="mt-3">
                {existingTranscripts.length === 0 ? (
                  <p
                    data-testid="transcript-existing-empty"
                    className="text-xs text-[var(--g5-text2)]"
                  >
                    No transcripts are stored under meetings/ yet. Upload one
                    first.
                  </p>
                ) : (
                  <ul
                    data-testid="transcript-existing-list"
                    aria-label="Transcripts in Project Docs"
                    className="max-h-48 space-y-1 overflow-y-auto"
                  >
                    {existingTranscripts.map((doc) => (
                      <li key={doc.id}>
                        <label className="flex cursor-pointer items-center gap-2 rounded-[var(--g5-radius-sm)] px-2 py-1 text-sm text-[var(--g5-text1)] hover:bg-[var(--g5-bg2)]">
                          <input
                            type="radio"
                            name="transcript-existing"
                            data-testid={`transcript-existing-${doc.id}`}
                            checked={selectedTranscriptId === doc.id}
                            onChange={() => setSelectedTranscriptId(doc.id)}
                          />
                          <span className="min-w-0 truncate">
                            {doc.name || doc.filePath}
                          </span>
                        </label>
                      </li>
                    ))}
                  </ul>
                )}
              </div>
            )}
          </div>
        )}

        {isUploadMode && (!transcriptMode || transcriptSource === "upload") && (
          <div>
            <label className="block text-sm font-medium text-[var(--g5-text1)] mb-1">
              {transcriptMode ? "Transcript file" : "Upload Files"}{" "}
              <span className="text-[var(--g5-blocked)]">*</span>
            </label>
            <label
              data-testid="upload-file-drop-zone"
              onDragEnter={handleUploadDragEnter}
              onDragOver={handleUploadDragOver}
              onDragLeave={handleUploadDragLeave}
              onDrop={handleUploadDrop}
              className={classNames(
                "flex cursor-pointer items-center gap-3 rounded-[var(--g5-radius-md)] border px-3 py-3 text-sm transition-all",
                isUploadRunning ? "cursor-not-allowed opacity-50" : "",
                isUploadDropActive && !isUploadRunning
                  ? "border-[var(--g5-active)] bg-[var(--g5-bg2)] ring-1 ring-[var(--g5-active)]"
                  : errors.uploadFile
                    ? "border-[var(--g5-blocked)] bg-[var(--g5-bg1)]"
                    : "border-[var(--g5-stroke)] bg-[var(--g5-bg1)] hover:bg-[var(--g5-bg2)]",
              )}
            >
              <Upload size={18} className="text-[var(--g5-text2)]" />
              <span className="min-w-0 flex-1 truncate text-[var(--g5-text1)]">
                {isUploadDropActive && !isUploadRunning
                  ? "Drop files here"
                  : transcriptMode
                    ? "Choose a .txt, .md, .vtt or .srt transcript, or drag and drop it here"
                    : "Choose .md, .html, or .jsonl files, or drag and drop them here"}
              </span>
              <input
                type="file"
                accept={
                  transcriptMode
                    ? ".md,.txt,.vtt,.srt,text/plain,text/markdown,text/vtt"
                    : ".md,.html,.htm,.jsonl,text/markdown,text/html"
                }
                multiple={!transcriptMode}
                disabled={isUploadRunning}
                className="sr-only"
                aria-label={
                  transcriptMode ? "Upload a transcript" : "Upload a doc"
                }
                onChange={(e) => {
                  handleUploadSelection(Array.from(e.target.files ?? []));
                  e.target.value = "";
                }}
              />
            </label>
            <p className="mt-1 text-xs text-[var(--g5-text2)]">
              Up to {MAX_UPLOAD_DOC_FILES} files,{" "}
              {formatFileSizeMb(MAX_UPLOAD_DOC_BYTES)} each and{" "}
              {formatFileSizeMb(MAX_UPLOAD_BATCH_BYTES)} total.
            </p>
            {errors.uploadFile && (
              <p className="mt-1 text-xs text-[var(--g5-blocked)]">
                {errors.uploadFile}
              </p>
            )}
            {uploadProgress && (
              <p
                className="mt-2 text-sm text-[var(--g5-text1)]"
                role="status"
                aria-live="polite"
              >
                Uploading {uploadProgress.current} of {uploadProgress.total}…
              </p>
            )}
            {uploadQueue.length > 0 && (
              <ul className="mt-3 space-y-2" aria-label="Upload queue">
                {uploadQueue.map((item) => (
                  <li
                    key={item.id}
                    data-testid="upload-queue-item"
                    className="rounded-[var(--g5-radius-md)] border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-3 py-2"
                  >
                    <div className="flex items-start justify-between gap-3">
                      <div className="min-w-0">
                        <p className="break-all text-sm text-[var(--g5-text0)]">
                          {labelWorkspaceFolderInPath(
                            item.destination,
                            workspaceNamesById,
                          )}
                        </p>
                        <p className="mt-0.5 text-xs text-[var(--g5-text2)]">
                          {formatUploadFileSize(item.file.size)} · {item.status}
                        </p>
                        {item.error && (
                          <p className="mt-1 text-xs text-[var(--g5-blocked)]">
                            {item.error}
                          </p>
                        )}
                      </div>
                      {item.status !== "succeeded" && (
                        <button
                          type="button"
                          data-testid="create-project-doc-remove-upload-button"
                          onClick={() => removeUploadItem(item.id)}
                          disabled={isUploadRunning}
                          className="shrink-0 rounded-[var(--g5-radius-sm)] p-1 text-[var(--g5-text2)] hover:bg-[var(--g5-bg2)] hover:text-[var(--g5-text0)] disabled:cursor-not-allowed disabled:opacity-50"
                          aria-label={`Remove ${item.file.name}`}
                        >
                          <X size={14} />
                        </button>
                      )}
                    </div>
                  </li>
                ))}
              </ul>
            )}
          </div>
        )}

        {!isUploadMode && (
          <div>
            <label className="block text-sm font-medium text-[var(--g5-text1)] mb-1">
              File Name <span className="text-[var(--g5-blocked)]">*</span>
            </label>
            <input
              type="text"
              value={fileName}
              onChange={(e) => {
                setFileName(e.target.value);
                if (e.target.value.trim()) {
                  clearError("fileName");
                }
              }}
              placeholder="e.g., auth-review"
              className={classNames(
                "w-full px-3 py-2 text-sm rounded-[var(--g5-radius-md)] bg-[var(--g5-bg1)] border border-[var(--g5-stroke)]",
                "text-[var(--g5-text0)] placeholder:text-[var(--g5-text2)] focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-[var(--g5-active)]",
                errors.fileName ? "ring-1 ring-[var(--g5-blocked)]" : "",
              )}
            />
            {errors.fileName && (
              <p className="mt-1 text-xs text-[var(--g5-blocked)]">
                {errors.fileName}
              </p>
            )}
            {previewName && (
              <p className="mt-1 text-xs text-[var(--g5-text2)]">
                Preview:{" "}
                <span className="text-[var(--g5-text1)]">{previewDisplay}</span>
              </p>
            )}
          </div>
        )}
      </div>

      <div className="flex items-center justify-between px-5 py-4 border-t border-[var(--g5-stroke)]">
        <div className="space-y-1">
          {submitError && (
            <p className="text-xs text-[var(--g5-blocked)]">{submitError}</p>
          )}
        </div>
        <div className="flex items-center gap-3">
          <button
            type="button"
            data-testid="create-doc-cancel"
            onClick={onClose}
            disabled={closeBlocked}
            className="px-4 py-2 text-sm font-medium rounded-[var(--g5-radius-md)] text-[var(--g5-text1)] hover:bg-[var(--g5-bg2)] disabled:cursor-not-allowed disabled:opacity-50"
          >
            Cancel
          </button>
          {isUploadMode && hasFailedUploads && !isUploadRunning && (
            <button
              type="button"
              data-testid="retry-failed-uploads"
              onClick={() => void runUploadQueue(true)}
              disabled={liveBlockedReason !== null}
              title={liveBlockedReason ?? undefined}
              className="px-4 py-2 text-sm font-medium rounded-[var(--g5-radius-md)] border border-[var(--g5-stroke)] text-[var(--g5-text1)] hover:bg-[var(--g5-bg2)] disabled:cursor-not-allowed disabled:opacity-50"
            >
              Retry failed
            </button>
          )}
          {/* A stored transcript only seeds the extraction chat, which the
              composer's Start and send can carry; the write paths need the
              running VM (documents g5.workbench.meeting). */}
          <GatedControl
            verb="create documents"
            reason={
              useExistingTranscript || (isUploadMode && isUploadRunning)
                ? null
                : liveBlockedReason
            }
            testId="create-doc-start-hint"
          >
            <button
              type="button"
              data-testid="create-doc-submit"
              onClick={
                useExistingTranscript
                  ? handleUseExistingTranscript
                  : isUploadMode && isUploadRunning
                    ? requestUploadStop
                    : handleSubmit
              }
              title={
                useExistingTranscript || (isUploadMode && isUploadRunning)
                  ? undefined
                  : (liveBlockedReason ?? undefined)
              }
              disabled={
                useExistingTranscript
                  ? selectedTranscript === null
                  : isUploadMode && isUploadRunning
                    ? stopRequested
                    : liveBlockedReason !== null ||
                      (isUploadMode
                        ? isSubmitting || !hasQueuedUploads
                        : isSubmitting || createDoc.isPending)
              }
              className="px-4 py-2 text-sm font-medium rounded-[var(--g5-radius-md)] bg-[var(--g5-active)] text-white hover:opacity-90 disabled:opacity-50 disabled:cursor-not-allowed"
            >
              {useExistingTranscript
                ? "Extract decisions"
                : isUploadMode && isUploadRunning
                  ? stopRequested
                    ? "Stopping…"
                    : "Stop"
                  : createDoc.isPending
                    ? "Creating…"
                    : transcriptMode
                      ? "Upload and extract decisions"
                      : isUploadMode
                        ? uploadQueue.length <= 1
                          ? "Upload Doc"
                          : "Upload Docs"
                        : "Create Doc"}
            </button>
          </GatedControl>
        </div>
      </div>
    </Modal>
  );
});
