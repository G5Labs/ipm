// =============================================================================
// G5 Shared Components — Comment Time Formatting
// =============================================================================

/**
 * Compact timestamp formatter for comment UIs.
 * Examples: now, 8m, 3h, 2d, Jan 5, Jan 5, 24
 */
export function formatCommentTime(isoString: string): string {
  const date = new Date(isoString);
  if (Number.isNaN(date.getTime())) {
    return isoString;
  }

  const now = Date.now();
  const diffMs = Math.max(0, now - date.getTime());
  const sec = Math.floor(diffMs / 1000);
  if (sec < 60) return "now";

  const min = Math.floor(sec / 60);
  if (min < 60) return `${min}m`;

  const hours = Math.floor(min / 60);
  if (hours < 24) return `${hours}h`;

  const days = Math.floor(hours / 24);
  if (days < 7) return `${days}d`;

  const sameYear = date.getFullYear() === new Date(now).getFullYear();
  return new Intl.DateTimeFormat("en-US", {
    month: "short",
    day: "numeric",
    ...(sameYear ? {} : { year: "2-digit" }),
  }).format(date);
}
