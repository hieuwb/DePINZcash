// $ZePIN Merkle-distributor program.
//
// One Distributor PDA per (cycle, merkle_root). Snapshots are published by the
// DePINZcash server: each leaf is sha256(wallet_b58_str || points_le). Internal
// nodes use sorted-pair hashing so proofs are just a list of sibling hashes —
// no left/right index is encoded. This matches server/src/merkle.rs byte-for-byte.

use anchor_lang::prelude::*;
use anchor_lang::solana_program::hash::hashv;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};

declare_id!("ZePiNCLA1MdistRibu4orProgRam1111111111111111");

#[program]
pub mod zepin_claim {
    use super::*;

    // Authority publishes a snapshot: stores the root + per-point payout multiplier.
    // payout_per_point is in mint base units (e.g. for a 9-decimal token with $0.001
    // per point, pass 1_000_000 = 0.001 * 10^9).
    pub fn initialize_distributor(
        ctx: Context<InitializeDistributor>,
        cycle: u64,
        merkle_root: [u8; 32],
        payout_per_point: u64,
    ) -> Result<()> {
        let d = &mut ctx.accounts.distributor;
        d.authority = ctx.accounts.authority.key();
        d.mint = ctx.accounts.mint.key();
        d.vault = ctx.accounts.vault.key();
        d.cycle = cycle;
        d.merkle_root = merkle_root;
        d.payout_per_point = payout_per_point;
        d.bump = ctx.bumps.distributor;
        Ok(())
    }

    // Claim for `wallet_str` (the base58 pubkey string the snapshot was built with).
    // The program enforces wallet_str.decode() == signer.key, then verifies the
    // Merkle proof and transfers payout_per_point * points to the claimer's ATA.
    pub fn claim(
        ctx: Context<ClaimRewards>,
        wallet_str: String,
        points: u64,
        merkle_proof: Vec<[u8; 32]>,
    ) -> Result<()> {
        // 1) Bind the snapshot identity to the signer.
        let signer_b58 = ctx.accounts.claimer.key().to_string();
        require!(wallet_str == signer_b58, ClaimError::WalletMismatch);

        // 2) Compute the leaf in the same format the server uses.
        let leaf = hash_leaf(&wallet_str, points);

        // 3) Verify against the snapshot root.
        let d = &ctx.accounts.distributor;
        require!(
            verify_proof(&leaf, &merkle_proof, &d.merkle_root),
            ClaimError::InvalidProof
        );

        // 4) One claim per (distributor, claimer) — guaranteed by the
        //    ClaimReceipt init constraint in the accounts struct.

        // 5) Transfer points * payout_per_point from the vault to the claimer's ATA.
        let amount = (points as u128)
            .checked_mul(d.payout_per_point as u128)
            .ok_or(ClaimError::Overflow)?;
        let amount_u64: u64 = amount.try_into().map_err(|_| ClaimError::Overflow)?;

        let cycle_bytes = d.cycle.to_le_bytes();
        let seeds: &[&[u8]] = &[b"distributor", &cycle_bytes, &[d.bump]];
        let signer_seeds: &[&[&[u8]]] = &[seeds];

        let cpi_accounts = Transfer {
            from: ctx.accounts.vault.to_account_info(),
            to: ctx.accounts.claimer_ata.to_account_info(),
            authority: ctx.accounts.distributor.to_account_info(),
        };
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            cpi_accounts,
            signer_seeds,
        );
        token::transfer(cpi_ctx, amount_u64)?;

        let receipt = &mut ctx.accounts.receipt;
        receipt.distributor = d.key();
        receipt.claimer = ctx.accounts.claimer.key();
        receipt.points = points;
        receipt.amount = amount_u64;
        receipt.claimed_at = Clock::get()?.unix_timestamp;

        emit!(ClaimEvent {
            distributor: d.key(),
            claimer: ctx.accounts.claimer.key(),
            cycle: d.cycle,
            points,
            amount: amount_u64,
        });

        Ok(())
    }
}

// ---- account layouts -------------------------------------------------------

