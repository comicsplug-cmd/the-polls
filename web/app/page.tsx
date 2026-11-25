import Link from "next/link";
import { LiveTally } from "@/components/LiveTally";
import { apiConfigured, formatDate, getConfig, getElections, getTally } from "@/lib/api";

type Election = {
  slug: string;
  title: string;
  region: string;
  election_date: string;
  open_at: string;
  close_at: string;
  included: boolean;
  roster_status: string;
  source_url: string;
  candidates: { party: string }[];
  is_open: boolean;
};

function statusLabel(election: Election) {
  if (!election.included || election.roster_status !== "ready") return "ROSTER CHECK IN PROGRESS";
  return election.is_open ? "OPEN · VOTE NOW" : "SCHEDULED POLL";
}

export default async function HomePage() {
  const [pollsRaw, config] = await Promise.all([getElections(), getConfig()]);
  const polls = pollsRaw as Election[];
  const tallies = await Promise.all(polls.map((poll) => getTally(poll.slug)));
  const featuredPoll = polls.find((poll) => poll.included) || polls[0];

  return <>
    {config.demo_mode ? <div className="demo-banner wrap" role="status"><strong>TEST MODE</strong> Paystack test keys only. You can walk the full ballot → payment → receipt loop, but no real money moves and these counts are not a live result.</div> : null}
    <section className="hero wrap">
      <div className="hero-copy">
        <p className="eyebrow"><span className="live-dot" /> 2027 · NIGERIA</p>
        <h1>Cast your ballot.<br />Follow the <em>count.</em></h1>
        <p className="hero-deck">Choose an election, select one candidate, and follow the public standings. The Polls is an independent mock poll, not an official election.</p>
        <div className="hero-actions">
          {featuredPoll ? <Link className="button button-dark" href={`/election/${featuredPoll.slug}`}>View the ballot <span aria-hidden="true">→</span></Link> : <a className="button button-dark" href="#polls">Choose a poll <span aria-hidden="true">↓</span></a>}
          <Link className="text-link" href="/method">How the count works <span aria-hidden="true">↗</span></Link>
        </div>
      </div>
      <aside className="hero-note" aria-label="Planned poll dates">
        <span className="note-index">2026 / 27</span>
        <div className="note-rule" />
        <p className="eyebrow">PLANNED POLL WINDOW</p>
        <strong>26 Dec —<br />04 Jan</strong>
        <span className="note-year">WEST AFRICA TIME</span>
        <p className="note-copy">Ballots open only after roster, payment, legal, and launch checks are complete.</p>
      </aside>
      <div className="hero-side-label" aria-hidden="true">RECORD · 001</div>
    </section>

    <section className="trust-strip" aria-label="How participation works">
      <div><span>01</span>Choose an election</div><div><span>02</span>Pick one candidate</div><div><span>03</span>Pay securely when open</div><div><span>04</span>Follow the live count</div>
    </section>

    <section className="poll-section wrap" id="polls">
      <div className="section-heading"><div><p className="eyebrow">THE 2027 RECORD</p><h2>Choose a poll</h2></div><p className="heading-note">Open a ballot to review the candidate list, make a selection, and see who is leading. Only races with a complete, checked INEC roster can accept ballots.</p></div>
      <div className="poll-grid">
        {polls.map((poll, index) => <article className={`poll-card ${index === 0 ? "poll-card-main" : ""}`} key={poll.slug}>
          <div className="poll-card-top"><span className="record-number">POLL / {String(index + 1).padStart(2, "0")}</span><span className={`status-pill ${poll.is_open ? "status-open" : ""}`}><span className="status-dot" />{statusLabel(poll)}</span></div>
          <p className="poll-region">{poll.region.toUpperCase()} <span>·</span> ELECTION {formatDate(poll.election_date).toUpperCase()}</p>
          <h3>{poll.title}</h3>
          <p className="poll-card-info">{poll.candidates.length ? `${poll.candidates.length} candidate tickets in the current source list` : "Candidate list is being prepared for publication"}</p>
          <LiveTally slug={poll.slug} initial={tallies[index]} live={apiConfigured} compact />
          <div className="poll-card-footer"><a href={poll.source_url} target="_blank" rel="noreferrer">INEC source <span aria-hidden="true">↗</span></a><Link href={`/election/${poll.slug}`}>{poll.is_open ? "Vote now" : "Open ballot"} <span aria-hidden="true">→</span></Link></div>
        </article>)}
        <article className="soon-card"><span className="soon-mark" aria-hidden="true">＋</span><p className="eyebrow">GOVERNORSHIP POLLS</p><p>More state polls will appear after official INEC candidate lists are complete and checked.</p><span className="soon-caption">No National or State Assembly polls</span></article>
      </div>
    </section>

    <section className="manifesto-wrap">
      <div className="manifesto wrap"><p className="eyebrow">A NOTE ON THE NUMBERS</p><p className="manifesto-quote">“This is a mock poll.<br /><em>It is not the ballot box.</em>”</p><div className="manifesto-bottom"><p>Paid participation is a signal from a self-selected group. It is not representative by design, and it cannot guarantee one person, one ballot. The method and limits stay visible beside every result.</p><Link className="text-link" href="/method">See methodology <span aria-hidden="true">↗</span></Link></div></div>
    </section>

    <section className="archive-promo wrap"><div><p className="eyebrow">PERMANENT RECORD</p><h2>Every poll stays findable.</h2></div><p>When a poll closes, its final result and methodology move into the archive. The public record remains available after ballot details are detached.</p><Link className="button button-outline" href="/archive">Visit the archive <span aria-hidden="true">→</span></Link></section>
  </>;
}
