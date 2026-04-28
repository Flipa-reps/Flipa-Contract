import { gameLoader, playerStatsLoader } from "./dataloader";

export const resolvers = {
  Query: {
    game: async (_: unknown, { id }: { id: string }) => {
      return gameLoader.load(id);
    },

    playerGames: async (
      _: unknown,
      { address, limit = 10, offset = 0 }: { address: string; limit?: number; offset?: number }
    ) => {
      // In production: query indexer/RPC for player's games
      return [];
    },

    playerStats: async (_: unknown, { address }: { address: string }) => {
      return playerStatsLoader.load(address);
    },

    recentTransactions: async (_: unknown, { limit = 20 }: { limit?: number }) => {
      // In production: query transaction history
      return [];
    },
  },

  Subscription: {
    gameUpdated: {
      subscribe: (_: unknown, { id }: { id: string }) => {
        // In production: subscribe to game state changes via WebSocket or SSE
        return {
          [Symbol.asyncIterator]: async function* () {
            yield { gameUpdated: await gameLoader.load(id) };
          },
        };
      },
    },

    playerStatsUpdated: {
      subscribe: (_: unknown, { address }: { address: string }) => {
        return {
          [Symbol.asyncIterator]: async function* () {
            yield { playerStatsUpdated: await playerStatsLoader.load(address) };
          },
        };
      },
    },
  },
};
