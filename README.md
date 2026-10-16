# ANSEM.TIPS Distribution V3

Standalone website for the ANSEM.TIPS social-distribution network. This app is
intentionally separate from `dashboard/`: it has its own package, routes, build,
deployment configuration, X authentication boundary, and Solana wallet connection.

## Rust program and router

The Solana program lives **inside this project** at
[`programs/distribution-router/src/lib.rs`](programs/distribution-router/src/lib.rs).
Its allocation arithmetic, Node client, localnet tests and
[program documentation](programs/distribution-router/README.md) are alongside it.
`Anchor.toml` and `Cargo.toml` at this root build the program independently of Vite.

The earlier Pump/Stonk TypeScript adapter remains at
`../backend/src/distribution-v3/feeRouter.ts`, where the backend currently imports
it. It prepares launchpad transactions; the Rust program receives and routes SOL.
The existing website is not yet connected to the Rust program, and nothing here
deploys it or changes production balances.

## Routes

- `/` — public vault network
- `/new` — manager verification and token onboarding
- `/:slug` — public vault, contribution history, allocation and manager controls

## Local development

```bash
npm install --legacy-peer-deps
cp .env.example .env
npm run dev
```

The backend must already have the additive Distribution V3 migration and routes.
Keep `DISTRIBUTION_V3_ENABLED=false` until that migration is deployed.

## Environment

| Variable | Purpose |
| --- | --- |
| `VITE_API_BASE_URL` | Backend origin or `/api` URL |
| `VITE_PRIVY_APP_ID` | Public Privy application ID used for X identity |
| `VITE_SOLANA_RPC_URL` | Browser RPC used by Wallet Standard adapters |
| `VITE_DASHBOARD_URL` | Existing ANSEM.TIPS dashboard, used for SOL top-up links |

The production website origin must be added to backend `CORS_ORIGINS` and to the
Privy application's allowed origins and OAuth redirect URLs.
