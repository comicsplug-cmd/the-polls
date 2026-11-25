# The Polls — Build Plan

An unofficial Nigerian mock-poll site. ₦100 per ballot, paid through Paystack. Every vote gets a unique, publicly verifiable receipt ID. Rust backend, Next.js frontend.

**Scope:** 1 presidential race + 37 governorship races (36 states + FCT). Standings live. Each poll runs a short, admin-set window with a visible countdown.

**Companion:** `HANDOVER.md` — decision log, setup, gotchas, risks, glossary.

---

## 1. Honest note on the Rust decision

Rust will not make this faster. The bottleneck is Paystack's ~300ms API round-trip and SQLite writes; a 100k-vote week averages 0.16 writes/second, which Node handles without noticing.

The real reasons Rust is right here:

- **The payment state machine becomes compiler-checked.** `counted` and `refunded` are distinct states, and exhaustive `match` refuses to compile if a transition is unhandled. That is the bug class that costs real money.
- **Single static binary**, ~10MB RAM, no `node_modules`, no cold starts.
- **Cheap hosting** — a $5 VPS is genuinely enough.

Because the author is new to Rust, the code is kept deliberately plain:

- One crate, flat modules. No workspaces, no generics beyond `Option`/`Result`, no `#[async_trait]`, no custom extractors beyond two.
- `sqlx` with **runtime-checked** queries and `FromRow` structs — not the `query!` macro, which would force a live DB at compile time.
- `anyhow` in handlers; `thiserror` only for payment errors.
- Admin is **Askama templates + HTMX**, not React. CRUD forms are the one place a template engine genuinely beats a SPA, and it keeps the project in one language.
- The state machine lives in one file (`payments/state.rs`), heavily commented. That is the only file needing careful reading.

---

## 2. Design: "Civic Record"

"Official but unmistakably not INEC, and not AI slop." Black + yellow is the right instinct, but there is a trap: **black + yellow + "pay ₦100 to vote" reads as a betting app** (SportyBet, Bet9ja territory). So yellow becomes a *marker*, not a brand fill.

**Thesis:** The Polls looks like a printed electoral record — a tally sheet, a ballot stub, a gazette. Ink on paper. It reads institutional because of its *typography and rules*, not because it borrows government colours.

**Guardrails against looking like betting:**

- Paper-toned background, never black. Black is ink (type, hairlines), not surface.
- Yellow is a highlighter — a swash behind a word, tally strokes, a 2px rule. Never a big flat CTA block.
- Serif display type. Betting apps are uniformly heavy sans.
- A physical rubber-stamp motif instead of gradients, glows, or glassmorphism.

### Tokens (OKLCH, warm-tinted neutrals)

| Token | Value | Use |
| --- | --- | --- |
| `--paper` | `oklch(96.5% 0.012 88)` | Base surface (warm newsprint) |
| `--paper-2` | `oklch(93% 0.015 88)` | Recessed bands, table stripes |
| `--ink` | `oklch(18% 0.012 80)` | Primary text (never `#000`) |
| `--ink-2` | `oklch(38% 0.010 80)` | Secondary text |
| `--rule` | `oklch(84% 0.012 85)` | Hairlines, dotted leaders |
| `--marker` | `oklch(84% 0.165 92)` | The yellow — highlighter swashes, tally strokes |
| `--stamp` | `oklch(52% 0.17 27)` | Oxidised red — UNOFFICIAL stamp, live dot, closing-soon |

`--ink` on `--paper` ≈ 15:1. Yellow is only ever a *background behind ink text* (≈11:1) or a pure shape. **Never yellow text on paper** — it fails WCAG and looks cheap. Party colours are muted print-ink tones so the tally reads like a printed legend.

### Type

- **Display:** Instrument Serif (italic for emphasis) — editorial, memorable, not Inter.
- **UI/body:** Archivo — a grotesque drawn from civic lettering; institutional without being government.
- **Data:** IBM Plex Mono for references, countdowns, tallies — justified, because these are literal document numbers.

Fluid `clamp()` scale, tight display tracking, `tabular-nums` on every changing number.

