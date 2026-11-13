// =============================================================================
// MSW Server Instance — Test API Mock Server
// =============================================================================
// Creates and exports the MSW server instance configured with baseline handlers.
// Used by setup.ts for vitest lifecycle management.
// =============================================================================

import { setupServer } from "msw/node";
import { handlers } from "./handlers";

export const server = setupServer(...handlers);
