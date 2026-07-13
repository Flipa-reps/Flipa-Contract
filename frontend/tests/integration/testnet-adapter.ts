/**
 * Minimal ContractAdapter implementation that calls the deployed Flipa contract
 * on Stellar testnet via stellar-sdk.
 *
 * Used exclusively by testnet integration tests. Not imported by the app.
 */

import {
  Contract,
  Keypair,
  Networks,
  rpc as SorobanRpc,
  TransactionBuilder,
  xdr,
  nativeToScVal,
  scValToNative,
  BASE_FEE,
} from "@stellar/stellar-sdk";
import type {
  ContractAdapter,
  StartGameInput,
  RevealInput,
  CashOutInput,
  ContinueInput,
} from "../../hooks/contract";

const RPC_URL = process.env.TESTNET_RPC_URL ?? "https://soroban-testnet.stellar.org";
const CONTRACT_ID = process.env.TESTNET_CONTRACT_ID!;
const SECRET_KEY = process.env.TESTNET_SECRET_KEY!;

const server = new SorobanRpc.Server(RPC_URL, { allowHttp: false });

async function invoke(method: string, args: xdr.ScVal[]): Promise<xdr.ScVal> {
  const keypair = Keypair.fromSecret(SECRET_KEY);
  const account = await server.getAccount(keypair.publicKey());
  const contract = new Contract(CONTRACT_ID);

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: Networks.TESTNET,
  })
    .addOperation(contract.call(method, ...args))
    .setTimeout(30)
    .build();

  const simResult = await server.simulateTransaction(tx);
  if (SorobanRpc.Api.isSimulationError(simResult)) {
    throw new Error(`Simulation failed: ${simResult.error}`);
  }

  const assembled = SorobanRpc.assembleTransaction(tx, simResult).build();
  assembled.sign(keypair);

  const sendResult = await server.sendTransaction(assembled);
  if (sendResult.status === "ERROR") {
    throw new Error(`Send failed: ${JSON.stringify(sendResult.errorResult)}`);
  }

  // Poll for confirmation (testnet ~5 s per ledger)
  let getResult: SorobanRpc.Api.GetTransactionResponse;
  do {
    await new Promise((r) => setTimeout(r, 1000));
    getResult = await server.getTransaction(sendResult.hash);
  } while (getResult.status === SorobanRpc.Api.GetTransactionStatus.NOT_FOUND);

  if (getResult.status !== SorobanRpc.Api.GetTransactionStatus.SUCCESS) {
    throw new Error(`Transaction failed: ${getResult.status}`);
  }

  return getResult.returnValue ?? xdr.ScVal.scvVoid();
}

export function createTestnetAdapter(): ContractAdapter {
  return {
    async startGame({ wagerStroops, side, commitmentHash }: StartGameInput) {
      const result = await invoke("start_game", [
        nativeToScVal(wagerStroops, { type: "i128" }),
        nativeToScVal(side === "heads" ? 0 : 1, { type: "u32" }),
        nativeToScVal(Buffer.from(commitmentHash.replace("0x", ""), "hex"), { type: "bytes" }),
      ]);
      const native = scValToNative(result) as { tx_hash: string };
      return { txHash: native.tx_hash };
    },

    async reveal({ gameId, secret }: RevealInput) {
      const result = await invoke("reveal", [
        nativeToScVal(gameId),
        nativeToScVal(Buffer.from(secret, "hex"), { type: "bytes" }),
      ]);
      const native = scValToNative(result) as { tx_hash: string; outcome: number };
      return { txHash: native.tx_hash, outcome: native.outcome === 0 ? "win" : "loss" };
    },

    async cashOut({ gameId }: CashOutInput) {
      const result = await invoke("cash_out", [nativeToScVal(gameId)]);
      const native = scValToNative(result) as { tx_hash: string; payout_stroops: number };
      return { txHash: native.tx_hash, payoutStroops: native.payout_stroops };
    },

    async continueGame({ gameId }: ContinueInput) {
      const result = await invoke("continue_streak", [nativeToScVal(gameId)]);
      const native = scValToNative(result) as { tx_hash: string };
      return { txHash: native.tx_hash };
    },
  };
}
