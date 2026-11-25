"use client";

import { FormEvent, useState } from "react";

type Candidate = { id: number; party: string; name: string; running_mate?: string | null };
type Transfer = { bank?: string | null; account_name?: string | null; account_number?: string | null; expires_at?: string | null; message: string };
type Checkout = { receipt_id: string; channel: string; base_amount_kobo: number; requested_amount_kobo: number; estimated_fee_kobo: number; checkout_url?: string | null; transfer?: Transfer | null };

const states = [
  ["AB", "Abia"], ["AD", "Adamawa"], ["AK", "Akwa Ibom"], ["AN", "Anambra"], ["BA", "Bauchi"], ["BY", "Bayelsa"], ["BE", "Benue"], ["BO", "Borno"], ["CR", "Cross River"], ["DE", "Delta"], ["EB", "Ebonyi"], ["ED", "Edo"], ["EK", "Ekiti"], ["EN", "Enugu"], ["FC", "FCT"], ["GO", "Gombe"], ["IM", "Imo"], ["JI", "Jigawa"], ["KD", "Kaduna"], ["KN", "Kano"], ["KT", "Katsina"], ["KE", "Kebbi"], ["KO", "Kogi"], ["KW", "Kwara"], ["LA", "Lagos"], ["NA", "Nasarawa"], ["NI", "Niger"], ["OG", "Ogun"], ["ON", "Ondo"], ["OS", "Osun"], ["OY", "Oyo"], ["PL", "Plateau"], ["RI", "Rivers"], ["SO", "Sokoto"], ["TA", "Taraba"], ["YO", "Yobe"], ["ZA", "Zamfara"],
];

function naira(kobo: number) {
  return new Intl.NumberFormat("en-NG", { style: "currency", currency: "NGN", maximumFractionDigits: 2 }).format(kobo / 100);
}

function dateInNigeria(value: string) {
  return new Intl.DateTimeFormat("en-NG", { day: "numeric", month: "long", year: "numeric", timeZone: "Africa/Lagos" }).format(new Date(value));
}

function deviceToken() {
  const key = "the-polls-device-token";
  const generate = () => {
    try {
      return Array.from(crypto.getRandomValues(new Uint8Array(24)), (byte) => byte.toString(16).padStart(2, "0")).join("");
    } catch {
      // Non-secure contexts (plain http on a LAN address) have no Web Crypto.
      return Array.from({ length: 48 }, () => Math.floor(Math.random() * 16).toString(16)).join("");
    }
  };
  try {
    const existing = window.localStorage.getItem(key);
    if (existing) return existing;
    const value = generate();
    window.localStorage.setItem(key, value);
    return value;
  } catch {
    // Private modes can block localStorage. A per-submit token is still a
    // useful soft signal and never blocks checkout.
    return generate();
  }
}

