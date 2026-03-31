export interface StartGameInput {
  wagerStroops: number;
  side: "heads" | "tails";
  commitmentHash: string;
}

export interface ContinueInput {
  gameId: string;
}

export interface RevealInput {
  gameId: string;
  secret: string;
}

export interface CashOutInput {
  gameId: string;
}

export interface ContractAdapter {
  startGame(input: StartGameInput): Promise<{ txHash: string }>;
  reveal(input: RevealInput): Promise<{ txHash: string; outcome: "win" | "loss" }>;
  cashOut(input: CashOutInput): Promise<{ txHash: string; payoutStroops: number }>;
  continueGame(input: ContinueInput): Promise<{ txHash: string }>;
}
