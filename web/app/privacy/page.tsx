import type { Metadata } from "next";

export const metadata: Metadata = { title: "Privacy", description: "What The Polls collects, why it is linked, and when it is detached or deleted." };

export default function PrivacyPage() {
  return <article className="document-page wrap">
    <p className="eyebrow">PRIVACY · VERSION 1.0</p><h1>Less data, for less time.</h1>
    <p className="document-lead">The Polls does not ask for a voter’s name, phone number, NIN, or bank account. Payment processing happens through Paystack. Data linked to a ballot is treated as sensitive until it is detached or purged.</p>
    <div className="document-meta"><span>Published 25 November 2026</span><span>Last reviewed before launch</span></div>
    <section><h2>Data we keep</h2><p>We keep a random receipt identifier, the selected poll and candidate, optional state or country selection, ballot status, payment base amount, actual amount paid, provider fee, payment reference, and limited fraud-prevention fingerprints. We record acceptance of the current terms. We send Paystack a synthetic email address made from a random payment reference; we do not send candidate choice or location in payment metadata.</p></section>
    <section><h2>Payment provider</h2><p>Paystack processes the payment and may collect information required by its payment methods. The Polls does not receive or store card numbers, bank credentials, account numbers, or payer contact details from that checkout.</p></section>
    <section><h2>Retention schedule</h2><ul><li>Candidate and optional location links are kept through poll close plus 60 days, then detached from individual ballot records.</li><li>Minimal payment-to-receipt support references are kept for up to one year after poll close to resolve refunds and disputes.</li><li>At one year, payment references, provider transaction IDs, card and device hashes, webhook event keys, and terms-acceptance timestamps are purged. Receipt status and aggregate results remain.</li></ul></section>
    <section><h2>Receipt privacy</h2><p>A receipt page shows the poll, status, and time only. It never shows the candidate selected or payment identifiers. Its title and share metadata are also candidate-blind.</p></section>
    <section><h2>Security</h2><p>Payment events are verified against the raw webhook body and deduplicated. Raw webhook payloads and payer details are not stored. The restricted admin console uses password hashing, secure session cookies, CSRF checks, and an audit trail. Production backups must be encrypted and access-controlled.</p></section>
    <p className="document-endnote">This draft privacy notice requires legal review before paid polling opens. The final notice must identify the data controller and support contact.</p>
  </article>;
}
