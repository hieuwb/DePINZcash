import { useEffect, useMemo, useState } from "react";
import { Link, useParams } from "react-router-dom";

import {
  api,
  type NodeDailyBucket,
  type ProofRecord,
  type PublicNode,
} from "../lib/api";
import { ErrorBanner, Loading } from "../components/Loading";
import { formatNumber, formatRelative, formatUptime, shortAddress } from "../lib/format";

export function NodeDetail() {
  const { id } = useParams();
  const [node, setNode] = useState<PublicNode | null>(null);
  const [proofs, setProofs] = useState<ProofRecord[] | null>(null);
  const [series, setSeries] = useState<NodeDailyBucket[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!id) return;
    let cancelled = false;
    setNode(null);
    setProofs(null);
    setSeries(null);
    setError(null);

    async function load() {
      try {
        const [n, p, s] = await Promise.all([
          api.node(id!),
          api.nodeProofs(id!, 100),
          api.nodeSeries(id!, 14),
        ]);
        if (cancelled) return;
        setNode(n);
        setProofs(p);
        setSeries(s);
      } catch (e: unknown) {
        if (cancelled) return;
        setError(e instanceof Error ? e.message : String(e));
      }
    }
    load();
    return () => {
      cancelled = true;
    };
  }, [id]);

  if (!id) return <ErrorBanner message="missing node id" />;
  if (error) return <ErrorBanner message={error} />;
  if (!node || !proofs || !series) return <Loading />;

  const accepted = proofs.filter((p) => p.verdict === "accepted").length;
  const lastProof = proofs[0] ?? null;

  return (
    <div className="flex flex-col gap-6">
      <header className="flex flex-col gap-2">
        <Link to={`/dashboard/${encodeURIComponent(node.wallet)}`} className="text-xs text-zcash-subtle hover:text-zcash-text">
          ← back to wallet dashboard
        </Link>
        <h1 className="text-2xl font-semibold">
          {node.label || <span className="text-zcash-subtle">unlabeled node</span>}
        </h1>
        <div className="flex flex-wrap items-center gap-2 text-xs text-zcash-subtle">
          <span className="pill">{node.kind}</span>
          <StatusBadge status={node.status} />
          <span>· registered {formatRelative(node.registered_at)}</span>
          <span>· network {node.network}</span>
        </div>
      </header>

      <section className="grid gap-3 md:grid-cols-4">
        <StatCard label="Points" value={formatNumber(node.points)} accent />
        <StatCard label="Uptime" value={formatUptime(node.uptime_seconds)} />
        <StatCard label="Accepted proofs" value={`${formatNumber(accepted)} / ${formatNumber(proofs.length)}`} />
        <StatCard label="Last proof" value={formatRelative(node.last_proof_at)} />
      </section>

      <section className="card flex flex-col gap-3">
        <div className="flex items-baseline justify-between">
          <h2 className="text-lg font-semibold">Last 14 days</h2>
          <span className="text-xs text-zcash-subtle">points per day</span>
        </div>
        {series.length === 0 ? (
          <p className="text-sm text-zcash-subtle">No activity in the window yet.</p>
        ) : (
          <PointsBarChart buckets={series} />
        )}
      </section>

      <section className="card flex flex-col gap-3">
        <h2 className="text-lg font-semibold">Node identity</h2>
        <div className="grid gap-3 md:grid-cols-2">
          <Kv label="Wallet" value={node.wallet} mono />
          <Kv label="Node id" value={node.id} mono />
          <Kv label="Last height" value={node.last_height != null ? formatNumber(node.last_height) : "—"} />
          <Kv
            label="Last block hash"
            value={node.last_block_hash ? shortAddress(node.last_block_hash, 8, 8) : "—"}
            mono
          />
          {lastProof && (
            <>
              <Kv label="Peers (last proof)" value={lastProof.peers != null ? formatNumber(lastProof.peers) : "—"} />
              <Kv
                label="Reported uptime (last proof)"
                value={lastProof.uptime_seconds != null ? formatUptime(lastProof.uptime_seconds) : "—"}
              />
            </>
          )}
        </div>
      </section>

      <section className="flex flex-col gap-3">
        <h2 className="text-lg font-semibold">Recent proofs</h2>
        {proofs.length === 0 ? (
          <div className="card text-sm text-zcash-subtle">
            No proofs submitted yet. Start the relay against this node to begin earning points.
          </div>
        ) : (
          <div className="card overflow-x-auto p-0">
            <table className="w-full text-sm">
              <thead className="border-b border-zcash-border text-left text-xs uppercase tracking-wider text-zcash-subtle">
                <tr>
                  <th className="px-4 py-3">Received</th>
                  <th className="px-4 py-3">Height</th>
                  <th className="px-4 py-3">Block hash</th>
                  <th className="px-4 py-3">Verdict</th>
                  <th className="px-4 py-3 text-right">Points</th>
                </tr>
              </thead>
              <tbody>
                {proofs.map((p) => (
                  <tr key={p.id} className="border-b border-zcash-border/60 last:border-b-0">
                    <td className="px-4 py-2 whitespace-nowrap">{formatRelative(p.received_at)}</td>
                    <td className="px-4 py-2">{formatNumber(p.claimed_height)}</td>
                    <td className="px-4 py-2 font-mono text-xs">{shortAddress(p.claimed_block_hash, 8, 6)}</td>
                    <td className="px-4 py-2">
                      <VerdictBadge verdict={p.verdict} reason={p.reject_reason} />
                    </td>
                    <td className="px-4 py-2 text-right font-semibold">{formatNumber(p.points_awarded)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>
    </div>
  );
}

function PointsBarChart({ buckets }: { buckets: NodeDailyBucket[] }) {
  const max = useMemo(() => Math.max(1, ...buckets.map((b) => b.points)), [buckets]);
  return (
    <div className="flex h-40 items-end gap-1">
      {buckets.map((b) => {
        const h = Math.max(4, Math.round((b.points / max) * 100));
        return (
          <div
            key={b.day}
            className="group flex flex-1 flex-col items-center gap-1"
            title={`${b.day} — ${b.points} pts (${b.accepted}/${b.proofs} accepted)`}
          >
            <div
              className="w-full rounded-t bg-zcash-gold/70 transition group-hover:bg-zcash-gold"
              style={{ height: `${h}%` }}
            />
            <span className="text-[10px] text-zcash-subtle">{b.day.slice(5)}</span>
          </div>
        );
      })}
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

function Kv({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="flex flex-col gap-0.5">
      <span className="stat-label">{label}</span>
      <span className={`${mono ? "break-all font-mono text-xs" : "text-sm"}`}>{value}</span>
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

function VerdictBadge({ verdict, reason }: { verdict: string; reason: string | null }) {
  const colour =
    verdict === "accepted"
      ? "border-zcash-success/40 bg-zcash-success/10 text-emerald-300"
      : verdict === "rejected"
        ? "border-zcash-danger/40 bg-zcash-danger/10 text-red-200"
        : "border-zcash-warn/40 bg-zcash-warn/10 text-amber-200";
  return (
    <span
      className={`inline-flex items-center rounded-full border px-2 py-0.5 text-[10px] uppercase tracking-wider ${colour}`}
      title={reason ?? undefined}
    >
      {verdict}
    </span>
  );
}
