# Distribution V3 — Rust / Anchor

The on-chain program, Rust keeper, unsigned JavaScript client and isolated
transaction tests are all inside `distribution-v3-site/`. This is a new,
undeployed account layout, not an in-place migration of the earlier prototype.
No existing ANSEM.TIPS database tables, bot handlers or production balances are
changed by this code. The current website/backend still use the earlier
TypeScript integration; connecting them to these instructions is separate work.

## Flow

```text
Creator + approved manager register the project's canonical vault
    ↓
Creator authorizes a supported launchpad fee connection
    ↓
Pump permissionless claim / LaunchLab creator-signed claim / community deposit
    ↓
Project SOL vault → five internal allocation buckets
    ├─ campaigns / giveaways / tips → native SOL campaign escrow
    ├─ TIPANSEM purchases → approved RFQ → reward-token bank → SPL campaign escrow
    └─ reserve → manager-controlled withdrawal
    ↓
Off-chain verifier checks X evidence and signs a bounded reward
    ↓
On-chain rule checks → unique reward receipt → recipient SOL/SPL payment
    ↓
Keeper refills within the configured cap / advances recurring epochs
```

The program does not fetch X, judge content, execute arbitrary DEX instructions,
launch a Pump token, or modify a centralized user balance. Those are separate
services. Actual reward transfers here are on-chain payments to a bound wallet.

## Source map

| File | Responsibility |
| --- | --- |
| `src/lib.rs` | Public instructions and errors |
| `src/state.rs` | Config, vault, bank, campaign, participant, order and receipt accounts |
| `src/vault.rs` | Registrar bootstrap, canonical vault, management, allocations and reserve |
| `src/accounting.rs`, `src/policy.rs` | Checked arithmetic, rent retention and split conservation |
| `src/fees.rs` | Fixed Pump / Pump Fees / PumpSwap / Stonk-LaunchLab CPI adapters |
| `src/purchases.rs` | SPL bank and atomic SOL-for-TIPANSEM RFQ settlement |
| `src/campaigns.rs` | Escrow, funding, limits, awards, recurring epochs and refunds |
| `client.mjs` | Unsigned instruction builders for every public instruction |
| `tests/localnet.test.mjs` | Real local-validator transactions and adversarial checks |
| `../../keeper/` | Separate host Rust process; no verifier or manager key needed |

Amounts are **integer base units**: lamports for native SOL, mint base units for
SPL tokens. Client monetary values should be `bigint`, never floating point.

## Authority boundaries

1. `initialize_router`: only the deployed program's upgrade authority can
   bootstrap the registrar, activation treasury and immutable reward mint. Set
   `reward_mint` to the actual TIPANSEM mint; there is no symbol-based lookup.
2. `register_vault`: registrar **and** manager co-sign. The registrar must verify
   creator/CTO ownership off-chain before signing. This prevents first-come vault
   squatting. Registration defaults to 0.2 SOL, paid separately from vault funds.
3. Managers set allocations, create campaigns and purchase orders, pause their
   vault/campaigns, refund campaigns and withdraw the reserve bucket. The
   registrar cannot spend an existing vault's funds through registrar controls.
4. Only a campaign's configured verifier can sign rewards. A transaction payer
   can pay rent/fees without becoming the verifier. The verifier is a trusted
   content attestor; a compromised verifier can approve false evidence within
   the on-chain budget/identity/reward limits.
5. Keepers can collect supported fees, allocate, fill approved purchases, perform
   configured refills, advance recurring epochs and refund expired campaigns.
   They cannot change managers, rules, recipients or split percentages.
6. Manager and registrar changes use proposal + acceptance. Global/vault pauses
   stop new spending. Deposits and refund paths remain available. Closed campaigns
   and filled/canceled orders cannot be reopened.

Program upgrades remain an administrative trust boundary. Use a reviewed
multisig/timelock before production; no deployment or key changes are performed
by the test commands below.

Managers are trusted policy authorities in this version, not trustless guardians
of community donations: they choose campaign verifiers and can rebalance free
SOL into reserve. A malicious manager/verifier combination can therefore divert
funding despite per-campaign limits. If the product promises irrevocably locked
community funds, add immutable spending/approved-verifier restrictions before
launching that promise; this version does not provide them.

## Vault accounting

There is one vault per project mint. Project mints can be legacy SPL or Token2022;
reward banks currently accept **legacy SPL only** (no transfer-fee or hook mints).

Bucket order is fixed:

| Index | Bucket |
| --- | --- |
| 0 | Campaigns |
| 1 | Giveaways |
| 2 | Tips |
| 3 | TIPANSEM purchases |
| 4 | Reserve |

