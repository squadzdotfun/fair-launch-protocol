//! Project Treasury (founders 20 Oct 2026, docs/27). An optional extra a creator adds in the creation
//! transaction: 1-10% of total supply, taken from the members' part (the HODL pot is never touched), released
//! to one fixed wallet: 10% at launch, then 30% at 30, 60 and 90 days. The fee (1 SOL per 1%) is held in the
//! treasury record until launch (then it goes to the burn jar, so it buys and burns $SQUADZ) or refunded to the
//! creator if the lobby never launches.
//!
//!   add_treasury         - creator, only before anyone has joined (like `set_max_per_wallet`)
//!   settle_treasury_fee  - anyone, once: fee to the burn jar after launch, back to the creator after a refund opens
//!   release_treasury     - anyone: sends every unlock that is due to the treasury wallet; never early, never twice
//!
//! Built beside the existing code: `create_launch` is unchanged; `launch_token` only reads `treasury_bps`
//! in its split (no new accounts); the tokens stay in the lobby vault, which only ever pays booked amounts.

use anchor_lang::prelude::*;
use anchor_lang::system_program;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token_2022;

use crate::constants::*;
use crate::errors::FlpError;
use crate::instructions::claims::move_tokens;
use crate::instructions::fees::debit;
use crate::math;
use crate::state::*;

// ---------------------------------------------------------------- add

#[derive(Accounts)]
pub struct AddTreasury<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,
    #[account(mut)]
    pub launch: AccountLoader<'info, Launch>,
    #[account(
        init,
        payer = creator,
        space = TreasuryState::SPACE,
        seeds = [TREASURY_SEED, launch.key().as_ref()],
        bump
    )]
    pub treasury: Account<'info, TreasuryState>,
    pub system_program: Program<'info, System>,
}

/// The creator adds a Project Treasury, at creation only: before anyone has joined (the site does it in the
/// creation transaction, before the creator's own seat), so nobody ever joins under different terms.
pub fn handle_add_treasury(ctx: Context<AddTreasury>, bps: u16, wallet: Pubkey) -> Result<()> {
    let launch_key = ctx.accounts.launch.key();
    // Launch #1 ($SQUADZ, founders 22 Oct): up to 15% and no fee (the team would be paying itself). Everyone else: as before.
    let squadz = launch_key == SQUADZ_LAUNCH;
    let max_bps = if squadz { SQUADZ_MAX_TREASURY_BPS } else { MAX_TREASURY_BPS };
    require!(bps >= TREASURY_STEP_BPS && bps <= max_bps && bps % TREASURY_STEP_BPS == 0, FlpError::BadTreasury);
    require!(wallet_ok(&wallet, &launch_key, &ctx.accounts.treasury.key()), FlpError::BadTreasury);
    {
        let mut l = ctx.accounts.launch.load_mut()?;
        require_keys_eq!(l.creator, ctx.accounts.creator.key(), FlpError::NotCreator);
        require!(l.state == STATE_OPEN, FlpError::NotOpen);
        // Creation transaction only (review 20 Oct, M1): `created_at` is set by create_launch, so only an add in the
        // same transaction (same clock) passes. Nobody can join a lobby before it exists, so no join can be
        // front-run, and a lobby whose members all left can't get a treasury later. Works through a multisig's CPI
        // too (launch #1), unlike an instructions-sysvar check.
        require!(l.filled == 0 && l.treasury_bps == 0 && Clock::get()?.unix_timestamp == l.created_at, FlpError::TreasuryLocked);
        l.treasury_bps = bps;
    }
    let fee = if squadz { 0 } else { bps as u64 * TREASURY_FEE_PER_BPS };
    system_program::transfer(
        CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            system_program::Transfer { from: ctx.accounts.creator.to_account_info(), to: ctx.accounts.treasury.to_account_info() },
        ),
        fee,
    )?;
    let t = &mut ctx.accounts.treasury;
    t.launch = launch_key;
    t.wallet = wallet;
    t.fee = fee;
    t.released = 0;
    t.bps = bps;
    t.releases = 0;
    t.fee_settled = false;
    t.bump = ctx.bumps.treasury;
    emit!(TreasuryAdded { launch: launch_key, wallet, bps, fee });
    Ok(())
}

/// A treasury wallet must be a real destination: not empty, and not one of this lobby's own accounts.
/// Normal wallets and multisig vaults (off-curve addresses) are both fine.
fn wallet_ok(w: &Pubkey, launch: &Pubkey, treasury: &Pubkey) -> bool {
    let own = |seed: &[u8]| Pubkey::find_program_address(&[seed, launch.as_ref()], &crate::ID).0;
    *w != Pubkey::default()
        && w != launch
        && w != treasury
        && *w != own(VAULT_SEED)
        && *w != own(CREATOR_SEED)
        && *w != own(OPS_SEED)
        && *w != Pubkey::find_program_address(&[BURN_JAR_SEED], &crate::ID).0
}

// ---------------------------------------------------------------- settle the fee

