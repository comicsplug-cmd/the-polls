"use client";

import { useEffect, useState } from "react";

type Props = {
  openAt: string;
  closeAt: string;
  isOpen: boolean;
  serverNow: string;
  demoMode?: boolean;
};

function parts(ms: number) {
  const total = Math.max(0, Math.floor(ms / 1000));
  const days = Math.floor(total / 86_400);
  const hours = Math.floor((total % 86_400) / 3_600);
  const minutes = Math.floor((total % 3_600) / 60);
  const seconds = total % 60;
  return { days, hours, minutes, seconds };
}

function format(ms: number) {
  const { days, hours, minutes, seconds } = parts(ms);
  const clock = `${String(hours).padStart(2, "0")}:${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
  return days > 0 ? `${days}d ${clock}` : clock;
}

/**
 * Purely cosmetic countdown. The server re-checks the window at settlement, so
 * a wrong phone clock can never admit a late ballot.
 */
export function Countdown({ openAt, closeAt, isOpen, serverNow, demoMode = false }: Props) {
  const [clock, setClock] = useState<{ now: number; offset: number } | null>(null);

  useEffect(() => {
    const server = new Date(serverNow).getTime();
    const offset = Number.isFinite(server) ? server - Date.now() : 0;
    const tick = () => setClock({ now: Date.now(), offset });
    // Schedule the first tick outside the effect body to avoid a cascading render.
    const frame = window.requestAnimationFrame(tick);
    const timer = window.setInterval(tick, 1_000);
    return () => {
      window.cancelAnimationFrame(frame);
      window.clearInterval(timer);
    };
  }, [serverNow]);

  if (demoMode) {
    return <span className="countdown countdown-demo">TEST WINDOW · NO REAL MONEY</span>;
  }

  const open = new Date(openAt).getTime();
  const close = new Date(closeAt).getTime();

  if (clock === null) {
    return <span className="countdown">Checking poll clock…</span>;
  }

  const serverTime = clock.now + clock.offset;
  if (serverTime < open) {
    return <span className="countdown">Opens in {format(open - serverTime)}</span>;
  }
  if (!isOpen || serverTime > close) {
    return <span className="countdown countdown-closed">Poll closed</span>;
  }
  const remaining = close - serverTime;
  return (
    <span className={`countdown ${remaining < 3_600_000 ? "countdown-soon" : ""}`}>
      Closes in {format(remaining)}
    </span>
  );
}
