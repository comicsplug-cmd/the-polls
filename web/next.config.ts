import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  // Emit a self-contained server bundle so the Docker image can run without
  // node_modules. Vercel ignores this and uses its own builder.
  output: "standalone",
  async rewrites() {
    const target = process.env.API_PROXY_TARGET || (process.env.VERCEL ? "" : "http://127.0.0.1:8080");
    return target ? [{ source: "/api/:path*", destination: `${target}/api/:path*` }] : [];
  },
  async headers() {
    return [
      {
        source: "/:path*",
        headers: [
          { key: "X-Content-Type-Options", value: "nosniff" },
          { key: "X-Frame-Options", value: "DENY" },
          { key: "Referrer-Policy", value: "no-referrer" },
          { key: "Permissions-Policy", value: "camera=(), microphone=(), geolocation=()" },
        ],
      },
    ];
  },
  poweredByHeader: false,
  reactStrictMode: true,
};

export default nextConfig;
