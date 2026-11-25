import type { Metadata } from "next";
import Link from "next/link";
import { FindReceipt } from "@/components/FindReceipt";

export const metadata: Metadata = {
  title: "Find your ballot",
  description: "Look up a candidate-blind mock-poll receipt by receipt ID or ballot code.",
  robots: { index: false, follow: false },
};

export default function FindPage() {
  return <section className="receipt-page wrap">
    <p className="eyebrow">BALLOT LOOKUP</p>
    <h1>Find your ballot.</h1>
    <p className="receipt-copy">Enter the receipt ID from checkout, or the ballot code created when Paystack confirmed your payment. The page shows whether the ballot counted, never who it was for.</p>
    <FindReceipt />
    <p className="receipt-privacy">Lost both identifiers? We cannot recover them from a name, phone number, card, or bank account, because none of those are stored.</p>
    <Link href="/" className="text-link">← Back to The Polls</Link>
  </section>;
}
