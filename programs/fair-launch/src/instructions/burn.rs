//! Protocol fees and the burn. Each launch holds what it owes the protocol in
//! its own account; `sweep_protocol` moves it to the shared burn jar (so busy
//! launches never queue on one shared account). At most hourly, anyone can
//! make the burn jar buy $SQUADZ (launch #1) on its Meteora DAMM v2 pool and
//! destroy it (founders 22 Oct: $SQUADZ trades on Meteora; this replaced the
//! PumpSwap version). `close_ops` ends a launch's operations budget into the burn jar.

use anchor_lang::prelude::*;
use anchor_lang::system_program;
use anchor_spl::associated_token::{self, get_associated_token_address_with_program_id, AssociatedToken};
use anchor_spl::{token, token_2022};

use crate::constants::*;
use crate::errors::FlpError;
use crate::instructions::fees::debit;
use crate::math;
use crate::meteora::{self, damm_swap_ix, DAMM_SWAP_LEN};
use crate::pump;
use crate::state::*;

#[account]
pub struct BurnState {
    pub last_burn_at: i64,
    pub sol_used: u64,
    pub squadz_burned: u64,
    pub bump: u8,
}

/// $SQUADZ's graduated Meteora pool for a mint, with the bump `buy_and_burn` expects: (pool, pool_bump).
/// Off-chain helper.
pub fn squadz_pool(mint: &Pubkey) -> (Pubkey, u8) {
    let (a, b) = meteora::pool_mints(mint);
    Pubkey::find_program_address(&[b"pool", METEORA_DAMM_CONFIG.as_ref(), a.as_ref(), b.as_ref()], &METEORA_DAMM_V2)
}

// ---------------------------------------------------------------- sweep_protocol

#[derive(Accounts)]
pub struct SweepProtocol<'info> {
    #[account(mut)]
    pub launch: AccountLoader<'info, Launch>,
    /// CHECK: protocol burn jar PDA.
    #[account(mut, seeds = [BURN_JAR_SEED], bump)]
    pub burn_jar: UncheckedAccount<'info>,
}

/// Anyone moves a launch's protocol fees (launch fee, the protocol's share of
/// creator fees, pots nobody qualified for) to the burn jar.
pub fn handle_sweep_protocol(ctx: Context<SweepProtocol>) -> Result<()> {
    let launch_info = ctx.accounts.launch.to_account_info();
    let floor = Rent::get()?.minimum_balance(Launch::SPACE);
    let amount = {
        let mut l = ctx.accounts.launch.load_mut()?;
        let amount = l.protocol_owed;
        require!(amount > 0, FlpError::NothingToClaim);
        require!(launch_info.lamports().saturating_sub(amount) >= floor, FlpError::Overflow);
        l.protocol_owed = 0;
        amount
    };
    debit(&launch_info, &ctx.accounts.burn_jar.to_account_info(), amount)?;
    emit!(ProtocolSwept { launch: ctx.accounts.launch.key(), amount });
    Ok(())
}

// ---------------------------------------------------------------- close_ops

#[derive(Accounts)]
pub struct CloseOps<'info> {
    #[account(mut)]
    pub caller: Signer<'info>,
    pub launch: AccountLoader<'info, Launch>,
    /// CHECK: this launch's operations budget PDA, checked by seeds.
    #[account(mut, seeds = [OPS_SEED, launch.key().as_ref()], bump)]
    pub ops: UncheckedAccount<'info>,
    /// CHECK: protocol burn jar PDA.
    #[account(mut, seeds = [BURN_JAR_SEED], bump)]
    pub burn_jar: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