### Layout & signature interaction

- A 12-col "sheet" with a visible left gutter carrying mono metadata (file no., cycle, status) like a printed form.
- **Dotted leader lines** between candidate name and vote count — classic ballot/index typography. Distinctive *and* functional.
- **The tally bar** replaces a generic chart: each row is `name ····· count`, with a segmented bar underneath where the marker swipes in. Clicking a segment drills into state-by-state breakdown.
- No cards-in-cards. Sections separated by hairlines and whitespace.

### Motion

`ease-out-quart`, 180–420ms. Numbers roll on tabular figures; the stamp presses on receipt. Full `prefers-reduced-motion` fallback.

### Mobile-first is non-negotiable

Android Chrome on mid-range phones over mobile data. No 3D, no shaders, no hero video. Server Components by default — **only five client islands ship JS**: `TallyBar`, `Countdown`, `StateSelect`, `RecordDrawer`, `TermsGate`. Everything else is zero-JS HTML. Sticky bottom vote bar, ≥44px targets, verified at 360×640.

---

## 3. Architecture

```
the-polls/
  server/                       # Rust — single crate
    Cargo.toml
    migrations/0001_init.sql
    src/
      main.rs                   # axum router + state
      config.rs                 # env parsing
      db.rs                     # pool, migrations, WAL
      models.rs                 # rows + enums
      terms.rs                  # SINGLE source of the T&C text + version
      watermark.rs              # SVG generator, shared by page + OG image
      payments/
        state.rs                # VoteState enum + transitions  ← the one careful file
        paystack.rs             # reqwest client
        settle.rs               # settle(), refunds, reconciliation
      routes/
        public.rs               # elections, tally, candidates, terms
        vote.rs                 # init
        webhook.rs              # paystack webhook
        og.rs                   # OG image via resvg
      admin/
        mod.rs auth.rs views.rs
        templates/*.html        # askama
    src/bin/seed.rs             # reads data/seed/*.json
  web/                          # Next.js 16 + React 19 + Tailwind 4
    app/  components/  lib/
  data/seed/*.json
  dev.sh
```

**Dev:** Axum on `:8080`, Next on `:3000` with `rewrites` proxying `/api/*` → `127.0.0.1:8080`. `dev.sh` runs both with a `trap` to kill both on Ctrl-C.

**Prod:** Caddy in front — `/api/*`, `/admin/*`, `/og/*` → Axum; everything else → Next standalone. Two processes, one box.

**Why not let Axum serve the frontend too:** the receipt page needs SSR for OG tags, and Next SSR needs Node. Accepting two processes buys far faster UI iteration than hand-rolling SSR in Rust.

---

## 4. Data model

```sql
elections(
  id TEXT PK, slug TEXT UNIQUE, title TEXT, kind TEXT, cycle_year INTEGER,
  fee_kobo INTEGER, opens_at INTEGER, closes_at INTEGER,
  force_closed INTEGER DEFAULT 0, created_at INTEGER)

candidates(
  id TEXT PK, election_id TEXT, slug TEXT, name TEXT, party TEXT, party_code TEXT,
  photo_url TEXT, wiki_title TEXT, one_liner TEXT,
  standing TEXT,              -- declared | expected | incumbent
  sort_order INTEGER)

votes(
  id INTEGER PK, reference TEXT UNIQUE, election_id TEXT, candidate_id TEXT,
  amount_kobo INTEGER, status TEXT, reason TEXT,
  paystack_txn_id INTEGER, channel TEXT, domain TEXT,
  card_sig_hash TEXT, lock_kind TEXT,          -- card | none
  device_hash TEXT, state_code TEXT, country_code TEXT,
  terms_version TEXT, terms_accepted_at INTEGER,   -- consent record
  refund_ref TEXT UNIQUE, verify_attempts INTEGER DEFAULT 0,
  refund_attempts INTEGER DEFAULT 0, claimed_at INTEGER, expires_at INTEGER,
  created_at INTEGER, counted_at INTEGER, refunded_at INTEGER)

card_locks(election_id TEXT, card_sig_hash TEXT, vote_reference TEXT,
           created_at INTEGER, PRIMARY KEY(election_id, card_sig_hash))

payment_events(id INTEGER PK, reference TEXT, source TEXT, event TEXT,
               paystack_event_id TEXT, created_at INTEGER,
               UNIQUE(event, paystack_event_id))
```

