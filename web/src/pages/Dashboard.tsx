import { useEffect, useMemo, useState } from "react";
import { useParams, useNavigate, Link } from "react-router-dom";
import { useWallet } from "@solana/wallet-adapter-react";
import { WalletMultiButton } from "@solana/wallet-adapter-react-ui";

import {
  ApiError,
  api,
  type ClaimPayload,
  type PublicNode,
  type WalletStats,
} from "../lib/api";
import { ErrorBanner, Loading } from "../components/Loading";
import { formatNumber, formatRelative, formatUptime, shortAddress } from "../lib/format";

export function Dashboard() {
  const wallet = useWallet();
  const { wallet: walletParam } = useParams();
  const navigate = useNavigate();
  const [manual, setManual] = useState("");

  const target = useMemo(() => {
    if (walletParam) return walletParam;
    if (wallet.publicKey) return wallet.publicKey.toBase58();
    return null;
  }, [walletParam, wallet.publicKey]);

  return (
    <div className="flex flex-col gap-6">
      <header className="flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
        <div>
          <h1 className="text-2xl font-semibold">Dashboard</h1>
          <p className="text-sm text-zcash-subtle">
            Per-wallet view of nodes, points, and the latest Merkle claim payload.
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          {!target && <WalletMultiButton />}
          <form
            className="flex gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              if (manual.trim()) navigate(`/dashboard/${encodeURIComponent(manual.trim())}`);
            }}
          >
            <input
              className="input w-72"
              value={manual}
              onChange={(e) => setManual(e.target.value)}
              placeholder="lookup by wallet pubkey…"
            />
            <button type="submit" className="btn-outline">Go</button>
          </form>
        </div>
      </header>

      {!target ? (
        <div className="card text-sm text-zcash-subtle">
          Connect a wallet or paste a Solana pubkey to look up its dashboard.
        </div>
      ) : (
        <WalletDashboard wallet={target} />
      )}
    </div>
  );
}

