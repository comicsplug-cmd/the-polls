# The Polls — Build Plan (v2)

Updated 25 Nov 2026. Replaces `BUILD-PLAN.md`. Companion: `HANDOVER-v2.md`.

An unofficial Nigerian mock-poll site. ₦100 per ballot, paid through Paystack. Every ballot gets a unique public receipt that confirms it was counted but never says who it was for. Rust backend, Next.js frontend.

**Poll window:** one window for every race, **Saturday 26 Dec 2026 to Monday 4 Jan 2027** (Africa/Lagos; recommended close 23:59 WAT, set in the seed file). That is 12 days before the presidential and National Assembly election on 16 Jan 2027, and 33 days before the governorship and State Assembly election on 6 Feb 2027 (INEC revised timetable).

---

## 0. What changed in v2

- **Dates fixed:** single window, 26 Dec to 4 Jan.
- **Scope split into phases.** Phase 1 is the presidential race and must ship. Phase 2 (governorship races) ships only if rosters are verified by 18 Dec.
- **"Zero PII" replaced** by an honest data-handling section (§11): what is stored, why, and when it is purged.
- **Receipts are candidate-blind.** A public receipt URL proves a ballot was counted, not who it was for. This removes the verifiable proof that vote-buyers would want.
- **Rosters come from INEC's published final lists.** The "expected" and "provisional" standing is gone.
- **New §14, product guardrails:** no prizes, no payouts, no odds, never resolved against the real result. This keeps the product a poll and not a betting market.
- **T&C copy updated:** it now says results are not representative of the public and that there are no prizes.
- **Device and card hashes are election-scoped.** The HMAC input includes the election id, so hashes are actually unlinkable across polls.
- **Admin UI and dynamic OG images moved to Phase 2.** Phase 1 uses seed files and a static pre-stamped OG image.
- **Name:** "The Polls" only. The "NOT INEC" stamp stays as a disclaimer. Domain is TBD.

---

## 1. Honest note on the Rust decision

Rust will not make this faster. The bottleneck is Paystack's ~300ms API round-trip and SQLite writes; a 100k-vote week averages 0.16 writes/second, which Node handles without noticing.

The real reasons Rust is right here:

- **The payment state machine becomes compiler-checked.** `counted` and `refunded` are distinct states, and exhaustive `match` refuses to compile if a transition is unhandled. That is the bug class that costs real money.
- **Single static binary**, ~10MB RAM, no `node_modules`, no cold starts.
- **Cheap hosting.** A $5 VPS is genuinely enough.

Because the author is new to Rust, the code is kept deliberately plain:

- One crate, flat modules. No workspaces, no generics beyond `Option`/`Result`, no `#[async_trait]`, no custom extractors beyond two.
- `sqlx` with **runtime-checked** queries and `FromRow` structs, not the `query!` macro, which would force a live DB at compile time.
- `anyhow` in handlers; `thiserror` only for payment errors.
- Admin (Phase 2) is **Askama templates + HTMX**, not React.
- The state machine lives in one file (`payments/state.rs`), heavily commented. That is the only file needing careful reading.

---

## 2. Design: "Civic Record"

"Official but unmistakably not INEC, and not AI slop." Black + yellow is the right instinct, but there is a trap: **black + yellow + "pay ₦100 to vote" reads as a betting app** (SportyBet, Bet9ja territory). So yellow becomes a *marker*, not a brand fill. This matters more now that §14 commits the product to *not* being a betting product.

**Thesis:** The Polls looks like a printed electoral record: a tally sheet, a ballot stub, a gazette. Ink on paper. It reads institutional because of its *typography and rules*, not because it borrows government colours.

**Guardrails against looking like betting:**

- Paper-toned background, never black. Black is ink (type, hairlines), not surface.
- Yellow is a highlighter: a swash behind a word, tally strokes, a 2px rule. Never a big flat CTA block.
- Serif display type. Betting apps are uniformly heavy sans.
- A physical rubber-stamp motif instead of gradients, glows, or glassmorphism.
- No odds-style numbers, no green/red up-down arrows, no "trending" language on the tally.

