import type { Metadata } from "next";
import { AdminPanel } from "@/components/AdminPanel";
import { apiConfigured } from "@/lib/api";

export const metadata: Metadata = { title: "Admin operations", robots: { index: false, follow: false } };

export default function AdminPage() { return <AdminPanel previewOnly={!apiConfigured} />; }
