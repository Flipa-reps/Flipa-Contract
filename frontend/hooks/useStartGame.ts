import { useCallback, useState } from "react";
import { ContractAdapter, StartGameInput } from "./contract";

export function useStartGame(contract: ContractAdapter) {
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const startGame = useCallback(
    async (input: StartGameInput) => {
      setLoading(true);
      setError(null);
      try {
        return await contract.startGame(input);
      } catch (err) {
        const message = err instanceof Error ? err.message : "Failed to start game";
        setError(message);
        throw err;
      } finally {
        setLoading(false);
      }
    },
    [contract]
  );

  return { startGame, loading, error };
}
