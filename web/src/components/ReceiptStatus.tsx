"use client";

import Link from "next/link";
import { useEffect, useState } from "react";

type Receipt = { receipt_id: string; ballot_code?: string | null; poll: string; status: string; created_at: string; counted_at?: string | null };
const labels: Record<string, { title: string; text: string }> = {
  payment_pending: { title: "Payment not confirmed yet", text: "We are waiting for Paystack to confirm your payment. This page updates automatically." },
  counted: { title: "Ballot counted", text: "Your verified ballot is included in the live tally." },
  refund_in_progress: { title: "Refund in progress", text: "This payment could not be counted. A full refund request has been sent to Paystack." },
  refunded: { title: "Refund processed", text: "Paystack has confirmed that the refund was processed." },
  refund_needs_attention: { title: "Refund needs attention", text: "The refund request needs follow-up with Paystack. The support team has been alerted." },
  not_counted: { title: "Ballot not counted", text: "This receipt is not counted. If a payment was completed, a refund request will be tracked here." },
};

function humanStatus(value?: string | null) {
  if (!value) return "Checking";
  const words = value.replaceAll("_", " ");
  return words.charAt(0).toUpperCase() + words.slice(1);
}

export function ReceiptStatus({ receiptId, previewOnly = false }: { receiptId: string; previewOnly?: boolean }) {
  const [receipt, setReceipt] = useState<Receipt | null>(null);
  const [problem, setProblem] = useState<"missing" | "invalid" | null>(null);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    if (previewOnly) return;
    let live = true;
    const refresh = async () => {
      if (!live) return;
      try {
        const response = await fetch(`/api/receipt/${encodeURIComponent(receiptId)}`, { cache: "no-store" });
        if (!live) return;
        if (response.ok) {
          const next = await response.json() as Receipt;
          setReceipt(next);
          setProblem(null);
          setLoaded(true);
          // Terminal states never change; stop fetching and let the page settle.
          if (["counted", "refunded", "refund_needs_attention", "not_counted"].includes(next.status)) {
            live = false;
          }
        } else if (response.status === 404) {
          setProblem("missing");
          setLoaded(true);
          live = false;
        } else if (response.status === 400) {
          // A malformed identifier can never resolve; stop retrying.
          setProblem("invalid");
          setLoaded(true);
          live = false;
        }
      } catch { /* Retry on the next refresh. */ }
    };
    void refresh();
    const timer = window.setInterval(refresh, 8_000);
    return () => { live = false; window.clearInterval(timer); };
  }, [receiptId, previewOnly]);

  if (previewOnly) return <section className="receipt-page wrap"><p className="eyebrow">READ-ONLY SHOWCASE</p><h1>Receipts are unavailable in this preview.</h1><p className="receipt-copy">Candidate-blind receipt status is provided by the private API and is not connected to this showcase deployment.</p><Link href="/">Back to The Polls</Link></section>;

  if (problem) return <section className="receipt-page wrap">
    <p className="eyebrow">{problem === "invalid" ? "RECEIPT ID NOT RECOGNISED" : "RECEIPT NOT FOUND"}</p>
    <h1>{problem === "invalid" ? "That code does not look right." : "We could not find this receipt."}</h1>
    <p className="receipt-copy">{problem === "invalid"
      ? "A receipt ID starts with pr_ followed by 32 hexadecimal characters. A ballot code looks like TP26-1A2B3C4D-5E6F7A8B, and is created only after Paystack confirms a payment."
      : "No ballot matches that identifier. If you paid by transfer, the receipt is ready as soon as checkout creates it — check the link you were shown before paying."}</p>
    <div className="receipt-actions"><Link href="/find" className="button button-dark">Try another code</Link><Link href="/" className="text-link">Back to The Polls</Link></div>
  </section>;

  if (!loaded) return <section className="receipt-page wrap">
    <p className="eyebrow">PRIVATE RECEIPT · CANDIDATE BLIND</p>
    <span className="receipt-icon" aria-hidden="true">…</span>
    <h1>Looking up your receipt.</h1>
    <p className="receipt-copy">Checking the candidate-blind status of this ballot with the poll server. This page updates automatically.</p>
  </section>;

  const status = receipt ? labels[receipt.status] || labels.not_counted : labels.payment_pending;
  return <section className="receipt-page wrap">
    <p className="eyebrow">PRIVATE RECEIPT · CANDIDATE BLIND</p>
    <span className={`receipt-icon ${receipt?.status === "counted" ? "receipt-icon-good" : ""}`} aria-hidden="true">{receipt?.status === "counted" ? "✓" : "…"}</span>
    <h1>{status.title}</h1>
    <p className="receipt-copy">{status.text}</p>
    <dl className="receipt-details">
      <div><dt>Poll</dt><dd>{receipt?.poll || "Loading poll"}</dd></div>
      <div><dt>Receipt ID</dt><dd className="receipt-id">{receipt?.receipt_id || receiptId}</dd></div>
      {receipt?.ballot_code ? <div><dt>Ballot code</dt><dd className="receipt-id">{receipt.ballot_code}</dd></div> : null}
      <div><dt>Status</dt><dd>{humanStatus(receipt?.status)}</dd></div>
      {receipt?.counted_at ? <div><dt>Counted at</dt><dd>{new Date(receipt.counted_at).toLocaleString("en-NG", { timeZone: "Africa/Lagos" })}</dd></div> : null}
    </dl>
    <p className="receipt-privacy">This receipt does not store or display your candidate choice or payment reference. Sharing this page will not reveal either. The ballot code is derived from your verified Paystack transaction and can be used to find this page again.</p>
    <div className="receipt-actions"><Link href="/" className="button button-outline">Back to The Polls</Link><Link href="/find" className="text-link">Find another receipt →</Link></div>
  </section>;
}
