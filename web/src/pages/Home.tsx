import { useEffect, useState } from "react";
import { Link } from "react-router-dom";

import { api, type NetworkStats, type ServerInfo, type WalletStats } from "../lib/api";
import { ErrorBanner, Loading } from "../components/Loading";
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
              <Stat label="Total nodes" value={formatNumber(stats.total_nodes)} />
              <Stat label="Active nodes" value={formatNumber(stats.active_nodes)} />
              <Stat label="Accepted proofs" value={formatNumber(stats.accepted_proofs)} />
              <Stat label="Total points" value={formatNumber(stats.total_points)} />
              <Stat
                label="Trusted tip"
                value={
                  stats.trusted_tip_height != null
                    ? formatNumber(stats.trusted_tip_height)
                    : "—"
                }
                hint="from RPC quorum"
              />
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
            <strong className="text-zcash-text">official Zebra full node</strong> from the
            Zcash Foundation — exactly the same software the network already uses — and the
            relay CLI just signs and reports its state.
          </p>
        </div>
        <div className="flex flex-wrap gap-2 md:shrink-0">
          <a
            href="https://github.com/ZcashFoundation/zebra"
            target="_blank"
            rel="noreferrer"
            className="btn-primary"
          >
            Download Zebra ↗
          </a>
          <Link to="/run-node" className="btn-outline">
            Setup guide
          </Link>
        </div>
      </section>

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
