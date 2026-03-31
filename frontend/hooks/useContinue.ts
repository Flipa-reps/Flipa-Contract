import { useCallback, useState } from "react";
import { ContinueInput, ContractAdapter } from "./contract";

export function useContinue(contract: ContractAdapter) {
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const continueGame = useCallback(
    async (input: ContinueInput) => {
      setLoading(true);
      setError(null);
      try {
        return await contract.continueGame(input);
      } catch (err) {
        const message = err instanceof Error ? err.message : "Failed to continue game";
        setError(message);
        throw err;
      } finally {
        setLoading(false);
      }
    },
    [contract]
  );

  return { continueGame, loading, error };
}
