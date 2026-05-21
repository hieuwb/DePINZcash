import { useEffect, useState } from "react";
import { Link } from "react-router-dom";

import { api, type WalletStats } from "../lib/api";
import { ErrorBanner, Loading } from "../components/Loading";
import { formatNumber, formatRelative, formatUptime, shortAddress } from "../lib/format";

export function Leaderboard() {
  const [rows, setRows] = useState<WalletStats[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    async function load() {
      try {
        const r = await api.leaderboard(200);
        if (!cancelled) setRows(r);
      } catch (e: unknown) {
        if (!cancelled) setError(e instanceof Error ? e.message : String(e));
      }
    }
    load();
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <div className="flex flex-col gap-4">
      <header>
        <h1 className="text-2xl font-semibold">Leaderboard</h1>
        <p className="text-sm text-zcash-subtle">
          Ranked by lifetime points. Points settle to $ZePIN on each snapshot cycle.
        </p>
      </header>
      {error && <ErrorBanner message={error} />}
      {!rows && !error && <Loading />}
      {rows && rows.length === 0 && (
        <div className="card text-sm text-zcash-subtle">No operators yet — be the first.</div>
      )}
      {rows && rows.length > 0 && (
        <div className="card overflow-x-auto p-0">
          <table className="w-full text-sm">
            <thead className="border-b border-zcash-border text-left text-xs uppercase tracking-wider text-zcash-subtle">
              <tr>
                <th className="px-4 py-3">#</th>
                <th className="px-4 py-3">Wallet</th>
                <th className="px-4 py-3">Nodes</th>
                <th className="px-4 py-3">Uptime</th>
                <th className="px-4 py-3">Last seen</th>
                <th className="px-4 py-3 text-right">Points</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((row, idx) => (
                <tr key={row.wallet} className="border-b border-zcash-border/60 last:border-b-0">
                  <td className="px-4 py-2 text-zcash-subtle">{idx + 1}</td>
                  <td className="px-4 py-2 font-mono text-xs">
                    <Link
                      className="hover:text-zcash-gold"
                      to={`/dashboard/${encodeURIComponent(row.wallet)}`}
                    >
                      {shortAddress(row.wallet, 8, 8)}
                    </Link>
                  </td>
                  <td className="px-4 py-2">{formatNumber(row.nodes)}</td>
                  <td className="px-4 py-2">{formatUptime(row.total_uptime_seconds)}</td>
                  <td className="px-4 py-2">{formatRelative(row.last_seen)}</td>
                  <td className="px-4 py-2 text-right font-semibold">{formatNumber(row.total_points)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
