# The Polls — Handover

Everything needed to pick this project up cold. Planning is complete; no code has been written yet.

**Companion:** `BUILD-PLAN.md` — the technical build plan.

---

## A. Project snapshot

| | |
| --- | --- |
| **Product** | "The Polls" — unofficial Nigerian mock-poll site |
| **Ballot price** | ₦100 (10,000 kobo) via Paystack |
| **Scope** | Presidential 2027 + 37 governorship races |
| **Status** | Planning complete. Zero lines of code written. Workspace empty. |
| **Stack** | Rust (Axum, SQLx, Askama) + Next.js 16 / React 19 / Tailwind 4 |
| **Environment** | Windows, Git Bash. `rustc 1.99.1`, Node 22.22, npm 10.9.7 |
| **Author Rust level** | Beginner — code deliberately kept plain (see Plan §1) |

### What is already settled

- Name is "The Polls" ("Not-INEC" was abandoned as problematic).
- Rust backend + Next.js UI. Admin is server-rendered Rust templates.
- Standings are always live (chosen over hiding results until vote).
- State is a dropdown, not a map. Countdown is server-authoritative.
- Zero PII. T&C consent is recorded server-side. Results are watermarked.

---

## B. Decision log

Every significant fork and why. If a decision is ever revisited, start here.

| # | Decision | Rationale |
| --- | --- | --- |
| 1 | **Rust for backend only**, Next.js for UI | Keeps money logic compiler-checked without paying the WASM/SSR tax of a full-Rust frontend. Leptos is still at `0.9.0-rc1` — too early to bet a payments product on. |
| 2 | **Not full-Rust Leptos** | Slow WASM rebuilds, RC-status framework, and rich chart interactions take materially longer. |
| 3 | **Not Askama/HTMX for the public site** | Smallest payload, but chart drill-downs and drawers are noticeably less fluid than React. Chosen *only* for admin, where forms dominate. |
| 4 | **Yellow is a marker, not a fill** | Black+yellow+"pay to vote" collides with Nigerian betting-app visual language. Paper-dominant + highlighter-yellow reads civic instead. |
| 5 | **Card signature as the identity primitive** | `authorization.signature` is Paystack's per-card fingerprint — harder to fake than a browser fingerprint, and it is what the fee is actually attached to. |
| 6 | **`channels: ["card"]` only** | Bank transfer and USSD transactions carry no card signature. Allowing them would silently break the one-vote-per-card lock. |
| 7 | **Duplicates are refunded, never confiscated** | Taking ₦100 and discarding the vote is indefensible. Refund API's 409-on-reuse gives free idempotency. |
| 8 | **Zero PII: synthetic email** | Paystack mandates an email; we generate `ballot+<ref>@thepolls.ng` rather than collect a real one. Removes the only genuine PII in the system. |
| 9 | **State is a dropdown, not a map** | Maps are heavy, often inaccurate at state level, and add real bundle cost on mobile data. A native `<select>` is faster and more accessible. |
| 10 | **Consent is recorded, not just shown** | `terms_version` + `terms_accepted_at` on the vote row is the defensible artifact if a charge is disputed. |
| 11 | **Watermark = blurred tile + sharp stamp** | The tile signals intent; the sharp corner stamp survives a screenshot crop, which is how misuse actually spreads. |
| 12 | **Countdown is server-authoritative** | Client timers drift and phone clocks lie. `settle()` re-checks the window; late payments are refunded. |
| 13 | **Election status derived, never stored** | Stored status drifts from timestamps. One pure function removes the entire class of bug. |
| 14 | **`card_locks` as a separate table** | The lock must outlive vote archival, and `INSERT ... ON CONFLICT DO NOTHING` is the race-free primitive. |
| 15 | **Standings always live** (user choice) | More shareable and transparent. Accepted trade-off: early numbers bias later voters. |
| 16 | **Governorship rosters marked provisional** | 2027 nominations are not final. Claiming otherwise would be publishing falsehoods about real people. |

### Explicitly rejected

- **Rust purely for speed.** There is no measurable speed win at this workload; the real justification is compiler-checked money logic and a tiny deployable.
- **Full-Rust frontend.** Too much framework risk and interaction friction for a first release.
- **Maps.** Requested initially, then dropped by the user in favour of a dropdown.

---

## C. Environment setup

```bash
# 1. Rust (already present: 1.99.1)
rustup component add clippy rustfmt

# 2. Scaffold
cargo new server --bin
npx create-next-app@latest web --ts --tailwind --app --eslint

# 3. Server deps
cd server
cargo add axum tokio --features tokio/full
cargo add sqlx --features runtime-tokio,sqlite,migrate,chrono
cargo add reqwest --features json,rustls-tls
cargo add serde --features derive
cargo add askama askama_axum tower-http --features tower-http/trace
cargo add anyhow thiserror tracing tracing-subscriber
cargo add hmac sha2 subtle hex rand base32
cargo add argon2 axum-extra --features axum-extra/cookie
cargo add resvg
```

**Windows note:** `resvg` and `sqlx` compile cleanly, but the first `cargo build` takes several minutes. Expect it; it is not a hang.

---

## D. Environment variables

`server/.env` — gitignored, never committed.

