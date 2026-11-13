// =============================================================================
// WebSocket Subscription Manager — Multiplexed Event Channel
// =============================================================================
// This module provides a client-side multiplexed WebSocket connection to the
// g5-api event channel at WS /api/v1/ws?token=<jwt>. It is the single source
// of all real-time server-pushed events in the browser client.
//
// Features:
// - Single multiplexed WebSocket connection per browser tab
// - Channel-based subscriptions with reference counting
// - Automatic reconnection with exponential backoff + jitter
// - React Query cache invalidation on events
// - Optimistic streaming for chat.chunk events
// - Custom event listener API (on/off) for side effects
// - React hooks: useWebSocket, useWSChannel
// - React context provider for dependency injection
// - 5-second grace period before disconnecting (prevents flapping)
//
// =============================================================================

import React, { useEffect, useCallback, useSyncExternalStore } from "react";
import type { QueryClient, QueryKey } from "@tanstack/react-query";
import type {
  ChatMessageDTO,
  LiveSubscriptionUsage,
  TimelineEntry,
  WorkspaceDetail,
} from "./types";
import { projectTaskTimelineEntry } from "./taskTimeline";
import {
  insertLiveRows,
  isLiveTimelineRow,
  liveRowsOpenGap,
  requestLiveTail,
  type TimelineWindow,
} from "./liveTimeline";
import { findCoveredHoles, requestTimelineHoleFill } from "./timelineHoles";
import { FileChangeCoalescer } from "./fileChangeCoalescer";
import { questionVotesQueryKey } from "./questionSet";
import { invalidateAndReadAgain } from "./readAgain";
import {
  getApiToken,
  notifyOrganizationAccessChanged,
  tokenPrincipal,
} from "./api";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const WS_BASE_DELAY = 1000; // 1 second initial reconnect delay
const WS_MAX_DELAY = 16000; // 16 second max reconnect delay
const WS_JITTER_MAX = 500; // Max jitter added to reconnect delay
const WS_DISCONNECT_GRACE = 5000; // 5 second grace period before disconnecting

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/** Handler function for WebSocket events */
export type WSEventHandler = (event: WSEvent) => void;

/** Connection state exposed to React components */
export interface WebSocketState {
  connected: boolean;
  subscribe: (channel: string) => void;
  unsubscribe: (channel: string) => void;
}

/**
 * Discriminated union of all WebSocket event types from the server.
 * The `type` field determines the shape of the rest of the event.
 */
export type WSEvent =
  | { type: "task.status"; taskId: string; status: string }
  | { type: "task.timeline"; taskId: string; entry: unknown }
  | {
      type: "chat.timeline";
      sessionId: string;
      taskId: string;
      entry: unknown;
    }
  | {
      type: "workspace.sync";
      workspaceId: string;
      behindBase: number;
      aheadOfBase: number;
    }
  | {
      type: "workspace.branch_updated";
      workspaceId: string;
      repoId?: string | null;
      branch: string;
      headCommit: string;
    }
  | {
      type: "workspace.source_control_saved";
      workspaceId: string;
      generation: number;
    }
  | {
      type: "workspace.pull_request_updated";
      workspaceId: string;
      pullRequestId: string;
    }
  | {
      type: "workspace.flush_completed";
      workspaceId: string;
      flushId: string;
    }
  | {
      type: "workspace.file_changed";
      workspaceId: string;
      repoId: string;
      path: string;
      rescanRequired: boolean;
    }
  | {
      type: "workspace.ownership_transferred";
      workspaceId: string;
      fromUserId: string;
      toUserId: string;
      transferId: string;
    }
  | {
      type: "workspace.remote_tip_advisory_updated";
      workspaceId: string;
      repoId?: string | null;
      remoteTipAdvisorySha: string;
    }
  | {
      type: "chat.chunk";
      sessionId: string;
      messageId: string;
      delta: string;
      messageIndex?: number;
      createdAt?: string;
    }
  | { type: "chat.message_updated"; sessionId: string; messageId: string }
  | {
      type: "chat.subscription_usage";
      sessionId: string;
      authMode: LiveSubscriptionUsage["authMode"];
      accountId: LiveSubscriptionUsage["accountId"];
      usage: LiveSubscriptionUsage["usage"];
    }
  | {
      type: "chat.done";
      sessionId: string;
      messageId: string;
      usage: unknown;
      citations: unknown[];
    }
  | {
      type: "chat.error";
      sessionId: string;
      messageId: string;
      error: string;
      message: string;
    }
  | {
      type: "comment.new";
      workspaceId: string;
      commentId: string;
      scope: string;
      nodeId: string;
      originBranch: string;
    }
  | {
      type: "comment.reply";
      workspaceId: string;
      commentId: string;
      replyId: string;
      authorName: string;
    }
  | {
      // A vote on a referenced question was cast, changed or withdrawn
      // (`g5.documents.question.votes`); the set's tally is read again.
      type: "question.vote";
      workspaceId: string;
      path: string;
      questionId: string;
    }
  | {
      type: "comment.updated";
      workspaceId: string;
      commentId: string;
      resolved: boolean;
    }
  | { type: "comment.deleted"; workspaceId: string; commentId: string }
  | {
      type: "workspace.spend_limit_reached";
      workspaceId: string;
      periodSpendUsd: number;
      limitSpendUsd: number;
      reason: string;
    }
  | {
      type: "workspace.spend_limit_cleared";
      workspaceId: string;
      reason: string;
    }
  | {
      type: "workspace.settings_updated";
      workspaceId: string;
      updatedBy: string;
      changes: unknown[];
    }
  | {
      // Content-free: a chat's turn activity or this person's inspection of
      // it changed; the session list is the authority (chat
      // g5.chat.activity.refresh). No sessionId when several chats changed.
      type: "chat.activity";
      workspaceId: string;
      sessionId?: string;
    }
  | { type: "notification"; workspaceId: string; notification: unknown }
  | { type: "doc.content_changed"; workspaceId: string; docId: string }
  | { type: "doc.created"; workspaceId: string; docId: string }
  | { type: "doc.deleted"; workspaceId: string; docId: string }
  | { type: "subscribed"; channel: string }
  | { type: "unsubscribed"; channel: string }
  | { type: "error"; code: string; message: string };

// ---------------------------------------------------------------------------
// WebSocketManager Class
// ---------------------------------------------------------------------------

/**
 * Manages a single multiplexed WebSocket connection for real-time events.
 *
 * Usage:
 * ```ts
 * const manager = new WebSocketManager({
 *   getBaseUrl: () => 'wss://api.g5.dev',
 *   getWsTicket: async () => { ... },
 *   queryClient,
 * });
 *
 * manager.subscribe('workspace:abc');
 * manager.on('task.status', (event) => { ... });
 * ```
 */
/** The category of a notification payload, when it carries one. */
function notificationCategory(notification: unknown): string | null {
  if (typeof notification !== "object" || notification === null) return null;
  const category = (notification as { category?: unknown }).category;
  return typeof category === "string" ? category : null;
}

