// Vite exposes env vars prefixed with VITE_. Treat empty strings as missing,
// since vercel env paste-in can produce ""s that ?? doesn't catch.
function envStr(value: string | undefined, fallback: string): string {
  const v = (value ?? "").trim();
  return v === "" ? fallback : v;
}

const PROD_API = "https://depinzcash-server.fly.dev";
const DEV_API = "http://localhost:3000";

function defaultApi(): string {
  if (typeof window === "undefined") return PROD_API;
  return window.location.hostname === "localhost" || window.location.hostname === "127.0.0.1"
    ? DEV_API
    : PROD_API;
}

export const config = {
  apiUrl: envStr(import.meta.env.VITE_API_URL, defaultApi()),
  network: envStr(import.meta.env.VITE_ZCASH_NETWORK, "mainnet") as "mainnet" | "testnet",
  solanaCluster: envStr(import.meta.env.VITE_SOLANA_CLUSTER, "devnet") as
    | "devnet"
    | "testnet"
    | "mainnet-beta",
};