### Tokens (OKLCH, warm-tinted neutrals)

| Token | Value | Use |
| --- | --- | --- |
| `--paper` | `oklch(96.5% 0.012 88)` | Base surface (warm newsprint) |
| `--paper-2` | `oklch(93% 0.015 88)` | Recessed bands, table stripes |
| `--ink` | `oklch(18% 0.012 80)` | Primary text (never `#000`) |
| `--ink-2` | `oklch(38% 0.010 80)` | Secondary text |
| `--rule` | `oklch(84% 0.012 85)` | Hairlines, dotted leaders |
| `--marker` | `oklch(84% 0.165 92)` | The yellow: highlighter swashes, tally strokes |
| `--stamp` | `oklch(52% 0.17 27)` | Oxidised red: UNOFFICIAL stamp, live dot, closing-soon |

`--ink` on `--paper` ≈ 15:1. Yellow is only ever a *background behind ink text* (≈11:1) or a pure shape. **Never yellow text on paper**; it fails WCAG and looks cheap. Party colours are muted print-ink tones so the tally reads like a printed legend.

### Type

- **Display:** Instrument Serif (italic for emphasis).
- **UI/body:** Archivo.
- **Data:** IBM Plex Mono for references, countdowns, tallies.

Fluid `clamp()` scale, tight display tracking, `tabular-nums` on every changing number.

### Layout & signature interaction

- A 12-col "sheet" with a visible left gutter carrying mono metadata (file no., cycle, status) like a printed form.
- **Dotted leader lines** between candidate name and ballot count, classic ballot/index typography.
- **The tally bar** replaces a generic chart: each row is `name ····· count`, with a segmented bar underneath where the marker swipes in. Clicking a segment drills into state-by-state breakdown.
- No cards-in-cards. Sections separated by hairlines and whitespace.

### Motion

`ease-out-quart`, 180–420ms. Numbers roll on tabular figures; the stamp presses on the confirmation screen. Full `prefers-reduced-motion` fallback.

### Mobile-first is non-negotiable

Android Chrome on mid-range phones over mobile data. No 3D, no shaders, no hero video. Server Components by default; **only five client islands ship JS**: `TallyBar`, `Countdown`, `StateSelect`, `RecordDrawer`, `TermsGate`. Everything else is zero-JS HTML. Sticky bottom vote bar, ≥44px targets, verified at 360×640.

---

## 3. Architecture

```
the-polls/
  server/                       # Rust, single crate
    Cargo.toml
    migrations/0001_init.sql
    src/
      main.rs                   # axum router + state
      config.rs                 # env parsing
      db.rs                     # pool, migrations, WAL
      models.rs                 # rows + enums
      terms.rs                  # SINGLE source of the T&C text + version
      watermark.rs              # SVG generator, shared by page (+ OG image in Phase 2)
      purge.rs                  # scheduled purge of identifiers after close
      payments/
        state.rs                # VoteState enum + transitions  <- the one careful file
        paystack.rs             # reqwest client
        settle.rs               # settle(), refunds, reconciliation
      routes/
        public.rs               # elections, tally, candidates, terms
        vote.rs                 # init
        webhook.rs              # paystack webhook
        og.rs                   # PHASE 2: OG image via resvg
      admin/                    # PHASE 2
        mod.rs auth.rs views.rs
        templates/*.html        # askama
    src/bin/seed.rs             # reads data/seed/*.json
  web/                          # Next.js 16 + React 19 + Tailwind 4
    app/  components/  lib/
  data/seed/*.json
  dev.sh
```

**Dev:** Axum on `:8080`, Next on `:3000` with `rewrites` proxying `/api/*` to `127.0.0.1:8080`. `dev.sh` runs both with a `trap` to kill both on Ctrl-C.

**Prod:** Caddy in front: `/api/*`, `/admin/*`, `/og/*` go to Axum; everything else goes to Next standalone. Two processes, one box. Put Cloudflare (or similar) in front of Caddy for DDoS and bot protection.

