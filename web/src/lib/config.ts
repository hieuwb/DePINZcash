// Vite exposes env vars prefixed with VITE_. We don't fall back to mock data —
// if these are misconfigured the UI surfaces the actual error from the API.
export const config = {
  apiUrl: import.meta.env.VITE_API_URL ?? "http://localhost:3000",
  network: (import.meta.env.VITE_ZCASH_NETWORK ?? "mainnet") as "mainnet" | "testnet",
  solanaCluster: (import.meta.env.VITE_SOLANA_CLUSTER ?? "devnet") as
    | "devnet"
    | "testnet"
    | "mainnet-beta",
};
