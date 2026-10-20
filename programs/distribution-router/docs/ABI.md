# External interface references

Adapters were checked against these official interfaces on 2026-10-16. Treat
upstream layouts as versioned dependencies; recheck them before deployment.

- [Pump creator-fee sharing](https://github.com/pump-fun/pump-public-docs/blob/main/docs/instructions/CREATOR_FEE_SHARING.md)
- [Pump fee collection](https://github.com/pump-fun/pump-public-docs/blob/main/docs/instructions/COLLECT_CREATOR_FEE.md)
- [Pump IDL](https://github.com/pump-fun/pump-public-docs/blob/main/idl/pump.json)
- [Pump Fees IDL](https://github.com/pump-fun/pump-public-docs/blob/main/idl/pump_fees.json)
- [Pump AMM IDL](https://github.com/pump-fun/pump-public-docs/blob/main/idl/pump_amm.json)
- [Raydium LaunchLab IDL](https://github.com/raydium-io/raydium-idl/blob/master/raydium_launchpad/raydium_launchpad.json)
- [Raydium creator-fee model](https://github.com/raydium-io/raydium-docs-v1/blob/master/products/launchlab/creator-fees.mdx)

## Fixed program identities

| Program | Public key |
| --- | --- |
| Pump | `6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P` |
| Pump Fees | `pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ` |
| Pump AMM | `pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA` |
| LaunchLab | `LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj` |
| Stonk platform fee wallet | `AvVCE7Ue49iZjYzkkHz6ZhVyvY6NLHw67vB8eQaffVPz` |

The Stonk wallet alone is not creator authority. The adapter also verifies the
official LaunchLab owner, pool/platform discriminators, creator, base and quote.

## Supported instructions

- Pump Fees `create_fee_sharing_config`, `update_fee_shares_v2`.
- Pump / Pump AMM `sweep_creator_fee`.
- Pump AMM `transfer_creator_fees_to_pump_v2`.
- Pump `distribute_creator_fees_v2` with `initialize_ata = false`, native quote.
- LaunchLab `claim_creator_fee`, creator-signed WSOL claim.

The router supplies only these fixed instruction payloads. It checks exact
remaining-account keys/ordering, ownership, discriminators and canonical PDAs.
See `fees.rs` for the complete account lists and hard-coded discriminators.

## Keeper polling layout

Offsets include Anchor's eight-byte discriminator:

| Account field | Byte range |
| --- | --- |
| Pump curve complete | 48 |
| Pump curve creator | 49–80 |
| Pump curve native quote mint, newer layout | 83–114 |
| Pump curve creator fee counter, newer layout | 125–132 |
| Pump AMM pool creator fee counter | 279–286 |
| Pump SharingConfig shareholder count | 76–79 |
| Pump SharingConfig shareholder entries | 80 onwards, 34 bytes each |
| Legacy SPL token amount | 64–71 |

The keeper treats missing newer counters as zero, and can still collect fees
already held by legacy creator vaults. Missing/changed upstream layouts may need
a keeper update; they are not permission to guess account ownership or bypass
on-chain checks. The on-chain adapter performs authoritative validation even
when a keeper polling hint is wrong.

## Local fixtures are not live protocol tests

`test-programs/launchpad-fixture` is intentionally a test double, loaded only on
an isolated local validator under the external program IDs. It checks selected
signers/layouts and makes deterministic transfers. It is not the official Pump
or LaunchLab implementation and must never be deployed under a real service.

`test-programs/spl-token` wraps the published SPL Token v8 processor for actual
local token transfer/close-account validation. External protocol integration
still needs a pinned fork with official executable binaries and representative
pre/post-graduation account states before real funds are involved.
