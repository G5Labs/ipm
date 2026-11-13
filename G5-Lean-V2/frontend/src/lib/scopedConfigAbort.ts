// Abort seam for in-flight scoped-config requests (settings-page D9).
//
// Scoped-config PUT responses carry secret key names and bounded display
// hints, so a request issued under one auth principal must never settle its
// response into client state after a principal transition (logout, session
// expiry, org switch, or a periodic refresh that changed identity).
// Clearing React Query's caches is not enough: a running mutation keeps its
// observer and would still deliver the old principal's response. Every
// scoped-config mutation therefore attaches the CURRENT signal from this
// module, and the auth transition seam aborts the controller before
// evicting the caches.

let controller = new AbortController();

/** The abort signal scoped-config mutations must attach to their requests. */
export function scopedConfigSignal(): AbortSignal {
  return controller.signal;
}

/**
 * Abort every in-flight scoped-config request and arm a fresh controller.
 * Called by the auth principal-transition seam before cache eviction.
 */
export function abortScopedConfigRequests(): void {
  const previous = controller;
  controller = new AbortController();
  previous.abort();
}