/// Once a launch needs no more upkeep (every HODL Rewards drop is out, or it
/// failed and every member was refunded), anyone empties its operations
/// budget: a small tip to the caller, everything else to the burn jar.
pub fn handle_close_ops(ctx: Context<CloseOps>) -> Result<()> {
    let a = &ctx.accounts;
    let launch_key = a.launch.key();
    let now = Clock::get()?.unix_timestamp;
    {
        let l = a.launch.load()?;
        require!(l.upkeep_done(now), FlpError::UpkeepNotDone);
        // Audit run 6 (23 Oct): a launched pump.fun coin's shield payback (creator + bot rent) comes out of this budget,
        // so it must be paid before the rest goes to the burn jar. Anyone can call repay_shield. $SQUADZ has none.
        if l.state == STATE_LAUNCHED && launch_key != SQUADZ_LAUNCH {
            require!(l.launch_flags & LF_SHIELD_REPAID != 0, FlpError::ShieldNotRepaid);
        }
    }
    let left = a.ops.lamports();
    require!(left > 0, FlpError::NothingToClaim);
    let tip = CLOSE_TIP.min(left);
    let seeds: &[&[u8]] = &[OPS_SEED, launch_key.as_ref(), &[ctx.bumps.ops]];
    for (to, amount) in [(a.caller.to_account_info(), tip), (a.burn_jar.to_account_info(), left - tip)] {
        if amount > 0 {
            system_program::transfer(
                CpiContext::new_with_signer(a.system_program.to_account_info(), system_program::Transfer { from: a.ops.to_account_info(), to }, &[seeds]),
                amount,
            )?;
        }
    }
    emit!(OpsClosed { launch: launch_key, burned: left - tip, tip });
    Ok(())
}

// ---------------------------------------------------------------- repay_shield

#[derive(Accounts)]
pub struct RepayShield<'info> {
    pub caller: Signer<'info>,
    #[account(mut)]
    pub launch: AccountLoader<'info, Launch>,
    /// CHECK: this launch's operations budget PDA, checked by seeds.
    #[account(mut, seeds = [OPS_SEED, launch.key().as_ref()], bump)]
    pub ops: UncheckedAccount<'info>,
    /// CHECK: must be the launch's creator (checked in the handler).
    #[account(mut)]
    pub creator: UncheckedAccount<'info>,
    /// CHECK: the protocol's upkeep bot (constants.rs KEEPER): receives the wSOL accounts' part (fix #5, A2).
    #[account(mut, address = KEEPER)]
    pub keeper: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

/// Graduation shield payback: once a launch is live, anyone can pay back, once, the rent of the pool token
/// accounts opened for it, from that launch's operations budget. Fix #5 (A2, founders 21 Oct): the 4 wSOL accounts
/// are opened by our bot before the creator presses Launch (one-click launch), so their part goes to the bot
/// (KEEPER); the 3 coin accounts are opened inside the launch, paid by the creator, so their part goes to the
/// creator (paid first). Kept out of launch_token on purpose: the launch transaction is close to Solana's 64-step
/// limit. Pays what the budget can afford (it keeps its own rent).
pub fn handle_repay_shield(ctx: Context<RepayShield>) -> Result<()> {
    let launch_key = ctx.accounts.launch.key();
    // $SQUADZ is on Meteora: no pump.fun pool accounts were opened for it, nothing to pay back.
    require_keys_neq!(launch_key, SQUADZ_LAUNCH, FlpError::WrongVenue);
    let creator = {
        let l = ctx.accounts.launch.load()?;
        require!(l.state == STATE_LAUNCHED, FlpError::NotLaunched);
        require_keys_eq!(ctx.accounts.creator.key(), l.creator, FlpError::AccountMismatch);
        require!(l.launch_flags & LF_SHIELD_REPAID == 0, FlpError::AlreadyClaimed);
        l.creator
    };
    let rent = Rent::get()?;
    let coin_part = SHIELD_COIN_ACCOUNTS * rent.minimum_balance(TOKEN_2022_ACCOUNT_LEN);
    let wsol_part = SHIELD_WSOL_ACCOUNTS * rent.minimum_balance(TOKEN_ACCOUNT_LEN);
    let a = &ctx.accounts;
    let avail = |ops: &UncheckedAccount| ops.lamports().saturating_sub(RENT_EXEMPT_ZERO);
    let to_creator = crate::ops::tip(&a.ops.to_account_info(), &a.creator.to_account_info(), &a.system_program.to_account_info(), &launch_key, ctx.bumps.ops, coin_part.min(avail(&a.ops)))?;
    let to_keeper = crate::ops::tip(&a.ops.to_account_info(), &a.keeper.to_account_info(), &a.system_program.to_account_info(), &launch_key, ctx.bumps.ops, wsol_part.min(avail(&a.ops)))?;
    ctx.accounts.launch.load_mut()?.launch_flags |= LF_SHIELD_REPAID;
    emit!(ShieldRepaid { launch: launch_key, creator, amount: to_creator, keeper: to_keeper });
    Ok(())
}