#[derive(Accounts)]
#[instruction(cycle: u64)]
pub struct InitializeDistributor<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(
        init,
        payer = authority,
        space = 8 + Distributor::SIZE,
        seeds = [b"distributor", &cycle.to_le_bytes()],
        bump,
    )]
    pub distributor: Account<'info, Distributor>,

    pub mint: Account<'info, Mint>,

    // Vault must be owned by the distributor PDA so the program can sign transfers.
    #[account(
        constraint = vault.mint == mint.key() @ ClaimError::VaultMintMismatch,
        constraint = vault.owner == distributor.key() @ ClaimError::VaultOwnerMismatch,
    )]
    pub vault: Account<'info, TokenAccount>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
#[instruction(wallet_str: String, points: u64)]
pub struct ClaimRewards<'info> {
    #[account(mut)]
    pub claimer: Signer<'info>,

    #[account(
        seeds = [b"distributor", &distributor.cycle.to_le_bytes()],
        bump = distributor.bump,
    )]
    pub distributor: Account<'info, Distributor>,

    #[account(
        mut,
        constraint = vault.key() == distributor.vault @ ClaimError::VaultMismatch,
    )]
    pub vault: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = claimer_ata.owner == claimer.key() @ ClaimError::ClaimerAtaOwnerMismatch,
        constraint = claimer_ata.mint == distributor.mint @ ClaimError::ClaimerAtaMintMismatch,
    )]
    pub claimer_ata: Account<'info, TokenAccount>,

    // One receipt per (distributor, claimer) — enforces single-claim per cycle.
    #[account(
        init,
        payer = claimer,
        space = 8 + ClaimReceipt::SIZE,
        seeds = [b"receipt", distributor.key().as_ref(), claimer.key().as_ref()],
        bump,
    )]
    pub receipt: Account<'info, ClaimReceipt>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
}

#[account]
pub struct Distributor {
    pub authority: Pubkey,
    pub mint: Pubkey,
    pub vault: Pubkey,
    pub cycle: u64,
    pub merkle_root: [u8; 32],
    pub payout_per_point: u64,
    pub bump: u8,
}

impl Distributor {
    pub const SIZE: usize = 32 + 32 + 32 + 8 + 32 + 8 + 1;
}

#[account]
pub struct ClaimReceipt {
    pub distributor: Pubkey,
    pub claimer: Pubkey,
    pub points: u64,
    pub amount: u64,
    pub claimed_at: i64,
}

impl ClaimReceipt {
    pub const SIZE: usize = 32 + 32 + 8 + 8 + 8;
}

#[event]
pub struct ClaimEvent {
    pub distributor: Pubkey,
    pub claimer: Pubkey,
    pub cycle: u64,
    pub points: u64,
    pub amount: u64,
}

#[error_code]
pub enum ClaimError {
    #[msg("wallet string does not match signer pubkey")]
    WalletMismatch,
    #[msg("merkle proof did not verify against the snapshot root")]
    InvalidProof,
    #[msg("arithmetic overflow computing payout")]
    Overflow,
    #[msg("vault account does not belong to this distributor")]
    VaultMismatch,
    #[msg("vault mint does not match distributor mint")]
    VaultMintMismatch,
    #[msg("vault owner must be the distributor PDA")]
    VaultOwnerMismatch,
    #[msg("claimer ATA owner does not match signer")]
    ClaimerAtaOwnerMismatch,
    #[msg("claimer ATA mint does not match distributor mint")]
    ClaimerAtaMintMismatch,
}

// ---- merkle (mirrors server/src/merkle.rs byte-for-byte) -------------------

fn hash_leaf(wallet_b58: &str, points: u64) -> [u8; 32] {
    let h = hashv(&[wallet_b58.as_bytes(), &points.to_le_bytes()]);
    h.to_bytes()
}

fn hash_pair_sorted(a: &[u8; 32], b: &[u8; 32]) -> [u8; 32] {
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    let h = hashv(&[lo, hi]);
    h.to_bytes()
}

fn verify_proof(leaf: &[u8; 32], proof: &[[u8; 32]], root: &[u8; 32]) -> bool {
    let mut cur = *leaf;
    for sib in proof {
        cur = hash_pair_sorted(&cur, sib);
    }
    &cur == root
}
