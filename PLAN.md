# The Polls — paid mock-poll launch

## Summary

Build a mobile-first Nigerian mock-poll site from the empty workspace, using the v2 stack: Rust/Axum, SQLx/SQLite, and Next.js. Target 26 Dec 2026–4 Jan 2027, closing at 23:59:59 WAT. If a launch gate is missed, move the whole poll window and publish new dates before taking payments.

Include the presidential poll and a clearly labelled subset of scheduled governorship races whose official INEC candidate lists and QA are complete by 18 Dec. Exclude National Assembly and State Assembly polls. INEC’s current calendar supports the 16 Jan and 6 Feb election dates; update the plans’ older roster references to the latest official candidate lists. ([INEC calendar](https://inecnigeria.org/elections/calendar), [presidential list](https://www.inecnigeria.org/press/updated-list-of-candidates-for-2027-presidential-election), [governorship lists](https://www.inecnigeria.org/press/updated-final-list-of-candidates-for-2027-governorship-election))

## Implementation plan

- **Public experience:** Use the “Civic Record” design, server-rendered pages, and minimal client-side code. Ship election pages, live tallies, candidate records sourced equally from INEC, methodology and privacy pages, versioned terms, and a permanent labelled archive. Keep results live. Receipts and their URLs must never expose a candidate.
- **Payments:** Use Paystack hosted checkout with card and temporary **Pay with Transfer** accounts. Do not use Dedicated Virtual Accounts: those are reusable and customer-linked, with personal-detail requirements. Keep the ₦100 ballot price as the base amount; use the Paystack fee-pass-through setting and state clearly that Paystack adds its fee at checkout. Store the base price, actual amount paid, and fee separately. Confirm the final total and amount fields for card, transfer, and international-card flows before launch. ([Pay with Transfer](https://paystack.com/docs/v2/payments/channels/pwt), [Dedicated Virtual Accounts](https://paystack.com/docs/payments/dedicated-virtual-accounts/), [fee pass-through](https://support.paystack.com/en/articles/2130306))
- **Payment integrity:** Generate separate, cryptographically random 128-bit public receipt IDs and private Paystack references. Count a ballot only after server-side verification of the successful NGN transaction and the ₦100 base amount. Enforce one ballot per election when a card signature is available; use an election-scoped HMAC device fingerprint only as a soft abuse signal. Each distinct successful transfer reference can count once. The methodology must say this cannot guarantee one person, one ballot.
- **Settlement and refunds:** Verify Paystack webhook signatures against the raw request body, deduplicate events, and make the election cutoff check, card lock, and count update atomic. Use Paystack’s `paid_at` as the cutoff authority: a payment completed before close can count even if its webhook arrives later; a payment completed after close is refunded. For any paid ballot that cannot count, request a full refund of the amount actually paid and track Paystack’s refund status. Paystack executes refunds; it does not return the original processing fee, so maintain a refund reserve. Surface “needs attention” refunds in the admin console for resolution through Paystack. ([Webhook verification](https://paystack.com/docs/payments/webhooks/), [refund policy](https://support.paystack.com/en/articles/2127106))
- **Data and privacy:** Collect no voter name, phone, NIN, or account. Use a synthetic email for Paystack; never send candidate information in Paystack metadata or logs. Do not persist raw webhook payloads or payer details. Keep candidate and location links only through close + 60 days, then detach them while retaining aggregate results and candidate-blind receipt status. Keep restricted payment-to-receipt support references for up to one year, then purge them with transaction IDs, card/device hashes, event IDs, and consent timestamps. Treat the data as sensitive while linked.
- **Operations:** Include a small authenticated admin console for force-close, refund status/retry, and payment-reference lookup; exclude candidate editing. Use Argon2, secure cookies, CSRF protection, and an audit trail. Deploy Axum and Next.js behind Caddy on one Linux VPS, with SQLite WAL, encrypted off-host backups, health checks, and alerts for stuck payments or failed refunds. Start on a temporary HTTPS domain; select and configure the permanent domain before live Paystack setup.

## Interfaces and state

- `POST /api/vote/init` validates the open race, candidate, optional location, and accepted terms; creates the ballot and payment records; and returns the private checkout URL plus public receipt ID.
- `GET /api/elections/{slug}/tally` returns counted totals only. `GET /api/receipt/{id}` returns poll, status, and time—never candidate or payment identifiers.
- `POST /api/paystack/webhook` validates and deduplicates Paystack events, then invokes the same settlement path used by reconciliation.
- Store product amount as `10,000` kobo independently from the Paystack total and fee. Use explicit states for pending, verifying, counted, refund pending, refunding, refunded, refund failed, and abandoned.
- A transfer account expires after one hour or at its provider-set limit, whichever comes first. Display its expiry. A late successful payment is refunded.

## Tests and launch gates

- Cover every state transition, duplicate webhooks/callbacks, concurrent settlement, invalid webhook signatures, card duplicates, missing card signatures, transfer completion/rejection/expiry, and payments that cross the poll close.
- Verify Paystack fee pass-through and amount semantics for local cards, international cards, and transfers. Check that duplicate, late, and mismatched payments request full refunds, and that the UI waits for Paystack’s processed status before claiming a refund is complete.
- Verify receipts and share metadata never reveal candidates; verify the 60-day detachment and one-year purge leave aggregate results and receipt status intact.
- Check the admin authorization and CSRF paths, backup restoration, 360×640 mobile layout, keyboard access, reduced motion, and tally accessibility.
- Do not open for paid polling until legal review, Paystack live approval, current roster verification, fee display, payment/refund rehearsals, admin controls, production domain, backups, and refund funds are ready. Feature freeze target: 23 Dec; otherwise move the entire window.

## Assumptions

- Use the stack and product guardrails already chosen in v2: no prizes, odds, or accuracy claims; optional state/country selection; candidate-blind receipts; live results.
- Keep the user-provided keys out of source and commands. Replace the exposed test secret before integration and store keys only in ignored local configuration or server secrets.
- The app requests refunds from Paystack and tracks outcomes; it never holds or transfers customer funds itself.