`card_locks` is the enforcement point, not `votes`. Tally is always `COUNT(*) WHERE status='counted'` — never a denormalised counter.

**Election status is derived, never stored:** `opens_at > now` → `upcoming`; `now > closes_at` or `force_closed` → `closed`; else `open`. One function, one source of truth, no drift.

---

## 5. Vote state machine

```
pending ──claim──▶ verifying ──▶ counted               (terminal, in tally)
                      ├────────▶ refund_pending ─▶ refunding ─▶ refunded   (terminal)
                      │                                   └────▶ refund_failed (needs a human)
                      └────────▶ pending (120s claim-timeout reclaim)
pending ──expiry sweep──▶ abandoned                     (terminal)
```

`payments/state.rs` holds a `VoteState` enum plus one `fn transition(from, event) -> Result<To, Refused>`. Illegal transitions return `Refused`, not a panic. Unit-tested with a table of every from/event pair — this is the file to write tests for first.

**Reference = the receipt.** `TP-2027-PRES-8F42K9` — 8 Crockford-base32 chars (40 bits) so public receipt URLs aren't enumerable. Valid for Paystack (alnum + `-`).

**`settle(reference)` — the only settlement path,** called by both the receipt page and the webhook:

1. *Claim:* `UPDATE votes SET status='verifying' WHERE reference=? AND status='pending'`. If `rows_affected == 0`, another caller owns it → return current row. This is what makes the double-fire safe.
2. *Verify:* `GET /transaction/verify/:reference` (outside any DB transaction).
3. *Apply:* require `status='success'`, `currency='NGN'`, `channel='card'`, `amount == amount_kobo`, `domain` matches key prefix, **and the election still open at this instant**. Then `INSERT INTO card_locks ... ON CONFLICT DO NOTHING` — 1 row → `counted`; 0 rows → `refund_pending`.

### Outcomes

| Situation | Outcome | User sees |
| --- | --- | --- |
| Happy path | `counted`, receipt live | "Ballot recorded. TP-…" |
| Card already voted | `refund_pending` → auto-refund | "This card already voted. ₦100 is being refunded — RF-…" + link to the original receipt |
| **Poll closed mid-payment** | `refund_pending` | "Voting closed. ₦100 refunded." |
| Amount / domain mismatch | `refund_pending`, flagged | "Payment didn't match. Refunded." |
| Abandoned checkout | `abandoned` after 45 min | Receipt: "not completed", offers retry |

**Refund:** `POST /refund` with `{ transaction, amount, reference: "RF-<ref>", merchant_note }`. Paystack returns **409 on reference reuse** — free double-refund protection. Treat 409 as "already refunded", then `GET /refund?reference=RF-…`. Transition to `refunding` exactly once.

**Reconciliation:** a Tokio interval task every 10 min re-verifies `pending` rows past `expires_at` (settles or abandons) and reclaims `verifying` rows older than 120s. Tallies never count unpaid votes.

---

## 6. One card, one vote — three layers

1. **Device pre-check (soft).** FingerprintJS hash, stored as `HMAC-SHA256(server_secret, hash)` — never raw, unlinkable across elections. A match *disables* the pay button with: *"We think this device already voted. Device checks are imperfect — private browsing slips through. If your card already voted, we'll refund automatically."* Soft, because a hard block punishes shared phones and incognito users.
2. **Card lock (hard).** `UNIQUE(election_id, card_sig_hash)` on `card_locks`. The real enforcement.
3. **Auto-refund (safety net).** Anyone who pays twice gets ₦100 back without asking.

`channels: ["card"]` only — bank-transfer and USSD carry **no card signature**, so allowing them would silently break the lock.

