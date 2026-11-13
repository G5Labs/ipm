export const SCM_OAUTH_SETTINGS_PARAM = "settings";
export const SCM_OAUTH_SETTINGS_VALUE = "scm";

export function buildScmOAuthReturnTo(location: Location): string {
  const searchParams = new URLSearchParams(location.search);
  searchParams.set(SCM_OAUTH_SETTINGS_PARAM, SCM_OAUTH_SETTINGS_VALUE);

  const search = searchParams.toString();
  return `${location.pathname}${search ? `?${search}` : ""}${location.hash}`;
}

export function isScmSettingsReturn(searchParams: URLSearchParams): boolean {
  return (
    searchParams.get(SCM_OAUTH_SETTINGS_PARAM) === SCM_OAUTH_SETTINGS_VALUE
  );
}

export function clearScmOAuthParams(
  searchParams: URLSearchParams,
): URLSearchParams {
  const next = new URLSearchParams(searchParams);
  next.delete("scm_connected");
  next.delete("scm_error");
  // The `settings=scm` hint routes the OAuth return to the workspace settings
  // surface (now a center-panel tab opened by WorkspacePage). Once the callback
  // params are consumed there is nothing left to route, so drop it too — this
  // keeps `WorkspaceSettingsContent` the single mutator of the SCM return URL
  // and avoids a race with WorkspacePage stripping the same param.
  next.delete(SCM_OAUTH_SETTINGS_PARAM);
  return next;
}

export function scmConnectionErrorMessage(reason: string): string {
  switch (reason) {
    case "github_email_lookup_failed":
      return "GitHub account access worked, but the server tried to read email data during SCM setup. Retry after updating G5.";
    case "oauth_token_exchange_failed":
      return "GitHub approved the request, but G5 could not exchange the callback code. Check the SCM OAuth client ID, secret, and callback URL.";
    case "scm_encryption_key_missing":
      return "GitHub approved the request, but G5 cannot store SCM tokens because the SCM encryption key is not configured.";
    case "oauth_state_invalid":
      return "The GitHub connection session expired. Start Connect GitHub again.";
    default:
      return "Failed to connect SCM provider. Please try again.";
  }
}
