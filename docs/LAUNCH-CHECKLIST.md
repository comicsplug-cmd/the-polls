# Paid poll launch checklist

**Target window:** 26 December 2026, 00:00:00 WAT to 4 January 2027, 23:59:59 WAT.  
**Feature freeze target:** 23 December 2026.  
**Rule:** if a gate misses, shift the whole poll window and publish the replacement dates before taking any payment.

The API and website remain in preview until each applicable item is signed off. `PAID_POLLING_ENABLED`, `PAYSTACK_FEE_PASS_THROUGH_CONFIRMED`, and every launch-gate flag in `.env.example` default to `false`. The API independently refuses checkout unless all gates are true, terms are no longer marked draft, and the server has a live-mode Paystack secret. The flags record completed operations; they do not perform or verify those reviews themselves. `DEMO_MODE` is the single test-only exception: it accepts an `sk_test_` key and bypasses roster and schedule gates so the payment loop can be rehearsed. It must be `false` in production and can never run with an `sk_live_` key.

## Before feature freeze

- [ ] Legal review approves the paid mock-poll model, privacy notice, versioned terms, refund language, operator identity, and support route.
- [ ] Confirm the 16 January 2027 presidential and 6 February 2027 governorship election dates against the current INEC calendar.
- [ ] Recheck the presidential roster against the latest INEC PDF and resolve court-order or candidate status changes.
- [ ] Select the governorship subset. Include only states whose official lists are published, transcribed, independently checked, and QA-signed. Label all omitted races.
- [ ] Confirm no National Assembly or State Assembly poll appears in the roster.
- [ ] Choose the permanent domain; configure DNS, TLS, callback URL, and Paystack webhook URL.
- [ ] Replace any test credentials that were shared or exposed. Keep live keys only in root-owned server secrets.
- [ ] Obtain Paystack live approval and confirm temporary Pay with Transfer is enabled on the account. Do not use Dedicated Virtual Accounts.
- [ ] Run a full test-mode rehearsal for local card, international card (including American Express if enabled), and transfer. Capture Paystack's checkout total, verify response fields, actual amount, fee, refund amount, and webhook event for each flow.
- [ ] Confirm that the hosted card checkout passes the correct fees and the Charge API transfer amount nets the ₦100 base after fees. Adjust the transfer fee formula if provider pricing or rounding differs.
- [ ] Test underpayment, overpayment, duplicate card signature, missing signature, late payment, expired transfer, bad currency, invalid webhook signature, duplicate webhook, and concurrent settlement. Verify full refund behavior in the Paystack dashboard.
- [ ] Verify the ballot code is created only after verification, `/find` resolves both the receipt ID and the ballot code, and neither the receipt page nor its JSON reveals the candidate or payment reference.
- [ ] Verify rate limits on vote init, admin login, and the payment callback; confirm `429` responses do not block legitimate receipt polling.
- [ ] Verify `/api/health` reports `degraded` without returning `503`, and `/api/health/ready` is the only readiness probe.
- [ ] Keep a funded reserve for full refunds; Paystack does not return the original processing fee.
- [ ] Verify admin password hash, secure cookies, CSRF rejection, force-close, reference lookup, refund status reconciliation, retry rules, and audit entries.
- [ ] Enable off-host encrypted backups, perform a restore rehearsal, and set up uptime/payment/refund alerts.
- [ ] Check keyboard navigation, reduced motion, screen-reader tally labels, and 360×640 mobile layout.

## Final opening check

- [ ] Roster freeze and source links reviewed by two people; publish the included and missing governorship states.
- [ ] Terms/privacy/legal sign-off complete and published with version numbers.
- [ ] Domain and HTTPS verified; webhook signature rehearsal succeeds on the production endpoint.
- [ ] Paystack fee totals and full refunds rehearsed on the account that will take payment.
- [ ] Admin, refund reserve, backups, alerts, and on-call contact are ready.
- [ ] Publish legally approved, versioned non-draft terms.
- [ ] Set `COOKIE_SECURE=true`; after every sign-off, configure the matching launch-gate flags, production secret values, paid mode, and fee-pass-through confirmation.
- [ ] Verify the homepage shows the correct poll window and every election reports open before announcing checkout.

## Date and roster sources

- INEC election calendar: <https://inecnigeria.org/elections/calendar>
- INEC presidential candidate list: <https://www.inecnigeria.org/press/updated-list-of-candidates-for-2027-presidential-election>
- INEC governorship candidate lists: <https://www.inecnigeria.org/press/updated-final-list-of-candidates-for-2027-governorship-election>

