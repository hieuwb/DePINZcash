import { useMemo, useState } from "react";
import { Link } from "react-router-dom";
import { useWallet } from "@solana/wallet-adapter-react";
import { WalletMultiButton } from "@solana/wallet-adapter-react-ui";
import bs58 from "bs58";

import { api, registrationMessage, type RegisterResponse } from "../lib/api";
import { config } from "../lib/config";
import { ErrorBanner } from "../components/Loading";
import { randomNonce, shortAddress } from "../lib/format";

const NODE_KINDS = [
  { value: "zebra-full", label: "Zebra full node" },
  { value: "lightwalletd", label: "lightwalletd" },
];

export function Register() {
  const wallet = useWallet();
  const [kind, setKind] = useState("zebra-full");
  const [label, setLabel] = useState("");
  const [rpcEndpoint, setRpcEndpoint] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<RegisterResponse | null>(null);

  const walletPubkey = useMemo(() => wallet.publicKey?.toBase58() ?? null, [wallet.publicKey]);
  const canSign = !!wallet.signMessage && !!walletPubkey;

  async function onRegister(e: React.FormEvent) {
    e.preventDefault();
    setError(null);
    setResult(null);
    if (!walletPubkey || !wallet.signMessage) {
      setError("connect a wallet that supports message signing (Phantom / Solflare)");
      return;
    }
    setSubmitting(true);
    try {
      const nonce = randomNonce();
      const timestamp = new Date().toISOString();
      const msg = registrationMessage({
        wallet: walletPubkey,
        nonce,
        timestamp,
        kind,
        network: config.network,
        label,
      });
      const sigBytes = await wallet.signMessage(msg);
      const signature = bs58.encode(sigBytes);
      const resp = await api.register({
        wallet: walletPubkey,
        signature,
        nonce,
        timestamp,
        kind,
        label: label || null,
        rpc_endpoint: rpcEndpoint.trim() || null,
      });
      setResult(resp);
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <div className="grid gap-8 md:grid-cols-[2fr_3fr]">
      <div className="flex flex-col gap-4">
        <h1 className="text-2xl font-semibold">Register a node</h1>
        <p className="text-sm text-zcash-subtle">
          Sign the registration message with your Solana wallet. The server returns a node ID
          and an auth token — you paste them into the relay CLI on the machine that runs your
          Zebra node.
        </p>
        <div className="card text-xs text-zcash-subtle">
          <p className="font-semibold text-zcash-text">Why am I signing?</p>
          <p className="mt-1">
            Your signature proves the Solana wallet owns this node registration. No tokens are
            transferred; the signed message is sent off-chain to the DePINZcash server, which
            stores your public key as the node's owner.
          </p>
        </div>
        <div className="card text-xs">
          <p className="font-semibold">Network: <span className="text-zcash-gold">{config.network}</span></p>
          <p className="mt-1 text-zcash-subtle">
            Wallet adapter is configured for Solana <code className="text-zcash-text">{config.solanaCluster}</code>.
            The wallet only signs an off-chain message — no RPC call is made.
          </p>
        </div>
      </div>

      <div className="card flex flex-col gap-5">
        {!walletPubkey && (
          <div className="flex flex-col gap-3">
            <p className="text-sm text-zcash-subtle">
              Connect a wallet to begin.
            </p>
            <WalletMultiButton />
          </div>
        )}

        {walletPubkey && !result && (
          <form className="flex flex-col gap-4" onSubmit={onRegister}>
            <div>
              <label className="stat-label">Wallet</label>
              <p className="mt-1 break-all font-mono text-xs text-zcash-text">{walletPubkey}</p>
            </div>
            <div>
              <label className="stat-label" htmlFor="kind">Node kind</label>
              <select
                id="kind"
                className="input mt-1"
                value={kind}
                onChange={(e) => setKind(e.target.value)}
              >
                {NODE_KINDS.map((k) => (
                  <option key={k.value} value={k.value}>{k.label}</option>
                ))}
              </select>
            </div>
            <div>
              <label className="stat-label" htmlFor="label">Label <span className="text-zcash-subtle/70">(optional)</span></label>
              <input
                id="label"
                className="input mt-1"
                value={label}
                onChange={(e) => setLabel(e.target.value)}
                placeholder="primary, eu-1, …"
                maxLength={64}
              />
              <p className="mt-1 text-[10px] text-zcash-subtle">
                Same wallet can register multiple nodes only if each has a unique kind+label pair.
              </p>
            </div>
            <div>
              <label className="stat-label" htmlFor="rpc">Public RPC URL <span className="text-zcash-subtle/70">(optional)</span></label>
              <input
                id="rpc"
                className="input mt-1"
                value={rpcEndpoint}
                onChange={(e) => setRpcEndpoint(e.target.value)}
                placeholder="https://zebra.example.com:8232"
              />
            </div>
            {error && <ErrorBanner message={error} />}
            <button
              type="submit"
              className="btn-primary"
              disabled={!canSign || submitting}
            >
              {submitting ? "signing…" : "Sign + register"}
            </button>
          </form>
        )}

        {result && (
          <RegistrationReceipt resp={result} />
        )}
      </div>
    </div>
  );
}

function RegistrationReceipt({ resp }: { resp: RegisterResponse }) {
  const cli = `# In your relay state file (default: config/relay-state.json)
{
  "api": "${config.apiUrl}",
  "wallet": "${resp.node.wallet}",
  "node_id": "${resp.node.id}",
  "auth_token": "${resp.auth_token}",
  "kind": "${resp.node.kind}",
  "label": ${JSON.stringify(resp.node.label ?? "")},
  "registered_at": "${resp.node.registered_at}"
}`;
  return (
    <div className="flex flex-col gap-4">
      <div className="rounded-md border border-zcash-success/40 bg-zcash-success/10 px-3 py-2 text-sm text-emerald-200">
        Node registered. Save the auth token below — it's shown once.
      </div>
      <div className="grid gap-2 text-sm">
        <Row label="Node ID" value={resp.node.id} />
        <Row label="Wallet" value={shortAddress(resp.node.wallet, 8, 8)} />
        <Row label="Kind" value={resp.node.kind} />
        {resp.node.label && <Row label="Label" value={resp.node.label} />}
        <Row label="Auth token" value={resp.auth_token} mono />
      </div>
      <div>
        <p className="stat-label mb-1">Paste into relay-state.json</p>
        <pre className="overflow-x-auto rounded-md border border-zcash-border bg-zcash-dark p-3 font-mono text-xs leading-5 text-zcash-text">
{cli}
        </pre>
      </div>
      <div className="flex flex-wrap gap-2">
        <Link to={`/dashboard/${encodeURIComponent(resp.node.wallet)}`} className="btn-primary">
          Open dashboard
        </Link>
        <button
          type="button"
          className="btn-outline"
          onClick={() => navigator.clipboard.writeText(cli)}
        >
          Copy JSON
        </button>
      </div>
    </div>
  );
}

function Row({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="flex flex-col gap-0.5">
      <span className="stat-label">{label}</span>
      <span className={`break-all ${mono ? "font-mono text-xs" : ""}`}>{value}</span>
    </div>
  );
}
