// =============================================================================
// G5 Client — WebSocket Singleton & Channel Hooks
// =============================================================================
// Re-exports from the WebSocket module (client/src/lib/ws.ts).
// useWebSocket is the singleton connection manager.
// useWSChannel auto-subscribes on mount and unsubscribes on unmount.
// =============================================================================

export { useWebSocket, useWSChannel, useWSEventListener } from "../lib/ws";
