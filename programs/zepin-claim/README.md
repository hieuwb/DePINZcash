# zepin-claim — $ZePIN Merkle distributor

Solana program that lets DePINZcash node operators claim $ZePIN rewards from
weekly snapshots published by the server. One program, one Distributor PDA per
snapshot cycle, one ClaimReceipt PDA per (cycle, claimer) — so each wallet can
only claim its slice once per cycle.

## Leaf format

Matches `server/src/merkle.rs` byte-for-byte:

```
leaf  = sha256( base58_wallet_string_bytes || u64_le(points) )
node  = sha256( sort(left, right) )           // sorted-pair hashing
```

The program only stores the 32-byte root. Proofs are passed in by the client as
`Vec<[u8; 32]>` siblings (root-bound list, no left/right index needed thanks to
sorted-pair hashing).

## Instructions

### `initialize_distributor(cycle, merkle_root, payout_per_point)`

Authority-only. Creates `Distributor` PDA at seeds `["distributor", cycle_le]`
and binds it to a mint + vault token account. The vault MUST be owned by the
Distributor PDA itself so the program can sign outbound transfers.

`payout_per_point` is in mint base units. For a 9-decimal token at $0.001/point
that's `1_000_000` (= 0.001 × 10⁹).

### `claim(wallet_str, points, merkle_proof)`

Anyone. Steps:

1. Bind the snapshot identity: `wallet_str` must equal `base58(signer.key)`.
2. Recompute `leaf = sha256(wallet_str || u64_le(points))`.
3. Walk the proof with sorted-pair hashing — must equal `distributor.merkle_root`.
4. Init the `ClaimReceipt` PDA (atomically guarantees one claim per wallet).
5. CPI transfer `points × payout_per_point` from the vault → claimer's ATA,
   signed by the Distributor PDA.

## Build / deploy

```bash
# inside programs/zepin-claim/
anchor build
anchor deploy --provider.cluster devnet

# replace declare_id! with the deployed program id, rebuild, redeploy.
solana address -k target/deploy/zepin_claim-keypair.json
```

## Initializing a snapshot

```ts
// pseudo-flow; see ../../web/src/lib/claim.ts for the production client.

const distributor = PublicKey.findProgramAddressSync(
  [Buffer.from("distributor"), u64ToLeBytes(cycle)],
  programId,
)[0];

// Create the vault as an ATA-style account owned by `distributor` and fund it.
await program.methods
  .initializeDistributor(new BN(cycle), Array.from(rootBytes), new BN(payoutPerPoint))
  .accounts({ authority, distributor, mint, vault, tokenProgram, systemProgram })
  .rpc();
```

## Claiming

```ts
// Pull the claim payload from the server: GET /api/wallet/<wallet>/claim/latest
const payload = await api.latestClaim(walletStr);
const siblings = payload.proof.siblings.map((hex) => Buffer.from(hex, "hex"));

await program.methods
  .claim(walletStr, new BN(payload.points), siblings.map((b) => Array.from(b)))
  .accounts({
    claimer,
    distributor,
    vault,
    claimerAta,
    receipt: claimReceiptPda(distributor, claimer),
    tokenProgram,
    systemProgram,
  })
  .rpc();
```

## Security notes

- The vault must be owned by the Distributor PDA — the program asserts this in
  `initialize_distributor`. Anything else and `claim` cannot sign the transfer.
- One snapshot = one Distributor PDA. Re-running `initialize_distributor` for
  the same cycle hits the `init` constraint and fails — you cannot overwrite a
  published root.
- `ClaimReceipt` is created with `init`, not `init_if_needed`, so double-claims
  are rejected at account-init time, not by an in-program flag that could
  drift out of sync.
- `wallet_str` is sanity-checked against `signer.key.to_string()`. A client
  cannot claim someone else's leaf even if they have the proof.

## Status

This is the scaffold + verified logic — `anchor build` succeeds. Wiring it to
the live snapshot pipeline (and the audited deploy on mainnet-beta) is tracked
as the next phase. The web UI shows the Merkle proof JSON today; once the
program is deployed and `VITE_CLAIM_PROGRAM_ID` is set, the "Claim $ZePIN"
button in [web/src/lib/claim.ts](../../web/src/lib/claim.ts) goes live.
