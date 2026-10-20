# Distribution keeper (Rust)

This host process performs permissionless maintenance for configured on-chain
vaults. It is not deployed as a Solana program and is not an X/content verifier.

```bash
cargo test --manifest-path keeper/Cargo.toml --locked
cargo build --manifest-path keeper/Cargo.toml --release --locked
keeper/target/release/distribution-keeper inspect keeper/config.example.json
keeper/target/release/distribution-keeper tick /absolute/config.json /absolute/keeper-wallet.json
keeper/target/release/distribution-keeper watch /absolute/config.json /absolute/keeper-wallet.json
```

`inspect` never loads a signing key or sends transactions. The example has no
projects, so it is inert. For real work, copy it to a private local config, set
the deployed program address and RPC, then add explicit project mint addresses,
campaign IDs and optional approved purchase orders. IDs are 64-character hex
encodings of the raw 32-byte seeds, not campaign names.

Example project entry (replace every placeholder; amounts are integer base units):

```json
{
  "mint": "PROJECT_MINT_ADDRESS",
  "minimum_fee_lamports": 1000000,
  "campaigns": ["64_HEX_CHARACTERS_FROM_CAMPAIGN_ID"],
  "purchase_source_token_account": "OPTIONAL_KEEPER_OWNED_TIPANSEM_TOKEN_ACCOUNT",
  "purchases": [
    {
      "id": "64_HEX_CHARACTERS_FROM_APPROVED_ORDER_ID",
      "maximum_tokens": 1000000000,
      "minimum_sol_lamports": 10000000
    }
  ]
}
```

Omit `purchase_source_token_account` and `purchases` to disable inventory fills.
Only explicitly listed orders can fill. Both the on-chain manager-approved
minimum output and the keeper's independent max-token/min-SOL approval must pass.
The source must be a legacy SPL account for the fixed reward mint, owned by this
keeper. An unexpired approved order with insufficient inventory waits. Expired
listed orders are refunded to the project; filled/canceled receipts are skipped.
No DEX quotes, swaps or purchase-order creation happen in this worker.

Each tick:

1. Reads finalized router/vault state and checks pauses.
2. Sweeps supported Pump curve/AMM fee buckets above the configured threshold,
   moves AMM creator fees to Pump, and distributes configured shareholder fees.
3. Allocates previously unallocated SOL using the current version and sequence.
4. Optionally fills explicitly approved TIPANSEM inventory orders atomically.
5. Advances listed recurring campaigns; refunds expired nonrecurring campaigns;
   refills eligible native/SPL campaigns within their fixed rules and caps.

Campaign creation, purchase approval, creator fee setup and content awards are
intentionally not autonomous. LaunchLab claims still require a creator signature
and are not attempted by this permissionless worker. It does not discover all
campaigns automatically; configure the IDs you intend to maintain.

Use a **dedicated keeper/solver wallet**, never the registrar, manager, verifier,
shared payout wallet or creator key. It needs transaction fees and optionally
approved solver inventory, but has no authority over vault policies or awards.
Do not commit configs containing private RPC URLs or wallet JSON. The worker
does not log private keys or the configured RPC URL.

Transactions use preflight, a 600,000-compute-unit limit and finalized state. Each
write is finalized before the next dependent action. A timeout is not proof of
failure: the next tick reloads receipt/sequence/status state before attempting
work again. Do not manually bypass those guards to force a retry. `watch` logs
per-project failures and tries again after the interval (minimum ten seconds).
Scheduling is best-effort; downtime does not guarantee an exact refill time.

The on-chain constraints remain authoritative even if a keeper is malicious or
working from stale RPC data. Before production, validate upstream fee adapters
on a pinned fork and complete a security review as described in the
[program documentation](../programs/distribution-router/README.md).
