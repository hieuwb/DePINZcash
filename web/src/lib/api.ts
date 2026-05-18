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
};

export { ApiError };
