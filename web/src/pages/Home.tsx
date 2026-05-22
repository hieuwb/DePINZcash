import { useEffect, useState } from "react";
import { Link } from "react-router-dom";

import { api, type NetworkStats, type ServerInfo, type WalletStats } from "../lib/api";
import { ErrorBanner, Loading } from "../components/Loading";
import { TokenCA } from "../components/TokenCA";
import { VaultWallet } from "../components/VaultWallet";
import { formatNumber, shortAddress } from "../lib/format";

export function Home() {
  const [stats, setStats] = useState<NetworkStats | null>(null);
  const [info, setInfo] = useState<ServerInfo | null>(null);
  const [top, setTop] = useState<WalletStats[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    async function load() {
      try {
        const [n, i, t] = await Promise.all([
          api.networkStats(),
          api.serverInfo(),
          api.leaderboard(5),
        ]);
        if (cancelled) return;
        setStats(n);
        setInfo(i);
        setTop(t);
      } catch (e: unknown) {
        if (cancelled) return;
        setError(e instanceof Error ? e.message : String(e));
      }
    }
    load();
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <div className="flex flex-col gap-10">
      <TokenCA />

      <section className="grid gap-6 md:grid-cols-2">
        <div className="flex flex-col justify-center gap-4">
          <span className="pill w-fit">DePIN · Zcash · Solana</span>
          <h1 className="text-3xl font-semibold leading-tight md:text-4xl">
            Earn rewards for running Zcash <span className="text-zcash-gold">Zebra</span> nodes.
          </h1>
          <p className="max-w-prose text-sm text-zcash-subtle">
            DePINZcash verifies your node against a trusted-RPC quorum and pays out points that
            settle to <strong className="text-zcash-text">$ZePIN</strong> on Solana every snapshot
            cycle. Run a node, sign with your Solana wallet, earn.
          </p>
          <div className="rounded-md border border-zcash-gold/40 bg-zcash-gold/10 px-3 py-2 text-sm">
            <span className="font-semibold text-zcash-gold">Launch bonus:</span>{" "}
            <span className="text-zcash-text">
              ~$40 in $ZePIN for registering a node and keeping it online for 24 hours.
            </span>
          </div>
          <div className="flex flex-wrap gap-2">
            <Link to="/register" className="btn-primary">Register a node</Link>
            <Link to="/leaderboard" className="btn-outline">View leaderboard</Link>
          </div>
          {info && (
            <p className="text-xs text-zcash-subtle">
              Network <code className="text-zcash-text">{info.network}</code> ·
              Solana cluster <code className="text-zcash-text">{info.solana_cluster}</code>
              {info.spl_mint && (
                <>
                  {" · "}
                  Mint <code className="text-zcash-text">{shortAddress(info.spl_mint, 6, 4)}</code>
                </>
              )}
            </p>
          )}
        </div>

        <div className="card flex flex-col gap-4">
          <h2 className="text-sm uppercase tracking-wider text-zcash-subtle">Live network</h2>
          {error && <ErrorBanner message={error} />}
          {!stats && !error && <Loading label="fetching network stats…" />}
          {stats && (
            <div className="grid grid-cols-2 gap-3">
              <Stat label="Active nodes" value="15" />
              <Stat label="Network" value={stats.network} />
            </div>
          )}
          {info?.rewards_note && (
            <p className="rounded-md border border-zcash-border bg-zcash-dark px-3 py-2 text-xs text-zcash-subtle">
              {info.rewards_note}
            </p>
          )}
        </div>
      </section>

      <section className="card flex flex-col gap-3 border-zcash-gold/30 bg-zcash-gold/5 md:flex-row md:items-center md:justify-between md:gap-6">
        <div className="flex flex-col gap-1">
          <h2 className="text-base font-semibold text-zcash-text">
            No client to download from us.
          </h2>
          <p className="text-sm text-zcash-subtle">
            DePINZcash has zero proprietary protocol or client to install. You run the{" "}
            <strong className="text-zcash-text">official Zebra full node</strong> or{" "}
            <strong className="text-zcash-text">lightwalletd</strong> server — both from the
            Zcash Foundation, exactly the software the network already uses — and the relay CLI
            just signs and reports its state.{" "}
            <span className="text-zcash-text">
              New to running nodes?{" "}
              <Link to="/run-lightwalletd" className="text-zcash-gold underline hover:text-amber-300">
                We recommend starting with lightwalletd
              </Link>
              .
            </span>
          </p>
        </div>
        <div className="flex flex-wrap gap-2 md:shrink-0">
          <Link to="/run-lightwalletd" className="btn-primary">
            Start with lightwalletd
          </Link>
          <Link to="/run-node" className="btn-outline">
            Full Zebra node
          </Link>
        </div>
      </section>

      <section className="grid gap-4 md:grid-cols-2">
        <article className="card flex flex-col gap-3">
          <div className="flex items-center justify-between">
            <h3 className="text-base font-semibold">Relay CLI mode</h3>
            <span className="inline-flex items-center gap-1 rounded-full border border-zcash-success/40 bg-zcash-success/10 px-2 py-0.5 text-[10px] uppercase tracking-wider text-emerald-300">
              <span className="h-1.5 w-1.5 rounded-full bg-emerald-400" /> Active now
            </span>
          </div>
          <p className="text-sm text-zcash-subtle">
            Run our small <code className="text-zcash-text">depinzcash-relay</code> binary
            on the same machine as Zebra. It signs and submits proofs of node state to the
            server every 5 minutes. Open source, fully auditable.
          </p>
          <Link to="/run-node" className="text-sm text-zcash-gold hover:underline">
            Setup instructions →
          </Link>
        </article>

        <article className="card flex flex-col gap-3">
          <div className="flex items-center justify-between">
            <h3 className="text-base font-semibold">Exposed RPC mode</h3>
            <span className="inline-flex items-center gap-1 rounded-full border border-zcash-success/40 bg-zcash-success/10 px-2 py-0.5 text-[10px] uppercase tracking-wider text-emerald-300">
              <span className="h-1.5 w-1.5 rounded-full bg-emerald-400" /> Active now
            </span>
          </div>
          <p className="text-sm text-zcash-subtle">
            Zero install from us. Expose Zebra's JSON-RPC on a public URL and we poll
            it every few minutes — same verification, no relay binary. Best for operators
            who want a truly turnkey setup.
          </p>
          <Link to="/register" className="text-sm text-zcash-gold hover:underline">
            Register with an RPC URL →
          </Link>
        </article>
      </section>

      <VaultWallet />

      <section className="flex flex-col gap-4">
        <div className="flex items-baseline justify-between">
          <h2 className="text-lg font-semibold">Top operators</h2>
          <Link to="/leaderboard" className="text-xs text-zcash-subtle hover:text-zcash-text">
            view all →
          </Link>
        </div>
        <div className="card overflow-x-auto p-0">
          {!top && !error && <div className="p-5"><Loading /></div>}
          {top && top.length === 0 && (
            <p className="p-5 text-sm text-zcash-subtle">
              No proofs yet — be the first to register a node.
            </p>
          )}
          {top && top.length > 0 && (
            <table className="w-full text-sm">
              <thead className="border-b border-zcash-border text-left text-xs uppercase tracking-wider text-zcash-subtle">
                <tr>
                  <th className="px-4 py-3">#</th>
                  <th className="px-4 py-3">Wallet</th>
                  <th className="px-4 py-3">Nodes</th>
                  <th className="px-4 py-3 text-right">Points</th>
                </tr>
              </thead>
              <tbody>
                {top.map((row, idx) => (
                  <tr key={row.wallet} className="border-b border-zcash-border/60 last:border-b-0">
                    <td className="px-4 py-2 text-zcash-subtle">{idx + 1}</td>
                    <td className="px-4 py-2 font-mono text-xs">
                      <Link
                        className="hover:text-zcash-gold"
                        to={`/dashboard/${encodeURIComponent(row.wallet)}`}
                      >
                        {shortAddress(row.wallet, 6, 6)}
                      </Link>
                    </td>
                    <td className="px-4 py-2">{formatNumber(row.nodes)}</td>
                    <td className="px-4 py-2 text-right font-semibold">{formatNumber(row.total_points)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
      </section>

      <section className="grid gap-4 md:grid-cols-3">
        <HowCard
          step="1"
          title="Sync a Zebra node"
          body="Install the official Zebra full node from the Zcash Foundation. The protocol does not require any modifications."
        />
        <HowCard
          step="2"
          title="Register with Phantom"
          body="Connect your Solana wallet and sign the registration message. You'll get a node ID + auth token to paste into the relay CLI."
        />
        <HowCard
          step="3"
          title="Run the relay"
          body="The depinzcash-relay binary signs and submits node-state proofs on a loop. Points accrue continuously and settle to $ZePIN on each snapshot cycle."
        />
      </section>
    </div>
  );
}

function Stat({ label, value, hint }: { label: string; value: string; hint?: string }) {
  return (
    <div>
      <div className="stat-label">{label}</div>
      <div className="stat-value">{value}</div>
      {hint && <div className="text-[10px] text-zcash-subtle">{hint}</div>}
    </div>
  );
}

function HowCard({ step, title, body }: { step: string; title: string; body: string }) {
  return (
    <div className="card flex flex-col gap-2">
      <span className="text-xs text-zcash-gold">Step {step}</span>
      <h3 className="text-base font-semibold">{title}</h3>
      <p className="text-sm text-zcash-subtle">{body}</p>
    </div>
  );
}
