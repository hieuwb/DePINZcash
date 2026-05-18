/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_API_URL?: string;
  readonly VITE_ZCASH_NETWORK?: "mainnet" | "testnet";
  readonly VITE_SOLANA_CLUSTER?: "devnet" | "testnet" | "mainnet-beta";
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
