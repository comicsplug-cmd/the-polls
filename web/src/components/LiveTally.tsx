"use client";

import { useEffect, useState } from "react";

type Tally = {
  election: string;
  total_counted: number;
  updated_at: string;
  preview_only?: boolean;
  results: { party: string; name: string; running_mate?: string | null; total: number }[];
};

export function LiveTally({ slug, initial, live, compact = false }: { slug: string; initial: Tally | null; live: boolean; compact?: boolean }) {
  const [tally, setTally] = useState(initial);
  const [updated, setUpdated] = useState(false);
  const headingId = `tally-heading-${slug.replace(/[^a-z0-9-]/gi, "-")}`;

  useEffect(() => {
    if (!live) return;
    let stillMounted = true;
    const refresh = async () => {
      try {
        const response = await fetch(`/api/elections/${encodeURIComponent(slug)}/tally`, { cache: "no-store" });
        if (response.ok && stillMounted) {
          setTally(await response.json());
          setUpdated(true);
        }
      } catch { /* Keep the last server-rendered tally visible. */ }
    };
    const timer = window.setInterval(refresh, 30_000);
    return () => { stillMounted = false; window.clearInterval(timer); };
  }, [slug, live]);

  if (!tally) return <p className="muted">Standings are temporarily unavailable.</p>;

  const sorted = [...tally.results].sort((a, b) => b.total - a.total || a.party.localeCompare(b.party));
  const topTotal = sorted[0]?.total || 0;
  const leaders = topTotal > 0 ? sorted.filter((row) => row.total === topTotal) : [];
  const shown = compact ? sorted.slice(0, 4) : sorted;
  const hiddenCount = sorted.length - shown.length;

  return (
    <section className={`tally-section ${compact ? "tally-compact" : ""}`} aria-labelledby={headingId}>
      <div className="section-heading tally-heading">
        <div><p className="eyebrow">{compact ? "POLL STANDINGS" : "THE COUNT"}</p><h2 id={headingId}>{tally.preview_only ? "Preview standings" : compact ? "Who's leading?" : "Live standings"}</h2></div>
        <p className="count-total"><strong>{tally.total_counted.toLocaleString("en-NG")}</strong><span>counted ballots</span></p>
      </div>
      {tally.total_counted === 0 ? <p className="empty-tally">No ballots counted yet. Verified payments will appear here.</p> : null}
      {shown.length ? <ol className="tally-list" aria-label="Candidate standings">
        {shown.map((row) => {
          const share = tally.total_counted ? Math.round((row.total / tally.total_counted) * 100) : 0;
          const isLeader = leaders.some((leader) => leader.party === row.party);
          return (
            <li className={`tally-row ${isLeader ? "tally-row-leader" : ""}`} key={row.party}>
              <div className="tally-label">
                <span className="party-tag">{row.party}</span>
                <span className="candidate-tally-name">{row.name}{isLeader ? <small className="leader-mark">LEADING</small> : null}</span>
                <strong>{row.total.toLocaleString("en-NG")}</strong>
                {!compact ? <span className="tally-share">{share}%</span> : null}
              </div>
              <div className="bar-track" role="img" aria-label={`${row.name}, ${row.total} counted ballots, ${share} percent${isLeader ? ", currently leading" : ""}`}>
                <span className="bar-fill" style={{ width: `${share}%` }} />
              </div>
            </li>
          );
        })}
      </ol> : <p className="empty-tally">Candidate standings will appear once the roster is published.</p>}
      {compact && hiddenCount > 0 ? <p className="tally-more">Showing the top {shown.length} of {sorted.length} tickets</p> : null}
      <p className="tally-caption" aria-live="polite">{tally.preview_only ? "Read-only showcase · no payments or live counts" : updated ? "Updated moments ago" : "Server-rendered count · refreshes every 30 seconds"}</p>
    </section>
  );
}
