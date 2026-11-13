const POST_AUTH_REDIRECT_KEY = "g5_post_auth_redirect";

function normalizeAppPath(path: string | null | undefined): string | null {
  if (!path || !path.startsWith("/")) {
    return null;
  }
  const pathname = path.split(/[?#]/, 1)[0];
  if (
    path === "/" ||
    path === "/login" ||
    path === "/landing" ||
    path === "/coming-soon.html" ||
    pathname === "/home"
  ) {
    return null;
  }
  return path;
}

export function rememberPostAuthRedirect(
  path: string | null | undefined,
): void {
  const normalized = normalizeAppPath(path);
  if (!normalized) {
    sessionStorage.removeItem(POST_AUTH_REDIRECT_KEY);
    return;
  }
  sessionStorage.setItem(POST_AUTH_REDIRECT_KEY, normalized);
}

export function getPostAuthRedirect(): string | null {
  const value = sessionStorage.getItem(POST_AUTH_REDIRECT_KEY);
  const normalized = normalizeAppPath(value);
  if (value && !normalized) {
    sessionStorage.removeItem(POST_AUTH_REDIRECT_KEY);
  }
  return normalized;
}

export function clearPostAuthRedirect(): void {
  sessionStorage.removeItem(POST_AUTH_REDIRECT_KEY);
}
