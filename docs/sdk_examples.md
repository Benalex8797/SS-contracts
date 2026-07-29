# StellarSettle Client Integration SDK — Example Recipes (JavaScript/TypeScript)

Copy-pasteable recipes for integrating StellarSettle contracts from a JavaScript/TypeScript front-end or Node.js backend using the `@stellar/stellar-sdk`.

---

## Prerequisites

```bash
npm install @stellar/stellar-sdk
```

```typescript
import {
  SorobanRpc,
  Contract,
  TransactionBuilder,
  Networks,
  BASE_FEE,
  Address,
  nativeToScVal,
  scValToNative,
  xdr,
} from "@stellar/stellar-sdk";

const rpc = new SorobanRpc.Server("https://soroban-testnet.stellar.org");
const networkPassphrase = Networks.TESTNET;
```

---

## Recipe 1 — Read an Escrow

```typescript
async function getEscrow(contractId: string, invoiceId: string) {
  const contract = new Contract(contractId);
  const tx = new TransactionBuilder(await rpc.getAccount(sourcePublicKey), {
    fee: BASE_FEE,
    networkPassphrase,
  })
    .addOperation(contract.call("get_escrow", nativeToScVal(invoiceId, { type: "symbol" })))
    .setTimeout(30)
    .build();

  const simResult = await rpc.simulateTransaction(tx);
  if (SorobanRpc.Api.isSimulationSuccess(simResult)) {
    return scValToNative(simResult.result!.retval);
  }
  throw new Error("Simulation failed");
}
```

---

## Recipe 2 — Create an Escrow

```typescript
async function createEscrow(
  contractId: string,
  seller: string,
  debtor: string,
  faceValue: bigint,
  purchasePrice: bigint,
  dueDate: number,
  paymentToken: string,
  invoiceToken: string,
  invoiceId: string,
) {
  const contract = new Contract(contractId);
  const sourceAccount = await rpc.getAccount(seller);

  const tx = new TransactionBuilder(sourceAccount, {
    fee: BASE_FEE,
    networkPassphrase,
  })
    .addOperation(
      contract.call(
        "create_escrow",
        nativeToScVal(invoiceId, { type: "symbol" }),
        new Address(seller).toScVal(),
        new Address(debtor).toScVal(),
        nativeToScVal(faceValue, { type: "i128" }),
        nativeToScVal(purchasePrice, { type: "i128" }),
        nativeToScVal(dueDate, { type: "u64" }),
        new Address(paymentToken).toScVal(),
        new Address(invoiceToken).toScVal(),
      ),
    )
    .setTimeout(30)
    .build();

  const prepared = await rpc.prepareTransaction(tx);
  // Sign `prepared` with seller's keypair, then submit
  const result = await rpc.sendTransaction(prepared);
  return result;
}
```

---

## Recipe 3 — Query Token Balance

```typescript
async function getTokenBalance(tokenContractId: string, owner: string) {
  const contract = new Contract(tokenContractId);
  const tx = new TransactionBuilder(await rpc.getAccount(owner), {
    fee: BASE_FEE,
    networkPassphrase,
  })
    .addOperation(contract.call("balance", new Address(owner).toScVal()))
    .setTimeout(30)
    .build();

  const sim = await rpc.simulateTransaction(tx);
  if (SorobanRpc.Api.isSimulationSuccess(sim)) {
    return scValToNative(sim.result!.retval) as bigint;
  }
  return 0n;
}
```

---

## References

- Contract ABI: [`contracts/invoice-escrow/src/lib.rs`](../contracts/invoice-escrow/src/lib.rs)
- Token interface: [`contracts/invoice-token/src/lib.rs`](../contracts/invoice-token/src/lib.rs)
- Error catalog: [`docs/error_catalog.md`](error_catalog.md)
