/** Only local application destinations may survive the GitHub OAuth round trip. */
export function githubConnectionReturnPath(path: string | null): string {
  if (
    !path?.startsWith("/") ||
    path.startsWith("//") ||
    path.includes("\\") ||
    Array.from(path).some(
      (character) =>
        character.charCodeAt(0) < 32 || character.charCodeAt(0) === 127,
    ) ||
    path.length > 1500
  ) {
    return "/";
  }
  const pathname = new URL(path, "https://g5.invalid").pathname;
  return pathname === "/account/github" || pathname === "/login" ? "/" : path;
}

export function githubConnectionPath(returnTo: string): string {
  return `/account/github?returnTo=${encodeURIComponent(githubConnectionReturnPath(returnTo))}`;
}

export function isGithubConnectionPath(path: string | null): boolean {
  return !!path && path.split(/[?#]/, 1)[0] === "/account/github";
}

const CONNECTION_REQUIRED = "github_user_connection_required: ";

export function githubPrConnectionRequired(
  error: string | null | undefined,
): boolean {
  return !!error?.includes(CONNECTION_REQUIRED);
}

export function formatGithubPrError(error: string): string {
  const marker = error.indexOf(CONNECTION_REQUIRED);
  return marker < 0 ? error : error.slice(marker + CONNECTION_REQUIRED.length);
}
