import Link from "next/link";

export function SiteHeader() {
  return (
    <header className="site-header">
      <Link className="brand" href="/" aria-label="The Polls home">
        <span className="brand-mark" aria-hidden="true">P</span>
        <span>THE POLLS<span className="brand-sub">CIVIC RECORD · NIGERIA</span></span>
      </Link>
      <nav aria-label="Main navigation" className="main-nav">
        <Link href="/#polls">Polls</Link>
        <Link href="/method">Method</Link>
        <Link href="/find">Find receipt</Link>
        <Link href="/archive">Archive</Link>
      </nav>
    </header>
  );
}
