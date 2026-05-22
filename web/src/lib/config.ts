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
  solanaCluster: envStr(import.meta.env.VITE_SOLANA_CLUSTER, "mainnet-beta") as
    | "devnet"
    | "testnet"
    | "mainnet-beta",
  // Public treasury wallet that pays operator rewards in $ZePIN.
  // Override via VITE_VAULT_WALLET if it needs to move.
  vaultWallet: envStr(
    import.meta.env.VITE_VAULT_WALLET,
    "pG814gCczXTcswtE2UzZp2TJTqVvVVdumZ85UUDEp1n",
  ),
  vaultLabel: envStr(import.meta.env.VITE_VAULT_LABEL, "Treasury wallet ($ZePIN payouts)"),
  // $ZePIN SPL token mint on Solana mainnet. Override via VITE_TOKEN_MINT.
  tokenMint: envStr(
    import.meta.env.VITE_TOKEN_MINT,
    "61uaHBdWUnnYB6aseptCRthwFmeXmTkE3GSZ9smzcash",
  ),
  // Browser-friendly Solana mainnet RPC for treasury balance lookups.
  // The default `api.mainnet-beta.solana.com` blocks browser CORS with 403.
  // publicnode is free + CORS-allowed; for production traffic switch to a
  // Helius/QuickNode endpoint via VITE_SOLANA_RPC_URL.
  solanaRpcUrl: envStr(
    import.meta.env.VITE_SOLANA_RPC_URL,
    "https://solana-rpc.publicnode.com",
  ),
  // Set this to the deployed zepin-claim program id to enable the on-chain
  // claim button. While empty, the UI shows "claim program ships soon" and
  // exposes the Merkle proof JSON for manual / future redemption.
  claimProgramId: envStr(import.meta.env.VITE_CLAIM_PROGRAM_ID, ""),
};

export function isClaimProgramLive(): boolean {
  return config.claimProgramId.length > 0;
}