**Why not let Axum serve the frontend too:** the receipt page needs SSR for OG tags, and Next SSR needs Node. Accepting two processes buys far faster UI iteration than hand-rolling SSR in Rust.

**Domain:** TBD (placeholder `thepolls.ng`). The synthetic-email domain in §11 must be one you control.

---

## 4. Data model

```sql
elections(
  id TEXT PK, slug TEXT UNIQUE, title TEXT, kind TEXT, cycle_year INTEGER,
  fee_kobo INTEGER, opens_at INTEGER, closes_at INTEGER,
  force_closed INTEGER DEFAULT 0,
  tally_mode TEXT DEFAULT 'live',        -- live | after_close
  created_at INTEGER)

candidates(
  id TEXT PK, election_id TEXT, slug TEXT, name TEXT, party TEXT, party_code TEXT,
  photo_url TEXT, wiki_title TEXT, one_liner TEXT,
  source_url TEXT NOT NULL,              -- INEC list or other primary source
  source_checked_at INTEGER,             -- when the roster was last checked
  sort_order INTEGER)

votes(
  id INTEGER PK, reference TEXT UNIQUE, election_id TEXT, candidate_id TEXT,
  amount_kobo INTEGER, status TEXT, reason TEXT,
  paystack_txn_id INTEGER,               -- PURGED after close + refund window
  channel TEXT, domain TEXT,
  card_sig_hash TEXT, lock_kind TEXT,    -- PURGED. election-scoped HMAC. lock_kind: card | none
  device_hash TEXT,                      -- PURGED. election-scoped HMAC
  state_code TEXT, country_code TEXT,
  terms_version TEXT, terms_accepted_at INTEGER,
  refund_ref TEXT UNIQUE, verify_attempts INTEGER DEFAULT 0,
  refund_attempts INTEGER DEFAULT 0, claimed_at INTEGER, expires_at INTEGER,
  created_at INTEGER, counted_at INTEGER, refunded_at INTEGER)

card_locks(election_id TEXT, card_sig_hash TEXT, vote_reference TEXT,
           created_at INTEGER, PRIMARY KEY(election_id, card_sig_hash))   -- DELETED after close + refund window

payment_events(id INTEGER PK, reference TEXT, source TEXT, event TEXT,
               paystack_event_id TEXT, created_at INTEGER,
               UNIQUE(event, paystack_event_id))
```

`card_locks` is the enforcement point, not `votes`. Tally is always `COUNT(*) WHERE status='counted'`, never a denormalised counter.

**Election status is derived, never stored:** `opens_at > now` gives `upcoming`; `now > closes_at` or `force_closed` gives `closed`; else `open`. One function, one source of truth, no drift.

**Election-scoped hashes:** `HMAC-SHA256(secret, election_id || ":" || value)`. Using the same secret across elections without mixing in the election id would make a card or device linkable across polls.

---

## 5. Vote state machine

```
pending --claim--> verifying --> counted               (terminal, in tally)
                      |--------> refund_pending -> refunding -> refunded   (terminal)
                      |                                  \----> refund_failed (needs a human)
                      \--------> pending (120s claim-timeout reclaim)
pending --expiry sweep--> abandoned                     (terminal)
```

`payments/state.rs` holds a `VoteState` enum plus one `fn transition(from, event) -> Result<To, Refused>`. Illegal transitions return `Refused`, not a panic. Unit-tested with a table of every from/event pair. This is the file to write tests for first.

**Reference = the receipt.** `TP-2027-PRES-8F42K9`: 8 Crockford-base32 chars (40 bits) so public receipt URLs aren't enumerable. Valid for Paystack (alnum + `-`).

**The public receipt page is candidate-blind.** It shows the poll, the status (`counted`, `refunded`, `not completed`), and the time. It never shows which candidate the ballot was for. Immediately after payment, the voter's own session may see "Your ballot for [Name] was recorded", but that view is not retrievable later by URL. A durable, shareable receipt that names the candidate is proof of how someone voted, which is exactly what a vote-buyer wants.

