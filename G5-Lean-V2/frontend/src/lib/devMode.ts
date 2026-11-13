// =============================================================================
// G5 Client — Dev Mode Feature Flag
// =============================================================================
// Enable via VITE_DEV_MODE=true in .env or environment variables.
//
// Single remaining consumer: the login page offers the passwordless dev login
// button on this flag while the instance info request has not succeeded
// (App.tsx). The dev-mode notification settings section and the /ide mockup
// route that this flag used to gate were removed (Lean P8.2).
// =============================================================================

/**
 * Returns true when the app is running in developer mode.
 * Checks the VITE_DEV_MODE environment variable.
 */
export function isDevMode(): boolean {
  try {
    return import.meta.env.VITE_DEV_MODE === "true";
  } catch {
    return false;
  }
}
