"use client";

import { FormEvent, useState } from "react";

type Refund = { receipt_id: string; election: string; status: string; actual_amount_kobo?: number | null; refund_status?: string | null; refund_reference?: string | null };
type Lookup = { receipt_id: string; poll: string; status: string; base_amount_kobo?: number; actual_amount_kobo?: number | null; refund_status?: string | null };
type RiskSignal = { poll: string; repeated_device_groups: number; ballots_with_repeats: number };
type OverviewElection = { slug: string; title: string; open_at: string; close_at: string; included: number; roster_status: string; force_closed_at?: string | null; counted: number; awaiting_payment: number; refunds: number };
type Overview = { mode: string; demo_mode: boolean; payments_ready: boolean; launch_gates_complete: boolean; terms_version: string; elections: OverviewElection[] };

export function AdminPanel({ previewOnly = false }: { previewOnly?: boolean }) {
  const [csrf, setCsrf] = useState("");
  const [signedIn, setSignedIn] = useState(false);
  const [username, setUsername] = useState("admin");
  const [password, setPassword] = useState("");
  const [reference, setReference] = useState("");
  const [lookup, setLookup] = useState<Lookup | null>(null);
  const [refunds, setRefunds] = useState<Refund[]>([]);
  const [signals, setSignals] = useState<RiskSignal[]>([]);
  const [overview, setOverview] = useState<Overview | null>(null);
  const [notice, setNotice] = useState("");
  const [error, setError] = useState("");

  async function call(path: string, body?: unknown) {
    const response = await fetch(`/api/admin/${path}`, {
      method: body ? "POST" : "GET",
      credentials: "include",
      headers: { ...(body ? { "content-type": "application/json" } : {}), ...(csrf ? { "x-csrf-token": csrf } : {}) },
      ...(body ? { body: JSON.stringify(body) } : {}),
    });
    const data = await response.json();
    if (!response.ok) throw new Error(data.error || "Admin request failed");
    return data;
  }

  async function load() {
    try {
      const [refundData, signalData, overviewData] = await Promise.all([call("refunds"), call("risk-signals"), call("overview")]);
      setRefunds(refundData.refunds || []); setSignals(signalData.signals || []); setOverview(overviewData); setSignedIn(true);
    }
    catch { setSignedIn(false); }
  }

  async function login(event: FormEvent) {
    event.preventDefault(); setError(""); setNotice("");
    try {
      const response = await fetch("/api/admin/login", { method: "POST", credentials: "include", headers: { "content-type": "application/json" }, body: JSON.stringify({ username, password }) });
      const data = await response.json();
      if (!response.ok) throw new Error(data.error || "Login failed");
      setCsrf(data.csrf_token); setPassword(""); setSignedIn(true); setNotice("Signed in to the restricted admin console.");
      const [refundsResponse, signalsResponse, overviewResponse] = await Promise.all([
        fetch("/api/admin/refunds", { credentials: "include" }),
        fetch("/api/admin/risk-signals", { credentials: "include" }),
        fetch("/api/admin/overview", { credentials: "include" }),
      ]);
      if (refundsResponse.ok) setRefunds((await refundsResponse.json()).refunds || []);
      if (signalsResponse.ok) setSignals((await signalsResponse.json()).signals || []);
      if (overviewResponse.ok) setOverview(await overviewResponse.json());
    } catch (cause) { setError(cause instanceof Error ? cause.message : "Login failed"); }
  }

  async function findPayment(event: FormEvent) {
    event.preventDefault(); setError(""); setNotice(""); setLookup(null);
    try { const data = await call("lookup", { reference }); setLookup(data); }
    catch (cause) { setError(cause instanceof Error ? cause.message : "Lookup failed"); }
  }

  async function refund(receipt_id: string) {
    setError(""); setNotice("");
    try { const data = await call("refund", { receipt_id }); setNotice(data.message); await load(); }
    catch (cause) { setError(cause instanceof Error ? cause.message : "Refund request failed"); }
  }

  async function checkRefund(receipt_id: string) {
    setError(""); setNotice("");
    try { const data = await call("refund-check", { receipt_id }); setNotice(data.message); await load(); }
    catch (cause) { setError(cause instanceof Error ? cause.message : "Could not check refund"); }
  }

  async function forceClose() {
    const election = window.prompt("Enter an election slug to force-close, for example president-2027:");
    if (!election) return;
    setError(""); setNotice("");
    try { await call("force-close", { election }); setNotice(`${election} was closed. This action is recorded in the audit trail.`); }
    catch (cause) { setError(cause instanceof Error ? cause.message : "Could not close poll"); }
  }

  async function logout() {
    try { await call("logout", {}); } catch { /* Clear local admin view even if the network drops. */ }
    setSignedIn(false); setCsrf(""); setRefunds([]); setSignals([]); setLookup(null);
  }

  if (previewOnly) return <section className="admin-page wrap"><p className="eyebrow">READ-ONLY SHOWCASE</p><h1>Operations console</h1><p className="admin-intro">Admin tools are disabled in this Vercel showcase. Payment operations remain on the private API host.</p></section>;

  return <section className="admin-page wrap">
    <p className="eyebrow">RESTRICTED AREA · OPERATOR ACCESS</p><h1>Operations console</h1>
    <p className="admin-intro">Look up a private payment reference, review refunds needing attention, or force-close a poll. Candidate records cannot be edited here.</p>
    {error ? <p className="form-error" role="alert">{error}</p> : null}
    {notice ? <p className="success-note" role="status">{notice}</p> : null}
    {!signedIn ? <form className="admin-login" onSubmit={login}>
      <label className="field-label" htmlFor="admin-user">Username</label><input id="admin-user" autoComplete="username" value={username} onChange={(e) => setUsername(e.target.value)} required />
      <label className="field-label" htmlFor="admin-password">Password</label><input id="admin-password" type="password" autoComplete="current-password" value={password} onChange={(e) => setPassword(e.target.value)} required />
      <button className="button button-dark">Sign in</button>
      <p className="muted">Access is configured on the server. Passwords are stored as Argon2 hashes.</p>
    </form> : <>
      <div className="admin-toolbar"><span><span className="status-dot" /> Authenticated session · expires after 8 hours</span><button className="button button-outline" onClick={logout}>Sign out</button></div>
      <section className="admin-overview">
        <div className="section-heading"><div><p className="eyebrow">SYSTEM STATE</p><h2>Poll readiness</h2></div>
          <div className="admin-chips">
            <span className={overview?.demo_mode ? "admin-chip admin-chip-test" : "admin-chip"}>{overview?.mode === "live" ? "LIVE MODE" : "TEST MODE"}</span>
            <span className={overview?.payments_ready ? "admin-chip admin-chip-good" : "admin-chip"}>{overview?.payments_ready ? "CHECKOUT READY" : "CHECKOUT LOCKED"}</span>
            <span className={overview?.launch_gates_complete ? "admin-chip admin-chip-good" : "admin-chip"}>{overview?.launch_gates_complete ? "ALL GATES SIGNED OFF" : "GATES INCOMPLETE"}</span>
          </div>
        </div>
        <p>Terms version <code>{overview?.terms_version || "loading"}</code>. Test mode accepts Paystack test keys only and bypasses roster and schedule gates for rehearsal.</p>
        {overview?.elections.length ? <div className="admin-election-list">{overview.elections.map((election) => <article key={election.slug}>
          <div><strong>{election.title}</strong><span>{election.slug} · roster {election.roster_status.replaceAll("_", " ")}</span></div>
          <div><span>Counted</span><strong>{election.counted.toLocaleString("en-NG")}</strong></div>
          <div><span>Awaiting payment</span><strong>{election.awaiting_payment.toLocaleString("en-NG")}</strong></div>
          <div><span>Refunds</span><strong>{election.refunds.toLocaleString("en-NG")}</strong></div>
          <span className="admin-chip">{election.force_closed_at ? "FORCE-CLOSED" : election.included ? "INCLUDED" : "NOT INCLUDED"}</span>
        </article>)}</div> : <p className="empty-tally">No elections are seeded.</p>}
      </section>
      <div className="admin-grid">
        <section className="admin-card"><p className="eyebrow">PAYMENT SUPPORT</p><h2>Reference lookup</h2><form className="lookup-form" onSubmit={findPayment}><label className="sr-only" htmlFor="payment-reference">Private Paystack reference</label><input id="payment-reference" value={reference} onChange={(e) => setReference(e.target.value)} placeholder="Private Paystack reference" required /><button className="button button-dark">Look up</button></form>
          {lookup ? <dl className="admin-result"><div><dt>Receipt</dt><dd>{lookup.receipt_id}</dd></div><div><dt>Poll</dt><dd>{lookup.poll}</dd></div><div><dt>Ballot status</dt><dd>{lookup.status}</dd></div><div><dt>Base / paid</dt><dd>₦{((lookup.base_amount_kobo || 0) / 100).toFixed(2)} / {lookup.actual_amount_kobo ? `₦${(lookup.actual_amount_kobo / 100).toFixed(2)}` : "pending"}</dd></div><div><dt>Refund</dt><dd>{lookup.refund_status || "—"}</dd></div></dl> : null}
        </section>
        <section className="admin-card"><p className="eyebrow">POLL CONTROL</p><h2>Force-close</h2><p>Stops new ballots immediately. Payments are still settled against their verified completion time.</p><button className="button button-outline" onClick={forceClose}>Close an election</button><p className="muted">Every close is written to the audit trail.</p></section>
      </div>
      <section className="admin-refunds"><div className="section-heading"><div><p className="eyebrow">FOLLOW-UP REQUIRED</p><h2>Refunds</h2></div><button className="button button-outline" onClick={() => void load()}>Refresh list</button></div>
        {refunds.length === 0 ? <p className="empty-tally">No refund requests need attention.</p> : <div className="refund-list">{refunds.map((item) => <article className="refund-row" key={item.receipt_id}><div><strong>{item.election}</strong><span>{item.receipt_id}</span></div><div><span>{item.status.replaceAll("_", " ")}</span><small>{item.refund_status || "Awaiting provider"}</small></div><strong>{item.actual_amount_kobo ? `₦${(item.actual_amount_kobo / 100).toFixed(2)}` : "Amount unavailable"}</strong><div className="refund-actions"><button className="button button-outline" onClick={() => void checkRefund(item.receipt_id)}>Check status</button><button className="button button-dark" disabled={!['failed','rejected'].includes(item.refund_status || "")} onClick={() => void refund(item.receipt_id)}>Retry refund</button></div></article>)}</div>}
      </section>
      <section className="admin-signals"><div className="section-heading"><div><p className="eyebrow">SOFT ABUSE SIGNALS</p><h2>Repeated device tokens</h2></div><button className="button button-outline" onClick={() => void load()}>Refresh</button></div>
        <p>Election-scoped HMAC token repeats only. These counts do not identify a person and never block a ballot.</p>
        {signals.length === 0 ? <p className="empty-tally">No repeated device tokens recorded.</p> : <ul>{signals.map((signal) => <li key={signal.poll}><strong>{signal.poll}</strong><span>{signal.repeated_device_groups} repeated groups · {signal.ballots_with_repeats} ballots</span></li>)}</ul>}
      </section>
    </>}
  </section>;
}
