import type { Metadata } from "next";

export const metadata: Metadata = { title: "Terms", description: "Terms for taking part in The Polls mock poll." };

export default function TermsPage() {
  return <article className="document-page wrap">
    <p className="eyebrow">TERMS · DRAFT VERSION 2026-11-25</p><h1>Taking part in a mock poll.</h1>
    <p className="document-lead">By submitting a ballot, you agree to these terms. This version is a launch draft and requires legal review before paid polling.</p>
    <div className="document-meta"><span>Draft version 2026-11-25</span><span>Proposed poll window · 26 Dec 2026 – 4 Jan 2027 WAT</span></div>
    <section><h2>Nature of the service</h2><p>The Polls runs independent paid mock polls. It is not operated by INEC and is not an official election, scientific poll, or prediction. No prizes, odds, or accuracy claims are offered.</p></section>
    <section><h2>Price and payment</h2><p>The base price for one ballot in one election is ₦100. Paystack adds its processing fee to your total. The final total must be shown before you authorize payment. Each election is a separate ballot. You must not use the service to disrupt the poll or submit automated transactions.</p></section>
    <section><h2>Counting and refunds</h2><p>A ballot counts after the server verifies a successful NGN payment and confirms that Paystack completed it by the election cutoff. A successful payment that cannot count will be submitted to Paystack for a full refund of the amount actually paid. Paystack processes refunds; processing fees are not returned. A refund is complete only after Paystack confirms its processed status.</p></section>
    <section><h2>Limits</h2><p>We use card signatures where provided and soft abuse signals in other cases, but cannot guarantee one person, one ballot. Results reflect participants, not all voters. Polls may be moved as a whole if launch gates are missed; any new dates will be published before payment opens.</p></section>
    <section><h2>Privacy and contact</h2><p>See the privacy notice for collection, retention, and deletion details. The final terms will include the operator’s legal name and contact details before launch.</p></section>
  </article>;
}
