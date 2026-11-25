"use client";

import { FormEvent, useState } from "react";
import { useRouter } from "next/navigation";

function normalise(value: string) {
  const trimmed = value.trim();
  if (/^pr_[0-9a-fA-F]{32}$/.test(trimmed)) return trimmed.toLowerCase();
  const upper = trimmed.toUpperCase();
  if (/^TP26-[0-9A-F]{8}-[0-9A-F]{8}$/.test(upper)) return upper;
  return null;
}

export function FindReceipt() {
  const router = useRouter();
  const [value, setValue] = useState("");
  const [error, setError] = useState("");

  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const receipt = normalise(value);
    if (!receipt) {
      setError("Enter a receipt ID starting pr_, or a ballot code like TP26-1A2B3C4D-5E6F7A8B.");
      return;
    }
    setError("");
    router.push(`/receipt/${encodeURIComponent(receipt)}`);
  }

  return <form className="find-form" onSubmit={submit}>
    <label className="field-label" htmlFor="receipt-lookup">Receipt ID or ballot code</label>
    <div className="find-row">
      <input id="receipt-lookup" value={value} onChange={(event) => setValue(event.target.value)} placeholder="TP26-1A2B3C4D-5E6F7A8B" autoComplete="off" spellCheck={false} required />
      <button className="button button-dark">Find ballot</button>
    </div>
    {error ? <p className="form-error" role="alert">{error}</p> : null}
    <p className="muted">Both identifiers are candidate-blind. A ballot code is created only after Paystack confirms a successful payment.</p>
  </form>;
}
