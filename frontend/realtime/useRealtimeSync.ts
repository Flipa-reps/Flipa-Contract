import { useEffect, useRef, useState, useCallback } from "react";
import { RealtimeSyncClient, ConflictResolver } from "./SyncClient";

const WS_URL =
  typeof window !== "undefined"
    ? (window as any).__WS_SYNC_URL ?? `ws://${window.location.host}/sync`
    : "ws://localhost:3000/sync";

export function useRealtimeSync(
  gameId: string | null,
  resolveConflict?: ConflictResolver
) {
  const [gameState, setGameState] = useState<Record<string, unknown>>({});
  const [connected, setConnected] = useState(false);
  const [queueSize, setQueueSize] = useState(0);
  const clientRef = useRef<RealtimeSyncClient | null>(null);

  useEffect(() => {
    const client = new RealtimeSyncClient(
      WS_URL,
      (_id, state) => setGameState(state),
      resolveConflict
    );

    // Patch connect/disconnect to track connection state
    const origConnect = client.connect.bind(client);
    client.connect = () => {
      origConnect();
      // Poll connection state
      const t = setInterval(() => {
        setConnected((client as any).isConnected?.() ?? false);
        setQueueSize(client.queueSize);
      }, 500);
      (client as any)._pollTimer = t;
    };

    clientRef.current = client;
    client.connect();

    return () => {
      clearInterval((client as any)._pollTimer);
      client.disconnect();
    };
  }, [resolveConflict]);

  const sendUpdate = useCallback(
    (state: Record<string, unknown>) => {
      if (gameId) clientRef.current?.sendStateUpdate(gameId, state);
    },
    [gameId]
  );

  return { gameState, connected, queueSize, sendUpdate };
}
