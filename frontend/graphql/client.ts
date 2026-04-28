// Lightweight GraphQL client with subscription support (no heavy dependencies)

const GQL_ENDPOINT = (typeof window !== "undefined" && (window as any).__GQL_ENDPOINT) || "/graphql";
const WS_ENDPOINT = GQL_ENDPOINT.replace(/^http/, "ws");

export interface GqlResponse<T> {
  data?: T;
  errors?: { message: string }[];
}

export async function gqlQuery<T>(
  query: string,
  variables?: Record<string, unknown>
): Promise<T> {
  const res = await fetch(GQL_ENDPOINT, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ query, variables }),
  });
  const json: GqlResponse<T> = await res.json();
  if (json.errors?.length) throw new Error(json.errors[0].message);
  return json.data as T;
}

// --- Subscription client (graphql-ws protocol) ---

type SubscriptionHandler<T> = (data: T) => void;

export function gqlSubscribe<T>(
  query: string,
  variables: Record<string, unknown>,
  onData: SubscriptionHandler<T>,
  onError?: (err: Event) => void
): () => void {
  const ws = new WebSocket(WS_ENDPOINT, "graphql-ws");
  let id = 0;

  ws.onopen = () => {
    ws.send(JSON.stringify({ type: "connection_init" }));
    ws.send(
      JSON.stringify({
        id: String(++id),
        type: "subscribe",
        payload: { query, variables },
      })
    );
  };

  ws.onmessage = (event) => {
    const msg = JSON.parse(event.data as string);
    if (msg.type === "next" && msg.payload?.data) {
      onData(msg.payload.data as T);
    }
  };

  if (onError) ws.onerror = onError;

  return () => {
    ws.send(JSON.stringify({ id: String(id), type: "complete" }));
    ws.close();
  };
}
