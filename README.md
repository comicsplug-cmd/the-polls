# The Polls

A mobile-first Nigerian mock-poll application built with Next.js, Axum, SQLx, and SQLite. The app is designed for a public poll window of 26 December 2026 through 4 January 2027 (WAT), subject to every launch gate in the implementation plan. It must remain in preview until the legal, roster, payment, operations, and domain gates are complete.

## Local development

Prerequisites: Rust stable, Node.js 20+, and npm.

1. Copy `.env.example` to `.env`. Leave payment keys blank for preview. Never paste real keys into source control or command history.
2. Start the API from the repository root so it reads `.env`:

   ```powershell
   cargo run --manifest-path server/Cargo.toml
   ```

3. Start the web app in another terminal:

   ```powershell
   cd web
   $env:API_SERVER_BASE = "http://127.0.0.1:8080/api"
   npm install
   npm run dev
   ```

The preview has election pages and sample tallies; paid checkout stays unavailable until explicitly enabled and configured. Any credential shared in chat should be treated as exposed and replaced before integration. The Paystack secret belongs only in the API process environment.

## Deploy

Three supported paths, from least to most control.

### 1. Docker Compose on any VPS (recommended)

Prerequisites: Docker Engine with the Compose plugin, a DNS record pointing at the host, and ports 80/443 open.

```sh
git clone <repo-url> the-polls && cd the-polls
cp .env.example .env
# Fill in PAYSTACK_SECRET_KEY, both HMAC keys, ADMIN_PASSWORD_HASH,
# ADMIN_SESSION_HMAC_KEY, POLL_DOMAIN, PUBLIC_BASE_URL and API_PUBLIC_URL.
docker compose --profile tls up -d --build
```

`docker compose up -d --build` without the profile runs the API and site on `127.0.0.1:8080` and `127.0.0.1:3000` for a smoke test. The `tls` profile adds Caddy, which obtains and renews certificates for `POLL_DOMAIN`, routes `/api/*` straight to the API, and forwards everything else to Next.js.

The SQLite database lives in the `polls-data` volume at `/data/polls.sqlite`. Back it up with the procedure in `docs/OPERATIONS.md`; recreating the container without that volume loses every ballot.

### 2. Prebuilt images from GHCR

Tag a release (`git tag v0.1.0 && git push --tags`) and the publish workflow pushes:

- `ghcr.io/<owner>/<repo>/api:<tag>`
- `ghcr.io/<owner>/<repo>/web:<tag>`

The API needs a persistent volume at `/data` and the same environment variables as `.env.example`. The web image needs `API_SERVER_BASE` at run time and is built with `API_PROXY_TARGET` (default `http://api:8080`). This is the shortest path on Fly.io, Render, Railway, or Kubernetes.

### 3. VPS with systemd and Caddy

Use `deploy/the-polls-api.service`, `deploy/the-polls-web.service`, and `deploy/Caddyfile` with a release build of the API. `docs/OPERATIONS.md` covers the hardening, backup, and incident steps. The Caddyfile reads `API_UPSTREAM` and `WEB_UPSTREAM` and defaults to localhost, so it works unchanged for the systemd layout.

### Preview-only hosting

The Next.js app alone can be deployed to Vercel (project root `web/`) as a read-only showcase. Without `API_SERVER_BASE` it serves sample data and disables voting, receipts, and admin actions. That is not the live service: real ballots need the API and its durable SQLite volume.

## Shareable Vercel showcase

The Next.js app is under `web/`. Set that as the Vercel project root. If no `API_SERVER_BASE` is configured, the site serves a clearly labelled, read-only preview from `web/src/lib/preview-data.ts`; voting, receipts, and admin actions are disabled. This mode is suitable for sharing the prototype with friends. It is not the live poll service. A full deployment still needs the Axum API and durable SQLite host from `docs/OPERATIONS.md`, with `API_SERVER_BASE` and `API_PROXY_TARGET` pointing to that HTTPS API. Keep payment credentials on the API host only.