**`settle(reference)` is the only settlement path,** called by both the receipt page and the webhook:

1. *Claim:* `UPDATE votes SET status='verifying' WHERE reference=? AND status='pending'`. If `rows_affected == 0`, another caller owns it, so return the current row. This is what makes the double-fire safe.
2. *Verify:* `GET /transaction/verify/:reference` (outside any DB transaction).
3. *Apply:* require `status='success'`, `currency='NGN'`, `channel='card'`, `amount == amount_kobo`, `domain` matches key prefix, **and the election still open at this instant**. Then `INSERT INTO card_locks ... ON CONFLICT DO NOTHING`: 1 row gives `counted`; 0 rows gives `refund_pending`.

### Outcomes

| Situation | Outcome | User sees |
| --- | --- | --- |
| Happy path | `counted`, receipt live | "Ballot recorded. TP-…" |
| Card already voted | `refund_pending`, auto-refund | "This card already voted. ₦100 is being refunded, ref RF-…" |
| **Poll closed mid-payment** | `refund_pending` | "Voting closed. ₦100 refunded." |
| Amount / domain mismatch | `refund_pending`, flagged | "Payment didn't match. Refunded." |
| Abandoned checkout | `abandoned` after 45 min | Receipt: "not completed", offers retry |

**Refund:** `POST /refund` with `{ transaction, amount, reference: "RF-<ref>", merchant_note }`. Paystack returns **409 on reference reuse**, which is free double-refund protection. Treat 409 as "already refunded", then `GET /refund?reference=RF-…`. Transition to `refunding` exactly once.

**Reconciliation:** a Tokio interval task every 10 min re-verifies `pending` rows past `expires_at` (settles or abandons) and reclaims `verifying` rows older than 120s. Tallies never count unpaid votes.

**Fees:** Paystack lists local transactions at 1.5% + ₦100, with the ₦100 waived under ₦2,500, so a ₦100 ballot costs about ₦1.50. International cards are listed at 3.9% + ₦100; the waiver is only stated for local payments. See the foreign-card decision in §6.

---

## 6. One card, one vote: three layers

1. **Device pre-check (soft).** FingerprintJS hash, stored as the election-scoped HMAC from §4, never raw. A match *disables* the pay button with: *"We think this device already voted. Device checks are imperfect, and private browsing slips through. If your card already voted, we'll refund automatically."* Soft, because a hard block punishes shared phones and incognito users.
2. **Card lock (hard).** `UNIQUE(election_id, card_sig_hash)` on `card_locks`. The real enforcement.
3. **Auto-refund (safety net).** Anyone who pays twice gets ₦100 back without asking.

`channels: ["card"]` only, because bank-transfer and USSD carry **no card signature**, so allowing them would silently break the lock.

**Known trade-offs (owner decisions pending, see Handover §H):**

- **Card-only will cut turnout.** Many Nigerians pay online by transfer or USSD rather than card. Allowing them would need a different identity primitive.
- **Foreign cards.** A diaspora voter's international card can cost more in fees than the ₦100 collected. Options: accept the loss, refund non-NG cards (if the verify response exposes a card country), or drop "Outside Nigeria" for launch.
- **Cheap ballot stuffing.** Virtual cards are easy to create, so a determined party can buy ballots at ₦100 each. The card lock raises the cost but cannot stop it. This is why `/method` and the T&C say the poll counts paying participants, not the public.

**Test-mode collision:** Paystack's test card `4084 0840 8408 4081` is reusable, so every local test yields the *same* signature and the second test vote would be rejected. Solution: `DEMO_MODE=1`, guarded by a boot assertion that refuses to start unless the key starts with `sk_test_` and `NODE_ENV != production`. It skips the `card_locks` insert only; the hash is still recorded. Receipts show a "Demo mode" chip. Use distinct test cards (`5060 6666 6666 6666 666`, `5078 5078 5078 5078 12`, `5192 6027 2058 4796`, declined `4084 0800 0000 5408`), since distinct PANs give distinct signatures.