| Var | Example | Notes |
| --- | --- | --- |
| `PAYSTACK_SECRET_KEY` | `sk_test_…` | **Regenerate this** — the current one was pasted in plaintext into a chat. |
| `PAYSTACK_PUBLIC_KEY` | `pk_test_…` | Only needed if checkout ever moves to inline JS. |
| `APP_URL` | `http://localhost:3000` | Used to build `callback_url`. Must be reachable by the browser. |
| `DATABASE_URL` | `sqlite:polls.db` | SQLx connection string. |
| `DEVICE_HMAC_SECRET` | random 32 bytes | Salts device + card hashes. Changing it invalidates all existing locks. |
| `ADMIN_PASSWORD_HASH` | `$argon2id$…` | Generate via `cargo run -- --hash-password`. |
| `SESSION_SECRET` | random 32 bytes | Signs the admin cookie. |
| `DEMO_MODE` | `1` | Skips the card lock. Boot assertion refuses `production` or non-`sk_test_` keys. |

---

## E. Commands

```bash
./dev.sh                  # both processes, Ctrl-C kills both
cd server && cargo run    # API only, :8080
cd web    && npm run dev  # UI only,  :3000
cargo test                # state machine transition table
cargo run --bin seed      # idempotent upsert from data/seed/*.json
cargo run -- --hash-password
cargo clippy -- -D warnings
```

---

## F. Gotchas

These will each cost an hour if rediscovered the hard way.

1. **Test-card collision.** `4084 0840 8408 4081` is reusable, so every local run produces an identical `signature`. Without `DEMO_MODE`, the second test vote is rejected and the app looks broken.
2. **SQLite + WAL.** Set `journal_mode=WAL` and `busy_timeout=5000` at pool init, or concurrent webhook + callback writes throw `database is locked`.
3. **Next rewrites in dev.** `/api/*` must proxy to `:8080`. Without it, the frontend 404s and it looks like a routing bug in Axum.
4. **`metadata` is a stringified JSON object**, not a raw object. Sending an object silently fails.
5. **Amounts are kobo.** ₦100 is `10000`. A factor-of-100 error here is the most likely production incident.
6. **`domain` must match the key.** A `test` transaction verified with an `sk_live_` key returns an error — check `data.domain` against the key prefix.
7. **Refund 409 is success, not failure.** It means the refund already exists. Fetch it with `GET /refund?reference=`.
8. **Live refunds draw from your Paystack balance.** If the balance is short, refunds sit in `refund_failed` and need the admin queue.
9. **`authorization.signature` can be null** on some cards. Set `lock_kind='none'`, count the vote, and lean on device + IP rate limiting.
10. **`.local` emails are rejected** by Paystack. Use a real domain for the synthetic address.
11. **Next dev caches API responses.** Disable `fetch` caching on tally calls or live numbers go stale.

---

## G. Risk register

| Risk | Severity | Mitigation |
| --- | --- | --- |
| **Screenshots passed off as real INEC results** | High | Sharp corner stamp, OG watermark, `/method` page, T&C. Cannot be fully prevented. |
| **Chargeback / "I didn't agree to pay"** | Medium | Recorded `terms_version` + `terms_accepted_at`; T&C shown at display size before redirect. |
| **Paid but vote rejected** | Medium | Auto-refund on every rejection path; visible refund reference in the UI. |
| **Test key leaked in chat** | Low (test) / High (if live) | Regenerate now. Never paste `sk_live_` into a chat. |
| **Governorship candidate data wrong** | Medium | Provisional banner, `standing` field, single editable JSON. |
| **Regulatory attention (paid polling)** | Medium | Unambiguous "not official, no legal effect" framing; no claims of representativeness. |
| **Ballot stuffing via many cards** | Low | Genuinely hard to stop; the ₦100 fee is the primary deterrent, the card lock the secondary. |

---

## H. Open questions

Not blocking, but decide before launch:

1. **What happens to the ₦100?** Revenue, prize pool, or donated? Affects the T&C wording and possibly tax.
2. **Who can vote?** Anyone with a card, including non-Nigerians. Is diaspora voting a feature or noise in the data?
3. **Does the presidential poll run the same week as governorship?** Or staggered?
4. **What is the publish cadence?** One "poll week" per month, or continuous?
5. **Does a closed poll archive publicly** (a results history) or disappear?

---

## I. Glossary

For a first Rust project, these are the terms that actually matter.

| Term | Meaning here |
| --- | --- |
| **`enum` + `match`** | Rust's version of a tagged union. `VoteState` is an enum; `match` forces you to handle every variant. This is the compile-time safety the whole design leans on. |
| **`Result<T, E>`** | A return value that is either success (`Ok`) or failure (`Err`). There are no exceptions — errors are values you must handle. |
| **`?` operator** | "If this failed, return the error now." Replaces most try/catch. |
| **`anyhow`** | Makes error types easy to pass around in handlers. Use in `routes/`. |
| **`thiserror`** | Generates proper error types. Use in `payments/` where the caller must distinguish cases. |
| **`Arc<AppState>`** | Shared, thread-safe app state (DB pool, config). Cloned cheaply into every handler. |
| **`async` / `await`** | Non-blocking I/O. Every handler that touches the DB or Paystack is `async`. |
| **`sqlx::FromRow`** | Auto-maps a SQL row into a Rust struct. |
| **`tower` / middleware** | Layers wrapped around handlers — logging, auth, rate limiting. |
| **Askama** | Compile-time HTML templates. `{% if %}` / `{% for %}` inside `.html` files, type-checked at build. |
| **HTMX** | Attributes like `hx-post` that swap HTML fragments without writing JS. |
| **Server Component (RSC)** | A React component that renders on the server and ships zero JS. The default in Next 16. |
| **Client island** | A component marked `"use client"` that does ship JS. Only five of these exist. |

---

## J. Next actions

1. **Regenerate the Paystack test secret key** in the dashboard. Two minutes, removes a live credential from a chat log.
2. **Start build step 1** — Rust skeleton, `/api/health`, confirm it boots before anything else is written.
3. **Do not begin payments** until the state-machine transition tests pass.
