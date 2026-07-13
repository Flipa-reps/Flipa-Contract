// GraphQL schema definition for Flipa
export const typeDefs = `
  type GameState {
    id: ID!
    player: String!
    wager: String!
    streak: Int!
    multiplier: Float!
    status: GameStatus!
    result: CoinFace
    createdAt: String!
    updatedAt: String!
  }

  type PlayerStats {
    address: String!
    totalGames: Int!
    wins: Int!
    losses: Int!
    bestStreak: Int!
    totalWagered: String!
    totalWon: String!
  }

  type Transaction {
    id: ID!
    player: String!
    type: TxType!
    amount: String!
    timestamp: String!
    txHash: String
  }

  enum GameStatus {
    PENDING
    COMMITTED
    REVEALED
    CASHED_OUT
    FORFEITED
  }

  enum CoinFace {
    HEADS
    TAILS
  }

  enum TxType {
    WAGER
    PAYOUT
    CASHOUT
    RECLAIM
  }

  type Query {
    game(id: ID!): GameState
    playerGames(address: String!, limit: Int, offset: Int): [GameState!]!
    playerStats(address: String!): PlayerStats
    recentTransactions(limit: Int): [Transaction!]!
  }

  type Subscription {
    gameUpdated(id: ID!): GameState!
    playerStatsUpdated(address: String!): PlayerStats!
  }
`;
