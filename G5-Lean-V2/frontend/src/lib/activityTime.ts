export function formatActivityRelativeTime(
  source: string,
  now: Date = new Date(),
): string {
  const elapsedSeconds = Math.max(
    0,
    Math.floor((now.getTime() - new Date(source).getTime()) / 1000),
  );

  if (elapsedSeconds < 60) return "now";
  if (elapsedSeconds < 3_600) return `${Math.floor(elapsedSeconds / 60)}m ago`;
  if (elapsedSeconds < 86_400)
    return `${Math.floor(elapsedSeconds / 3_600)}h ago`;
  return `${Math.floor(elapsedSeconds / 86_400)}d ago`;
}

export function formatActivityRelativeTimeExpanded(
  source: string,
  now: Date = new Date(),
): string {
  const elapsedSeconds = Math.max(
    0,
    Math.floor((now.getTime() - new Date(source).getTime()) / 1000),
  );

  if (elapsedSeconds < 60) return "now";
  if (elapsedSeconds < 3_600) {
    const minutes = Math.floor(elapsedSeconds / 60);
    return `${minutes} ${minutes === 1 ? "minute" : "minutes"} ago`;
  }
  if (elapsedSeconds < 86_400) {
    const hours = Math.floor(elapsedSeconds / 3_600);
    return `${hours} ${hours === 1 ? "hour" : "hours"} ago`;
  }
  const days = Math.floor(elapsedSeconds / 86_400);
  return `${days} ${days === 1 ? "day" : "days"} ago`;
}