/**
 * Place `chat.timeline` rows into the session's held timeline window (chat
 * g5.chat.replay): projected exactly like saved rows, deduplicated by id,
 * dropped when saved history already covers their sequence, ordered by their
 * timestamp; a stored row replaces the live row with its id (chat
 * g5.chat.live-delivery). A window not loaded yet is left alone; the tail
 * read fills it. An unchanged window is not written back: every cache write
 * reaches the window's observers as an update, and only a read of saved
 * history may trigger a tail read (g5.chat.live-tail-recovery).
 */
export function placeLiveTimelineRows(
  queryClient: Pick<QueryClient, "getQueryData" | "setQueryData">,
  sessionId: string,
  rows: readonly TimelineEntry[],
): void {
  if (!rows.length) return;
  const key = ["session-timeline", sessionId];
  const held = queryClient.getQueryData<TimelineWindow>(key);
  if (!held) return;
  const placed = insertLiveRows(held, rows);
  if (placed !== held) queryClient.setQueryData<TimelineWindow>(key, placed);
  // A row that skips past the held sequence means rows were not delivered
  // (a dropped burst): resynchronize from the tail rather than render past
  // the hole (chat g5.chat.live-tail-recovery). One signal per batch.
  if (liveRowsOpenGap(held.items, rows)) requestLiveTail(sessionId);
  // A stored row (a boundary published after its batch committed) proves
  // the rows below it saved; skipped ones are read by sequence.
  if (
    placed !== held &&
    rows.some((row) => !isLiveTimelineRow(row)) &&
    findCoveredHoles(placed.items).length
  )
    void requestTimelineHoleFill(queryClient, sessionId);
}

// Live rows are placed once per animation frame, not once per row: a burst
// of deltas is one placement, one cache write and one render of the panel,
// so draining a backlog costs the panel one render per frame rather than one
// per row. Pending rows are kept per session, in arrival order.
const LIVE_ROW_FRAME_MS = 16;

type PendingLiveRows = {
  queryClient: Pick<QueryClient, "getQueryData" | "setQueryData">;
  rows: TimelineEntry[];
};

const pendingLiveRows = new Map<string, PendingLiveRows>();
let liveRowFlush: { cancel: () => void } | null = null;

function scheduleLiveRowFlush(): void {
  if (liveRowFlush) return;
  const flush = () => {
    liveRowFlush = null;
    flushLiveTimelineRows();
  };
  if (typeof requestAnimationFrame === "function") {
    const handle = requestAnimationFrame(flush);
    liveRowFlush = { cancel: () => cancelAnimationFrame(handle) };
  } else {
    const handle = setTimeout(flush, LIVE_ROW_FRAME_MS);
    liveRowFlush = { cancel: () => clearTimeout(handle) };
  }
}

/**
 * Queue one `chat.timeline` row for the session's next placement (see
 * `placeLiveTimelineRows`); the batch is placed on the next animation frame.
 */
export function applyLiveTimelineRow(
  queryClient: Pick<QueryClient, "getQueryData" | "setQueryData">,
  sessionId: string,
  raw: unknown,
): void {
  if (!raw || typeof raw !== "object") return;
  const entry = projectTaskTimelineEntry(raw as TimelineEntry);
  if (typeof entry.id !== "string" || !entry.id) return;
  let pending = pendingLiveRows.get(sessionId);
  // Rows queued for another cache (a page that replaced its client) are
  // placed there first, in order.
  if (pending && pending.queryClient !== queryClient) {
    flushLiveTimelineRows(sessionId);
    pending = undefined;
  }
  if (!pending) {
    pending = { queryClient, rows: [] };
    pendingLiveRows.set(sessionId, pending);
  }
  pending.rows.push(entry);
  scheduleLiveRowFlush();
}

/**
 * Place the rows queued for `sessionId`, or for every session, now. The
 * manager flushes on disconnect so nothing waits on a frame that may not
 * come; tests flush to observe a placement synchronously.
 */
export function flushLiveTimelineRows(sessionId?: string): void {
  const sessions =
    sessionId === undefined ? [...pendingLiveRows.keys()] : [sessionId];
  for (const id of sessions) {
    const pending = pendingLiveRows.get(id);
    if (!pending) continue;
    pendingLiveRows.delete(id);
    placeLiveTimelineRows(pending.queryClient, id, pending.rows);
  }
  if (!pendingLiveRows.size && liveRowFlush) {
    liveRowFlush.cancel();
    liveRowFlush = null;
  }
}

export class WebSocketManager {
  // -- Dependencies --
  private readonly getBaseUrl: () => string;
  private readonly getWsTicket: () => Promise<string>;
  private readonly queryClient: QueryClient;

  // -- WebSocket state --
  private ws: WebSocket | null = null;
  private channelRefCounts: Map<string, number> = new Map();
  private eventListeners: Map<string, Set<WSEventHandler>> = new Map();
  private reconnectAttempt: number = 0;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private disconnectGraceTimer: ReturnType<typeof setTimeout> | null = null;
  private _connected: boolean = false;
  private _reconnecting: boolean = false;
  private _destroyed: boolean = false;

  // -- For useSyncExternalStore --
  private stateVersion: number = 0;
  private stateListeners: Set<() => void> = new Set();

  // -- Pending subscriptions (registered while disconnected) --
  private pendingSubscriptions: Set<string> = new Set();

  // -- Guard: prevent duplicate reconnect from onerror + onclose --
  private reconnectScheduled: boolean = false;
  private connectPromise: Promise<void> | null = null;

  // -- Live task timeline rows, appended once per frame per task --
  private pendingTaskRows: Map<string, unknown[]> = new Map();
  private taskRowFlush: { cancel: () => void } | null = null;

  // -- Live file-change notifications, one refresh per window per workspace --
  /** The Files tree and Changes report: waits behind a read in flight. */
  private readonly fileChanges: FileChangeCoalescer;
  /** Open file bodies and documents: cheap, read again at once. */
  private readonly contentChanges: FileChangeCoalescer;

  constructor(config: {
    getBaseUrl: () => string;
    getWsTicket: () => Promise<string>;
    queryClient: QueryClient;
    /** The coalescing window for `workspace.file_changed`; tests shorten it. */
    fileChangeCoalesceMs?: number;
  }) {
    this.getBaseUrl = config.getBaseUrl;
    this.getWsTicket = config.getWsTicket;
    this.queryClient = config.queryClient;
    this.fileChanges = new FileChangeCoalescer(
      config.queryClient,
      (workspaceId, _repoIds, options) =>
        this.refreshLiveTree(workspaceId, options),
      config.fileChangeCoalesceMs,
    );
    this.contentChanges = new FileChangeCoalescer(
      config.queryClient,
      (workspaceId, repoIds) => this.refreshLiveContent(workspaceId, repoIds),
      config.fileChangeCoalesceMs,
      undefined,
      undefined,
      false,
    );
  }

  // -------------------------------------------------------------------------
  // Public Properties
  // -------------------------------------------------------------------------

  /** Whether the WebSocket is currently open */
  get connected(): boolean {
    return this._connected;
  }

  /** Whether currently in reconnection backoff */
  get reconnecting(): boolean {
    return this._reconnecting;
  }

  /** Currently active channels (ref count > 0) */
  get subscriptions(): ReadonlySet<string> {
    const active = new Set<string>();
    for (const [channel, count] of this.channelRefCounts) {
      if (count > 0) {
        active.add(channel);
      }
    }
    return active;
  }

