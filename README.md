# ANSEM.TIPS Distribution V3

Standalone website for the ANSEM.TIPS social-distribution network. This app is
intentionally separate from `dashboard/`: it has its own package, routes, build,
deployment configuration, X authentication boundary, and Solana wallet connection.

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