function WalletDashboard({ wallet }: { wallet: string }) {
  const [stats, setStats] = useState<WalletStats | null>(null);
  const [nodes, setNodes] = useState<PublicNode[] | null>(null);
  const [claim, setClaim] = useState<ClaimPayload | null>(null);
  const [claimError, setClaimError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setStats(null);
    setNodes(null);
    setClaim(null);
    setClaimError(null);
    setError(null);

    async function load() {
      try {
        const [s, n] = await Promise.all([api.walletStats(wallet), api.walletNodes(wallet)]);
        if (cancelled) return;
        setStats(s);
        setNodes(n);
      } catch (e: unknown) {
        if (cancelled) return;
        setError(e instanceof Error ? e.message : String(e));
      }
      try {
        const c = await api.latestClaim(wallet);
        if (cancelled) return;
        setClaim(c);
      } catch (e: unknown) {
        if (cancelled) return;
        if (e instanceof ApiError && e.status === 404) {
          setClaimError(null); // expected — no snapshot for this wallet yet
        } else {
          setClaimError(e instanceof Error ? e.message : String(e));
        }
      }
    }
    load();
    return () => {
      cancelled = true;
    };
  }, [wallet]);

  return (
    <div className="flex flex-col gap-6">
      <section className="card">
        <div className="flex flex-wrap items-center gap-3">
          <span className="stat-label">Wallet</span>
          <code className="break-all font-mono text-xs">{wallet}</code>
        </div>
      </section>

      {error && <ErrorBanner message={error} />}

      <section className="grid gap-3 md:grid-cols-4">
        <StatCard label="Nodes" value={stats ? formatNumber(stats.nodes) : "—"} />
        <StatCard label="Points" value={stats ? formatNumber(stats.total_points) : "—"} accent />
        <StatCard label="Total uptime" value={stats ? formatUptime(stats.total_uptime_seconds) : "—"} />
        <StatCard label="Last seen" value={stats ? formatRelative(stats.last_seen) : "—"} />
      </section>

      <section className="flex flex-col gap-3">
        <h2 className="text-lg font-semibold">Nodes</h2>
        {!nodes && !error && <Loading />}
        {nodes && nodes.length === 0 && (
          <div className="card text-sm text-zcash-subtle">
            No nodes registered for this wallet. <Link to="/register" className="text-zcash-gold">Register one →</Link>
          </div>
        )}
        {nodes && nodes.length > 0 && (
          <div className="card overflow-x-auto p-0">
            <table className="w-full text-sm">
              <thead className="border-b border-zcash-border text-left text-xs uppercase tracking-wider text-zcash-subtle">
                <tr>
                  <th className="px-4 py-3">Label</th>
                  <th className="px-4 py-3">Kind</th>
                  <th className="px-4 py-3">Status</th>
                  <th className="px-4 py-3">Last height</th>
                  <th className="px-4 py-3">Last proof</th>
                  <th className="px-4 py-3 text-right">Points</th>
                </tr>
              </thead>
              <tbody>
                {nodes.map((n) => (
                  <tr key={n.id} className="border-b border-zcash-border/60 last:border-b-0">
                    <td className="px-4 py-2">{n.label || <span className="text-zcash-subtle">—</span>}</td>
                    <td className="px-4 py-2">
                      <span className="pill">{n.kind}</span>
                    </td>
                    <td className="px-4 py-2">
                      <StatusBadge status={n.status} />
                    </td>
                    <td className="px-4 py-2">{n.last_height != null ? formatNumber(n.last_height) : "—"}</td>
                    <td className="px-4 py-2">{formatRelative(n.last_proof_at)}</td>
                    <td className="px-4 py-2 text-right font-semibold">{formatNumber(n.points)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>

      <section className="flex flex-col gap-3">
        <h2 className="text-lg font-semibold">Latest claim</h2>
        {!claim && !claimError && (
          <div className="card text-sm text-zcash-subtle">
            No published snapshot includes this wallet yet — claims appear after the next snapshot cycle
            (default cadence: weekly).
          </div>
        )}
        {claimError && <ErrorBanner message={claimError} />}
        {claim && (
          <div className="card flex flex-col gap-3 text-sm">
            <div className="grid gap-3 md:grid-cols-3">
              <Kv label="Cycle" value={`#${claim.cycle}`} />
              <Kv label="Points credited" value={formatNumber(claim.points)} accent />
              <Kv label="$ZePIN mint" value={shortAddress(claim.spl_mint, 6, 4) || "unset"} />
              <Kv label="Cluster" value={claim.solana_cluster} />
              <Kv label="Merkle root" value={shortAddress(claim.merkle_root, 8, 8)} mono />
              <Kv label="Leaf hash" value={shortAddress(claim.leaf_hash, 8, 8)} mono />
            </div>
            <details className="rounded-md border border-zcash-border bg-zcash-dark p-3 text-xs">
              <summary className="cursor-pointer text-zcash-subtle">
                Full Merkle proof JSON (paste into the Solana claim program once it ships)
              </summary>
              <pre className="mt-2 overflow-x-auto font-mono leading-5">{JSON.stringify(claim, null, 2)}</pre>
            </details>
          </div>
        )}
      </section>
    </div>
  );
}

function StatCard({ label, value, accent }: { label: string; value: string; accent?: boolean }) {
  return (
    <div className="card">
      <div className="stat-label">{label}</div>
      <div className={`stat-value ${accent ? "text-zcash-gold" : ""}`}>{value}</div>
    </div>
  );
}

function Kv({ label, value, accent, mono }: { label: string; value: string; accent?: boolean; mono?: boolean }) {
  return (
    <div className="flex flex-col gap-0.5">
      <span className="stat-label">{label}</span>
      <span className={`${mono ? "font-mono text-xs" : "text-sm"} ${accent ? "text-zcash-gold font-semibold" : ""}`}>
        {value}
      </span>
    </div>
  );
}

function StatusBadge({ status }: { status: string }) {
  const colour =
    status === "active"
      ? "border-zcash-success/40 bg-zcash-success/10 text-emerald-300"
      : status === "stale"
        ? "border-zcash-warn/40 bg-zcash-warn/10 text-amber-200"
        : status === "suspended"
          ? "border-zcash-danger/40 bg-zcash-danger/10 text-red-200"
          : "border-zcash-border bg-zcash-surface text-zcash-subtle";
  return (
    <span className={`inline-flex items-center rounded-full border px-2 py-0.5 text-[10px] uppercase tracking-wider ${colour}`}>
      {status}
    </span>
  );
}
