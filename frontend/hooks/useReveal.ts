import { useCallback, useState } from "react";
import { ContractAdapter, RevealInput } from "./contract";

export function useReveal(contract: ContractAdapter) {
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const reveal = useCallback(
    async (input: RevealInput) => {
      setLoading(true);
      setError(null);
      try {
        return await contract.reveal(input);
      } catch (err) {
        const message = err instanceof Error ? err.message : "Failed to reveal result";
        setError(message);
        throw err;
      } finally {
        setLoading(false);
      }
    },
    [contract]
  );

  return { reveal, loading, error };
}
