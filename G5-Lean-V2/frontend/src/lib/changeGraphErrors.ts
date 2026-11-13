import type { ApiError } from "./api";

export interface DiffGraphTooLargeMessageOptions {
  subject: string;
  fallback: string;
}

function isNonNegativeSafeInteger(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
}

function isPositiveSafeInteger(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value > 0;
}

function formatMebibytes(bytes: number): string {
  const value = bytes / (1024 * 1024);
  return `${Number(value.toFixed(1))} MiB`;
}

export function formatDiffGraphTooLargeMessage(
  error: ApiError,
  options: DiffGraphTooLargeMessageOptions,
): string {
  const observedRoots = error.details.observedRoots;
  const maxRoots = error.details.maxRoots;
  const observedBytes = error.details.observedBytes;
  const maxBytes = error.details.maxBytes;

  if (
    !isNonNegativeSafeInteger(observedRoots) ||
    !isPositiveSafeInteger(maxRoots) ||
    !isNonNegativeSafeInteger(observedBytes) ||
    !isPositiveSafeInteger(maxBytes)
  ) {
    return options.fallback;
  }

  const rootsExceeded = observedRoots > maxRoots;
  const bytesExceeded = observedBytes > maxBytes;
  if (rootsExceeded && bytesExceeded) {
    return `${options.subject} exceeds both supported limits: ${observedRoots} changed artifacts (limit ${maxRoots}) and ${formatMebibytes(observedBytes)} of diff input (limit ${formatMebibytes(maxBytes)}).`;
  }
  if (rootsExceeded) {
    return `${options.subject} has ${observedRoots} changed artifacts; the limit is ${maxRoots}.`;
  }
  if (bytesExceeded) {
    return `${options.subject} requires ${formatMebibytes(observedBytes)} of diff input; the limit is ${formatMebibytes(maxBytes)}.`;
  }
  return options.fallback;
}
