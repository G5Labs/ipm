import { ApiError } from "../../lib/api";
import type { CoordinatedRepoEvidence } from "../../lib/types";

export interface CoordinatedRepoSyncModalState {
  title: string;
  message: string;
  repos?: CoordinatedRepoEvidence[];
}

export function coordinatedModalFromError(
  error: unknown,
  fallbackTitle: string,
): CoordinatedRepoSyncModalState {
  if (error instanceof ApiError) {
    const modal = error.details.modal;
    if (isModalPayload(modal)) {
      return {
        title: modal.title,
        message: modal.message,
        repos: modal.repos,
      };
    }
    const message =
      typeof error.details.message === "string"
        ? error.details.message
        : error.message;
    return { title: fallbackTitle, message };
  }
  return {
    title: fallbackTitle,
    message: error instanceof Error ? error.message : "The operation failed.",
  };
}

function isModalPayload(value: unknown): value is {
  title: string;
  message: string;
  repos?: CoordinatedRepoEvidence[];
} {
  if (!value || typeof value !== "object") return false;
  const record = value as Record<string, unknown>;
  return typeof record.title === "string" && typeof record.message === "string";
}
