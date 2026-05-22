// Client for the zepin-claim Solana program (see ../../programs/zepin-claim).
//
// This module hand-rolls the Anchor-compatible instruction so we don't have to
// ship @coral-xyz/anchor + an IDL just to send one transaction. Stays in sync
// with programs/zepin-claim/src/lib.rs.

import {
  Connection,
  PublicKey,
  SystemProgram,
  Transaction,
  TransactionInstruction,
} from "@solana/web3.js";
import { sha256 } from "@noble/hashes/sha256";

import { config, isClaimProgramLive } from "./config";
import type { ClaimPayload } from "./api";

const TOKEN_PROGRAM_ID = new PublicKey("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
const ASSOCIATED_TOKEN_PROGRAM_ID = new PublicKey(
  "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL",
);

function programId(): PublicKey {
  if (!isClaimProgramLive()) {
    throw new Error("claim program is not deployed yet — VITE_CLAIM_PROGRAM_ID is empty");
  }
  return new PublicKey(config.claimProgramId);
}

// Anchor instruction discriminator = first 8 bytes of sha256("global:<name>").
function anchorDiscriminator(ixName: string): Uint8Array {
  const h = sha256(new TextEncoder().encode(`global:${ixName}`));
  return h.slice(0, 8);
}

function u64Le(n: bigint): Uint8Array {
  const buf = new Uint8Array(8);
  const view = new DataView(buf.buffer);
  view.setBigUint64(0, n, true);
  return buf;
}

function u32Le(n: number): Uint8Array {
  const buf = new Uint8Array(4);
  new DataView(buf.buffer).setUint32(0, n, true);
  return buf;
}

function concat(parts: Uint8Array[]): Uint8Array {
  const total = parts.reduce((acc, p) => acc + p.length, 0);
  const out = new Uint8Array(total);
  let off = 0;
  for (const p of parts) {
    out.set(p, off);
    off += p.length;
  }
  return out;
}

export function distributorPda(cycle: bigint): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("distributor"), Buffer.from(u64Le(cycle))],
    programId(),
  );
}

export function claimReceiptPda(distributor: PublicKey, claimer: PublicKey): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("receipt"), distributor.toBuffer(), claimer.toBuffer()],
    programId(),
  );
}

export function associatedTokenAddress(owner: PublicKey, mint: PublicKey): PublicKey {
  const [ata] = PublicKey.findProgramAddressSync(
    [owner.toBuffer(), TOKEN_PROGRAM_ID.toBuffer(), mint.toBuffer()],
    ASSOCIATED_TOKEN_PROGRAM_ID,
  );
  return ata;
}

// Wire format for `claim(wallet_str: String, points: u64, merkle_proof: Vec<[u8; 32]>)`:
//   [8 bytes discriminator]
//   [u32 LE len][utf8 wallet_str bytes]
//   [u64 LE points]
//   [u32 LE proof_len][proof_len * 32 bytes]
function encodeClaimArgs(walletStr: string, points: bigint, proof: Uint8Array[]): Uint8Array {
  const walletBytes = new TextEncoder().encode(walletStr);
  const proofBytes = concat(proof);
  return concat([
    anchorDiscriminator("claim"),
    u32Le(walletBytes.length),
    walletBytes,
    u64Le(points),
    u32Le(proof.length),
    proofBytes,
  ]);
}

export interface BuildClaimArgs {
  claimer: PublicKey;
  cycle: bigint;
  vault: PublicKey; // looked up off-chain from the distributor account
  payload: ClaimPayload;
}

export function buildClaimInstruction(args: BuildClaimArgs): TransactionInstruction {
  const pid = programId();
  const [distributor] = distributorPda(args.cycle);
  const [receipt] = claimReceiptPda(distributor, args.claimer);
  const mint = new PublicKey(args.payload.spl_mint ?? config.tokenMint);
  const claimerAta = associatedTokenAddress(args.claimer, mint);

  const proof = args.payload.proof.siblings.map((hex) => {
    const buf = new Uint8Array(32);
    for (let i = 0; i < 32; i++) {
      buf[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
    }
    return buf;
  });

  return new TransactionInstruction({
    programId: pid,
    keys: [
      { pubkey: args.claimer, isSigner: true, isWritable: true },
      { pubkey: distributor, isSigner: false, isWritable: false },
      { pubkey: args.vault, isSigner: false, isWritable: true },
      { pubkey: claimerAta, isSigner: false, isWritable: true },
      { pubkey: receipt, isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data: Buffer.from(encodeClaimArgs(args.payload.wallet, BigInt(args.payload.points), proof)),
  });
}

// Reads the vault pubkey out of the Distributor account so the caller doesn't
// have to thread it through env config. Layout matches the Anchor account:
//   8 bytes anchor discriminator
//   32 bytes authority
//   32 bytes mint
//   32 bytes vault     ← we read this
//   8 bytes cycle
//   32 bytes merkle_root
//   8 bytes payout_per_point
//   1 byte bump
export async function readDistributorVault(
  connection: Connection,
  distributor: PublicKey,
): Promise<PublicKey> {
  const acc = await connection.getAccountInfo(distributor);
  if (!acc) throw new Error(`distributor ${distributor.toBase58()} not found — was it initialized?`);
  if (acc.data.length < 8 + 32 + 32 + 32) {
    throw new Error("distributor account too small to be a Distributor");
  }
  return new PublicKey(acc.data.subarray(8 + 32 + 32, 8 + 32 + 32 + 32));
}

export async function sendClaim(
  connection: Connection,
  claimer: PublicKey,
  payload: ClaimPayload,
  signAndSend: (tx: Transaction) => Promise<string>,
): Promise<string> {
  const [distributor] = distributorPda(BigInt(payload.cycle));
  const vault = await readDistributorVault(connection, distributor);
  const ix = buildClaimInstruction({
    claimer,
    cycle: BigInt(payload.cycle),
    vault,
    payload,
  });
  const tx = new Transaction().add(ix);
  tx.feePayer = claimer;
  tx.recentBlockhash = (await connection.getLatestBlockhash()).blockhash;
  return signAndSend(tx);
}
