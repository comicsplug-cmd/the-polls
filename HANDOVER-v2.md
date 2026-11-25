# The Polls — Handover (v2)

Updated 25 Nov 2026. Replaces `HANDOVER.md`. Everything needed to pick this project up cold. Planning is complete; no code has been written yet.

**Companion:** `BUILD-PLAN-v2.md`, the technical build plan.

> **Implementation status (25 Nov 2026):** the workspace is no longer empty. The Axum/SQLite API, Next.js site, admin console, migrations, tests, and deploy examples exist. The 20 Rust tests cover settlement, duplicate cards, underpayment, transfer expiry, webhook signatures, receipts, admin sessions, retention, demo gating, and the ballot-code lookup. `DEMO_MODE` (test keys only) makes the whole payment → ballot → receipt loop rehearsable before launch; live mode still requires every gate in `Config::launch_gates_complete`. Section A's "zero lines of code" line is kept as the planning snapshot, not current status.

---

## A. Project snapshot

| | |
| --- | --- |
| **Product** | "The Polls": unofficial Nigerian mock-poll site |
| **Ballot price** | ₦100 (10,000 kobo) via Paystack. Revenue to the business; no prizes, no payouts. |
| **Poll window** | One window for all races: **26 Dec 2026 to 4 Jan 2027** (Africa/Lagos, recommended close 23:59 WAT) |
| **Real elections** | Presidential and National Assembly: 16 Jan 2027. Governorship and State Assembly: 6 Feb 2027 (INEC revised timetable). |
| **Scope** | Phase 1: presidential. Phase 2: governorship races from verified INEC lists, only if ready by 18 Dec. |
| **Status** | Planning complete. Zero lines of code written. Workspace empty. |
| **Stack** | Rust (Axum, SQLx, Askama) + Next.js 16 / React 19 / Tailwind 4 |
| **Environment** | Windows, Git Bash. `rustc 1.99.1`, Node 22.22, npm 10.9.7 |
| **Author Rust level** | Beginner; code deliberately kept plain (see Plan §1) |

### What is already settled

- Name is "The Polls". "Not-INEC.com" was dropped because the risks were too high. The "NOT INEC" stamp stays as a disclaimer.
- Rust backend + Next.js UI. Admin (Phase 2) is server-rendered Rust templates.
- Standings are live (chosen over hiding results until close). A `tally_mode` setting exists in case that changes.
- State is a dropdown, not a map. Countdown is server-authoritative.
- No names, emails, phones or NIN are collected. Hashed card and device identifiers are held and then purged (Plan §11).
- T&C consent is recorded server-side. Results are watermarked.
- Public receipts are candidate-blind.
- Fee only: no prizes, odds or payouts, ever (Plan §14).

---

## B. Decision log

Every significant fork and why. If a decision is ever revisited, start here. Rows marked **(v2)** are new or changed in this version.

