import { useMemo, type PropsWithChildren } from "react";
import { ConnectionProvider, WalletProvider as SolanaWalletProvider } from "@solana/wallet-adapter-react";
import { WalletModalProvider } from "@solana/wallet-adapter-react-ui";
import {
  PhantomWalletAdapter,
  SolflareWalletAdapter,
} from "@solana/wallet-adapter-wallets";
import { clusterApiUrl } from "@solana/web3.js";

import { config } from "./config";

// Wraps the standard Solana wallet-adapter providers. RPC endpoint is only used
// for our future Solana program calls (claim flow). Server-side stats are
// fetched via plain HTTP — see lib/api.ts.
export function WalletProvider({ children }: PropsWithChildren) {
  const endpoint = useMemo(() => {
    switch (config.solanaCluster) {
      case "mainnet-beta":
        return clusterApiUrl("mainnet-beta");
      case "testnet":
        return clusterApiUrl("testnet");
      case "devnet":
      default:
        return clusterApiUrl("devnet");
    }
  }, []);

  const wallets = useMemo(
    () => [new PhantomWalletAdapter(), new SolflareWalletAdapter()],
    [],
  );

  return (
    <ConnectionProvider endpoint={endpoint}>
      <SolanaWalletProvider wallets={wallets} autoConnect>
        <WalletModalProvider>{children}</WalletModalProvider>
      </SolanaWalletProvider>
    </ConnectionProvider>
  );
}
