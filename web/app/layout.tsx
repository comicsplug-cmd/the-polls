import type { Metadata, Viewport } from "next";
import { SiteFooter } from "@/components/SiteFooter";
import { SiteHeader } from "@/components/SiteHeader";
import "./globals.css";

export const metadata: Metadata = {
  title: { default: "The Polls — Civic Record", template: "%s | The Polls" },
  description: "A public record of Nigerian mock polls. Independent, candidate records sourced from INEC, and transparent about the method.",
  openGraph: { title: "The Polls — Civic Record", description: "Independent Nigerian mock polls and public results.", type: "website" },
  twitter: { card: "summary", title: "The Polls — Civic Record", description: "Independent Nigerian mock polls and public results." },
};

export const viewport: Viewport = { width: "device-width", initialScale: 1, themeColor: "#f3f0e8" };

export default function RootLayout({ children }: Readonly<{ children: React.ReactNode }>) {
  return <html lang="en"><body><div className="site-shell"><SiteHeader /><main>{children}</main><SiteFooter /></div></body></html>;
}
