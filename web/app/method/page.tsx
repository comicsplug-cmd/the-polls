import type { Metadata } from "next";

export const metadata: Metadata = { title: "Methodology", description: "How The Polls verifies payments, counts ballots, and describes the limits of a paid mock poll." };

export default function MethodPage() {
  return <article className="document-page wrap">
    <p className="eyebrow">THE PUBLIC METHOD · VERSION 1.0</p><h1>What the count can tell you.</h1>
    <p className="document-lead">The Polls is an independent, paid mock poll. It records the choices of people who choose to participate. It is not an official election, a probability sample, or a forecast.</p>
    <div className="document-meta"><span>Published 25 November 2026</span><span>Applies to the 2027 mock-poll window</span></div>
    <section><h2>Who may take part</h2><p>Anyone with access to an accepted Paystack payment method may submit a ballot while a listed poll is open. Participation is voluntary and self-selected. The results should not be read as representative of Nigeria or any state.</p></section>
    <section><h2>What makes a ballot count</h2><p>Paystack confirms each payment on the server. A ballot counts only for a successful NGN transaction matching the configured ₦100 base price and completed by the poll cutoff. The payment completion time reported by Paystack is the cutoff authority, so an on-time payment can count even if its webhook arrives later.</p><p>We use a card authorization signature to block a second ballot in the same election when Paystack supplies one. For transfers and cards without a signature, an election-scoped HMAC device fingerprint may flag abuse. These measures do not prove a person’s identity and <strong>cannot guarantee one person, one ballot.</strong></p></section>
    <section><h2>Payment and refunds</h2><p>The ballot base price is ₦100. Paystack’s fee is added to the customer’s total and shown before payment. A successful payment that is late, duplicated, mismatched, or otherwise unable to count is submitted for a full refund of the amount paid. Paystack processes the refund; the original processing fee is not returned. A refund is described as complete only after Paystack reports it processed.</p></section>
    <section><h2>Candidate records</h2><p>Candidate names and party affiliations are taken from the linked official INEC lists. A governorship poll is included only after its official list and internal roster QA are complete. Candidate lists can change; source links and roster status stay visible.</p></section>
    <section><h2>Live tallies and archive</h2><p>Results update after verified settlement. We publish counted ballot totals, not payment attempts. After the poll closes, the result remains in a permanent archive. Sixty days after close, candidate and location links are detached from individual ballot records while aggregate results remain.</p></section>
    <section><h2>What this poll does not claim</h2><p>We do not claim the result predicts an official election, reflects a representative sample, or measures the views of all Nigerians. The poll does not award prizes, calculate odds, or claim accuracy.</p></section>
    <p className="document-endnote">Questions about the method can be sent to the contact address published before the paid launch.</p>
  </article>;
}