  // -------------------------------------------------------------------------
  // Connection Management
  // -------------------------------------------------------------------------

  /**
   * Initiate WebSocket connection.
   * Acquires a fresh data-channel token and opens the connection.
   * On success, sends subscribe messages for all pending/active channels.
   */
  async connect(): Promise<void> {
    if (this._destroyed) return;

    // Do not stampede the socket while a ticket request or handshake is already in flight.
    if (
      this.ws &&
      (this.ws.readyState === WebSocket.OPEN ||
        this.ws.readyState === WebSocket.CONNECTING)
    ) {
      return this.connectPromise ?? Promise.resolve();
    }
    if (this.connectPromise) return this.connectPromise;

    this.connectPromise = this.establishConnection();
    try {
      await this.connectPromise;
    } finally {
      this.connectPromise = null;
    }
  }

  private async establishConnection(): Promise<void> {
    if (this._destroyed) return;

    if (
      this.ws &&
      (this.ws.readyState === WebSocket.OPEN ||
        this.ws.readyState === WebSocket.CONNECTING)
    ) {
      return;
    }

    // Close any existing connection cleanly
    if (this.ws) {
      try {
        this.ws.onopen = null;
        this.ws.onclose = null;
        this.ws.onerror = null;
        this.ws.onmessage = null;
        this.ws.close();
      } catch {
        // Ignore close errors
      }
      this.ws = null;
    }

    // Capture before requesting the ticket: the selected organization can
    // change while the ticket request or the socket itself is in flight.
    const principal = tokenPrincipal(getApiToken());
    let token: string;
    try {
      token = await this.getWsTicket();
      if (!token?.trim()) throw new Error("WebSocket ticket is empty");
    } catch {
      // Token acquisition failure — treat as connection failure, trigger backoff
      this.scheduleReconnect();
      return;
    }

    if (this._destroyed) return;
    if (this.channelRefCounts.size === 0) return;

    const baseUrl = this.getBaseUrl();
    // Convert http(s):// to ws(s):// if needed
    const wsUrl = baseUrl.replace(/^http:/, "ws:").replace(/^https:/, "wss:");
    const url = `${wsUrl}/api/v1/ws?token=${encodeURIComponent(token)}`;

    try {
      this.ws = new WebSocket(url);
    } catch {
      this.scheduleReconnect();
      return;
    }

    this.reconnectScheduled = false;

    this.ws.onopen = () => {
      if (this._destroyed) return;
      // Capture reconnect state before it is reset below: a true reconnect
      // (vs the initial connect) must reconcile all subscribed channel caches.
      const wasReconnect = this._reconnecting || this.reconnectAttempt > 0;
      this._connected = true;
      this._reconnecting = false;
      this.reconnectAttempt = 0;
      this.notifyStateChange();

      // Worker-host (org-scoped cloud VM) list can drift while the socket is
      // down — a provisioning host may have flipped to ready. Refresh it too.
      this.queryClient.invalidateQueries({ queryKey: ["org-worker-hosts"] });

      // Send subscribe messages for each active channel once. Channels queued
      // while disconnected are also present in channelRefCounts.
      const channelsToSubscribe = new Set<string>();
      for (const [channel, count] of this.channelRefCounts) {
        if (count > 0) {
          channelsToSubscribe.add(channel);
        }
      }

      for (const channel of this.pendingSubscriptions) {
        if (
          !this.channelRefCounts.has(channel) ||
          (this.channelRefCounts.get(channel) ?? 0) === 0
        ) {
          // Already unsubscribed before connection opened
          this.pendingSubscriptions.delete(channel);
          continue;
        }
        channelsToSubscribe.add(channel);
      }

      for (const channel of channelsToSubscribe) {
        this.sendSubscribe(channel);
      }
      this.pendingSubscriptions.clear();

      // #603 / D16 / D4: a reconnect may have missed terminal chat/task events
      // (e.g. chat.done, task.status) while the socket was down. Now that the
      // socket is actually OPEN, invalidate all subscribed channel caches so
      // the chat timeline/messages reconverge to server-authoritative state and
      // completed tools never stay rendered as "running". Only on reconnect —
      // the initial connect already fetches fresh data.
      if (wasReconnect) {
        this.queryClient.invalidateQueries({
          queryKey: ["source-control-history"],
        });
        this.invalidateAllSubscribedCaches();
      }
    };

    this.ws.onmessage = (event: MessageEvent) => {
      if (this._destroyed) return;
      this.handleMessage(event);
    };

    this.ws.onerror = () => {
      // onerror is always followed by onclose in the browser.
      // We schedule reconnect in onclose to avoid double scheduling.
    };

    this.ws.onclose = (event) => {
      if (this._destroyed) return;
      if (event.code === 4003) notifyOrganizationAccessChanged(principal);
      this._connected = false;
      this.ws = null;
      this.notifyStateChange();

      // Only schedule one reconnect (guards against onerror + onclose both firing)
      if (!this.reconnectScheduled) {
        this.scheduleReconnect();
      }
    };
  }

  /** Queue a `task.timeline` row; the task's rows are appended together. */
  private queueTaskTimelineRow(taskId: string, entry: unknown): void {
    let rows = this.pendingTaskRows.get(taskId);
    if (!rows) {
      rows = [];
      this.pendingTaskRows.set(taskId, rows);
    }
    rows.push(entry);
    if (this.taskRowFlush) return;
    const flush = () => {
      this.taskRowFlush = null;
      this.flushTaskTimelineRows();
    };
    if (typeof requestAnimationFrame === "function") {
      const handle = requestAnimationFrame(flush);
      this.taskRowFlush = { cancel: () => cancelAnimationFrame(handle) };
    } else {
      const handle = setTimeout(flush, LIVE_ROW_FRAME_MS);
      this.taskRowFlush = { cancel: () => clearTimeout(handle) };
    }
  }

  /**
   * Append the queued `task.timeline` rows to their tasks' cached pages,
   * one cache write per task. Idempotent by entry id: the same event can be
   * delivered more than once (one publish per subscribed channel, a REST
   * refetch racing a live append), so an id already cached in any page is
   * skipped and one logical entry is cached exactly once
   * (wire_contract/ws_event_envelope#D5, issue #1313).
   */
  flushTaskTimelineRows(): void {
    if (this.taskRowFlush) {
      this.taskRowFlush.cancel();
      this.taskRowFlush = null;
    }
    const pending = [...this.pendingTaskRows];
    this.pendingTaskRows.clear();
    for (const [taskId, entries] of pending) {
      this.queryClient.setQueryData(
        ["task-timeline", taskId],
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        (old: any) => {
          if (!old?.pages) return old;
          const cached = new Set<string>();
          for (const page of old.pages)
            for (const item of page?.items ?? [])
              if (item?.id != null) cached.add(item.id);
          const added: unknown[] = [];
          for (const entry of entries) {
            const id = (entry as { id?: string } | null)?.id;
            if (id != null) {
              if (cached.has(id)) continue;
              cached.add(id);
            }
            added.push(entry);
          }
          if (!added.length) return old;
          // Append new entries to the last page (normalised to { items })
          const pages = [...old.pages];
          const lastPage = { ...pages[pages.length - 1] };
          lastPage.items = [...(lastPage.items ?? []), ...added];
          pages[pages.length - 1] = lastPage;
          return { ...old, pages };
        },
      );
    }
  }

