import type { Metadata } from "next";
import Link from "next/link";

export const metadata: Metadata = { title: "Poll archive", description: "Permanent archive of The Polls results and methodologies." };

export default function ArchivePage() {
  return <section className="archive-page wrap">
    <p className="eyebrow">THE PERMANENT RECORD</p><h1>Poll archive</h1>
    <p className="document-lead">Closed polls, final tallies, and the methodology that applied at the time stay available here.</p>
    <div className="archive-empty"><span className="archive-number">2027 / 01</span><div><p className="eyebrow">UPCOMING · NOT YET CLOSED</p><h2>First public mock-poll record</h2><p>The archive will hold the final result once the target poll window closes. Results remain live while the poll is open.</p></div><Link href="/method">Read current method →</Link></div>
    <p className="muted archive-note">The archive is permanent. Individual candidate and location links are detached after poll close plus 60 days; the aggregate record remains.</p>
  </section>;
}