The five `share_bps` sum to 10,000. `route` accounts for all unallocated SOL above
rent, excluding already allocated balances. It **does not transfer to five
external wallets**. Allocated SOL stays in the vault until escrowed or withdrawn.
Rounding dust is assigned deterministically to a funded bucket. Policy version,
route sequence and minimum-amount checks reject stale/replayed allocation work.

`rebalance_sol` moves a manager-approved amount between free SOL buckets. It
cannot reach existing campaign or purchase escrows. Standard SPL deposits name a
reward bucket (0–2); purchase/reserve buckets are SOL-only. Unsolicited bank token
transfers are recognized only through the
manager's `sync_token_donations`. Allocation does not magically convert SOL into
TIPANSEM: use the purchase path or deposit reward tokens explicitly.

## Creator fees

### Pump.fun / PumpSwap

- `create_pump_sharing` requires the real creator and manager, validates the
  bonding curve, and invokes the official Pump Fees creation instruction.
- `finalize_pump_sharing` supports a sole creator with 100% of an unrevoked
  configuration. It finalizes the creator/vault split and verifies the result.
  **This is irreversible** under the upstream fee-sharing rules. Preview and
  obtain explicit creator approval before signing. Existing multi-party or
  revoked configurations cannot be silently rewritten.
- `connect_pump` attaches an already configured vault beneficiary without
  changing the existing split.
- `sweep_pump_buckets` collects the current curve/pool creator-fee buckets;
  `sweep_pump_amm` moves AMM creator fees into the Pump creator vault;
  `collect_pump_fees` invokes native fee distribution to the exact configured
  shareholders. The router records only the vault's observed lamport increase.
- All CPI program IDs, instruction discriminators, native quote accounts and
  PDAs are checked. No caller-supplied arbitrary instruction is signed by a vault.
  Holder-reward fee configurations and non-native quote assets are rejected.

### Stonkfun / Raydium LaunchLab

- `connect_launchlab` verifies the pool, project mint, WSOL quote, actual creator,
  LaunchLab owner and Stonk platform fee wallet. Creator and manager co-sign.
- `claim_launchlab_fees` requires the recorded creator to co-sign the published
  LaunchLab claim. Only **newly claimed** WSOL is measured. The configured share
  is unwrapped and deposited atomically; preexisting creator WSOL is excluded,
  and temporary token-account rent goes back to its payer.
- LaunchLab's claim is aggregated by **creator + quote mint**, not exclusively by
  project mint. The event is deliberately a creator-authorized aggregate claim,
  not proof that every lamport originated from this one project's trades.
- `relay_launchlab_fees` is an explicitly attributed creator-signed SOL relay.
  It is not counted as verified per-mint Pump revenue.
- Existing LaunchLab creator authority is not bypassed. The permissionless
  keeper does **not** load creator keys or claim these fees automatically.
  Post-graduation CPMM creator-fee claims require a separate adapter and are
  **not implemented** here. Do not advertise perpetual automatic Stonk fee
  routing until those authority and graduation paths are integrated and tested.

See [ABI references](docs/ABI.md) for upstream sources and test limitations.

## TIPANSEM purchase settlement

`create_purchase` locks an exact SOL amount from bucket 3, fixes a minimum token
output, an expiry (maximum 24 hours), and an output bucket (0–2). The reward mint
is fixed by router configuration. Anyone can `fill_purchase` by delivering at
least those tokens to the correct bank **before** receiving the locked SOL in the
same atomic transaction. Only a successful transfer releases payment.

This is an **RFQ/inventory settlement**, not a Jupiter swap. An external market
maker can source inventory separately. The included keeper optionally fills only
explicitly allowlisted orders, with independent minimum SOL / maximum token
approval. It does not discover quotes or create orders itself. Managers can
cancel open orders; anyone can refund expired orders. Order accounts remain as
single-use receipts. Automatic DEX price discovery/purchasing is future work.

## Campaign lifecycle

1. Manager calls `create_campaign` with an immutable rule commitment, verifier,
   reward mint (zero pubkey = native SOL), bucket 0–2, start/end, reward min/max,
   rolling/daily/lifetime limits, epoch cap and optional refill/recurrence.
2. For SPL rewards, initialize the token bank and `initialize_campaign_tokens`.
   SOL and token campaign funding are separate paths; wrong-mint substitution
   fails account constraints.
3. Manager prefunds the campaign or enables permissionless fixed-policy refill.
   Refill requires the configured threshold, active window, available source
   balance and remaining epoch funding cap. Multiple campaigns compete for the
   same free bucket; no balance is guaranteed twice.
