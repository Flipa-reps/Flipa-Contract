// WebSocket client with state sync, conflict resolution, and offline queue

export type SyncMessage =
  | { type: "state_update"; gameId: string; state: Record<string, unknown>; version: number }
  | { type: "ack"; gameId: string; version: number }
  | { type: "conflict"; gameId: string; serverState: Record<string, unknown>; serverVersion: number }
  | { type: "ping" }
  | { type: "pong" };

export type StateHandler = (gameId: string, state: Record<string, unknown>) => void;
export type ConflictResolver = (
  local: Record<string, unknown>,
  server: Record<string, unknown>
) => Record<string, unknown>;

interface QueuedMessage {
  gameId: string;
  state: Record<string, unknown>;
  version: number;
  attempts: number;
}

const DEFAULT_CONFLICT_RESOLVER: ConflictResolver = (_local, server) => server; // server wins

export class RealtimeSyncClient {
  private ws: WebSocket | null = null;
  private url: string;
  private onStateUpdate: StateHandler;
  private resolveConflict: ConflictResolver;
  private offlineQueue: QueuedMessage[] = [];
  private localVersions = new Map<string, number>();
  private reconnectDelay = 1000;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private pingTimer: ReturnType<typeof setInterval> | null = null;
  private closed = false;

  constructor(
    url: string,
    onStateUpdate: StateHandler,
    resolveConflict: ConflictResolver = DEFAULT_CONFLICT_RESOLVER
  ) {
    this.url = url;
    this.onStateUpdate = onStateUpdate;
    this.resolveConflict = resolveConflict;
  }

  connect(): void {
    if (this.closed) return;
    try {
      this.ws = new WebSocket(this.url);
      this.ws.onopen = () => {
        this.reconnectDelay = 1000;
        this.flushQueue();
        this.startPing();
      };
      this.ws.onmessage = (ev) => this.handleMessage(JSON.parse(ev.data as string));
      this.ws.onclose = () => {
        this.stopPing();
        if (!this.closed) this.scheduleReconnect();
      };
      this.ws.onerror = () => this.ws?.close();
    } catch {
      if (!this.closed) this.scheduleReconnect();
    }
  }

  disconnect(): void {
    this.closed = true;
    this.stopPing();
    if (this.reconnectTimer) clearTimeout(this.reconnectTimer);
    this.ws?.close();
  }

  sendStateUpdate(gameId: string, state: Record<string, unknown>): void {
    const version = (this.localVersions.get(gameId) ?? 0) + 1;
    this.localVersions.set(gameId, version);
    const msg: QueuedMessage = { gameId, state, version, attempts: 0 };
    if (this.isConnected()) {
      this.transmit(msg);
    } else {
      this.offlineQueue.push(msg);
    }
  }

  private handleMessage(msg: SyncMessage): void {
    switch (msg.type) {
      case "state_update": {
        const localVersion = this.localVersions.get(msg.gameId) ?? 0;
        if (msg.version >= localVersion) {
          this.localVersions.set(msg.gameId, msg.version);
          this.onStateUpdate(msg.gameId, msg.state);
        }
        this.send({ type: "ack", gameId: msg.gameId, version: msg.version });
        break;
      }
      case "conflict": {
        const localState = {}; // caller should provide current local state if needed
        const resolved = this.resolveConflict(localState, msg.serverState);
        this.localVersions.set(msg.gameId, msg.serverVersion);
        this.onStateUpdate(msg.gameId, resolved);
        break;
      }
      case "ping":
        this.send({ type: "pong" });
        break;
    }
  }

  private transmit(msg: QueuedMessage): void {
    this.send({
      type: "state_update",
      gameId: msg.gameId,
      state: msg.state,
      version: msg.version,
    });
  }

  private send(msg: SyncMessage): void {
    if (this.isConnected()) {
      this.ws!.send(JSON.stringify(msg));
    }
  }

  private flushQueue(): void {
    const pending = this.offlineQueue.splice(0);
    for (const msg of pending) {
      this.transmit(msg);
    }
  }

  private isConnected(): boolean {
    return this.ws?.readyState === WebSocket.OPEN;
  }

  private scheduleReconnect(): void {
    this.reconnectTimer = setTimeout(() => {
      this.reconnectDelay = Math.min(this.reconnectDelay * 2, 30000);
      this.connect();
    }, this.reconnectDelay);
  }

  private startPing(): void {
    this.pingTimer = setInterval(() => this.send({ type: "ping" }), 30000);
  }

  private stopPing(): void {
    if (this.pingTimer) clearInterval(this.pingTimer);
  }

  get queueSize(): number {
    return this.offlineQueue.length;
  }
}
