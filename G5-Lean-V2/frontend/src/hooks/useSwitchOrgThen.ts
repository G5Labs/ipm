// =============================================================================
// G5 Client — Switch the session organization, then continue
// =============================================================================
// Organization-scoped writes (forking, creating a workspace) run under the
// organization's JWT. Pages that act across organizations — the projects
// page, the fork picker — switch first and continue only on success,
// reporting the failure per organization. One hook owns that sequence.
// =============================================================================

import { useCallback, useState } from "react";

import { useAuth } from "./useAuth";

export interface SwitchOrgThen {
  /** Switch to `org`, then run `onReady`; a failure sets `error` instead. */
  run: (org: { orgId: string; orgName: string }, onReady: () => void) => void;
  /** The organization a switch is in flight for, if any. */
  pendingOrgId: string | null;
  /** The last switch failure's message, cleared by the next attempt. */
  error: string | null;
  clearError: () => void;
}

export function useSwitchOrgThen(): SwitchOrgThen {
  const { switchOrg } = useAuth();
  const [pendingOrgId, setPendingOrgId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const run = useCallback<SwitchOrgThen["run"]>(
    (org, onReady) => {
      setError(null);
      setPendingOrgId(org.orgId);
      void switchOrg(org.orgId)
        .then(onReady)
        .catch(() => setError(`Could not switch to ${org.orgName}.`))
        .finally(() => setPendingOrgId(null));
    },
    [switchOrg],
  );

  const clearError = useCallback(() => setError(null), []);

  return { run, pendingOrgId, error, clearError };
}
