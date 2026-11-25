import type { Metadata } from "next";
import Link from "next/link";
import { notFound } from "next/navigation";
import { Countdown } from "@/components/Countdown";
import { LiveTally } from "@/components/LiveTally";
import { VotePanel } from "@/components/VotePanel";
import { apiConfigured, getConfig, getElection, getTally, formatDate } from "@/lib/api";

type Candidate = { id: number; party: string; name: string; running_mate?: string | null };
type Election = { slug: string; title: string; region: string; election_date: string; open_at: string; close_at: string; server_now?: string; included: boolean; roster_status: string; source_url: string; source_label: string; is_open: boolean; demo_mode?: boolean; candidates: Candidate[] };

export const dynamic = "force-dynamic";

export async function generateMetadata({ params }: { params: Promise<{ slug: string }> }): Promise<Metadata> {
  const { slug } = await params;
  const election = await getElection(slug) as Election | null;
  return { title: election ? `${election.title} mock poll` : "Election poll", description: "Choose a candidate, follow public standings, and review the poll method. This is not an official election." };
}

export default async function ElectionPage({ params }: { params: Promise<{ slug: string }> }) {
  const { slug } = await params;
  const election = await getElection(slug) as Election | null;
  if (!election) notFound();
  const [tally, config] = await Promise.all([getTally(slug), getConfig()]);
  const serverNow = election.server_now || config.server_now;
  const closed = !election.is_open && new Date(serverNow).getTime() > new Date(election.close_at).getTime();
  const demoMode = election.demo_mode ?? config.demo_mode;
  const ballotStatus = election.is_open
    ? demoMode ? "TEST MODE · VOTING OPEN" : "VOTING OPEN"
    : closed ? "POLL CLOSED"
    : election.included && election.roster_status === "ready" ? "SCHEDULED"
    : "ROSTER QA PENDING";

  return <>
    <section className="election-mast wrap">
      <Link className="back-link" href="/#polls">← All polls</Link>
      {demoMode ? <p className="demo-banner" role="status"><strong>TEST MODE</strong> Paystack test keys only. No real money moves and no live result is published from this instance.</p> : null}
      <div className="election-title-row"><div><p className="eyebrow">MOCK POLL · {election.region.toUpperCase()}</p><h1>{election.title}</h1><p className="election-subtitle">Election scheduled for {formatDate(election.election_date)} · Poll closes {formatDate(election.close_at)}</p></div><span className={`status-pill ${election.is_open ? "status-open" : ""}`}><span className="status-dot" />{ballotStatus}</span></div>
      <div className="election-meta"><div><span>Poll clock</span><strong><Countdown openAt={election.open_at} closeAt={election.close_at} isOpen={election.is_open} serverNow={serverNow} demoMode={demoMode} /></strong><small>{demoMode ? "Schedule gates bypassed in test mode" : "Server-authoritative deadline"}</small></div><div><span>Poll window</span><strong>{formatDate(election.open_at)} — {formatDate(election.close_at)}</strong><small>Closes 23:59:59 West Africa Time</small></div><div><span>Candidate tickets</span><strong>{election.candidates.length ? `${election.candidates.length} tickets listed` : "Roster unpublished"}</strong><small>Names follow the linked INEC source, pending final QA</small></div><div><span>Official source</span><a href={election.source_url} target="_blank" rel="noreferrer">{election.source_label} ↗</a></div></div>
    </section>

    <div className="election-layout wrap">
      <section className="ballot-stage" aria-labelledby="ballot-heading">
        <div className="ballot-stage-heading"><p className="eyebrow">YOUR BALLOT</p><h2 id="ballot-heading">Choose one ticket</h2><p>Review the list, select one candidate, then continue to secure payment when voting opens.</p></div>
        <ol className="ballot-steps" aria-label="Voting steps"><li aria-current="step"><span>1</span>Choose a ticket</li><li><span>2</span>Pay securely</li><li><span>3</span>Track your receipt</li></ol>
        <VotePanel slug={slug} candidates={election.candidates} isOpen={election.is_open} closed={closed} openAt={election.open_at} included={election.included} termsVersion={config.terms_version} demoMode={demoMode} />
        <div className="small-print"><p className="eyebrow">BEFORE YOU TAKE PART</p><p>A ₦100 base ballot price applies. Paystack’s fee is added at checkout. No voter name, phone, NIN, or account number is collected.</p><Link href="/privacy">Read privacy details →</Link></div>
      </section>

      <aside className="standings-column" aria-label={`${election.title} standings`}>
        <LiveTally slug={slug} initial={tally} live={apiConfigured} />
        <div className="standings-note"><p className="eyebrow">HOW TO READ THIS</p><p>Standings include successfully verified ballots only. A tie is shown when candidates share the top count.</p><Link href="/method">Read the counting method →</Link></div>
      </aside>
    </div>

    <section className="candidate-record wrap" aria-labelledby="candidate-heading" id="candidate-record">
      <div className="section-heading"><div><p className="eyebrow">THE SOURCE RECORD</p><h2 id="candidate-heading">Candidate tickets</h2></div><span className="record-count">{election.candidates.length} RECORDS</span></div>
      <p className="source-note">{election.candidates.length ? "Names and party affiliations follow the linked INEC source list. Spellings and eligibility remain subject to final roster verification before paid polling opens." : "The candidate roster is withheld while the official INEC list is transcribed and checked. This race is excluded from paid polling until review is complete."}</p>
      {election.candidates.length ? <ol className="candidate-record-list">
        {election.candidates.map((candidate, index) => <li key={candidate.party}><span className="candidate-number">{String(index + 1).padStart(2, "0")}</span><span className="party-tag">{candidate.party}</span><span className="candidate-record-name">{candidate.name}<small>Running mate · {candidate.running_mate || "not listed"}</small></span><span className="candidate-arrow" aria-hidden="true">↗</span></li>)}
      </ol> : <div className="pending-roster">OFFICIAL SOURCE LINKED · QA NOT COMPLETE</div>}
    </section>
  </>;
}
