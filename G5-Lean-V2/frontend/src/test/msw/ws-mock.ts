// =============================================================================
// Mock WebSocket Controller — Test WebSocket Simulation
// =============================================================================
// Provides MockWsController for simulating WebSocket events in tests and
// MockWebSocketProvider for replacing the real WebSocketProvider in the
// component tree.
//
// Does NOT use a real WebSocket connection — provides a programmatic API for
// pushing events to subscribers.
// =============================================================================

import React from "react";
import { WebSocketProvider, applyLiveTimelineRow } from "../../lib/ws";
import type { WebSocketManager } from "../../lib/ws";
import type { QueryClient } from "@tanstack/react-query";
import type { ChatMessageDTO } from "../../lib/types";

// ---------------------------------------------------------------------------
// WsEvent Interface
// ---------------------------------------------------------------------------

export interface WsEvent<T = unknown> {
  channel: string;
  event: string;
  payload: T;
}

// ---------------------------------------------------------------------------
// MockWsController Class
// ---------------------------------------------------------------------------

/**
 * Simulates WebSocket events for integration tests. Provides a programmatic
 * API for pushing events to subscribers without a real WebSocket connection.
 */
export class MockWsController {
  private listeners: Map<string, Set<(event: WsEvent) => void>>;
  private eventListeners: Map<
    string,
    Set<(event: Record<string, unknown>) => void>
  >;
  private subscriptions: Set<string>;
  private _connected: boolean;
  /** Optional QueryClient — when set, emit() mirrors real WebSocketManager cache updates */
  queryClient: QueryClient | null = null;

  constructor() {
    this.listeners = new Map();
    this.eventListeners = new Map();
    this.subscriptions = new Set();
    this._connected = true;
  }

  /** Push a WebSocket event to all listeners on the matching channel */
  emit(event: WsEvent): void {
    if (!this._connected) return;
    const channelListeners = this.listeners.get(event.channel);
    if (channelListeners) {
      for (const listener of channelListeners) {
        listener(event);
      }
    }
    const typedListeners = this.eventListeners.get(event.event);
    if (typedListeners) {
      const typedEvent = {
        type: event.event,
        ...(event.payload as Record<string, unknown>),
      };
      for (const listener of typedListeners) {
        listener(typedEvent);
      }
    }
    // Mirror real WebSocketManager cache invalidation/update logic
    if (this.queryClient) {
      this.handleEventForCache(event);
    }
  }

  private invalidateChatSessionCaches(
    sessionId?: string,
    options?: { includeTaskState?: boolean },
  ): void {
    const qc = this.queryClient;
    if (!qc) return;

    if (sessionId) {
      qc.invalidateQueries({ queryKey: ["chat-messages", sessionId] });
      qc.invalidateQueries({ queryKey: ["session-timeline", sessionId] });
      qc.invalidateQueries({ queryKey: ["chat-session", sessionId] });
    } else {
      qc.invalidateQueries({ queryKey: ["chat-messages"] });
      qc.invalidateQueries({ queryKey: ["session-timeline"] });
      qc.invalidateQueries({ queryKey: ["chat-session"] });
    }
    qc.invalidateQueries({ queryKey: ["chat-sessions"] });

    if (options?.includeTaskState) {
      qc.invalidateQueries({ queryKey: ["workspace-tasks"] });
      qc.invalidateQueries({ queryKey: ["workspace-tasks"] });
    }
  }

  private invalidateSubscribedCachesForReconnect(): void {
    const qc = this.queryClient;
    if (!qc) return;

    for (const channel of this.subscriptions) {
      const colonIdx = channel.indexOf(":");
      if (colonIdx === -1) continue;
      const channelType = channel.substring(0, colonIdx);
      const channelId = channel.substring(colonIdx + 1);

      switch (channelType) {
        case "workspace":
          qc.invalidateQueries({
            queryKey: ["source-control-checkpoint", channelId],
          });
          qc.invalidateQueries({ queryKey: ["workspace", channelId] });
          qc.invalidateQueries({ queryKey: ["workspace-tasks"] });
          qc.invalidateQueries({ queryKey: ["workspace-tasks"] });
          break;
        case "task":
          qc.invalidateQueries({ queryKey: ["task", channelId] });
          qc.invalidateQueries({ queryKey: ["task-timeline", channelId] });
          break;
        case "chat":
          this.invalidateChatSessionCaches(channelId, {
            includeTaskState: true,
          });
          break;
        default:
          break;
      }
    }
  }