---

## 7. The T&C gate (the legal core)

**Single source of truth:** the text lives in `server/src/terms.rs` with a `TERMS_VERSION` constant, served at `GET /api/terms`. The frontend renders whatever the API returns, so the copy can never drift between the two.

Copy (short, bold, unmistakable):

> ### THIS IS NOT AN OFFICIAL ELECTION.
>
> The Polls is independent. We are **not INEC**, we are not affiliated with INEC, and we are not connected to any government body.
>
> **Nothing here is an official result.** Your ballot has no legal effect and will not be counted by any electoral authority.
>
> **This is not a representative poll.** Results show how people who paid ₦100 chose to vote, not what Nigerians think.
>
> You are paying **₦100 for one mock ballot.** There are no prizes, and this is not a donation to any party or candidate.

**Flow:** select candidate, then "Cast ballot · ₦100", then a modal sheet shows the statement at display size, then a required checkbox "I understand", and only then does "Continue to payment" enable, which calls `POST /api/vote/init`.

**Consent is recorded, not just displayed.** The init payload carries `terms_version` + `terms_accepted: true`; the server writes `terms_version` and `terms_accepted_at`, and refuses if the version is stale. If a charge is ever disputed, there is proof of which text that person agreed to and when.

The same statement repeats on the confirmation page and in the footer. No small print anywhere.

---

## 8. Anti-misuse watermark

Two layers, because they defend against different things:

1. **Tiled diagonal watermark** behind the tally: repeating "THE POLLS · UNOFFICIAL" in the display serif, rotated −30°, `opacity: .07`, `blur(.3px)`. Blurred so it never competes with the numbers, but legible when you look. Tile it across the whole background so any crop of the numbers still contains it. Rendered as an inline SVG `<pattern>`, `aria-hidden`, `pointer-events-none`.
2. **A solid corner stamp, not blurred**: rotated −8°, 2px `--stamp` border, "UNOFFICIAL RESULT · NOT INEC" plus the cycle year.

Tally numbers get a `text-shadow: 0 0 6px var(--paper)` halo so contrast stays above 4.5:1 over the watermark.

**Share image.** Phase 1 ships a single static, pre-stamped OG image for every page. Phase 2 replaces it with `/og/e/[slug].png`, rendered SVG to PNG with `resvg`, stamp included. A "Share" button should produce a branded image with the disclaimer baked in, so numbers travel in the safe format.

**After close:** the site becomes a read-only archive with a permanent banner: "Mock poll, closed. Not an official result."

**Honest limitation:** a watermark deters casual misuse; it does not stop determined forgery, since anyone can remove an overlay in DevTools. The real protections are the domain in the screenshot, the public receipt page, and the T&C consent record. The watermark is the cheap 90%. It also cannot help with text-only spread ("62% on that poll").

---

## 9. Countdown timer

- Server returns `opens_at`, `closes_at`, **and `server_now`**. The client computes its own clock offset from `server_now`, because phone clocks are wrong constantly and a countdown that disagrees with the server's decision to accept a vote is a support nightmare.
- Display: mono, tabular, `Closes in 3d 14h 22m 08s`. Under one hour it switches to `hh:mm:ss` in `--stamp` red with a pulsing live dot. Before opening: `Opens in …` until 26 Dec.
- At zero: the tally freezes, voting is disabled, a "POLL CLOSED" stamp replaces the CTA, and the final result stands.
- **The client timer is purely cosmetic.** `settle()` re-checks the window server-side; a ballot paid after `closes_at` is refunded. That is the only thing actually enforcing the deadline.

---

## 10. State selection: a dropdown, no maps

A native `<select>` with `<optgroup>` per geopolitical zone (NC, NE, NW, SE, SS, SW), all 36 states + FCT, then a final "Outside Nigeria" option that reveals a country field (subject to the foreign-card decision in §6).

The dropdown records where the *voter* lives, so the FCT stays in it. Which *races* exist is a separate question (§13).

