import { previewElections, previewTally } from "@/lib/preview-data";

export const serverApi = process.env.API_SERVER_BASE || (process.env.VERCEL ? "" : "http://127.0.0.1:8080/api");
export const apiConfigured = Boolean(serverApi);

export type PublicConfig = {
  terms_version: string;
  base_amount_kobo: number;
  base_amount_ngn: number;
  demo_mode: boolean;
  mode: "test" | "live";
  payments_ready: boolean;
  server_now: string;
};

export const PREVIEW_TERMS_VERSION = "2026-11-25-draft";

export async function getConfig(): Promise<PublicConfig> {
  if (serverApi) {
    try {
      const response = await fetch(`${serverApi}/config`, { cache: "no-store" });
      if (response.ok) return await response.json();
    } catch { /* Fall through to a locked preview configuration. */ }
  }
  return {
    terms_version: PREVIEW_TERMS_VERSION,
    base_amount_kobo: 10_000,
    base_amount_ngn: 100,
    demo_mode: false,
    mode: "test",
    payments_ready: false,
    server_now: new Date().toISOString(),
  };
}

export async function getElection(slug: string) {
  if (serverApi) {
    try {
      const response = await fetch(`${serverApi}/elections/${encodeURIComponent(slug)}`, { cache: "no-store" });
      if (response.ok) return await response.json();
    } catch { /* Keep the read-only showcase available if the API is offline. */ }
  }
  return previewElections.find((election) => election.slug === slug) || null;
}

export async function getElections() {
  if (serverApi) {
    try {
      const response = await fetch(`${serverApi}/elections`, { cache: "no-store" });
      if (response.ok) return await response.json();
    } catch { /* Use the read-only showcase data while the API is unavailable. */ }
  }
  return previewElections;
}

export async function getTally(slug: string) {
  if (serverApi) {
    try {
      const response = await fetch(`${serverApi}/elections/${encodeURIComponent(slug)}/tally`, { cache: "no-store" });
      if (response.ok) return await response.json();
    } catch { /* Fall through to a clearly labelled zero-count showcase tally. */ }
  }
  return previewTally(slug);
}

export function formatDate(value: string) {
  return new Intl.DateTimeFormat("en-NG", {
    day: "numeric",
    month: "long",
    year: "numeric",
    timeZone: "Africa/Lagos",
  }).format(new Date(value));
}