## Components

- `server/`: Axum API, SQLite migrations, Paystack verification/refunds, receipt and admin endpoints.
- `web/`: server-rendered Next.js public site and small client islands for voting and admin operations.
- `data/seed/elections.json`: roster seed. The file is a starting reference only; it must be checked against the current official INEC lists and QA signed off before launch. The sample governorship race is explicitly marked pending QA.
- `deploy/`: Caddy and systemd examples for a single Linux VPS.

## Payment behavior

The product price is ₦100 (10,000 kobo). For card, Paystack hosted checkout is used with the dashboard fee pass-through setting; enable paid mode only after card, international card, and transfer totals have been rehearsed. Pay with Transfer uses a one-time Paystack charge account with a one-hour expiry. The code computes a conservative transfer gross-up and displays it before the customer continues. Do not use Dedicated Virtual Accounts.

Paystack transaction verification is authoritative. A ballot is counted only after verification of a successful NGN payment for at least the base product amount, and `paid_at` is checked against the poll cutoff inside the settlement transaction. Late, duplicate, mismatched, or otherwise uncountable successful transactions are sent for a full refund of the actual paid amount; the UI does not call the refund complete until Paystack reports it processed. Paystack's original processing fee is not returned, so maintain a refund reserve.

Settlement takes an exclusive, two-minute claim on a ballot before calling Paystack Verify. That is what stops a webhook, a browser callback, and the reconciliation worker from settling the same payment twice. A refund is only requested by the worker that won the claim.

### Ballot identity

Every ballot has two identifiers:

- A **receipt ID** (`pr_` plus 128 random bits) created at checkout so a transfer payer can watch the payment land.
- A **ballot code** (`TP26-XXXXXXXX-XXXXXXXX`) derived with HMAC from the verified Paystack transaction id and the election id. It is created only when Paystack confirms success. It is one-way, election-scoped, and never reveals the candidate or the private payment reference.

`/find` resolves either identifier. Public receipts stay candidate-blind: they show the poll, the status, and the time, never the choice or the payment reference.

Receipts are candidate-blind. Candidate and location links are detached 60 days after close. Restricted transaction support references are retained up to one year, then purged while aggregate results and public receipt status remain.

## Test-mode rehearsal

The full loop can be exercised without a live account:

1. Put a Paystack **test** secret in `.env` (`sk_test_…`).
2. Set `PAID_POLLING_ENABLED=true`, `PAYSTACK_FEE_PASS_THROUGH_CONFIRMED=true`, `DEMO_MODE=true`, and the two HMAC keys.
3. Start both processes and open a poll.

`DEMO_MODE` refuses to work with an `sk_live_` key and is visibly labelled across the site. It bypasses roster and schedule gates so the rehearsal is possible before launch. Live mode still requires every launch gate in `Config::launch_gates_complete`.

### Admin access

Set `ADMIN_USERNAME`, `ADMIN_SESSION_HMAC_KEY`, and `ADMIN_PASSWORD_HASH`. Generate the hash with the server itself so the plaintext never enters shell history or the process list:

```sh
printf 'your-strong-password' | cargo run --manifest-path server/Cargo.toml -- --hash-password
```

The command rejects passwords shorter than 16 characters and prints the Argon2 PHC string to paste into `ADMIN_PASSWORD_HASH`. Restarting the API reseeds the admin row from that hash. Sign-in is rate limited to five attempts per ten minutes per client address.

## Launch gates

Paid polling is intentionally disabled by default. The API also requires each gate below to be set true, published non-draft terms, and a server secret beginning `sk_live_` before any race can open or accept checkout: legal review, Paystack live approval, official candidate roster verification, payment/refund rehearsals, permanent domain, backup and restore verification, refund reserve, and admin controls. The gate switches are operational attestations, not substitutes for completing the checklist. If any gate misses the freeze date, move the entire poll window and publish the new dates before payments begin. Consult `docs/LAUNCH-CHECKLIST.md`.

