# Distribution Router — Rust / Anchor

This is the Solana program for V3, kept in the same repository as the standalone
website. The earlier TypeScript Pump/Stonk transaction adapter is still at
`../../../backend/src/distribution-v3/feeRouter.ts`; it is an off-chain integration,
not this program. The current website/backend do not yet call this Rust program.

## Files

- `src/lib.rs`: instructions, account constraints, vault state and events.
- `src/policy.rs`: integer allocation arithmetic and conservation tests.
- `client.mjs`: Node instruction builders and vault decoding; no signing keys.
- `tests/localnet.test.mjs`: isolated local-validator integration tests.
- `../../Anchor.toml`, `../../Cargo.toml`, `../../Cargo.lock`: program build configuration.
- `../../target/deploy/distribution_router.so`: locally compiled SBF program (ignored by Git).

## Flow

One mint has one canonical program-owned vault PDA:

```text
SOL creator-fee distribution / community deposit
                      ↓
       ["distribution_vault", token mint]
                      ↓
            manager-defined allocation
          campaigns / giveaways / tips / reserve
```

All four recipients must be distinct, nonzero addresses different from the vault.
Shares total 10,000 bps. Routing retains the account's rent exemption and assigns
rounding dust to the last funded bucket, conserving every lamport. A manager can
pause routing; deposits remain possible during a pause.

`route` is permissionless, but cannot substitute recipients. It takes the expected
route sequence and policy version; retries/stale workers cannot spend later funds
or unexpectedly use an updated policy. It routes available SOL, not tokens, and
does not call an exchange or award campaign balances.

## Instructions and authority

| Instruction | Who signs | Purpose |
| --- | --- | --- |
| `initialize_router` | Deployed program upgrade authority | Bootstrap global config, default 0.2 SOL registration fee |
| `configure_router` | Registrar | Set future registration fee/treasury and global pause |
| `register_vault` | Registrar + approved manager | Create the mint's canonical vault; collect registration fee |
| `deposit` | Contributor | Deposit SOL with an attributable event |
| `route` | Any transaction payer | Send SOL only to the four recorded policy recipients |
| `configure_vault` | Current manager | Set recipients, split and minimum batch amount |
| `pause_vault` | Current manager | Pause/resume that vault's routing |
| `propose_manager` / `accept_manager` | Current / proposed manager | Two-step transfer of management |
| `propose_authority` / `accept_authority` | Current / proposed registrar | Two-step registrar rotation |

The registrar verifies creator/CTO management off-chain before co-signing
registration. This prevents someone claiming another project's canonical vault by
being first to submit a transaction. Registrar authority is not spending authority
for existing vaults; their managers control recipient policies.

Activation fees are paid separately from campaign funding. Existing backend-only
activation is not proof that an on-chain fee was paid; migration must explicitly
decide which existing projects receive a waived registration fee to avoid charging
twice. Changing a policy applies to SOL currently waiting in that vault, too.

## Pump and Stonk integration

The program is the receiving/splitting layer. Configuring Pump shareholders still
requires the actual Pump creator/admin signature through the adapter. Supply this
program's per-mint vault PDA as the fee destination only after registration and
launchpad compatibility have been verified. Nothing here can redirect somebody
else's creator fees or override an immutable Pump split.

Stonk/LaunchLab claims still follow LaunchLab creator authority. Existing creator
claims need a relay; this program does not impersonate the creator or invoke an
arbitrary third-party instruction. Receiving SOL does not prove its funding source.

The backend watcher must verify successful finalized transactions and attribute
receipts by vault/mint. Use signature + outer/inner instruction index for receipt
idempotency, and vault + sequence for `FundsRouted` batches. Do not credit the same
SOL once on deposit and again when routed to the treasury. Plain SOL transfers do
not emit `VaultDeposit`; read their actual transfer instructions. The program does
not change the current production ledger or enable the existing fee-router flag.

## Local build and checks

From `distribution-v3-site/`, with Anchor 0.32.1 and Solana 2.3.0 installed:

```bash
npm run router:test:unit
npm run router:build
npm run router:test
```

The localnet test starts its own validator on loopback with temporary accounts and
an isolated ledger; it never loads a user's wallet or contacts a public cluster.
The Node client uses the website's existing `@solana/web3.js` dependency.

The program ID committed in `lib.rs` and `Anchor.toml` is a development placeholder.
For an actual deployment, generate a program key, sync the declared public ID,
rebuild, and initialize with the deployed program's upgrade authority. No mainnet
address or deployment is included. The frontend's `npm run build` remains a
separate Vite build and does not bundle Rust sources or deployment keypairs.

Primary references: [Anchor account constraints](https://www.anchor-lang.com/docs/references/account-constraints),
[PDA design](https://www.anchor-lang.com/docs/basics/pda),
[Pump creator-fee sharing](https://github.com/pump-fun/pump-public-docs/blob/main/docs/instructions/CREATOR_FEE_SHARING.md).