#[event]
pub struct ShieldRepaid {
    pub launch: Pubkey,
    pub creator: Pubkey,
    /// Paid to the creator (the coin accounts' part).
    pub amount: u64,
    /// Paid to the upkeep bot (the wSOL accounts' part, fix #5).
    pub keeper: u64,
}

// ---------------------------------------------------------------- buy_and_burn

#[derive(Accounts)]
pub struct BuyAndBurn<'info> {
    #[account(mut)]
    pub caller: Signer<'info>,
    #[account(init_if_needed, payer = caller, space = 8 + 8 + 8 + 8 + 1, seeds = [BURN_STATE_SEED], bump)]
    pub burn_state: Account<'info, BurnState>,
    /// CHECK: protocol burn jar PDA (system-owned; buys as the Meteora pool's user).
    #[account(mut, seeds = [BURN_JAR_SEED], bump)]
    pub burn_jar: UncheckedAccount<'info>,
    /// CHECK: burn jar's wSOL account; address checked.
    #[account(mut)]
    pub burn_jar_wsol: UncheckedAccount<'info>,
    /// CHECK: burn jar's $SQUADZ account; address checked.
    #[account(mut)]
    pub burn_jar_squadz: UncheckedAccount<'info>,
    /// CHECK: launch #1 ($SQUADZ) at its fixed address; may not exist yet (read in the handler).
    #[account(address = SQUADZ_LAUNCH)]
    pub squadz_launch: UncheckedAccount<'info>,
    /// CHECK: $SQUADZ mint; must equal launch #1's coin.
    #[account(mut)]
    pub squadz_mint: UncheckedAccount<'info>,
    /// CHECK: wSOL mint, fixed address.
    #[account(address = WSOL_MINT)]
    pub wsol_mint: UncheckedAccount<'info>,
    pub token_program: Program<'info, token::Token>,
    /// CHECK: Token-2022 program, fixed address.
    #[account(address = token_2022::ID)]
    pub token_2022_program: UncheckedAccount<'info>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

