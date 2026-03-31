import { useCallback, useState } from "react";
import { CashOutInput, ContractAdapter } from "./contract";

export function useCashOut(contract: ContractAdapter) {
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const cashOut = useCallback(
    async (input: CashOutInput) => {
      setLoading(true);
      setError(null);
      try {
        return await contract.cashOut(input);
      } catch (err) {
        const message = err instanceof Error ? err.message : "Failed to cash out";
        setError(message);
        throw err;
      } finally {
        setLoading(false);
      }
    },
    [contract]
  );

  return { cashOut, loading, error };
}