**Test-mode collision:** Paystack's test card `4084 0840 8408 4081` is reusable, so every local test yields the *same* signature — the second test vote would be rejected, making the app undemoable. Solution: `DEMO_MODE=1`, guarded by a boot assertion that refuses to start unless the key starts with `sk_test_` and `NODE_ENV != production`. It skips the `card_locks` insert only; the hash is still recorded. Receipts show a "Demo mode" chip. Document the distinct test cards (`5060 6666 6666 6666 666`, `5078 5078 5078 5078 12`, `5192 6027 2058 4796`, declined `4084 0800 0000 5408`) since distinct PANs give distinct signatures.

---

## 7. The T&C gate (the legal core)

**Single source of truth:** the text lives in `server/src/terms.rs` with a `TERMS_VERSION` constant, served at `GET /api/terms`. The frontend renders whatever the API returns, so the copy can never drift between the two.

Copy (short, bold, unmistakable):

> ### THIS IS NOT AN OFFICIAL ELECTION.
>
> The Polls is independent. We are **not INEC**, we are not affiliated with INEC, and we are not connected to any government body.
>
> **Nothing here is an official result.** Your vote has no legal effect and will not be counted by any electoral authority.
>
> You are paying **₦100 for one mock ballot**. This is not a donation to any party or candidate.

**Flow:** select candidate → "Cast vote · ₦100" → a modal sheet shows the statement at display size → a required checkbox "I understand" → only then does "Continue to payment" enable → `POST /api/vote/init`.

**Consent is recorded, not just displayed.** The init payload carries `terms_version` + `terms_accepted: true`; the server writes `terms_version` and `terms_accepted_at`, and refuses if the version is stale. If a charge is ever disputed, there is proof of which text that person agreed to and when.

The same statement repeats on the receipt page and in the footer. No exceptions, no "small print" anywhere.

---

## 8. Anti-misuse watermark

Two layers, because they defend against different things:

1. **Tiled diagonal watermark** behind the tally — repeating "THE POLLS · UNOFFICIAL" in the display serif, rotated −30°, `opacity: .07`, `blur(.3px)`. Blurred so it never competes with the numbers, but legible when you look. Rendered as an inline SVG `<pattern>`, `aria-hidden`, `pointer-events-none`.
2. **A solid corner stamp, not blurred** — rotated −8°, 2px `--stamp` border, "UNOFFICIAL RESULT — NOT INEC" plus the cycle year. This is the part that survives a screenshot crop.

Tally numbers get a `text-shadow: 0 0 6px var(--paper)` halo so contrast stays above 4.5:1 over the watermark.

**The OG image carries it too.** `/og/e/[slug].png` renders SVG → PNG with `resvg` in Rust, stamp included. Social cards are the most-copied artifact, so they get marked at the source.

**Honest limitation:** a watermark deters casual misuse; it does not stop determined forgery — anyone can DevTools an overlay off. The real protections are the domain in the screenshot, the public receipt page, and the T&C consent record. The watermark is the cheap 90%.

---

## 9. Countdown timer

- Server returns `opens_at`, `closes_at`, **and `server_now`**. The client computes its own clock offset from `server_now` — phone clocks are wrong constantly, and a countdown that disagrees with the server's decision to accept a vote is a support nightmare.
- Display: mono, tabular — `Closes in 3d 14h 22m 08s`. Under one hour it switches to `hh:mm:ss` in `--stamp` red with a pulsing live dot.
- At zero: the tally freezes, voting is disabled, a "POLL CLOSED" stamp replaces the CTA, and the final result stands.
- **The client timer is purely cosmetic.** `settle()` re-checks the window server-side; a vote paid after `closes_at` is refunded. That is the only thing actually enforcing the deadline.

---

## 10. State selection — a dropdown, no maps

A native `<select>` with `<optgroup>` per geopolitical zone (NC, NE, NW, SE, SS, SW), all 36 states + FCT, then a final "Outside Nigeria" option that reveals a country field.

Native `<select>` on purpose: it is the fastest, most accessible, most thumb-friendly control on a mid-range Android, and it costs zero JS. Server validates the submitted code against a whitelist of 37 state codes + ISO-3166 country codes. Fully skippable ("Prefer not to say").