| # | Decision | Rationale |
| --- | --- | --- |
| 1 | **Rust for backend only**, Next.js for UI | Keeps money logic compiler-checked without paying the WASM/SSR tax of a full-Rust frontend. Leptos is still at `0.9.0-rc1`, too early to bet a payments product on. |
| 2 | **Not full-Rust Leptos** | Slow WASM rebuilds, RC-status framework, and rich chart interactions take materially longer. |
| 3 | **Not Askama/HTMX for the public site** | Smallest payload, but chart drill-downs and drawers are noticeably less fluid than React. Chosen *only* for admin, where forms dominate. |
| 4 | **Yellow is a marker, not a fill** | Black+yellow+"pay to vote" collides with Nigerian betting-app visual language. Paper-dominant + highlighter-yellow reads civic instead. |
| 5 | **Card signature as the identity primitive** | `authorization.signature` is Paystack's per-card fingerprint, harder to fake than a browser fingerprint, and it is what the payment is attached to. |
| 6 | **`channels: ["card"]` only** | Bank transfer and USSD transactions carry no card signature. Allowing them would silently break the one-vote-per-card lock. **(v2)** Known trade-off: it likely reduces turnout. See open question 2. |
| 7 | **Duplicates are refunded, never confiscated** | Taking ₦100 and discarding the vote is indefensible. Refund API's 409-on-reuse gives free idempotency. |
| 8 | **No names, emails, phones or NIN; synthetic email** | Paystack mandates an email; we generate `ballot+<ref>@<our-domain>` rather than collect a real one. **(v2)** This is not "zero PII": hashed card and device values linked to a candidate are still sensitive. See #19. |
| 9 | **State is a dropdown, not a map** | Maps are heavy, often inaccurate at state level, and add real bundle cost on mobile data. A native `<select>` is faster and more accessible. The dropdown records where the *voter* lives, so the FCT stays in it. |
| 10 | **Consent is recorded, not just shown** | `terms_version` + `terms_accepted_at` on the vote row is the defensible artifact if a charge is disputed. |
| 11 | **Watermark = blurred tile + sharp stamp** | The tile signals intent; the sharp corner stamp survives a screenshot crop, which is how misuse actually spreads. **(v2)** The tile now covers the whole background so any crop includes it. |
| 12 | **Countdown is server-authoritative** | Client timers drift and phone clocks lie. `settle()` re-checks the window; late payments are refunded. |
| 13 | **Election status derived, never stored** | Stored status drifts from timestamps. One pure function removes the entire class of bug. |
| 14 | **`card_locks` as a separate table** | The lock must be race-free (`INSERT ... ON CONFLICT DO NOTHING`). **(v2)** It is deleted with the other identifiers after the purge date. |
| 15 | **Standings are live** (owner's choice) | More shareable and transparent. Accepted trade-off: early numbers bias later voters, and a live count lets a campaign watch the margin. `tally_mode` allows `after_close` if this is revisited. |
| 16 **(v2)** | **Rosters come from INEC's published lists** | INEC released its final presidential list on 12 Sep 2026 (reported as 18 candidates; some outlets say 19). Loading from INEC with a stored `source_url` replaces the old "provisional/expected" labels. |
| 17 **(v2)** | **One window, 26 Dec to 4 Jan, for all races** | Closes 12 days before the presidential election and 33 days before the governorship election, so the poll does not run in the final stretch. It still runs during the campaign period, so the "not official, not representative" framing matters. |
| 18 **(v2)** | **"The Polls" only; Not-INEC.com dropped** | The risk was judged too high. The stamp text "UNOFFICIAL · NOT INEC" keeps the disclaimer function. |
| 19 **(v2)** | **Purge identifiers after close + 60 days** | Card hash, device hash, transaction id, and consent timestamp are deleted once the refund window has passed. Hashes are election-scoped so they cannot be linked across polls. Lawyer to confirm the period. |
| 20 **(v2)** | **Candidate-blind public receipts** | A durable URL naming the candidate is proof of how someone voted, which is what a vote-buyer wants. Receipts show counted/refunded/not completed only. |
| 21 **(v2)** | **Fee only: no prizes, odds, payouts, or resolution against real results** | Keeps this a poll, not a prediction market or betting product (Plan §14). |
| 22 **(v2)** | **Positioning: paid mock poll for engagement; not representative** | The T&C and `/method` say results show how paying participants voted, not what Nigerians think. Avoids claims the design cannot back (ballot stuffing is possible). |
| 23 **(v2)** | **Revenue model: the ₦100 is business revenue** | Resolves the old question about prize pool or donation. T&C says "not a donation". |
| 24 **(v2)** | **Closed polls stay up as a read-only, labelled archive** | A permanent "Mock poll, closed. Not an official result." banner is the best defence if the numbers differ from the real outcome. |
| 25 **(v2)** | **No NIN or voter verification** | Owner's call: the site asks for no identity. This is consistent with a mock poll, but it means the site must not claim accuracy or representativeness. |
| 26 **(v2)** | **Phase 1 is presidential only; no admin UI or dynamic OG images in Phase 1** | With about a month, payments and the state machine matter most. Seed files and SQL cover admin needs; a static stamped OG image covers sharing. |

### Explicitly rejected

- **Rust purely for speed.** There is no measurable speed win at this workload; the real justification is compiler-checked money logic and a tiny deployable.
- **Full-Rust frontend.** Too much framework risk and interaction friction for a first release.
- **Maps.** Requested initially, then dropped by the owner in favour of a dropdown.
- **Not-INEC.com.** Dropped; see #18.
- **Prize pools, payouts, odds, or "accuracy" scorecards.** See #21 and Plan §14.
- **NIN or other identity collection.** See #25.

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
cargo add resvg        # Phase 2 (dynamic OG images)
```

**Windows note:** `resvg` and `sqlx` compile cleanly, but the first `cargo build` takes several minutes. Expect it; it is not a hang. Add `resvg` only when you reach Phase 2.

---

## D. Environment variables

`server/.env` is gitignored and never committed.

| Var | Example | Notes |
| --- | --- | --- |
| `PAYSTACK_SECRET_KEY` | `sk_test_…` | **Regenerate this**: the earlier one was pasted in plaintext into a chat. |
| `PAYSTACK_PUBLIC_KEY` | `pk_test_…` | Only needed if checkout ever moves to inline JS. |
| `APP_URL` | `http://localhost:3000` | Used to build `callback_url`. Must be reachable by the browser. |
| `DATABASE_URL` | `sqlite:polls.db` | SQLx connection string. |
| `DEVICE_HMAC_SECRET` | random 32 bytes | Salts device + card hashes (mixed with the election id). Changing it invalidates all existing locks. |
| `PURGE_AFTER_DAYS` | `60` | Days after `closes_at` before identifiers are purged. Confirm with your lawyer. |
| `TALLY_MODE` | `live` | Default for new elections: `live` or `after_close`. |
| `ADMIN_PASSWORD_HASH` | `$argon2id$…` | Phase 2. Generate via `cargo run -- --hash-password`. |
| `SESSION_SECRET` | random 32 bytes | Phase 2. Signs the admin cookie. |
| `DEMO_MODE` | `1` | Skips the card lock. Boot assertion refuses `production` or non-`sk_test_` keys. |

---

## E. Commands

```bash
./dev.sh                  # both processes, Ctrl-C kills both
cd server && cargo run    # API only, :8080
cd web    && npm run dev  # UI only,  :3000
cargo test                # state machine transition table, purge job, hash scoping
cargo run --bin seed      # idempotent upsert from data/seed/*.json
cargo run -- --hash-password   # Phase 2
cargo clippy -- -D warnings
```

---

## F. Gotchas

These will each cost an hour if rediscovered the hard way.

1. **Test-card collision.** `4084 0840 8408 4081` is reusable, so every local run produces an identical `signature`. Without `DEMO_MODE`, the second test vote is rejected and the app looks broken.
2. **SQLite + WAL.** Set `journal_mode=WAL` and `busy_timeout=5000` at pool init, or concurrent webhook + callback writes throw `database is locked`.
3. **Next rewrites in dev.** `/api/*` must proxy to `:8080`. Without it, the frontend 404s and it looks like a routing bug in Axum.
4. **`metadata` is a stringified JSON object**, not a raw object. Sending an object silently fails.
5. **Amounts are kobo.** ₦100 is `10000`. The NGN minimum on Paystack is ₦50 (5,000 kobo); lower amounts are rejected. A factor-of-100 error is the most likely production incident.
6. **`domain` must match the key.** A `test` transaction verified with an `sk_live_` key returns an error; check `data.domain` against the key prefix.
7. **Refund 409 is success, not failure.** It means the refund already exists. Fetch it with `GET /refund?reference=`.
8. **Live refunds draw from your Paystack balance.** If the balance is short, refunds sit in `refund_failed` and need manual action (Paystack dashboard in Phase 1).
9. **`authorization.signature` can be null** on some cards. Set `lock_kind='none'`, count the vote, and lean on device + IP rate limiting.
10. **`.local` emails are rejected** by Paystack. Use a real domain for the synthetic address, and either set up a catch-all or switch off customer receipt emails so nothing bounces.
11. **Next dev caches API responses.** Disable `fetch` caching on tally calls or live numbers go stale.
12. **Foreign-card fees.** Paystack lists international cards at 3.9% + ₦100; the ₦100 waiver is only stated for local payments. A foreign-card ₦100 ballot can cost more in fees than it earns.
13. **Hash scoping.** Always mix `election_id` into the HMAC input. Forgetting makes cards and devices linkable across polls.

---

## G. Risk register

| Risk | Severity | Mitigation |
| --- | --- | --- |
| **Approvals slip** (CAC, Paystack live approval, lawyer review) | High | Start all three this week; build in test mode; Paystack approval is outside your control, so keep a launch-day fallback (delay the opening, not the closing). |
| **Screenshots passed off as real INEC results** | High | Whole-background watermark, sharp corner stamp, static then dynamic OG stamp, `/method`, T&C, permanent archive banner. Cannot be fully prevented; text-only spread is untouched by any watermark. |
| **Ballot stuffing / bought ballots** (virtual cards make ₦100 ballots cheap) | High (reputational) | Card lock, soft device check, honest "not representative" T&C and `/method`, watch the Paystack dashboard for spikes, never claim accuracy. |
| **Drift into betting** (prizes, odds, payouts, "who called it right") | Medium | Plan §14 rules, banned-words CI check, written legal confirmation, fresh legal opinion before any change. |
| **Personal data: hashed card/device ids tied to political choice** | Medium | Privacy notice, purge job at close + 60 days, election-scoped hashes, minimal dashboard access, NDPC registration check. |
| **Regulatory attention (paid polling, politically themed)** | Medium | Unambiguous "not official, no legal effect, not representative" framing; lawyer review of Electoral Act and Cybercrimes Act exposure. |
| **Paystack flags or restricts a politically themed account** | Medium | Disclose the use case up front; keep refunds working; have a second processor option researched. |
| **Chargeback / "I didn't agree to pay"** | Medium | Recorded `terms_version` + `terms_accepted_at`; T&C shown at display size before redirect (note the consent timestamp is purged with the identifiers, so confirm the period against chargeback windows). |
| **Paid but vote rejected** | Medium | Auto-refund on every rejection path; visible refund reference in the UI. |
| **Candidate data wrong or disputed** | Low to Medium | Load from INEC's list, store `source_url` and `source_checked_at`, show "as listed by INEC on <date>", equal-treatment template. |
| **Foreign-card fees exceed the ballot price** | Low to Medium | Decide per open question 3. |
| **Test key leaked in chat** | Low (test) / High (if live) | Regenerate now. Never paste `sk_live_` into a chat. |

---

## H. Open questions

Not blocking Phase 1 code, but decide before launch:

1. **Closing time on 4 Jan.** The plan assumes 23:59 WAT. Confirm.
2. **Card-only vs transfers.** Card-only keeps the one-card-one-vote lock but likely cuts turnout. Is that acceptable, or do you want a different identity approach (which needs its own design)?
3. **Foreign cards and "Outside Nigeria".** Accept at a possible loss, refund non-NG cards (if Paystack's verify response exposes card country; check), or drop the option for launch?
4. **Domain.** `thepolls.ng` was a placeholder. Choose a registrar: a NiRA-accredited local registrar (naira, transfer or USSD) for `.ng`/`.com.ng`; Cloudflare Registrar (at-cost `.com`, requires Cloudflare DNS, needs a dollar card) or Namecheap (cheap first year, higher renewal) for `.com`.
5. **Governorship in or out?** Decide on 18 Dec based on whether you can source every race from INEC's lists.
6. **Retention period.** 60 days is a default; your lawyer may want longer or shorter.
7. **Lawyer's answers** to the Plan §15 checklist, especially the gaming-law confirmation and the NDPC registration question.
8. **Tax treatment** of ballot revenue (ask an accountant).

---

## I. Glossary

For a first Rust project, these are the terms that actually matter.

| Term | Meaning here |
| --- | --- |
| **`enum` + `match`** | Rust's version of a tagged union. `VoteState` is an enum; `match` forces you to handle every variant. This is the compile-time safety the whole design leans on. |
| **`Result<T, E>`** | A return value that is either success (`Ok`) or failure (`Err`). There are no exceptions; errors are values you must handle. |
| **`?` operator** | "If this failed, return the error now." Replaces most try/catch. |
| **`anyhow`** | Makes error types easy to pass around in handlers. Use in `routes/`. |
| **`thiserror`** | Generates proper error types. Use in `payments/` where the caller must distinguish cases. |
| **`Arc<AppState>`** | Shared, thread-safe app state (DB pool, config). Cloned cheaply into every handler. |
| **`async` / `await`** | Non-blocking I/O. Every handler that touches the DB or Paystack is `async`. |
| **`sqlx::FromRow`** | Auto-maps a SQL row into a Rust struct. |
| **`tower` / middleware** | Layers wrapped around handlers: logging, auth, rate limiting. |
| **Askama** | Compile-time HTML templates. `{% if %}` / `{% for %}` inside `.html` files, type-checked at build. |
| **HTMX** | Attributes like `hx-post` that swap HTML fragments without writing JS. |
| **Server Component (RSC)** | A React component that renders on the server and ships zero JS. The default in Next 16. |
| **Client island** | A component marked `"use client"` that does ship JS. Only five of these exist. |
| **HMAC** | A keyed hash. Used so stored card/device hashes can't be reversed or matched without the server secret. |
| **Election-scoped hash** | An HMAC whose input includes the election id, so the same card gives different hashes in different polls. |
| **Purge job** | The scheduled task that nulls identifiers and deletes `card_locks` after `PURGE_AFTER_DAYS`. |
| **Candidate-blind receipt** | A public receipt that confirms a ballot was counted without saying who it was for. |
| **NDPC / NDPA** | Nigeria Data Protection Commission / Nigeria Data Protection Act 2023. |
| **NLRC** | National Lottery Regulatory Commission, the federal gaming regulator (state gaming boards also license). |

---

## J. Next actions

1. **Regenerate the Paystack test secret key** in the dashboard. Two minutes, removes a live credential from a chat log.
2. **This week (by 1 Dec):** start CAC registration, apply for Paystack live access (disclose the use case), book your lawyer, and pick and buy the domain.
3. **Start build step 1:** Rust skeleton, `/api/health`; confirm it boots before anything else is written.
4. **Do not begin payments** until the state-machine transition tests pass.
5. **By 18 Dec:** governorship go/no-go, based on whether every race can be sourced from INEC's lists.
6. **23 Dec:** feature freeze. **26 Dec:** poll opens. **4 Jan:** poll closes. **5 Mar:** purge job runs.

---

*Companion document: `BUILD-PLAN-v2.md`*