Native `<select>` on purpose: it is the fastest, most accessible, most thumb-friendly control on a mid-range Android, and it costs zero JS. Server validates the submitted code against a whitelist of 37 state codes + ISO-3166 country codes. Fully skippable ("Prefer not to say").

---

## 11. Data handling (replaces "Zero PII")

**What we do not collect:** names, emails, phone numbers, NIN, or any account. No registration.

**What we do hold, and why:**

| Data | Where | Why | Retention |
| --- | --- | --- | --- |
| `reference` (receipt ID) | `votes` | Receipt, idempotency | Kept; not linkable to a person without Paystack's records |
| `paystack_txn_id` | `votes` | Refunds, reconciliation | **Purged** at close + 60 days |
| `card_sig_hash` | `votes`, `card_locks` | One card, one vote | **Purged** at close + 60 days |
| `device_hash` | `votes` | Soft duplicate check | **Purged** at close + 60 days |
| `candidate_id`, `state_code`, `country_code` | `votes` | Tally and geo breakdown | Kept; anonymous once the rows above are purged |
| `terms_version` | `votes` | Which text applied | Kept |
| `terms_accepted_at` | `votes` | Consent proof | **Purged** with the identifiers (lawyer to confirm; chargeback windows may argue for longer) |

**What Paystack holds, as the payment processor:** card details, the transaction record, and the customer email. Paystack processes the payment and also stores its own transaction data; the fingerprint it returns is what we hash for the card lock. The Paystack dashboard can tie a transaction to a payer, and our rows hold the reference and transaction id that point to it. So during the poll and the refund window, someone with access to both can link a payer to a ballot.

**Why this matters:** the hashed card and device values are persistent identifiers, and linked to a candidate they reveal political opinion. Treat that as sensitive personal data. That means:

- A short, plain-language **privacy notice** that names the device fingerprint and card fingerprint.
- The **purge job** (`purge.rs`, scheduled, tested), default `PURGE_AFTER_DAYS=60`, confirmed with your lawyer.
- A check on whether you must **register with the NDPC** as a data controller of major importance (tiers depend on the volume of data processed).
- Access to the Paystack dashboard limited to as few people as possible, with 2FA.

**Synthetic email.** Paystack requires an email per transaction, so we never ask for one: generate `ballot+<reference>@<your-domain>` per transaction. It must be a real domain (Paystack rejects `.local`). Either set up a catch-all mailbox or turn off customer receipt emails in the Paystack dashboard so nothing bounces.

---

## 12. Admin control panel (Phase 2)

**Phase 1 has no admin UI.** Windows, fee and candidates come from `data/seed/*.json` via `cargo run --bin seed`. Force-close is a one-line SQL update (`UPDATE elections SET force_closed=1 WHERE id=?`). Refund failures are resolved in the Paystack dashboard.

**Phase 2:** Askama + HTMX, mounted at `/admin`, single admin account.

| Route | Purpose |
| --- | --- |
| `/admin/login` | Argon2 verify, rate-limited, signed HttpOnly session cookie |
| `/admin` | Elections table: title, window, derived status, ballots, fee |
| `/admin/elections/:id` | Edit title, `opens_at`, `closes_at`, fee; force-close; clone to a new cycle |
| `/admin/elections/:id/candidates` | Add / edit / reorder / remove; party, source URL, photo |
| `/admin/refunds` | Queue of `refund_pending` + `refund_failed`, with a retry button |
| `/admin/lookup` | Paste a reference, see the full vote state and payment trail |

Auth: `ADMIN_PASSWORD_HASH` in env, generated by `cargo run -- --hash-password`. Session cookie `HttpOnly; Secure; SameSite=Strict`, plus a CSRF token in every form. Login rate-limited to 5 attempts per 10 min per IP. Editing a window takes effect immediately, because `settle()` reads the DB, not a cache.

---

## 13. Candidate records

