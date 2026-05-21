import { useState } from "react";

import { config } from "../lib/config";

// Public contract address banner for $ZePIN. Sits at the top of the home page
// so anyone landing can grab the CA and verify on-chain.
export function TokenCA() {
  const [copied, setCopied] = useState(false);
  const ca = config.tokenMint;

  async function copy() {
    try {
      await navigator.clipboard.writeText(ca);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      // best-effort fallback
      const ta = document.createElement("textarea");
      ta.value = ca;
      document.body.appendChild(ta);
      ta.select();
      document.execCommand("copy");
      document.body.removeChild(ta);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    }
  }

  return (
    <section className="card flex flex-col gap-2 border-zcash-gold/40 bg-zcash-gold/5">
      <div className="flex items-center gap-2">
        <span className="text-xs uppercase tracking-wider text-zcash-gold">
          $ZePIN contract address
        </span>
        <span className="pill">Solana · mainnet</span>
      </div>
      <button
        type="button"
        onClick={copy}
        title={copied ? "copied!" : "click to copy"}
        className="group flex flex-wrap items-center gap-2 break-all rounded-md border border-transparent bg-transparent p-0 text-left font-mono text-sm text-zcash-text transition-colors hover:text-zcash-gold md:text-base"
      >
        <span>{ca}</span>
        <span
          className={`shrink-0 text-[10px] uppercase tracking-wider transition-opacity ${
            copied ? "text-emerald-300 opacity-100" : "text-zcash-subtle opacity-60 group-hover:opacity-100"
          }`}
        >
          {copied ? "copied!" : "click to copy"}
        </span>
      </button>
    </section>
  );
}
