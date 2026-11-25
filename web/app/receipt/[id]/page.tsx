import type { Metadata } from "next";
import { ReceiptStatus } from "@/components/ReceiptStatus";
import { apiConfigured } from "@/lib/api";

export const dynamic = "force-dynamic";
export const metadata: Metadata = {
  title: "Receipt",
  description: "Candidate-blind receipt status for The Polls.",
  robots: { index: false, follow: false },
  openGraph: { title: "Receipt | The Polls", description: "Candidate-blind mock-poll receipt status." },
  twitter: { card: "summary", title: "Receipt | The Polls", description: "Candidate-blind mock-poll receipt status." },
};

export default async function ReceiptPage({ params }: { params: Promise<{ id: string }> }) {
  const { id } = await params;
  return <ReceiptStatus receiptId={id} previewOnly={!apiConfigured} />;
}
