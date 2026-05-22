import { config } from "./config";

// ---- types ------------------------------------------------------------------

export interface NetworkStats {
  total_nodes: number;
  active_nodes: number;
  total_proofs: number;
  accepted_proofs: number;
  total_points: number;
  network: string;
  spl_mint: string | null;
  solana_cluster: string;
  trusted_tip_height: number | null;
}

export interface WalletStats {
  wallet: string;
  nodes: number;
  total_points: number;
  total_uptime_seconds: number;
  last_seen: string | null;
}

export interface PublicNode {
  id: string;
  wallet: string;
  kind: string;
  label: string | null;
  rpc_endpoint: string | null;
  network: string;
  status: string;
  last_height: number | null;
  last_block_hash: string | null;
  last_proof_at: string | null;
  registered_at: string;
  points: number;
  uptime_seconds: number;
}

export interface ServerInfo {
  name: string;
  version: string;
  network: string;
  rpc_endpoints: number;
  trusted_tip_height: number | null;
  spl_mint: string | null;
  solana_cluster: string;
  scheduler_enabled: boolean;
  exposed_rpc_enabled: boolean;
  exposed_rpc_poll_seconds: number | null;
  registration_message_v1: string;
  rewards_note: string;
}

export interface RegisterRequest {
  wallet: string;
  signature: string;
  nonce: string;
  timestamp: string;
  kind: string;
  label?: string | null;
  rpc_endpoint?: string | null;
}

export interface RegisterResponse {
  node: PublicNode;
  auth_token: string;
}

export interface ProofRecord {
  id: string;
  node_id: string;
  wallet: string;
  claimed_height: number;
  claimed_block_hash: string;
  proof_timestamp: string;
  binary_hash: string | null;
  uptime_seconds: number | null;
  peers: number | null;
  verdict: string;
  reject_reason: string | null;
  points_awarded: number;
  received_at: string;
}

export interface NodeDailyBucket {
  day: string;
  proofs: number;
  accepted: number;
  points: number;
}

export interface ClaimPayload {
  wallet: string;
  cycle: number;
  merkle_root: string;
  points: number;
  leaf_hash: string;
  proof: { siblings: string[]; leaf_index: number };
  spl_mint: string | null;
  solana_cluster: string;
}

// ---- canonical signing messages --------------------------------------------
// Must match server/src/auth.rs byte-for-byte.

export function registrationMessage(args: {
  wallet: string;
  nonce: string;
  timestamp: string;
  kind: string;
  network: string;
  label: string;
}): Uint8Array {
  const text =
    `depinzcash:register:v1\n${args.wallet}\n${args.nonce}\n${args.timestamp}\n${args.kind}\n${args.network}\n${args.label}\n`;
  return new TextEncoder().encode(text);
}

// ---- fetch helpers ----------------------------------------------------------

class ApiError extends Error {
  constructor(public status: number, public code: string, message: string) {
    super(message);
  }
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const url = `${config.apiUrl.replace(/\/$/, "")}${path}`;
  const resp = await fetch(url, {
    ...init,
    headers: {
      "content-type": "application/json",
      ...(init?.headers ?? {}),
    },
  });
  const text = await resp.text();
  if (!resp.ok) {
    let code = "http_error";
    let message = `HTTP ${resp.status}`;
    try {
      const body = JSON.parse(text);
      code = body.error ?? code;
      message = body.message ?? message;
    } catch {
      message = text || message;
    }
    throw new ApiError(resp.status, code, message);
  }
  if (!text) return undefined as T;
  return JSON.parse(text) as T;
}

export const api = {
  serverInfo: () => request<ServerInfo>("/api/info"),
  networkStats: () => request<NetworkStats>("/api/stats/network"),
  leaderboard: (limit = 100) =>
    request<WalletStats[]>(`/api/stats/leaderboard?limit=${limit}`),
  walletStats: (wallet: string) =>
    request<WalletStats>(`/api/wallet/${encodeURIComponent(wallet)}/stats`),
  walletNodes: (wallet: string) =>
    request<PublicNode[]>(`/api/wallet/${encodeURIComponent(wallet)}/nodes`),
  register: (req: RegisterRequest) =>
    request<RegisterResponse>("/api/nodes/register", {
      method: "POST",
      body: JSON.stringify(req),
    }),
  latestClaim: (wallet: string) =>
    request<ClaimPayload>(`/api/wallet/${encodeURIComponent(wallet)}/claim/latest`),
  node: (id: string) => request<PublicNode>(`/api/nodes/${encodeURIComponent(id)}`),
  nodeProofs: (id: string, limit = 100) =>
    request<ProofRecord[]>(`/api/nodes/${encodeURIComponent(id)}/proofs?limit=${limit}`),
  nodeSeries: (id: string, days = 14) =>
    request<NodeDailyBucket[]>(`/api/nodes/${encodeURIComponent(id)}/series?days=${days}`),
  activeNodes: (limit = 200) => request<PublicNode[]>(`/api/nodes?limit=${limit}`),
  recentProofs: (limit = 100) => request<ProofRecord[]>(`/api/proofs/recent?limit=${limit}`),
};

// Block explorer link for a Zcash block hash. Blockchair confirmed working
// against mainnet block hashes (cross-checked against the server's proofs).
export function zcashExplorerUrl(blockHash: string): string {
  return `https://blockchair.com/zcash/block/${blockHash.replace(/^0x/, "")}`;
}

export { ApiError };
