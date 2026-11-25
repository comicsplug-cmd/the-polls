import Link from "next/link";

export function SiteFooter() {
  return (
    <footer className="site-footer">
      <div className="footer-brand">THE POLLS <span>·</span> A PUBLIC MOCK POLL</div>
      <p>Independent public opinion records. Not an official election, and not a scientific poll.</p>
      <nav aria-label="Footer navigation">
        <Link href="/method">Methodology</Link>
        <Link href="/privacy">Privacy</Link>
        <Link href="/terms">Terms</Link>
        <Link href="/admin">Admin</Link>
      </nav>
      <small>© 2026 The Polls · Nigeria</small>
    </footer>
  );
}