4. Off-chain backend verifies content, canonical X user/post IDs, eligibility and
   wallet ownership. It signs `award_sol` / `award_tokens` with `Award` evidence,
   the rules hash, explicit amount and expiry no more than one hour ahead.
5. The program enforces campaign status/window, amount, remaining escrow,
   configured verifier, rules hash, participant identity/wallet and limits. It
   creates a unique `(campaign, post_id)` receipt before transferring funds.
   Failure rolls back receipts, counters and transfers atomically.
6. Rolling limits count approved awards in the previous `window_seconds`
   (maximum 64, window up to seven days). Daily limits reset at **UTC midnight**;
   lifetime limit zero disables that extra cap. Rejected attempts consume none.
7. `advance_campaign_epoch` renews an active recurring campaign after expiry.
   Unspent escrow carries forward and consumes the new epoch cap. Participant
   limits and old post receipts do not reset.
8. Manager can close/refund at any time. After a nonrecurring expiry anyone can
   return unused escrow to its original project bucket. Anyone cannot close a
   recurring campaign merely because an epoch ended. Refunds do not pay callers.

The verifier must use stable IDs (including edit-history canonicalization) and
must not fabricate new IDs to bypass limits. Same-post protection is **per
campaign**, not a global prohibition on one post joining different campaigns.
Participants bind a recipient wallet at first reward; wallet rotation is not
implemented. The content rule commitment does not make an off-chain AI judgment
trustless.

Giveaways and individual tips use the same bounded reward machinery with bucket
1 or 2. Selection/content evaluation stays off-chain; there is no on-chain X
scraper or autonomous random-winner oracle.

## Canonical accounts

All seeds below are under this router program unless stated otherwise. IDs are
32 raw bytes (the client exports `evidenceId` for SHA-256 derivation).

| Account | Seeds |
| --- | --- |
| Router | `distribution_router` |
| Project vault | `distribution_vault`, project mint |
| Token bank | `token_bank`, vault, reward mint |
| Bank token account | `bank_tokens`, bank |
| Campaign | `campaign`, vault, campaign ID |
| Campaign token account | `campaign_tokens`, campaign |
| Participant | `participant`, campaign, canonical user ID |
| Reward receipt | `reward`, campaign, canonical post ID |
| Purchase order | `purchase`, vault, order ID |
| Temporary LaunchLab WSOL buffer | `claim_buffer`, vault |

Account schemas are intentionally fixed for this version. Existing deployed
prototype accounts would require an explicit reviewed migration or a new program
address. This implementation does not perform one automatically.

## Website / backend connection contract

- Use `client.mjs` or the generated Anchor IDL to prepare instructions, then get
  the required creator/manager signatures from their wallets. Never place those
  private keys in browser variables or the existing database.
- Run the dedicated [Rust keeper](../../keeper/README.md) for fee collection,
  allocations, refills, optional inventory settlement and epoch maintenance.
- Have the existing X verifier submit signed awards; the keeper cannot approve
  content. A Rust backend can import this crate with `no-entrypoint` and use
  generated `accounts` + `instruction` types just as the keeper does.
- Index successful **finalized** transactions and canonical receipt accounts.
  Deduplicate by program + signature + instruction/event index, and by campaign
  + receipt/post ID for rewards. Failed transactions can contain logs: do not
  credit their emitted events. Deposits, fee collection, allocation and campaign
  funding are movements of the same funds, not four separate user credits.
- If integrating with ANSEM.TIPS custodial balances, define an explicit treasury
  recipient and idempotent ledger bridge first. Do not both pay a user's wallet
  and credit their dashboard for the same reward. This bridge is not implemented
  or enabled by this program.
- Existing backend activation does not prove the on-chain registration fee was
  paid. Decide how existing projects are grandfathered before onboarding them.

## Build and verification

From `distribution-v3-site/` with Anchor 0.32.1 and Solana 2.3 installed:

```bash
npm run router:test:unit
npm run router:build
npm run router:test
npm run router:idl
cargo test --manifest-path keeper/Cargo.toml --locked
```

The transaction suite starts an isolated loopback validator with temporary
accounts and a temporary ledger. It uses the real published SPL Token processor,
but **simplified local launchpad fixtures**, not deployed Pump/Raydium binaries.
Those fixtures test account plumbing, signature gates, rollback and conservation;
they do not prove upstream compatibility. Before production, test the precise
live launchpad binaries/accounts on a pinned local fork, exercise graduation,
and get an independent security review.

The program ID in `lib.rs`, `Anchor.toml` and keeper example is a development
placeholder. No mainnet deployment, production wallet loading or production
transaction is part of these commands. `target/` and keypair files are ignored.
Vite's build remains separate and does not bundle Rust or deployment secrets.