  /**
   * Route WsEvent payloads to QueryClient cache operations, mirroring the
   * real WebSocketManager.handleEvent() behavior.
   */
  private handleEventForCache(event: WsEvent): void {
    const qc = this.queryClient!;
    const payload = event.payload as Record<string, unknown>;
    const eventType = event.event;

    switch (eventType) {
      case "workspace.source_control_saved":
        qc.setQueryData(["source-control-checkpoint", payload.workspaceId], {
          generation: payload.generation,
        });
        break;
      case "task.status":
        qc.invalidateQueries({ queryKey: ["task", payload.taskId] });
        qc.invalidateQueries({ queryKey: ["workspace-tasks"] });
        qc.invalidateQueries({ queryKey: ["workspace-tasks"] });
        qc.invalidateQueries({
          queryKey: ["pending-sync-files"],
          refetchType: "all",
        });
        qc.invalidateQueries({ queryKey: ["spend"] });
        qc.invalidateQueries({ queryKey: ["workspace-stats"] });
        qc.invalidateQueries({ queryKey: ["workspaces"] });
        if (
          payload.status === "waiting" ||
          payload.status === "succeeded" ||
          payload.status === "failed" ||
          payload.status === "cancelled"
        ) {
          this.invalidateChatSessionCaches();
        }
        break;
      case "task.timeline":
        qc.invalidateQueries({ queryKey: ["task-timeline", payload.taskId] });
        break;
      case "chat.timeline":
        // Shared with the real manager: one implementation of the live-row
        // placement rules (chat g5.chat.replay).
        applyLiveTimelineRow(qc, payload.sessionId as string, payload.entry);
        break;
      case "chat.chunk":
        // Optimistic update: append delta to the streamed assistant message
        // identified by messageId (not the array tail) — mirrors the real
        // WebSocketManager.handleChatChunk and websocket D15.
        qc.setQueryData(
          ["chat-messages", payload.sessionId],
          (old: ChatMessageDTO[] | undefined) => {
            if (!old) return old;
            return old.map((message) =>
              message.id === payload.messageId
                ? {
                    ...message,
                    content: message.content + (payload.delta as string),
                    status:
                      message.status === "pending"
                        ? "streaming"
                        : message.status,
                  }
                : message,
            );
          },
        );
        break;
      case "chat.done":
        qc.setQueryData(
          ["chat-messages", payload.sessionId],
          (old: ChatMessageDTO[] | undefined) =>
            old?.map((message) =>
              message.id === payload.messageId
                ? { ...message, status: "complete" }
                : message,
            ),
        );
        this.invalidateChatSessionCaches(payload.sessionId as string);
        break;
      case "chat.message_updated":
        this.invalidateChatSessionCaches(payload.sessionId as string);
        break;
      case "chat.error":
        // Status-only cache transition: preserve any partial streamed content;
        // do not clobber the message body with the raw error string — mirrors
        // the real WebSocketManager and websocket D15.
        qc.setQueryData(
          ["chat-messages", payload.sessionId],
          (old: ChatMessageDTO[] | undefined) =>
            old?.map((message) =>
              message.id === payload.messageId
                ? { ...message, status: "error" }
                : message,
            ),
        );
        this.invalidateChatSessionCaches(payload.sessionId as string);
        break;
      case "comment.new":
        qc.invalidateQueries({ queryKey: ["comments", payload.nodeId] });
        break;
      case "workspace.sync":
        qc.invalidateQueries({ queryKey: ["workspace", payload.workspaceId] });
        qc.invalidateQueries({ queryKey: ["file-tree"] });
        qc.invalidateQueries({ queryKey: ["file-content"] });
        break;
      case "workspace.branch_updated":
        qc.invalidateQueries({ queryKey: ["workspace", payload.workspaceId] });
        qc.invalidateQueries({ queryKey: ["workspaces"] });
        qc.invalidateQueries({ queryKey: ["workspace-tasks"] });
        qc.invalidateQueries({ queryKey: ["file-tree"] });
        qc.invalidateQueries({ queryKey: ["file-content"] });
        qc.invalidateQueries({ queryKey: ["source-control-history"] });
        qc.invalidateQueries({ queryKey: ["docs", payload.workspaceId] });
        qc.invalidateQueries({ queryKey: ["views", payload.workspaceId] });
        break;
      case "workspace.settings_updated":
        qc.invalidateQueries({ queryKey: ["settings", payload.workspaceId] });
        break;
      case "workspace.spend_limit_reached":
      case "workspace.spend_limit_cleared":
        qc.invalidateQueries({ queryKey: ["spend", payload.workspaceId] });
        break;
      case "notification":
        qc.invalidateQueries({
          queryKey: ["notifications", payload.workspaceId],
        });
        break;
      default:
        break;
    }
  }