---

## 11. Zero PII

**State selection is not personal data** — a state is a coarse region, not an identifier. Safe to store, and it powers the geo breakdown.

**Email is the real PII risk** — Paystack requires one per transaction. So we never ask: generate `ballot+<reference>@thepolls.ng` per transaction. Paystack accepts it; we never store a real address.

Net: no names, no emails, no phones, no raw IPs. Card signatures are hashed. What is held is a reference, a candidate, a state, a hashed device, a hashed card, and a consent timestamp.

---

## 12. Admin control panel

Askama + HTMX, mounted at `/admin`, single admin account.

| Route | Purpose |
| --- | --- |
| `/admin/login` | Argon2 verify, rate-limited, signed HttpOnly session cookie |
| `/admin` | Elections table: title, window, derived status, votes, fee |
| `/admin/elections/:id` | Edit title, `opens_at`, `closes_at`, fee; force-close; clone to a new cycle |
| `/admin/elections/:id/candidates` | Add / edit / reorder / remove; party, wiki title, photo, standing |
| `/admin/refunds` | Queue of `refund_pending` + `refund_failed`, with a retry button |
| `/admin/lookup` | Paste a reference, see the full vote state and payment trail |

Auth: `ADMIN_PASSWORD_HASH` in env, generated by `cargo run -- --hash-password`. Session cookie `HttpOnly; Secure; SameSite=Strict`, plus a CSRF token in every form (SameSite alone isn't enough for a money app). Login rate-limited to 5 attempts per 10 min per IP.

Editing a window takes effect immediately — `settle()` reads the DB, not a cache, so a force-close stops votes mid-flight and anything already paid is refunded.

---

## 13. Candidate records

Wikipedia's REST API (`/api/rest_v1/page/summary/{title}`) returns a short plain-text extract, a portrait, and a canonical link — exactly the "wiki-style but short" brief. Cached to disk at seed time by `src/bin/seed.rs`, so the site never hot-links Wikimedia per request.

The record page: portrait, one-line description, short extract, party + standing, their tally, and an outbound "Read the full profile". Opens as a drawer on desktop, a full page on mobile (deep-linkable either way).

**Roster honesty:** 37 governorship races is a large, fast-moving dataset, and 2027 nominations aren't final. Every candidate carries `standing` (`declared` / `expected` / `incumbent`) and an optional `source`; governorship pages show a **"Provisional roster"** banner. Incumbents are verifiable public fact; challengers live in one editable JSON file, so corrections take minutes, not a code change.

---

## 14. Build order

1. **Rust skeleton** — axum, config, pool, migrations, `/api/health`. Confirm it boots.
2. **Domain** — `models.rs`, `payments/state.rs` + its transition-table tests. Nothing else starts until those tests pass.
3. **Seed** — states, elections, candidates, wiki enrichment.
4. **Public API** — elections, tally, candidates, terms.
5. **Frontend foundation** — tokens, fonts, sheet shell, masthead + stamp.
6. **Election sheet** — candidate rows, tally bar, drill-down, drawer, countdown, state select. The centrepiece; get it right before payments.
7. **Payments** — terms gate → init → redirect → settle → receipt → webhook → refund paths.
8. **Watermark + OG images.**
9. **Admin panel.**
10. **`/method`** — methodology and disclaimers.
11. **Polish** — motion, a11y pass, 360px check, empty/error states.

## 15. Verification

- `cargo test` on the transition table: every from/event pair, including all illegal ones.
- Happy path with `4084 0840 8408 4081` → receipt shows `counted`, tally +1.
- Second vote, same card → duplicate message, refund created, tally unchanged.
- Declined card → `abandoned`, tally unchanged.
- Refresh receipt 5× and fire the webhook manually → exactly one vote counted.
- Force-close the poll between init and pay → auto-refund.
- Countdown with the system clock set 3 hours ahead → still trusts the server.
- 360px viewport, keyboard-only pass, `prefers-reduced-motion`, screen reader on the tally.

---

*Companion document: `HANDOVER.md`*