#[derive(Accounts)]
pub struct SettleTreasuryFee<'info> {
    pub launch: AccountLoader<'info, Launch>,
    #[account(mut, seeds = [TREASURY_SEED, launch.key().as_ref()], bump = treasury.bump)]
    pub treasury: Account<'info, TreasuryState>,
    /// CHECK: protocol burn jar PDA (receives the fee after a launch).
    #[account(mut, seeds = [BURN_JAR_SEED], bump)]
    pub burn_jar: UncheckedAccount<'info>,
    /// CHECK: the lobby's creator (gets the fee back if it never launches); checked against the launch.
    #[account(mut)]
    pub creator: UncheckedAccount<'info>,
}

/// Anyone, once: after launch the fee goes to the burn jar (it buys and burns $SQUADZ like every protocol fee);
/// if the lobby was cancelled or ran out of time, it goes back to the creator.
pub fn handle_settle_treasury_fee(ctx: Context<SettleTreasuryFee>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let (launched, refund) = {
        let l = ctx.accounts.launch.load()?;
        require_keys_eq!(ctx.accounts.creator.key(), l.creator, FlpError::AccountMismatch);
        (l.state == STATE_LAUNCHED, l.refunds_open(now))
    };
    require!(launched || refund, FlpError::TooEarly);
    let to = if launched { ctx.accounts.burn_jar.to_account_info() } else { ctx.accounts.creator.to_account_info() };
    let t = &mut ctx.accounts.treasury;
    require!(!t.fee_settled, FlpError::TreasurySettled);
    t.fee_settled = true;
    let fee = t.fee;
    debit(&t.to_account_info(), &to, fee)?;
    emit!(TreasuryFeeSettled { launch: ctx.accounts.launch.key(), fee, burned: launched });
    Ok(())
}

// ---------------------------------------------------------------- release

#[derive(Accounts)]
pub struct ReleaseTreasury<'info> {
    /// Whoever presses it (our bots do); pays the treasury wallet's token account if it's new.
    #[account(mut)]
    pub caller: Signer<'info>,
    pub launch: AccountLoader<'info, Launch>,
    #[account(mut, seeds = [TREASURY_SEED, launch.key().as_ref()], bump = treasury.bump)]
    pub treasury: Account<'info, TreasuryState>,
    /// CHECK: launch vault PDA, checked by seeds; signs the transfer.
    #[account(seeds = [VAULT_SEED, launch.key().as_ref()], bump)]
    pub vault: UncheckedAccount<'info>,
    /// CHECK: the launch's coin; must equal the mint stored at launch.
    pub mint: UncheckedAccount<'info>,
    /// CHECK: vault's token account; address checked.
    #[account(mut)]
    pub vault_token: UncheckedAccount<'info>,
    /// CHECK: the treasury wallet; must equal the one fixed at creation.
    pub wallet: UncheckedAccount<'info>,
    /// CHECK: the treasury wallet's token account (created if missing); address checked.
    #[account(mut)]
    pub wallet_token: UncheckedAccount<'info>,
    /// CHECK: Token-2022 program, fixed address.
    #[account(address = token_2022::ID)]
    pub token_2022_program: UncheckedAccount<'info>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

/// Anyone: sends every treasury unlock that is due (10% at launch, 30% at 30, 60 and 90 days) to the fixed
/// treasury wallet. Never early, never twice, never anywhere else.
pub fn handle_release_treasury(ctx: Context<ReleaseTreasury>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let a = &ctx.accounts;
    let launch_key = a.launch.key();
    require_keys_eq!(a.wallet.key(), a.treasury.wallet, FlpError::AccountMismatch);
    let (total, launched_at, vault_bump) = {
        let l = a.launch.load()?;
        require!(l.state == STATE_LAUNCHED, FlpError::NotLaunched);
        require_keys_eq!(a.mint.key(), l.mint, FlpError::AccountMismatch);
        (math::treasury_total(l.tokens_bought, l.hodl_reserve, l.per_member_tokens, l.slots), l.launched_at, l.vault_bump)
    };
    let mut amount = 0u64;
    let (mut released, mut k) = (a.treasury.released, a.treasury.releases as usize);
    while k < TREASURY_RELEASE_TIMES.len() && now >= launched_at + TREASURY_RELEASE_TIMES[k] {
        let part = math::treasury_release_amount(total, released, k);
        released += part;
        amount += part;
        k += 1;
    }
    require!(amount > 0, FlpError::TooEarly);
    move_tokens(
        a.vault.as_ref(),
        &a.vault_token,
        &a.wallet,
        &a.wallet_token,
        &a.mint,
        &a.caller,
        &a.token_2022_program,
        &a.associated_token_program,
        &a.system_program,
        &launch_key,
        vault_bump,
        amount,
    )?;
    let t = &mut ctx.accounts.treasury;
    t.released = released;
    t.releases = k as u8;
    emit!(TreasuryReleased { launch: launch_key, wallet: t.wallet, tokens: amount, releases: t.releases, released_total: released });
    Ok(())
}

#[event]
pub struct TreasuryAdded {
    pub launch: Pubkey,
    pub wallet: Pubkey,
    pub bps: u16,
    pub fee: u64,
}
#[event]
pub struct TreasuryFeeSettled {
    pub launch: Pubkey,
    pub fee: u64,
    pub burned: bool,
}
#[event]
pub struct TreasuryReleased {
    pub launch: Pubkey,
    pub wallet: Pubkey,
    pub tokens: u64,
    pub releases: u8,
    pub released_total: u64,
}