  /**
   * Close the WebSocket connection and clean up reconnection state.
   * Does NOT clear subscriptions or event listeners (use destroy() for that).
   */
  disconnect(): void {
    // Rows waiting for a frame are placed now: the connection they came on
    // is gone, and a page being torn down may see no further frame.
    flushLiveTimelineRows();
    this.flushTaskTimelineRows();
    // Cancel any pending reconnect
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    this.reconnectScheduled = false;
    // A refresh pending for a connection that is gone has nothing to follow.
    this.fileChanges.clear();
    this.contentChanges.clear();

    // Cancel grace period timer
    if (this.disconnectGraceTimer) {
      clearTimeout(this.disconnectGraceTimer);
      this.disconnectGraceTimer = null;
    }

    if (this.ws) {
      this.ws.onopen = null;
      this.ws.onclose = null;
      this.ws.onerror = null;
      this.ws.onmessage = null;
      try {
        this.ws.close();
      } catch {
        // Ignore close errors
      }
      this.ws = null;
    }

    this._connected = false;
    this._reconnecting = false;
    this.reconnectAttempt = 0;
    this.notifyStateChange();
  }

  // -------------------------------------------------------------------------
  // Subscription Management (Reference-Counted)
  // -------------------------------------------------------------------------

  /**
   * Subscribe to a channel. Increments the reference count.
   * Sends a subscribe message to the server only on 0→1 transition.
   * If not connected, initiates connection.
   */
  subscribe(channel: string): void {
    if (this._destroyed) return;
    if (!channel) return; // Guard: empty channel string

    // Cancel any pending graceful disconnect
    if (this.disconnectGraceTimer) {
      clearTimeout(this.disconnectGraceTimer);
      this.disconnectGraceTimer = null;
    }

    const count = this.channelRefCounts.get(channel) ?? 0;
    this.channelRefCounts.set(channel, count + 1);

    if (count === 0) {
      // First subscriber — ensure connected and send subscribe
      if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
        this.pendingSubscriptions.add(channel);
        this.connect(); // Deferred connection
      } else {
        this.sendSubscribe(channel);
      }
    }
  }

  /**
   * Unsubscribe from a channel. Decrements the reference count.
   * Sends an unsubscribe message to the server only on 1→0 transition.
   * If no subscriptions remain, schedules graceful disconnect.
   */
  unsubscribe(channel: string): void {
    if (!channel) return; // Guard: empty channel string

    const count = this.channelRefCounts.get(channel) ?? 0;
    if (count <= 1) {
      this.channelRefCounts.delete(channel);
      this.pendingSubscriptions.delete(channel);
      if (this.ws && this.ws.readyState === WebSocket.OPEN) {
        this.sendUnsubscribe(channel);
      }
    } else {
      this.channelRefCounts.set(channel, count - 1);
    }

    // If no more subscriptions, schedule graceful disconnect
    if (this.channelRefCounts.size === 0) {
      this.disconnectGraceTimer = setTimeout(() => {
        if (this.channelRefCounts.size === 0) {
          this.disconnect();
        }
      }, WS_DISCONNECT_GRACE);
    }
  }

  // -------------------------------------------------------------------------
  // Custom Event Listeners
  // -------------------------------------------------------------------------

  /** Register a custom event listener for a specific event type */
  on(eventType: string, handler: WSEventHandler): void {
    let listeners = this.eventListeners.get(eventType);
    if (!listeners) {
      listeners = new Set();
      this.eventListeners.set(eventType, listeners);
    }
    listeners.add(handler);
  }

  /** Remove a custom event listener */
  off(eventType: string, handler: WSEventHandler): void {
    const listeners = this.eventListeners.get(eventType);
    if (listeners) {
      listeners.delete(handler);
      if (listeners.size === 0) {
        this.eventListeners.delete(eventType);
      }
    }
  }

  // -------------------------------------------------------------------------
  // Cleanup
  // -------------------------------------------------------------------------

  /**
   * Close connection, clear all state, remove all listeners.
   * After calling destroy(), the manager cannot be used again.
   */
  destroy(): void {
    this._destroyed = true;
    this.disconnect();
    this.channelRefCounts.clear();
    this.eventListeners.clear();
    this.pendingSubscriptions.clear();
    this.stateListeners.clear();
  }

  // -------------------------------------------------------------------------
  // useSyncExternalStore Integration
  // -------------------------------------------------------------------------

  /**
   * Subscribe to state changes (for useSyncExternalStore).
   * Returns an unsubscribe function.
   */
  subscribeToState = (listener: () => void): (() => void) => {
    this.stateListeners.add(listener);
    return () => {
      this.stateListeners.delete(listener);
    };
  };

  /** Get a snapshot of the current state version (for useSyncExternalStore) */
  getStateSnapshot = (): number => {
    return this.stateVersion;
  };

  // -------------------------------------------------------------------------
  // Private: State Change Notification
  // -------------------------------------------------------------------------

  private notifyStateChange(): void {
    this.stateVersion++;
    for (const listener of this.stateListeners) {
      try {
        listener();
      } catch (err) {
        // Listener errors should not prevent other listeners from being notified
        console.warn("[ws] State change listener threw:", err);
      }
    }
  }

  // -------------------------------------------------------------------------
  // Private: WebSocket Message Sending
  // -------------------------------------------------------------------------

  private sendSubscribe(channel: string): void {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify({ type: "subscribe", channel }));
    }
  }

  private sendUnsubscribe(channel: string): void {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify({ type: "unsubscribe", channel }));
    }
  }

  // -------------------------------------------------------------------------
  // Private: Message Handling
  // -------------------------------------------------------------------------

  private handleMessage(messageEvent: MessageEvent): void {
    let parsed: WSEvent;
    try {
      parsed = JSON.parse(messageEvent.data as string);
    } catch {
      // Malformed message — log and continue (do NOT crash the handler loop)
      console.error("WS: Failed to parse message:", messageEvent.data);
      return;
    }

    if (!parsed || typeof parsed !== "object" || !("type" in parsed)) {
      console.error("WS: Invalid message format:", parsed);
      return;
    }

    // Handle control messages
    if (parsed.type === "subscribed" || parsed.type === "unsubscribed") {
      // Subscription confirmations — no action needed
      return;
    }

    if (parsed.type === "error") {
      // Server error messages — log but do NOT close the connection
      console.error(
        "WS server error:",
        (parsed as { code: string; message: string }).code,
        (parsed as { code: string; message: string }).message,
      );
      return;
    }

    // Handle event messages — route to cache invalidation and custom listeners
    try {
      this.handleEvent(parsed);
    } catch (e) {
      // Catch handler errors so they don't crash the message loop
      console.error("WS event handler error:", e);
    }
  }

  // -------------------------------------------------------------------------
  // Private: Event → React Query Cache Invalidation
  // -------------------------------------------------------------------------

  private invalidateOpenQuestionMetrics(): void {
    this.queryClient.invalidateQueries({
      queryKey: ["workspace-open-question-metrics"],
    });
  }

  private invalidateDocContentQueries(workspaceId: string): void {
    this.queryClient.invalidateQueries({
      predicate: (query) => {
        const key = query.queryKey;
        return Array.isArray(key) && key[0] === "doc" && key[1] === workspaceId;
      },
    });
  }

  /** A workspace's file and node thread lists, read again after a change. */
  private readCommentListsAgain(workspaceId: string): void {
    void invalidateAndReadAgain(this.queryClient, ["comments", workspaceId]);
    void invalidateAndReadAgain(this.queryClient, [
      "line-comments",
      workspaceId,
    ]);
    void invalidateAndReadAgain(this.queryClient, [
      "workspace-review-comments",
      workspaceId,
    ]);
    void invalidateAndReadAgain(this.queryClient, [
      "workspace-activity",
      workspaceId,
    ]);
  }

  private invalidateChatSessionCaches(
    sessionId?: string | null,
    options?: { includeTaskState?: boolean },
  ): void {
    if (sessionId) {
      this.queryClient.invalidateQueries({
        queryKey: ["session-inputs", sessionId],
      });
      this.queryClient.invalidateQueries({
        queryKey: ["chat-messages", sessionId],
      });
      this.queryClient.invalidateQueries({
        queryKey: ["session-timeline", sessionId],
      });
      this.queryClient.invalidateQueries({
        queryKey: ["chat-session", sessionId],
      });
    } else {
      this.queryClient.invalidateQueries({ queryKey: ["session-inputs"] });
      this.queryClient.invalidateQueries({ queryKey: ["chat-messages"] });
      this.queryClient.invalidateQueries({
        queryKey: ["session-timeline"],
      });
      this.queryClient.invalidateQueries({ queryKey: ["chat-session"] });
    }
    this.queryClient.invalidateQueries({ queryKey: ["chat-sessions"] });

    if (options?.includeTaskState) {
      this.queryClient.invalidateQueries({ queryKey: ["workspace-tasks"] });
      this.queryClient.invalidateQueries({ queryKey: ["workspace-tasks"] });
    }
  }

  /**
   * Refresh the live projections of `workspaceId` after paths changed in
   * `repoIds`: the Files tree, the Changes report, open file bodies and, for
   * the docs repository, the document projections. The tree and Changes
   * fetches are the expensive ones a burst used to starve: the coalescer
   * only calls this when none is in flight, or asks to cancel a hung one,
   * so `cancelRefetch` is its decision. Every other family is cheap and is
   * read again at once, cancelling a stale fetch in flight, so no
   * pre-change response can clear the mark.
   */
  private refreshLiveTree(
    workspaceId: string,
    options: { cancelRefetch: boolean },
  ): void {
    for (const queryKey of [
      ["workspace-changes", workspaceId],
      ["file-tree", workspaceId],
    ]) {
      if (options.cancelRefetch) {
        this.readAgain(queryKey);
      } else {
        this.queryClient.invalidateQueries({ queryKey }, options);
      }
    }
  }

  /**
   * Read the cheap live content of `workspaceId` again after paths changed
   * in `repoIds`: open file bodies, node content and, for the docs
   * repository, the document projections. Scoped to the workspace so a
   * first read in another workspace is never cancelled.
   */
  private refreshLiveContent(
    workspaceId: string,
    repoIds: ReadonlySet<string>,
  ): void {
    for (const queryKey of [
      ["workspace-file", workspaceId],
      ["file-content", workspaceId],
    ]) {
      this.readAgain(queryKey);
    }
    // Ordinary agent writes can create or edit documents without a doc API
    // mutation or a Git commit. Refresh the same live document projections
    // used by Project Docs, Explainers, and open document tabs.
    const docsRepoId = this.queryClient.getQueryData<WorkspaceDetail>([
      "workspace",
      workspaceId,
    ])?.docsRepoId;
    // Avoid refetching documents for code-repository churn, but keep all
    // docs-repository roots (including legacy designs/ and explainers/)
    // and conservatively refresh when workspace metadata is unavailable.
    if (!docsRepoId || repoIds.has(docsRepoId)) {
      // Referenced question sets live in the docs repository too
      // (`g5.documents.question.metrics`): the owner's open cards follow
      // the live file without a question event.
      for (const family of ["docs", "views", "view", "question-set"]) {
        this.readAgain([family, workspaceId]);
      }
    }
  }

  /**
   * Invalidate `queryKey` so it is read after this moment. TanStack cancels
   * a fetch in flight only when the query already holds data; a first read
   * still out would be kept and its pre-change answer would clear the
   * invalidation, so such a read is cancelled first (`invalidateAndReadAgain`).
   * Without one the invalidation is the ordinary synchronous call.
   */
  private readAgain(queryKey: QueryKey, exact = false): void {
    // Only an exact read names `exact`, so prefix filters stay `{ queryKey }`.
    const filters = exact ? { queryKey, exact } : { queryKey };
    const firstReadOut = this.queryClient
      .getQueryCache()
      .findAll(filters)
      .some(
        (query) =>
          query.state.data === undefined &&
          query.state.fetchStatus === "fetching",
      );
    if (firstReadOut) {
      void invalidateAndReadAgain(this.queryClient, queryKey, exact);
    } else {
      void this.queryClient.invalidateQueries(filters);
    }
  }

  private invalidateAuthoritativeCut(workspaceId: string): void {
    this.readAgain(["source-control-change-graph", workspaceId]);
    this.queryClient.invalidateQueries({
      queryKey: ["workspace", workspaceId],
    });
    this.queryClient.invalidateQueries({ queryKey: ["workspaces"] });
    this.queryClient.invalidateQueries({ queryKey: ["workspace-tasks"] });
    this.queryClient.invalidateQueries({ queryKey: ["file-tree"] });
    this.queryClient.invalidateQueries({ queryKey: ["file-content"] });
    // A reader's open FILES view follows its durable source: a branch,
    // flush or cut advance is its only signal, since file-change events go
    // to the owner alone (`g5.documents.files-view.refresh`).
    this.queryClient.invalidateQueries({
      queryKey: ["workspace-file", workspaceId],
    });
    this.queryClient.invalidateQueries({
      queryKey: ["source-control-history"],
    });
    this.queryClient.invalidateQueries({ queryKey: ["docs", workspaceId] });
    this.queryClient.invalidateQueries({ queryKey: ["views", workspaceId] });
    // Readers' referenced-question cards follow the durable cut; their
    // tallies are read again with them in case a vote event was missed.
    this.queryClient.invalidateQueries({
      queryKey: ["question-set", workspaceId],
    });
    this.queryClient.invalidateQueries({
      queryKey: ["question-votes", workspaceId],
    });
    this.queryClient.invalidateQueries({
      queryKey: ["workspace-changes", workspaceId],
    });
  }

  private handleEvent(event: WSEvent): void {
    // 1. React Query cache invalidation
    switch (event.type) {
      case "task.status":
        this.queryClient.invalidateQueries({
          queryKey: ["task", event.taskId],
        });
        this.queryClient.invalidateQueries({ queryKey: ["workspace-tasks"] });
        this.queryClient.invalidateQueries({ queryKey: ["spend"] });
        this.queryClient.invalidateQueries({ queryKey: ["workspace-stats"] });
        this.queryClient.invalidateQueries({ queryKey: ["workspaces"] });
        // The event identifies a task, not its owning chat session. Refresh
        // the whole query family so every mounted session-task projection
        // advances immediately instead of waiting for its polling fallback.
        this.queryClient.invalidateQueries({ queryKey: ["session-task"] });
        // Only a terminal task can have landed Project Doc changes that move
        // the open-question aggregate (websocket D4); active→active
        // transitions never touch docs, so only terminal statuses refetch
        // the workspace-wide metric.
        if (
          event.status === "succeeded" ||
          event.status === "failed" ||
          event.status === "cancelled"
        ) {
          this.invalidateOpenQuestionMetrics();
        }
        this.queryClient.invalidateQueries({
          queryKey: ["pending-sync-files"],
          refetchType: "all",
        });
        // A provider can finish its assistant message before/around the task
        // transition. If the chat.done event was missed (or the task panel was
        // not subscribed yet), task.status is the reliable workspace-level
        // signal that the turn ended; refresh chat caches so stale pending
        // assistant placeholders do not keep the Thinking indicator alive.
        if (
          event.status === "waiting" ||
          event.status === "succeeded" ||
          event.status === "failed" ||
          event.status === "cancelled"
        ) {
          this.invalidateChatSessionCaches();
        }
        break;

      case "task.timeline": {
        // Optimistic append: update the cache directly instead of full
        // invalidation. This avoids flicker during live trace streaming.
        // Rows are appended once per animation frame, together.
        this.queueTaskTimelineRow(event.taskId, event.entry);
        break;
      }

      case "chat.timeline":
        // A running turn's row delivered ahead of saved history
        // (chat g5.chat.live-delivery): placed into the session's timeline
        // window by identity, never by refetch.
        applyLiveTimelineRow(this.queryClient, event.sessionId, event.entry);
        break;

      case "chat.chunk":
        // Optimistic update: append delta to cached messages (NOT invalidateQueries)
        this.handleChatChunk(event);
        break;

      case "chat.subscription_usage":
        // A separate memory-only value survives ordinary session refetches.
        // It is never copied into the persisted session or transcript caches.
        this.queryClient.setQueryData(
          ["subscription-usage", event.sessionId],
          (old: LiveSubscriptionUsage | null | undefined) =>
            old &&
            old.authMode === event.authMode &&
            old.accountId === event.accountId &&
            Date.parse(old.usage.capturedAt) >
              Date.parse(event.usage.capturedAt)
              ? old
              : {
                  authMode: event.authMode,
                  accountId: event.accountId,
                  usage: event.usage,
                },
        );
        break;

      case "chat.message_updated":
        this.invalidateChatSessionCaches(event.sessionId);
        break;

      case "chat.activity":
        this.invalidateChatActivity(event.workspaceId, event.sessionId);
        break;

      case "chat.done":
        this.queryClient.setQueryData(
          ["chat-messages", event.sessionId],
          (old: ChatMessageDTO[] | undefined) =>
            old?.map((message) =>
              message.id === event.messageId
                ? // Mark complete, but drop the optimistic placeholder
                  // `createdAt` (a client-side turn-start time). The displayed
                  // message timestamp must always be the DB-persisted send time
                  // (message_rendering#D3), so blanking it makes the footer wait
                  // for the authoritative row from the invalidation-driven
                  // refetch below (and on reload) instead of briefly showing the
                  // stale turn-start time.
                  { ...message, status: "complete", createdAt: "" }
                : message,
            ),
        );
        this.invalidateChatSessionCaches(event.sessionId);
        break;

      case "chat.error":
        // Status-only terminal signal: mark the message errored but PRESERVE
        // any already-streamed partial content. The diagnostic error string is
        // recorded on the message's client-only `errorMessage` field so the
        // error indicator can surface it, WITHOUT clobbering the message body
        // (websocket D15). `message` carries the human-readable diagnostic
        // (e.g. "Rate limit exceeded"); `error` is the stable code.
        this.queryClient.setQueryData(
          ["chat-messages", event.sessionId],
          (old: ChatMessageDTO[] | undefined) =>
            old?.map((message) =>
              message.id === event.messageId
                ? {
                    ...message,
                    status: "error",
                    errorMessage: event.message || event.error || null,
                  }
                : message,
            ),
        );
        this.invalidateChatSessionCaches(event.sessionId);
        break;

      // A change made elsewhere: every list holding the thread is read
      // again after it, a first read still out included
      // (`g5.documents.comments.live`). The workspace's node lists include
      // the event's node.
      case "comment.new":
      case "comment.updated":
        this.readCommentListsAgain(event.workspaceId);
        break;

      case "comment.reply":
        // Reply counts change; so does the parent thread's replies.
        this.readCommentListsAgain(event.workspaceId);
        if (event.commentId) {
          void invalidateAndReadAgain(this.queryClient, [
            "comment-replies",
            event.commentId,
          ]);
          void invalidateAndReadAgain(this.queryClient, [
            "workspace-review-replies",
            event.commentId,
          ]);
        }
        break;

      case "question.vote":
        // Read again even if the event lands during a fetch (readAgain).
        void invalidateAndReadAgain(
          this.queryClient,
          questionVotesQueryKey(event.workspaceId, event.path),
        );
        break;

      case "comment.deleted":
        this.readCommentListsAgain(event.workspaceId);
        this.queryClient.removeQueries({
          queryKey: ["comment-replies", event.commentId],
          exact: true,
        });
        break;

      case "workspace.sync":
        this.readAgain(["source-control-change-graph", event.workspaceId]);
        this.queryClient.invalidateQueries({
          queryKey: ["workspace", event.workspaceId],
        });
        this.queryClient.invalidateQueries({ queryKey: ["file-tree"] });
        this.queryClient.invalidateQueries({ queryKey: ["file-content"] });
        break;

      case "workspace.source_control_saved":
        this.queryClient.setQueryData(
          ["source-control-checkpoint", event.workspaceId],
          (old: { generation: number } | undefined) => ({
            generation: Math.max(old?.generation ?? 0, event.generation),
          }),
        );
        break;
      case "workspace.pull_request_updated":
        void invalidateAndReadAgain(this.queryClient, [
          "workspace-pull-requests",
          event.workspaceId,
        ]);
        // A merge or merge-when-ready change is also an Activity entry
        // (g5.git.pr-merge-announcement).
        void invalidateAndReadAgain(this.queryClient, [
          "workspace-activity",
          event.workspaceId,
        ]);
        break;

      case "workspace.branch_updated":
        this.invalidateAuthoritativeCut(event.workspaceId);
        break;

      case "workspace.flush_completed":
        this.invalidateAuthoritativeCut(event.workspaceId);
        break;

      case "workspace.file_changed":
        // One event per written path; a build in the VM sends hundreds a
        // second. Coalesced so the live views refresh once per window and a
        // fetch in flight is never cancelled (`g5.documents.files-view.refresh`).
        this.fileChanges.changed(event.workspaceId, event.repoId);
        this.contentChanges.changed(event.workspaceId, event.repoId);
        break;

      case "workspace.ownership_transferred":
        // An ownership change revokes the former owner's live attachment and
        // may grant it to another browser. Remove cached live bodies before
        // refetching capability and durable-source state.
        this.queryClient.removeQueries({
          queryKey: ["workspace-file", event.workspaceId],
        });
        this.queryClient.removeQueries({ queryKey: ["file-tree"] });
        this.queryClient.removeQueries({ queryKey: ["file-content"] });
        this.queryClient.removeQueries({
          queryKey: ["workspace-preview", event.workspaceId],
        });
        this.invalidateAuthoritativeCut(event.workspaceId);
        this.queryClient.invalidateQueries({
          queryKey: ["workspace-vm", event.workspaceId],
        });
        this.queryClient.invalidateQueries({ queryKey: ["chat-sessions"] });
        this.queryClient.invalidateQueries({ queryKey: ["session-inputs"] });
        break;

      case "workspace.remote_tip_advisory_updated":
        // Advisory is pure UI metadata: a binary "remote moved / behind"
        // signal (not a commit count). It never moves the workspace tip, so
        // invalidate ONLY the branch sync-runs feed (which carries the
        // advisory SHA) and the workspace branch list. It MUST NOT invalidate
        // file-tree, ontology, or node caches (arch D21 / D11).
        this.queryClient.invalidateQueries({
          queryKey: ["branch-sync-runs", event.workspaceId],
        });
        this.queryClient.invalidateQueries({
          queryKey: ["workspace", event.workspaceId],
        });
        break;

      case "doc.created":
      case "doc.content_changed":
      case "doc.deleted":
        // Current document tabs read through the same file cache as Files.
        // Invalidate pending initial reads too, so an older response cannot hide a write.
        this.readAgain(["workspace-file", event.workspaceId]);
        // Discrete Project Doc lifecycle events carry whole-document semantic
        // change: a created, rewritten, or removed doc adds or drops
        // `g5-question` fences, so the derived open-question aggregate is
        // stale until it refetches. These fire once per doc action — unlike
        // Repeated editor-local state is never broadcast, so refreshing here
        // cannot introduce a per-keystroke workspace-wide document scan.
        // Without this the panel only caught up on window
        // focus, commit, or an escalation event.
        this.invalidateOpenQuestionMetrics();
        this.queryClient.invalidateQueries({
          queryKey: ["docs", event.workspaceId],
        });
        // A Project Doc write lands in the live docs checkout, so the Source
        // Control dirty list is stale until the Changes report refetches.
        this.queryClient.invalidateQueries({
          queryKey: ["workspace-changes", event.workspaceId],
        });
        break;

      case "workspace.settings_updated":
        this.queryClient.invalidateQueries({
          queryKey: ["settings", event.workspaceId],
        });
        break;

      case "workspace.spend_limit_reached":
        this.queryClient.invalidateQueries({
          queryKey: ["spend", event.workspaceId],
        });
        break;

      case "workspace.spend_limit_cleared":
        this.queryClient.invalidateQueries({
          queryKey: ["spend", event.workspaceId],
        });
        break;

      case "notification":
        this.queryClient.invalidateQueries({
          queryKey: ["notifications", event.workspaceId],
        });
        if (notificationCategory(event.notification) === "review") {
          // A review event elsewhere (a round pushed, a decision, the
          // source committed) changes what every review page and copy
          // shows: rounds regroup, "Outdated" and "has a new commit"
          // appear. Cheap metadata only; the copy's preview is not
          // refetched here because fetching it saves the copy.
          this.queryClient.invalidateQueries({
            queryKey: ["workspace-review-publications"],
          });
          this.queryClient.invalidateQueries({
            queryKey: ["workspace-review-suggestions"],
          });
          this.queryClient.invalidateQueries({
            queryKey: ["workspace-review-suggestion-counts"],
          });
          // A reply or decision notice: the open thread it concerns shows
          // the new message now, not at its next poll, a first read still
          // out included.
          void invalidateAndReadAgain(this.queryClient, [
            "workspace-review-replies",
          ]);
          this.queryClient.invalidateQueries({
            queryKey: ["workspace-changes"],
          });
        }
        break;

      default:
        // Unknown event type — ignore silently
        break;
    }

    // 2. Custom event listeners
    const listeners = this.eventListeners.get(event.type);
    if (listeners) {
      for (const handler of listeners) {
        try {
          handler(event);
        } catch (e) {
          console.error("WS event handler error:", e);
        }
      }
    }
  }

  // -------------------------------------------------------------------------
  // Private: Chat Chunk Streaming Optimization
  // -------------------------------------------------------------------------

  /**
   * Handle chat.chunk events with optimistic update.
   * Folds the delta into the streamed assistant message identified by
   * `messageId` (wherever it sits in the cached list — not the array tail)
   * via queryClient.setQueryData(), avoiding a full query invalidation.
   * Custom event listeners are dispatched by the caller (handleEvent).
   */
  private handleChatChunk(
    event: Extract<WSEvent, { type: "chat.chunk" }>,
  ): void {
    this.queryClient.setQueryData(
      ["chat-messages", event.sessionId],
      (old: ChatMessageDTO[] | undefined) => {
        if (!old) return old;
        // Fold the delta into the streamed assistant message identified by
        // messageId, wherever it sits in the list — NOT the array tail. A
        // follow-up send optimistically appends a pending user message to the
        // tail, so a tail-only fold would silently drop every delta the moment
        // the user steers mid-stream (websocket D15).
        const updated = old.map((message) =>
          message.id === event.messageId
            ? {
                ...message,
                content: message.content + event.delta,
                // Mark as streaming so the UI drops opacity-60 styling
                status:
                  message.status === "pending" ? "streaming" : message.status,
              }
            : message,
        );
        if (
          updated.some((message) => message.id === event.messageId) ||
          event.messageIndex === undefined ||
          !event.createdAt
        ) {
          return updated;
        }
        return [
          ...updated,
          {
            id: event.messageId,
            sessionId: event.sessionId,
            role: "assistant" as const,
            content: event.delta,
            artifacts: null,
            citations: [],
            inputTokens: null,
            outputTokens: null,
            status: "streaming",
            messageIndex: event.messageIndex,
            createdAt: event.createdAt,
          },
        ].sort((left, right) => left.messageIndex - right.messageIndex);
      },
    );
  }

  // -------------------------------------------------------------------------
  // Private: Reconnection with Exponential Backoff + Jitter
  // -------------------------------------------------------------------------

  /**
   * Schedule a reconnection attempt with exponential backoff and jitter.
   *
   * delay = min(BASE_DELAY * 2^attempt, MAX_DELAY) + random(0, JITTER)
   */
  private scheduleReconnect(): void {
    if (this._destroyed) return;
    if (this.reconnectTimer) return; // Already scheduled

    this.reconnectScheduled = true;

    const jitter = Math.random() * WS_JITTER_MAX;
    const delay =
      Math.min(
        WS_BASE_DELAY * Math.pow(2, this.reconnectAttempt),
        WS_MAX_DELAY,
      ) + jitter;
    this.reconnectAttempt++;
    this._reconnecting = true;
    this.notifyStateChange();

    this.reconnectTimer = setTimeout(async () => {
      this.reconnectTimer = null;
      this.reconnectScheduled = false;
      if (this._destroyed) return;

      // Acquire fresh token and reconnect. Cache invalidation happens in the
      // socket's onopen handler (where the connection is actually OPEN), not
      // here: connect() resolves while the socket is still CONNECTING, so a
      // `this._connected` check here is always false on a fresh reconnect.
      await this.connect();
    }, delay);
  }

  /**
   * Re-read the workspace's session list (every scope variant) and the named
   * chat's detail after a `chat.activity` hint (chat g5.chat.activity.refresh).
   * The list carries the order, activity and inspection state; transcripts are
   * left alone, since a background chat's hint says nothing about its rows.
   */
  private invalidateChatActivity(
    workspaceId: string,
    sessionId?: string,
  ): void {
    // A first read already out would answer from before the change.
    this.readAgain(["chat-sessions", workspaceId]);
    if (sessionId) {
      this.readAgain(["chat-session", sessionId], true);
    } else {
      this.readAgain(["chat-session"]);
    }
  }

  /**
   * Invalidate all React Query caches for currently subscribed channels.
   * Called after reconnection to ensure no events were missed.
   */
  private invalidateAllSubscribedCaches(): void {
    for (const [channel, count] of this.channelRefCounts) {
      if (count <= 0) continue;

      // Parse channel format: <type>:<id> or <type>:<id>:<extra>
      const colonIdx = channel.indexOf(":");
      if (colonIdx === -1) continue;

      const channelType = channel.substring(0, colonIdx);
      const channelId = channel.substring(colonIdx + 1);

      switch (channelType) {
        case "workspace":
          this.readAgain(["source-control-change-graph", channelId]);
          this.invalidateOpenQuestionMetrics();
          void invalidateAndReadAgain(this.queryClient, [
            "workspace-pull-requests",
            channelId,
          ]);
          this.queryClient.invalidateQueries({
            queryKey: ["workspace", channelId],
          });
          this.queryClient.invalidateQueries({ queryKey: ["file-tree"] });
          this.queryClient.invalidateQueries({ queryKey: ["file-content"] });
          this.queryClient.invalidateQueries({ queryKey: ["docs", channelId] });
          this.queryClient.invalidateQueries({
            queryKey: ["views", channelId],
          });
          this.queryClient.invalidateQueries({
            queryKey: ["workspace-changes", channelId],
          });
          this.queryClient.invalidateQueries({ queryKey: ["workspaces"] });
          this.queryClient.invalidateQueries({ queryKey: ["workspace-tasks"] });
          this.queryClient.invalidateQueries({ queryKey: ["workspace-tasks"] });
          this.queryClient.invalidateQueries({
            queryKey: ["settings", channelId],
          });
          this.queryClient.invalidateQueries({
            queryKey: ["spend", channelId],
          });
          this.queryClient.invalidateQueries({
            queryKey: ["notifications", channelId],
          });
          this.queryClient.invalidateQueries({
            queryKey: ["source-control-checkpoint", channelId],
          });
          // Thread changes missed while disconnected: the stored threads
          // are the authority after a reconnect, read again after it, a
          // first read still out included (`g5.documents.comments.live`).
          this.readCommentListsAgain(channelId);
          void invalidateAndReadAgain(this.queryClient, ["comment-replies"]);
          void invalidateAndReadAgain(this.queryClient, [
            "workspace-review-replies",
          ]);
          // Turn starts, completions and inspections missed while
          // disconnected (chat g5.chat.activity.refresh).
          this.invalidateChatActivity(channelId);
          break;
        case "task":
          this.queryClient.invalidateQueries({ queryKey: ["task", channelId] });
          this.queryClient.invalidateQueries({
            queryKey: ["task-timeline", channelId],
          });
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
}

// ---------------------------------------------------------------------------
// React Context for Dependency Injection
// ---------------------------------------------------------------------------

const WebSocketContext = React.createContext<WebSocketManager | null>(null);

/**
 * React context provider for the WebSocketManager instance.
 * Wrap your app root with this provider to enable useWebSocket and useWSChannel hooks.
 *
 * ```tsx
 * <WebSocketProvider manager={wsManager}>
 *   <App />
 * </WebSocketProvider>
 * ```
 */
export function WebSocketProvider({
  children,
  manager,
}: {
  children: React.ReactNode;
  manager: WebSocketManager;
}): React.JSX.Element {
  return React.createElement(
    WebSocketContext.Provider,
    { value: manager },
    children,
  );
}

/**
 * Internal hook to access the WebSocketManager instance from context.
 * Throws if used outside of a WebSocketProvider.
 */
function useWebSocketManager(): WebSocketManager {
  const manager = React.useContext(WebSocketContext);
  if (!manager) {
    throw new Error("useWebSocket must be used within a WebSocketProvider");
  }
  return manager;
}

// ---------------------------------------------------------------------------
// React Hooks
// ---------------------------------------------------------------------------

/**
 * Singleton hook for accessing WebSocket connection state and subscription methods.
 * Uses useSyncExternalStore for efficient re-renders when connection state changes.
 *
 * Returns:
 * - `connected`: Whether the WebSocket is currently open
 * - `subscribe`: Subscribe to a channel (reference-counted)
 * - `unsubscribe`: Unsubscribe from a channel (reference-counted)
 */
export function useWebSocket(): WebSocketState {
  const manager = useWebSocketManager();

  // Subscribe to state changes for re-renders
  useSyncExternalStore(
    manager.subscribeToState,
    manager.getStateSnapshot,
    manager.getStateSnapshot, // server snapshot (same as client for this use case)
  );

  const subscribe = useCallback(
    (channel: string) => manager.subscribe(channel),
    [manager],
  );

  const unsubscribe = useCallback(
    (channel: string) => manager.unsubscribe(channel),
    [manager],
  );

  return {
    connected: manager.connected,
    subscribe,
    unsubscribe,
  };
}

/**
 * Per-component subscription hook. Auto-subscribes on mount, unsubscribes on unmount.
 * This is the primary interface for components that need real-time updates for a
 * specific entity.
 *
 * ```tsx
 * function TaskView({ taskId }: { taskId: string }) {
 *   useWSChannel(`task:${taskId}`);
 *   // Component will receive real-time updates for this task
 *   // via React Query cache invalidation
 * }
 * ```
 */
export function useWSChannel(channel: string): void {
  const { subscribe, unsubscribe } = useWebSocket();

  useEffect(() => {
    if (!channel) return; // Guard: empty channel string
    subscribe(channel);
    return () => unsubscribe(channel);
  }, [channel, subscribe, unsubscribe]);
}

/**
 * Hook that registers a custom event listener on the WebSocket manager.
 * The handler is called for every WS event matching the given type.
 * Automatically unregisters on unmount.
 *
 * ```tsx
 * useWSEventListener('task.timeline', (event) => {
 *   if (event.type === 'task.timeline') {
 *     console.log('Timeline entry:', event.entry);
 *   }
 * });
 * ```
 */
export function useWSEventListener(
  eventType: string,
  handler: WSEventHandler,
): void {
  const manager = useWebSocketManager();

  useEffect(() => {
    if (!eventType) return;
    manager.on(eventType, handler);
    return () => manager.off(eventType, handler);
  }, [eventType, handler, manager]);
}