  /** Subscribe a listener to a specific channel */
  subscribe(channel: string, listener: (event: WsEvent) => void): () => void {
    this.subscriptions.add(channel);
    if (!this.listeners.has(channel)) {
      this.listeners.set(channel, new Set());
    }
    this.listeners.get(channel)!.add(listener);

    // Return unsubscribe function
    return () => {
      const set = this.listeners.get(channel);
      if (set) {
        set.delete(listener);
        if (set.size === 0) {
          this.listeners.delete(channel);
        }
      }
    };
  }

  /** Assert that a subscription was made to a specific channel */
  expectSubscription(channel: string): void {
    if (!this.subscriptions.has(channel)) {
      throw new Error(
        `Expected subscription to channel "${channel}", but none was found. ` +
          `Active subscriptions: [${Array.from(this.subscriptions).join(", ")}]`,
      );
    }
  }

  /** Get all active subscription channel names */
  getSubscriptions(): string[] {
    return Array.from(this.subscriptions);
  }

  getEventListenerCount(eventType: string): number {
    return this.eventListeners.get(eventType)?.size ?? 0;
  }

  on(
    eventType: string,
    listener: (event: Record<string, unknown>) => void,
  ): void {
    if (!this.eventListeners.has(eventType)) {
      this.eventListeners.set(eventType, new Set());
    }
    this.eventListeners.get(eventType)!.add(listener);
  }

  off(
    eventType: string,
    listener: (event: Record<string, unknown>) => void,
  ): void {
    const listeners = this.eventListeners.get(eventType);
    if (!listeners) {
      return;
    }
    listeners.delete(listener);
    if (listeners.size === 0) {
      this.eventListeners.delete(eventType);
    }
  }

  /** Clear all subscriptions and listeners (for cleanup between tests) */
  reset(): void {
    this.listeners.clear();
    this.eventListeners.clear();
    this.subscriptions.clear();
    this._connected = true;
  }

  /** Simulate WebSocket disconnection */
  simulateDisconnect(): void {
    this._connected = false;
  }

  /** Simulate WebSocket reconnection (re-triggers subscription callbacks) */
  simulateReconnect(): void {
    this._connected = true;
    this.invalidateSubscribedCachesForReconnect();
  }
}

// ---------------------------------------------------------------------------
// MockWebSocketProvider
// ---------------------------------------------------------------------------

export interface MockWebSocketProviderProps {
  controller: MockWsController;
  children: React.ReactNode;
}

/**
 * Creates a mock WebSocketManager that satisfies the real WebSocketProvider
 * interface but delegates subscription operations to the MockWsController.
 * This ensures that components using useWebSocket() / useWSChannel() find
 * a valid context value from the real WebSocketContext.
 */
function createMockManager(controller: MockWsController): WebSocketManager {
  // Create a minimal object that satisfies the WebSocketManager interface
  // as consumed by useWebSocket (connected, subscribeToState, getStateSnapshot,
  // subscribe, unsubscribe).
  const stateListeners = new Set<() => void>();
  const stateVersion = 0;

  const manager = {
    get connected() {
      return true;
    },
    get reconnecting() {
      return false;
    },
    get subscriptions(): ReadonlySet<string> {
      return new Set(controller.getSubscriptions());
    },
    subscribe(channel: string) {
      controller.subscribe(channel, () => {});
    },
    unsubscribe(_channel: string) {
      // No-op in mock
    },
    on(eventType: string, handler: unknown) {
      controller.on(
        eventType,
        handler as (event: Record<string, unknown>) => void,
      );
    },
    off(eventType: string, handler: unknown) {
      controller.off(
        eventType,
        handler as (event: Record<string, unknown>) => void,
      );
    },
    connect: async () => {},
    disconnect: () => {},
    destroy: () => {},
    subscribeToState: (listener: () => void) => {
      stateListeners.add(listener);
      return () => {
        stateListeners.delete(listener);
      };
    },
    getStateSnapshot: () => stateVersion,
  } as unknown as WebSocketManager;

  return manager;
}

export function MockWebSocketProvider({
  controller,
  children,
}: MockWebSocketProviderProps): React.JSX.Element {
  const manager = React.useMemo(
    () => createMockManager(controller),
    [controller],
  );

  return React.createElement(WebSocketProvider, { manager, children });
}

/**
 * Hook to access the MockWsController from test components.
 * Kept for backward compatibility — stores controller in its own context.
 */
const MockWebSocketContext = React.createContext<MockWsController | null>(null);

export function MockWsControllerProvider({
  controller,
  children,
}: MockWebSocketProviderProps): React.JSX.Element {
  return React.createElement(
    MockWebSocketContext.Provider,
    { value: controller },
    children,
  );
}

export function useMockWebSocket(): MockWsController {
  const controller = React.useContext(MockWebSocketContext);
  if (!controller) {
    throw new Error(
      "useMockWebSocket must be used within a MockWebSocketProvider",
    );
  }
  return controller;
}
