// DataLoader implementation for batching and caching N+1 queries

type BatchLoadFn<K, V> = (keys: readonly K[]) => Promise<(V | Error)[]>;

class DataLoader<K, V> {
  private readonly batchFn: BatchLoadFn<K, V>;
  private cache = new Map<K, Promise<V>>();
  private queue: { key: K; resolve: (v: V) => void; reject: (e: Error) => void }[] = [];
  private scheduled = false;

  constructor(batchFn: BatchLoadFn<K, V>) {
    this.batchFn = batchFn;
  }

  load(key: K): Promise<V> {
    if (this.cache.has(key)) return this.cache.get(key)!;

    const promise = new Promise<V>((resolve, reject) => {
      this.queue.push({ key, resolve, reject });
      if (!this.scheduled) {
        this.scheduled = true;
        Promise.resolve().then(() => this.dispatch());
      }
    });

    this.cache.set(key, promise);
    return promise;
  }

  clearAll(): void {
    this.cache.clear();
  }

  private async dispatch(): Promise<void> {
    this.scheduled = false;
    const batch = this.queue.splice(0);
    const keys = batch.map((b) => b.key);
    try {
      const results = await this.batchFn(keys);
      batch.forEach(({ resolve, reject }, i) => {
        const r = results[i];
        r instanceof Error ? reject(r) : resolve(r);
      });
    } catch (err) {
      batch.forEach(({ reject }) => reject(err as Error));
    }
  }
}

// --- Loaders ---

export interface GameState {
  id: string;
  player: string;
  wager: string;
  streak: number;
  multiplier: number;
  status: string;
  result?: string;
  createdAt: string;
  updatedAt: string;
}

export interface PlayerStats {
  address: string;
  totalGames: number;
  wins: number;
  losses: number;
  bestStreak: number;
  totalWagered: string;
  totalWon: string;
}

// Batch-load game states by ID
export const gameLoader = new DataLoader<string, GameState>(async (ids) => {
  // In production this would call the Stellar RPC / indexer
  return ids.map((id) => ({
    id,
    player: "GPLACEHOLDER",
    wager: "10",
    streak: 0,
    multiplier: 1.0,
    status: "PENDING",
    createdAt: new Date().toISOString(),
    updatedAt: new Date().toISOString(),
  }));
});

// Batch-load player stats by address
export const playerStatsLoader = new DataLoader<string, PlayerStats>(async (addresses) => {
  return addresses.map((address) => ({
    address,
    totalGames: 0,
    wins: 0,
    losses: 0,
    bestStreak: 0,
    totalWagered: "0",
    totalWon: "0",
  }));
});