/// `remaining_accounts`: Meteora DAMM v2 swap accounts, IDL order. The pool must be $SQUADZ's graduated pool, given
/// by its bump (cheap to check). Returns without doing anything if launch #1 hasn't launched, its pool doesn't exist
/// yet (not graduated) or is paused, or the jar is too small: fees keep building up.
pub fn handle_buy_and_burn<'info>(ctx: Context<'_, '_, 'info, 'info, BuyAndBurn<'info>>, pool_bump: u8) -> Result<()> {
    let a = &ctx.accounts;
    let ra = ctx.remaining_accounts;
    let now = Clock::get()?.unix_timestamp;
    require!(now >= a.burn_state.last_burn_at + BURN_INTERVAL, FlpError::TooEarly);
    require!(ra.len() >= DAMM_SWAP_LEN, FlpError::AccountCount);

    // Launch #1 not created or not launched yet: nothing to buy, fees wait in the jar.
    let info = a.squadz_launch.to_account_info();
    if info.owner != &crate::ID || info.data_is_empty() {
        emit!(BurnSkipped { reason: 0 });
        return Ok(());
    }
    let (mint, launched_at) = {
        let data = info.try_borrow_data()?;
        require!(data.len() >= Launch::SPACE && data[..8] == *Launch::DISCRIMINATOR, FlpError::AccountMismatch);
        let l: &Launch = bytemuck::from_bytes(&data[8..Launch::SPACE]);
        if l.state != STATE_LAUNCHED {
            emit!(BurnSkipped { reason: 0 });
            return Ok(());
        }
        (l.mint, l.launched_at)
    };
    let (ma, mb) = meteora::pool_mints(&mint);
    let pool = Pubkey::create_program_address(&[b"pool", METEORA_DAMM_CONFIG.as_ref(), ma.as_ref(), mb.as_ref(), &[pool_bump]], &METEORA_DAMM_V2)
        .map_err(|_| error!(FlpError::AccountMismatch))?;
    let jar = a.burn_jar.key();
    require_keys_eq!(a.squadz_mint.key(), mint, FlpError::AccountMismatch);
    require_keys_eq!(ra[damm_swap_ix::POOL].key(), pool, FlpError::AccountMismatch);
    require_keys_eq!(ra[damm_swap_ix::INPUT].key(), a.burn_jar_wsol.key(), FlpError::AccountMismatch);
    require_keys_eq!(ra[damm_swap_ix::OUTPUT].key(), a.burn_jar_squadz.key(), FlpError::AccountMismatch);
    require_keys_eq!(ra[damm_swap_ix::MINT_A].key(), mint, FlpError::AccountMismatch);
    require_keys_eq!(ra[damm_swap_ix::MINT_B].key(), WSOL_MINT, FlpError::AccountMismatch);
    require_keys_eq!(ra[damm_swap_ix::PAYER].key(), jar, FlpError::AccountMismatch);
    // no referral (passing the program itself means "none" to Meteora)
    require_keys_eq!(ra[damm_swap_ix::REFERRAL].key(), METEORA_DAMM_V2, FlpError::AccountMismatch);
    require_keys_eq!(ra[damm_swap_ix::PROGRAM].key(), METEORA_DAMM_V2, FlpError::AccountMismatch);
    require_keys_eq!(a.burn_jar_wsol.key(), get_associated_token_address_with_program_id(&jar, &WSOL_MINT, &token::ID), FlpError::AccountMismatch);
    require_keys_eq!(a.burn_jar_squadz.key(), get_associated_token_address_with_program_id(&jar, &mint, &token_2022::ID), FlpError::AccountMismatch);

    // No $SQUADZ pool yet (not graduated), or Meteora has paused it: nothing to do, fees wait in the jar.
    let pool_info = &ra[damm_swap_ix::POOL];
    if pool_info.owner != &METEORA_DAMM_V2 || pool_info.data_is_empty() {
        emit!(BurnSkipped { reason: 0 });
        return Ok(());
    }
    let p = meteora::read_damm_pool(pool_info)?;
    require_keys_eq!(p.token_a_mint, mint, FlpError::AccountMismatch);
    require_keys_eq!(p.token_b_mint, WSOL_MINT, FlpError::AccountMismatch);
    require_keys_eq!(ra[damm_swap_ix::VAULT_A].key(), p.token_a_vault, FlpError::AccountMismatch);
    require_keys_eq!(ra[damm_swap_ix::VAULT_B].key(), p.token_b_vault, FlpError::AccountMismatch);
    if !p.trading {
        emit!(BurnSkipped { reason: 0 });
        return Ok(());
    }

    // Size: at most 0.5% of the pool's SOL (its SOL vault, less Meteora's unclaimed share), never below the jar's reserve.
    let quote_real = pump::token_balance(&ra[damm_swap_ix::VAULT_B], &token::ID).saturating_sub(p.protocol_b_fee);
    let available = a.burn_jar.lamports().saturating_sub(BURN_JAR_RESERVE + BURN_TIP);
    let amount = available.min(math::bps(quote_real, BURN_MAX_POOL_BPS));
    // Price limit: what `amount` buys at the pool's current price, fee-free, minus 2% for the 1.25% fee + impact.
    let ideal = meteora::tokens_for_sol(amount, p.sqrt_price).unwrap_or(0);
    if amount < BURN_MIN_AMOUNT || ideal == 0 {
        emit!(BurnSkipped { reason: 1 });
        return Ok(());
    }

    let seeds: &[&[u8]] = &[BURN_JAR_SEED, &[ctx.bumps.burn_jar]];
    for (ata, m, prog) in [
        (&a.burn_jar_wsol, &a.wsol_mint.to_account_info(), a.token_program.to_account_info()),
        (&a.burn_jar_squadz, &a.squadz_mint.to_account_info(), a.token_2022_program.to_account_info()),
    ] {
        associated_token::create_idempotent(CpiContext::new_with_signer(
            a.associated_token_program.to_account_info(),
            associated_token::Create {
                payer: a.burn_jar.to_account_info(),
                associated_token: ata.to_account_info(),
                authority: a.burn_jar.to_account_info(),
                mint: m.clone(),
                system_program: a.system_program.to_account_info(),
                token_program: prog,
            },
            &[seeds],
        ))?;
    }
    // Wrap the SOL.
    system_program::transfer(
        CpiContext::new_with_signer(
            a.system_program.to_account_info(),
            system_program::Transfer { from: a.burn_jar.to_account_info(), to: a.burn_jar_wsol.to_account_info() },
            &[seeds],
        ),
        amount,
    )?;
    token::sync_native(CpiContext::new(a.token_program.to_account_info(), token::SyncNative { account: a.burn_jar_wsol.to_account_info() }))?;

    // 2%, or 11% after 3 days with no buyback (the safety valve: the burn can never get stuck for good)
    let min_out = math::bps(ideal, BPS - math::burn_slippage_bps(a.burn_state.last_burn_at, launched_at, now));
    let before = pump::token_balance(&a.burn_jar_squadz, &token_2022::ID);
    pump::invoke(METEORA_DAMM_V2, ra, meteora::damm_swap_data(amount, min_out)?, &[(damm_swap_ix::PAYER, jar)], &[seeds])?;
    let bought = pump::token_balance(&a.burn_jar_squadz, &token_2022::ID) - before;
    require!(bought >= min_out, FlpError::WrongTokenAmount);

    // Destroy everything the jar holds.
    let burn = before + bought;
    token_2022::burn(
        CpiContext::new_with_signer(
            a.token_2022_program.to_account_info(),
            token_2022::Burn { mint: a.squadz_mint.to_account_info(), from: a.burn_jar_squadz.to_account_info(), authority: a.burn_jar.to_account_info() },
            &[seeds],
        ),
        burn,
    )?;

    // Pay the caller back (network fee + private-relay tip) from the jar.
    let tip = BURN_TIP.min(a.burn_jar.lamports().saturating_sub(BURN_JAR_RESERVE));
    if tip > 0 {
        system_program::transfer(
            CpiContext::new_with_signer(
                a.system_program.to_account_info(),
                system_program::Transfer { from: a.burn_jar.to_account_info(), to: a.caller.to_account_info() },
                &[seeds],
            ),
            tip,
        )?;
    }

    let s = &mut ctx.accounts.burn_state;
    s.last_burn_at = now;
    s.sol_used += amount;
    s.squadz_burned += burn;
    s.bump = ctx.bumps.burn_state;
    emit!(Burned { sol: amount, squadz: burn, tip });
    Ok(())
}

#[event]
pub struct ProtocolSwept {
    pub launch: Pubkey,
    pub amount: u64,
}
#[event]
pub struct OpsClosed {
    pub launch: Pubkey,
    pub burned: u64,
    pub tip: u64,
}
#[event]
pub struct Burned {
    pub sol: u64,
    pub squadz: u64,
    pub tip: u64,
}
#[event]
pub struct BurnSkipped {
    /// 0 = no $SQUADZ pool yet (or Meteora paused it), 1 = amount too small.
    pub reason: u8,
}