export function VotePanel({ slug, candidates, isOpen, closed, openAt, included, termsVersion, demoMode }: { slug: string; candidates: Candidate[]; isOpen: boolean; closed: boolean; openAt: string; included: boolean; termsVersion: string; demoMode: boolean }) {
  const [party, setParty] = useState("");
  const [search, setSearch] = useState("");
  const [countryCode, setCountryCode] = useState("");
  const [stateCode, setStateCode] = useState("");
  const [channel, setChannel] = useState("card");
  const [accepted, setAccepted] = useState(false);
  const [checkout, setCheckout] = useState<Checkout | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const selected = candidates.find((candidate) => candidate.party === party);
  const query = search.trim().toLocaleLowerCase();
  const visibleCandidates = query ? candidates.filter((candidate) => `${candidate.name} ${candidate.party} ${candidate.running_mate || ""}`.toLocaleLowerCase().includes(query)) : candidates;

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError("");
    if (!isOpen) { setError(closed ? "This poll has closed." : "This poll is not accepting ballots yet."); return; }
    if (!party || !accepted) { setError("Choose a candidate and accept the terms to continue."); return; }
    setBusy(true);
    try {
      const response = await fetch("/api/vote/init", {
        method: "POST",
        headers: { "content-type": "application/json", "x-device-token": deviceToken() },
        body: JSON.stringify({ election: slug, candidate_party: party, country_code: countryCode || null, state_code: countryCode === "NG" ? stateCode || null : null, accepted_terms: true, terms_version: termsVersion, channel }),
      });
      const data = await response.json();
      if (!response.ok) throw new Error(data.error || "Could not start checkout.");
      if (data.checkout_url) { window.location.assign(data.checkout_url); return; }
      setCheckout(data);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Could not start checkout.");
    } finally { setBusy(false); }
  }

  if (checkout?.transfer) {
    const transfer = checkout.transfer;
    return <section className="transfer-card" aria-labelledby="transfer-title">
      <p className="eyebrow">TEMPORARY PAYSTACK ACCOUNT</p>
      <h2 id="transfer-title">Complete your transfer</h2>
      <p>Ballot {naira(checkout.base_amount_kobo)} · estimated fee {naira(checkout.estimated_fee_kobo)} · total {naira(checkout.requested_amount_kobo)}</p>
      <dl className="transfer-details">
        <div><dt>Bank</dt><dd>{transfer.bank || "Paystack Transfer"}</dd></div>
        <div><dt>Account name</dt><dd>{transfer.account_name || "Temporary Paystack account"}</dd></div>
        <div><dt>Account number</dt><dd className="account-number">{transfer.account_number || "Loading account details"}</dd></div>
        <div><dt>Expires</dt><dd>{transfer.expires_at ? new Date(transfer.expires_at).toLocaleString("en-NG", { timeZone: "Africa/Lagos" }) : "Within one hour"}</dd></div>
      </dl>
      <p>{transfer.message}</p>
      <p className="muted">Your receipt is <a href={`/receipt/${checkout.receipt_id}`}>{checkout.receipt_id}</a>. A transfer arriving after the temporary account expires will be refunded.</p>
      <a className="button button-dark" href={`/receipt/${checkout.receipt_id}`}>Check payment status</a>
    </section>;
  }

  const openDate = dateInNigeria(openAt);
  return <form className="vote-form" onSubmit={submit}>
    {!isOpen ? <div className="vote-preview-notice" role="status">
      <span className="preview-lock" aria-hidden="true">◷</span>
      <div><strong>{closed ? "This poll has closed" : included ? (demoMode ? "Test-mode ballot preview" : "Interactive ballot preview") : "This poll is not included yet"}</strong><span>{closed ? "Standings are now read-only and the final count stands. Receipts remain available." : included ? (demoMode ? "Payments use Paystack test keys only. Selecting a ticket and paying moves no real money." : `Voting is planned to open ${openDate}, if launch checks are complete. Selecting a ticket here does not submit or save a ballot.`) : "The roster and poll launch checks must be complete before voting can open."}</span></div>
    </div> : null}

    <div className="form-intro"><p className="eyebrow">STEP 1 OF 3</p><h2>Choose one ticket</h2><p>Select a candidate from the official source list. You can search by name or party.</p></div>
    {candidates.length ? <>
      {candidates.length > 8 ? <label className="candidate-search-label" htmlFor="candidate-search">Find a candidate or party
        <input id="candidate-search" type="search" value={search} onChange={(event) => setSearch(event.target.value)} placeholder="Search names or party codes" autoComplete="off" />
      </label> : null}
      <fieldset className="candidate-options">
        <legend className="candidate-legend">Candidate tickets <span>{visibleCandidates.length} of {candidates.length}</span></legend>
        {visibleCandidates.length ? visibleCandidates.map((candidate) => <label className={`candidate-option ${party === candidate.party ? "selected" : ""}`} key={candidate.party}>
          <input type="radio" name="candidate" value={candidate.party} checked={party === candidate.party} onChange={() => setParty(candidate.party)} />
          <span className="party-tag">{candidate.party}</span>
          <span className="candidate-option-name">{candidate.name}<small>Running mate: {candidate.running_mate || "Not listed"}</small></span>
          <span className="candidate-choice-mark" aria-hidden="true">{party === candidate.party ? "✓" : ""}</span>
        </label>) : <p className="no-candidates-match">No candidate matches “{search}”. Try a name or party code.</p>}
      </fieldset>
      {selected ? <p className="selected-ticket" role="status"><span aria-hidden="true">✓</span><span>Your selection</span><strong>{selected.name} <small>({selected.party})</small></strong></p> : null}
    </> : <p className="empty-candidate-list">Candidate names will appear here after the roster has been published and checked.</p>}

    {isOpen ? <>
      <div className="vote-extras">
        <div className="form-intro"><p className="eyebrow">STEP 2 OF 3</p><h3>Payment and location</h3><p>Location is optional. Paystack hosts the secure payment page.</p></div>
        <label className="field-label" htmlFor="country">Country <span>Optional</span></label>
        <select id="country" value={countryCode} onChange={(event) => { setCountryCode(event.target.value); setStateCode(""); }}>
          <option value="">Prefer not to say</option><option value="NG">Nigeria</option><option value="GH">Ghana</option><option value="KE">Kenya</option><option value="ZA">South Africa</option><option value="GB">United Kingdom</option><option value="US">United States</option>
        </select>
        {countryCode === "NG" ? <><label className="field-label state-select-label" htmlFor="state">State or territory <span>Optional</span></label>
          <select id="state" value={stateCode} onChange={(event) => setStateCode(event.target.value)}>
            <option value="">Prefer not to say</option>{states.map(([code, name]) => <option value={code} key={code}>{name}</option>)}
          </select></> : null}
        <fieldset className="channel-select">
          <legend className="field-label">Choose a payment method</legend>
          <label className={channel === "card" ? "selected" : ""}><input type="radio" name="channel" value="card" checked={channel === "card"} onChange={() => setChannel("card")} /> <strong>Card</strong><span>Secure Paystack checkout</span></label>
          <label className={channel === "transfer" ? "selected" : ""}><input type="radio" name="channel" value="transfer" checked={channel === "transfer"} onChange={() => setChannel("transfer")} /> <strong>Pay with Transfer</strong><span>One-time account, valid for one hour</span></label>
        </fieldset>
      </div>
      <div className="checkout-step"><p className="eyebrow">STEP 3 OF 3</p><h3>Review and continue</h3>
        <p className="price-note"><strong>₦100 base ballot price.</strong> Paystack’s fee is added to your total. The final amount is shown before you confirm payment.</p>
        <label className="terms-check"><input type="checkbox" checked={accepted} onChange={(event) => setAccepted(event.target.checked)} /> <span>I agree to the <a href="/terms" target="_blank" rel="noreferrer">terms</a> and understand this is a mock poll, not an official election.</span></label>
      </div>
    </> : <div className="locked-checkout"><p className="eyebrow">STEP 2 · PAYMENT</p><strong>Secure payment is locked until voting opens.</strong><span>When open, choose card or Pay with Transfer. The ₦100 base price and Paystack fee will be shown before payment.</span></div>}

    {error ? <p className="form-error" role="alert">{error}</p> : null}
    <button className="button button-dark full-width ballot-submit" disabled={busy || !party || candidates.length === 0 || !isOpen || !accepted}>{busy ? "Opening secure payment…" : isOpen ? `Continue to ${channel === "card" ? "card payment" : "transfer details"}` : closed ? "Poll closed" : `Voting opens ${openDate}`}</button>
    {demoMode ? <p className="demo-inline">Test mode · Paystack test keys · no real money is charged.</p> : null}
    <p className="privacy-inline">We do not ask for your name, phone number, NIN, or bank account details.</p>
  </form>;
}