**Source of truth: INEC's published lists.** INEC released its final list of presidential candidates on 12 Sep 2026; reports put it at 18 candidates (some outlets say 19), so load the names, parties and running mates directly from INEC's document and store its URL in `source_url`. Disputes exist over omissions (for example, former President Jonathan's nomination is not on the list), so treat INEC's list as authoritative for this poll, record the date you last checked it, and add a one-line "Candidates as listed by INEC on <date>" note to the page. Candidates do not carry "expected" or "provisional" labels.

**Governorship (Phase 2).** Do not assume one race per state. The FCT has no governorship election, and states with off-cycle governorship elections likely do not vote on 6 Feb 2027. I could not verify the exact list, so build the race list from INEC's governorship timetable and candidate lists, and drop any state you cannot source by the 18 Dec go/no-go.

**Equal treatment.** Every candidate gets the same template: name, party, running mate, one neutral line, a link to the INEC entry, and an optional Wikipedia link. Ordering is alphabetical by surname or randomised per page load, never by prominence.

**Bios and photos.** Wikipedia's REST API (`/api/rest_v1/page/summary/{title}`) returns a short extract and a canonical link, cached to disk at seed time so the site never hot-links Wikimedia per request. Photos are optional: use INEC-published images or openly licensed Wikimedia images with attribution, or go text-only for Phase 1.

The record page: one-line description, short extract, party, their tally, and an outbound "Read the full profile". Opens as a drawer on desktop, a full page on mobile (deep-linkable either way).

---

## 14. Product guardrails: what The Polls is not

The Polls sells a mock ballot for a fixed fee. It must not drift into a prediction market or betting product. Those are regulated as gambling in several countries, Polymarket already lists a Nigerian presidential market, and Nigerian gaming activity is licensed by gaming regulators (the federal NLRC and state boards). Getting this wrong would turn a poll into something needing a licence.

**Rules (hard):**

1. **Fee only.** ₦100 buys one ballot. No prizes, pools, rewards, referral payouts, giveaways or "win" mechanics tied to ballots or outcomes.
2. **No odds.** The tally shows ballot counts and each candidate's share of ballots. Never "odds", "implied probability", "price" or payout figures.
3. **One ballot per card per race.** No buying extra weight, no tiers, no higher-stake ballots.
4. **Never resolve against the real result.** After 16 Jan and 6 Feb, do not publish "we called it" scorecards, accuracy rankings or leaderboards comparing the poll to INEC's result.
5. **Vocabulary.** Always: poll, ballot, tally, mock. Never: bet, wager, stake, odds, market, prediction, winnings.
6. **No transfers.** Ballots and receipts cannot be traded, gifted or resold.

**Enforcement:** add a CI grep that fails the build if banned words appear in user-facing strings, and have your lawyer confirm in writing that a fee-only, prize-free poll falls outside gaming licensing before launch. Any future feature that touches rules 1 to 4 needs a fresh legal opinion first.

---

## 15. Legal and compliance checklist (for your Nigerian lawyer)

- **Business setup:** CAC registration; Paystack live approval, including disclosure that this is a politically themed poll.
- **Branding:** no INEC logo, colours or "official" claims; confirm the "not INEC" stamp is enough.
- **Electoral Act 2026:** check for any provision on opinion polls or on publishing results. I found none in my research, but I could not read the full text.
- **Cybercrimes Act (2024 amendment), s.24:** covers messages known to be false that are sent to cause a breakdown of law and order. Never present mock results as real.
- **Nigeria Data Protection Act 2023:** NDPC registration question, privacy notice, retention period, device fingerprinting disclosure, sensitive-data treatment.
- **Gaming law:** written confirmation per §14.
- **Tax:** how ballot revenue is taxed (ask an accountant).
- **Terms and privacy policy:** review of §7 copy, refund policy, and the "fee, not donation" wording.
- **Copyright:** candidate bios and photos.

---

## 16. Schedule (Africa/Lagos)

| Date | Milestone |
| --- | --- |
| 25 Nov to 1 Dec | Start CAC registration and Paystack application; engage lawyer; buy domain; get Paystack test keys |
| 1 to 12 Dec | Phase 1 build in test mode (steps 1 to 7 in §17) |
| 13 to 18 Dec | Watermark, `/method`, privacy and T&C pages; lawyer review; **18 Dec: governorship go/no-go**; Paystack live approval targeted by ~15 Dec (not in your control) |
| 19 to 24 Dec | Live-mode test with small real payments, including refund paths; mobile checks; feature freeze 23 Dec |
| **26 Dec** | **Poll opens** |
| **4 Jan** | **Poll closes**; tally freezes; archive banner |
| 14 Jan | Last day of campaigning per INEC timetable |
| 16 Jan | Presidential and National Assembly election |
| 6 Feb | Governorship and State Assembly election |
| 5 Mar | Purge job runs (close + 60 days) unless your lawyer sets a different period |

---

## 17. Build order

**Phase 1 (must ship; presidential only)**

1. **Rust skeleton:** axum, config, pool, migrations, `/api/health`. Confirm it boots.
2. **Domain:** `models.rs`, `payments/state.rs` + its transition-table tests. Nothing else starts until those tests pass.
3. **Seed:** states, the presidential election, INEC-listed candidates with `source_url`.
4. **Public API:** elections, tally, candidates, terms.
5. **Frontend foundation:** tokens, fonts, sheet shell, masthead + stamp.
6. **Election sheet:** candidate rows, tally bar, drill-down, countdown, state select. Get it right before payments.
7. **Payments:** terms gate, init, redirect, settle, confirmation, candidate-blind receipt, webhook, refund paths.
8. **Watermark, static OG image, `/method`, privacy notice, purge job.**
9. **Polish:** motion, a11y pass, 360px check, empty/error states.

**Phase 2 (only if time allows after Phase 1 is solid)**

10. Governorship races from verified INEC lists.
11. Dynamic OG images and share-image generator.
12. Admin panel.
13. Wikipedia enrichment and the record drawer.

---

## 18. Verification

- `cargo test` on the transition table: every from/event pair, including all illegal ones.
- Happy path with `4084 0840 8408 4081`: confirmation shows `counted`, tally +1.
- Second vote, same card: duplicate message, refund created, tally unchanged.
- Declined card: `abandoned`, tally unchanged.
- Refresh the receipt 5× and fire the webhook manually: exactly one vote counted.
- Force-close the poll between init and pay: auto-refund.
- **Public receipt URL never contains the candidate** (check HTML and JSON).
- **Purge job:** on a copy of the DB, `paystack_txn_id`, `card_sig_hash`, `device_hash` and `terms_accepted_at` are null and `card_locks` is empty, while tally counts are unchanged.
- **Hash scoping:** the same card in two elections yields different stored hashes.
- CI grep for banned words (§14).
- Countdown with the system clock set 3 hours ahead: still trusts the server.
- 360px viewport, keyboard-only pass, `prefers-reduced-motion`, screen reader on the tally.

---

## 19. Sources checked (25 Nov 2026)

- INEC revised timetable (16 Jan and 6 Feb 2027): Guardian, https://guardian.ng/?p=2792475
- INEC final presidential list (12 Sep 2026): Sahara Reporters, https://saharareporters.com/2026/09/12/inec-releases-final-list-18-political-parties-contest-2027-presidential-election
- Paystack pricing and NGN minimum: https://paystack.com/pricing and https://paystack.com/docs/api/
- Cybercrimes Act s.24: Daily Trust, https://dailytrust.com/?p=1951768
- NDPA registration tiers: Michalsons, https://www.michalsons.com/blog/nigeria-data-protection-act-ndpa-controller-and-processor-registration/77465
- Polymarket Nigerian presidential market: Legit.ng, https://www.legit.ng/politics/1734150-polymarket-predicts-nigerias-2027-presidential-election-winner-users-stake-money/

These are research notes, not legal advice. Have a Nigerian lawyer confirm each point.

---

*Companion document: `HANDOVER-v2.md`*
