//! Creator fees, forever, and the hold line. Permissionless steps a bot runs:
//!   collect_fees  - pump.fun / PumpSwap creator fees -> this launch (hourly)
//!   payout_round  - holdings snapshot of all members, HODL Rewards drops,
//!                   and the Fee Share split of the members' pot (hourly)
//!   spot_check    - re-reads chosen members' holdings at any time (right
//!                   after a sale), so selling and buying back between rounds
//!                   is caught
//!   pay_out       - sends what each member (and the fee wallet) is owed
//! Fees sit as lamports in the launch account (program-owned) until paid.

use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke_signed;
use anchor_lang::system_program;
use anchor_spl::associated_token::{self, get_associated_token_address_with_program_id, AssociatedToken};
use anchor_spl::{token, token_2022};

use crate::constants::*;
use crate::errors::FlpError;
use crate::math;
use crate::ops;
use crate::pump::{self, collect_amm_ix, collect_curve_ix, COLLECT_AMM_LEN, COLLECT_CURVE_LEN};
use crate::state::*;

/// Moves lamports out of a program-owned account (no CPI needed).
pub fn debit(from: &AccountInfo, to: &AccountInfo, amount: u64) -> Result<()> {
    if amount == 0 {
        return Ok(());
    }
    **from.try_borrow_mut_lamports()? = from.lamports().checked_sub(amount).ok_or(FlpError::Overflow)?;
    **to.try_borrow_mut_lamports()? = to.lamports().checked_add(amount).ok_or(FlpError::Overflow)?;
    Ok(())
}

// ---------------------------------------------------------------- collect

#[derive(Accounts)]
pub struct CollectFees<'info> {
    /// Whoever collects; tipped from the operations budget while it pays upkeep.
    #[account(mut)]
    pub caller: Signer<'info>,
    #[account(mut)]
    pub launch: AccountLoader<'info, Launch>,
    /// CHECK: this launch's pump.fun creator PDA.
    #[account(mut, seeds = [CREATOR_SEED, launch.key().as_ref()], bump)]
    pub creator_pda: UncheckedAccount<'info>,
    /// CHECK: creator PDA's wSOL account (PumpSwap pays in wSOL); address checked.
    #[account(mut)]
    pub creator_wsol: UncheckedAccount<'info>,
    /// CHECK: this launch's operations budget PDA, checked by seeds.
    #[account(mut, seeds = [OPS_SEED, launch.key().as_ref()], bump)]
    pub ops: UncheckedAccount<'info>,
    /// CHECK: wSOL mint, fixed address.
    #[account(address = WSOL_MINT)]
    pub wsol_mint: UncheckedAccount<'info>,
    pub token_program: Program<'info, token::Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

/// Anyone collects this launch's creator fees. `remaining_accounts`: pump.fun
/// collect_creator_fee_v2 (`curve_len`, 10 today), then PumpSwap
/// collect_coin_creator_fee (`amm_len`, 8 today; 0 = not graduated yet).
/// Lengths are passed so accounts pump.fun appends later still pass through:
/// a stuck collect would strand fees. New fees are split at once: 10% to the
/// protocol (held here until swept to the burn jar), the rest between the
/// creator and the members' pot.
pub fn handle_collect_fees<'info>(ctx: Context<'_, '_, 'info, 'info, CollectFees<'info>>, curve_len: u8, amm_len: u8) -> Result<()> {
    let (curve_len, amm_len) = (curve_len as usize, amm_len as usize);
    let graduated = amm_len > 0;
    require!(curve_len >= COLLECT_CURVE_LEN && (amm_len == 0 || amm_len >= COLLECT_AMM_LEN), FlpError::AccountCount);
    require!(ctx.remaining_accounts.len() == curve_len + amm_len, FlpError::AccountCount);
    let a = &ctx.accounts;
    let ra = ctx.remaining_accounts;
    let launch_key = a.launch.key();
    let cp = a.creator_pda.key();
    require!(a.launch.load()?.state == STATE_LAUNCHED, FlpError::NotLaunched);
    let seeds: &[&[u8]] = &[CREATOR_SEED, launch_key.as_ref(), &[ctx.bumps.creator_pda]];

    // 1. Bonding-curve creator vault -> creator PDA (lamports).
    let curve = &ra[..curve_len];
    require_keys_eq!(curve[collect_curve_ix::CREATOR].key(), cp, FlpError::AccountMismatch);
    require_keys_eq!(curve[collect_curve_ix::PROGRAM].key(), PUMP_PROGRAM, FlpError::AccountMismatch);
    pump::invoke(PUMP_PROGRAM, curve, pump::collect_curve_data(), &[], &[])?;

    // 2. PumpSwap coin-creator vault -> creator PDA's wSOL (unwrapped in step 3).
    if graduated {
        let amm = &ra[curve_len..];
        require_keys_eq!(a.creator_wsol.key(), get_associated_token_address_with_program_id(&cp, &WSOL_MINT, &token::ID), FlpError::AccountMismatch);
        require_keys_eq!(amm[collect_amm_ix::COIN_CREATOR].key(), cp, FlpError::AccountMismatch);
        require_keys_eq!(amm[collect_amm_ix::COIN_CREATOR_TOKEN_ACCOUNT].key(), a.creator_wsol.key(), FlpError::AccountMismatch);
        require_keys_eq!(amm[collect_amm_ix::PROGRAM].key(), PUMP_AMM_PROGRAM, FlpError::AccountMismatch);
        associated_token::create_idempotent(CpiContext::new_with_signer(
            a.associated_token_program.to_account_info(),
            associated_token::Create {
                payer: a.creator_pda.to_account_info(),
                associated_token: a.creator_wsol.to_account_info(),
                authority: a.creator_pda.to_account_info(),
                mint: a.wsol_mint.to_account_info(),
                system_program: a.system_program.to_account_info(),
                token_program: a.token_program.to_account_info(),
            },
            &[seeds],
        ))?;
        pump::invoke(PUMP_AMM_PROGRAM, amm, pump::collect_amm_data(), &[], &[])?;
    }

    // 3. Unwrap, book the split, keep the wSOL account ready.
    finish_fees(a, seeds)
}

/// Books whatever creator fees have already reached this launch's creator
/// PDA, WITHOUT calling pump.fun. Anyone can first call pump.fun's (or
/// PumpSwap's) own collect instruction directly, in whatever form pump.fun
/// supports at the time: those instructions are permissionless and pay the
/// creator address, not the caller. This keeps fees flowing even if
/// pump.fun replaces its collect instructions and `collect_fees` stops working.
pub fn handle_sweep_fees(ctx: Context<CollectFees>) -> Result<()> {
    let a = &ctx.accounts;
    let launch_key = a.launch.key();
    require!(a.launch.load()?.state == STATE_LAUNCHED, FlpError::NotLaunched);
    let seeds: &[&[u8]] = &[CREATOR_SEED, launch_key.as_ref(), &[ctx.bumps.creator_pda]];
    finish_fees(a, seeds)
}

/// Shared tail of collect and sweep:
/// 1. unwrap: close the creator PDA's wSOL account if it exists (PumpSwap pays
///    creator fees as wSOL), returning its balance + rent to the PDA;
/// 2. everything above the PDA's reserve is new fees: move it to the launch,
///    10% booked for the protocol, the rest creator / members;
/// 3. recreate the wSOL account (paid from the reserve) so the next external
///    PumpSwap collect has somewhere to pay into, at no cost to whoever runs it;
/// 4. tip the caller from the operations budget (at most hourly, while HODL
///    Rewards drops remain, and only when real fees came in).
/// Invariant after every call: PDA lamports + wSOL account rent = CREATOR_PDA_RESERVE.
pub(crate) fn finish_fees<'info>(a: &CollectFees<'info>, seeds: &[&[u8]]) -> Result<()> {
    let cp = a.creator_pda.key();
    let launch_key = a.launch.key();
    require_keys_eq!(a.creator_wsol.key(), get_associated_token_address_with_program_id(&cp, &WSOL_MINT, &token::ID), FlpError::AccountMismatch);
    if a.creator_wsol.owner == &token::ID && !a.creator_wsol.data_is_empty() {
        invoke_signed(
            &token::spl_token::instruction::close_account(&token::ID, &a.creator_wsol.key(), &cp, &cp, &[])?,
            &[a.creator_wsol.to_account_info(), a.creator_pda.to_account_info(), a.creator_pda.to_account_info()],
            &[seeds],
        )?;
    }

    let new = a.creator_pda.lamports().saturating_sub(CREATOR_PDA_RESERVE);
    let mut tip_due = false;
    let mut ops_bump = 0u8;
    if new > 0 {
        system_program::transfer(
            CpiContext::new_with_signer(
                a.system_program.to_account_info(),
                system_program::Transfer { from: a.creator_pda.to_account_info(), to: a.launch.to_account_info() },
                &[seeds],
            ),
            new,
        )?;
        let now = Clock::get()?.unix_timestamp;
        let mut l = a.launch.load_mut()?;
        let split = math::split_fees(new, l.creator_fee_bps);
        l.creator_owed += split.creator;
        l.members_pot += split.members;
        l.protocol_owed += split.protocol;
        l.fees_collected_total += new;
        l.protocol_fees_total += split.protocol;
        if l.hodl_active() && new >= COLLECT_TIP_MIN_FEES && now >= l.last_collect_tip_at + ROUND_INTERVAL {
            l.last_collect_tip_at = now;
            tip_due = true;
        }
        ops_bump = l.ops_bump;
        emit!(FeesCollected { launch: launch_key, amount: new, protocol: split.protocol, creator: split.creator, members: split.members });
    }

    associated_token::create_idempotent(CpiContext::new_with_signer(
        a.associated_token_program.to_account_info(),
        associated_token::Create {
            payer: a.creator_pda.to_account_info(),
            associated_token: a.creator_wsol.to_account_info(),
            authority: a.creator_pda.to_account_info(),
            mint: a.wsol_mint.to_account_info(),
            system_program: a.system_program.to_account_info(),
            token_program: a.token_program.to_account_info(),
        },
        &[seeds],
    ))?;
    if tip_due {
        ops::tip(&a.ops.to_account_info(), &a.caller.to_account_info(), &a.system_program.to_account_info(), &launch_key, ops_bump, COLLECT_TIP)?;
    }
    Ok(())
}

// ---------------------------------------------------------------- round

#[derive(Accounts)]
pub struct PayoutRound<'info> {
    /// Whoever runs the round; reimbursed from the pot when it pays, otherwise
    /// tipped from the operations budget while HODL Rewards drops remain.
    #[account(mut)]
    pub caller: Signer<'info>,
    #[account(mut)]
    pub launch: AccountLoader<'info, Launch>,
    /// CHECK: this launch's operations budget PDA, checked by seeds.
    #[account(mut, seeds = [OPS_SEED, launch.key().as_ref()], bump)]
    pub ops: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

/// At most hourly. `remaining_accounts`: every member's Token-2022 account for
/// the launch mint, in slot order. All balances are read at the same moment:
/// each member's lowest holding ever seen only goes down. Then any HODL
/// Rewards drop that is due is released (pass/fail at 75%), and if the Fee
/// Share pot has at least 0.25 SOL per member it is split by those lowest
/// holdings (straight line), so sellers' shares go to members who kept holding.
pub fn handle_payout_round(ctx: Context<PayoutRound>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let launch_key = ctx.accounts.launch.key();
    let launch_info = ctx.accounts.launch.to_account_info();
    let mut l = ctx.accounts.launch.load_mut()?;
    require!(l.state == STATE_LAUNCHED, FlpError::NotLaunched);
    require!(now >= l.last_round_at + ROUND_INTERVAL, FlpError::TooEarly);
    let n = l.filled as usize;
    require!(ctx.remaining_accounts.len() == n, FlpError::AccountCount);
    let was_active = l.hodl_active();

    // Snapshot: update everyone's lowest holding. A wallet holding 2 slots is measured against both slots'
    // tokens (17 Oct): selling half of a 2-slot holding is 50%, never "100% of one slot".
    let mint = l.mint;
    let per_slot = l.per_member_tokens;
    let mut weights = [0u16; MAX_SLOTS as usize];
    for i in 0..n {
        let m = &mut l.members[i];
        let held = if m.flags & FLAG_CLAIMED == 0 {
            BPS as u16 // unclaimed tokens are still held for them in the vault
        } else {
            let info = &ctx.remaining_accounts[i];
            require_keys_eq!(*info.key, get_associated_token_address_with_program_id(&m.wallet, &mint, &token_2022::ID), FlpError::AccountMismatch);
            math::held_bps(pump::token_balance(info, &token_2022::ID), per_slot.saturating_mul(m.slot_count() as u64))
        };
        m.lowest_bps = m.lowest_bps.min(held);
        // Fee Share weight: holding × slots (at most 10,000 × 2, fits a u16)
        weights[i] = m.lowest_bps * m.slot_count() as u16;
    }
    l.last_round_at = now;

    // HODL Rewards: release every drop that is due. Slots whose wallet has never held less than 75% share it
    // equally (a 2-slot wallet gets two shares); if nobody qualifies it is burned.
    while (l.hodl_drops_done as usize) < HODL_DROP_TIMES.len() && now >= l.launched_at + HODL_DROP_TIMES[l.hodl_drops_done as usize] {
        let k = l.hodl_drops_done as usize;
        let amount = math::hodl_drop_amount(l.hodl_reserve, l.hodl_released, k);
        let qualified: u64 = l.members[..n].iter().filter(|m| m.lowest_bps >= HODL_MIN_HELD_BPS).map(|m| m.slot_count() as u64).sum();
        let (each, to_burn) = math::split_drop(amount, qualified);
        for m in l.members[..n].iter_mut().filter(|m| m.lowest_bps >= HODL_MIN_HELD_BPS) {
            m.hodl_owed += each * m.slot_count() as u64;
        }
        l.hodl_released += amount;
        l.hodl_burn_pending += to_burn;
        l.hodl_drops_done += 1;
        emit!(HodlDrop { launch: launch_key, drop: k as u8, amount, qualified: qualified as u8, each, burned: to_burn });
    }

    let pot = l.members_pot;
    let ops_bump = l.ops_bump;
    if pot < MIN_PAYOUT_PER_MEMBER * l.slots_taken() as u64 {
        emit!(Round { launch: launch_key, paid: 0, pot });
        drop(l);
        if was_active {
            let a = &ctx.accounts;
            ops::tip(&a.ops.to_account_info(), &a.caller.to_account_info(), &a.system_program.to_account_info(), &launch_key, ops_bump, ROUND_TIP)?;
        }
        return Ok(());
    }
    let reimbursement = ROUND_REIMBURSEMENT.min(pot);
    let pot = pot - reimbursement;
    let mut shares = [0u64; MAX_SLOTS as usize];
    let distributed = math::distribute(pot, &weights[..n], &mut shares[..n]);
    // Nobody holds at all: the pot goes to the protocol (and so to the burn).
    let to_protocol = if distributed == 0 { pot } else { 0 };
    for i in 0..n {
        l.members[i].owed += shares[i];
    }
    l.members_pot = pot - distributed - to_protocol;
    l.protocol_owed += to_protocol;
    l.protocol_fees_total += to_protocol;
    let left = l.members_pot;
    drop(l);
    debit(&launch_info, &ctx.accounts.caller.to_account_info(), reimbursement)?;
    emit!(Round { launch: launch_key, paid: distributed, pot: left });
    Ok(())
}

// ---------------------------------------------------------------- spot check

#[derive(Accounts)]
pub struct SpotCheck<'info> {
    /// Whoever runs the check; tipped when it catches a member below the hold line.
    #[account(mut)]
    pub caller: Signer<'info>,
    #[account(mut)]
    pub launch: AccountLoader<'info, Launch>,
    /// CHECK: this launch's operations budget PDA, checked by seeds.
    #[account(mut, seeds = [OPS_SEED, launch.key().as_ref()], bump)]
    pub ops: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

/// Re-reads the holdings of the members at slot indices `members` right now
/// (`remaining_accounts`: their Token-2022 accounts for the coin, same order)
/// and lowers each one's lowest holding if it dropped. Meant to run right after
/// a sale is seen, so selling and buying back between hourly rounds is caught.
/// Releases nothing and pays no fees; tips the caller once per member who is
/// caught falling below the 75% hold line while HODL Rewards drops remain.
pub fn handle_spot_check(ctx: Context<SpotCheck>, members: Vec<u8>) -> Result<()> {
    require!(members.len() == ctx.remaining_accounts.len() && members.len() <= MAX_SLOTS as usize, FlpError::AccountCount);
    let launch_key = ctx.accounts.launch.key();
    let mut l = ctx.accounts.launch.load_mut()?;
    require!(l.state == STATE_LAUNCHED, FlpError::NotLaunched);
    let active = l.hodl_active();
    let (mint, per_slot, n) = (l.mint, l.per_member_tokens, l.filled as usize);
    let mut caught = 0u64;
    for (k, &i) in members.iter().enumerate() {
        let i = i as usize;
        require!(i < n, FlpError::NotMember);
        let m = &mut l.members[i];
        if m.flags & FLAG_CLAIMED == 0 {
            continue; // unclaimed tokens are held for them in the vault
        }
        let info = &ctx.remaining_accounts[k];
        require_keys_eq!(*info.key, get_associated_token_address_with_program_id(&m.wallet, &mint, &token_2022::ID), FlpError::AccountMismatch);
        // measured against all the wallet's slots, like the hourly round
        let held = math::held_bps(pump::token_balance(info, &token_2022::ID), per_slot.saturating_mul(m.slot_count() as u64));
        let before = m.lowest_bps;
        m.lowest_bps = before.min(held);
        if active && before >= HODL_MIN_HELD_BPS && m.lowest_bps < HODL_MIN_HELD_BPS && m.flags & FLAG_SPOT_TIPPED == 0 {
            m.flags |= FLAG_SPOT_TIPPED;
            caught += 1;
            emit!(HoldLineBroken { launch: launch_key, member: m.wallet, held_bps: held });
        }
    }
    let ops_bump = l.ops_bump;
    drop(l);
    let a = &ctx.accounts;
    ops::tip(&a.ops.to_account_info(), &a.caller.to_account_info(), &a.system_program.to_account_info(), &launch_key, ops_bump, SPOT_TIP * caught)?;
    Ok(())
}

// ---------------------------------------------------------------- pay out

#[derive(Accounts)]
pub struct PayOut<'info> {
    #[account(mut)]
    pub launch: AccountLoader<'info, Launch>,
}

/// Anyone sends owed fees. `remaining_accounts`: wallets (writable) of members
/// and/or the launch's fee wallet (creator's share), any order. A wallet that
/// can't receive the amount (a program address, or empty and below Solana's
/// minimum balance) is skipped and keeps its balance owed, so one bad address
/// never blocks everyone else's payout.
pub fn handle_pay_out(ctx: Context<PayOut>) -> Result<()> {
    let launch_info = ctx.accounts.launch.to_account_info();
    let floor = Rent::get()?.minimum_balance(Launch::SPACE);
    let mut total = 0u64;
    let mut l = ctx.accounts.launch.load_mut()?;
    for wallet in ctx.remaining_accounts {
        let mut amount = 0u64;
        let is_fee_wallet = *wallet.key == l.fee_wallet && l.creator_owed > 0;
        if is_fee_wallet {
            amount += l.creator_owed;
        }
        let idx = l.member_index(wallet.key);
        if let Some(i) = idx {
            amount += l.members[i].owed;
        }
        if amount == 0 || wallet.executable || (wallet.lamports() == 0 && amount < RENT_EXEMPT_ZERO) {
            continue;
        }
        // Owed amounts are always backed by this account's lamports; never dip below its rent.
        if launch_info.lamports().saturating_sub(amount) < floor {
            continue;
        }
        if is_fee_wallet {
            l.creator_owed = 0;
        }
        if let Some(i) = idx {
            l.members[i].owed = 0;
        }
        debit(&launch_info, wallet, amount)?;
        total += amount;
    }
    l.fees_paid_total += total;
    emit!(PaidOut { launch: ctx.accounts.launch.key(), total });
    Ok(())
}

#[event]
pub struct HodlDrop {
    pub launch: Pubkey,
    pub drop: u8,
    pub amount: u64,
    pub qualified: u8,
    pub each: u64,
    pub burned: u64,
}
#[event]
pub struct HoldLineBroken {
    pub launch: Pubkey,
    pub member: Pubkey,
    pub held_bps: u16,
}
#[event]
pub struct FeesCollected {
    pub launch: Pubkey,
    pub amount: u64,
    pub protocol: u64,
    pub creator: u64,
    pub members: u64,
}
#[event]
pub struct Round {
    pub launch: Pubkey,
    pub paid: u64,
    pub pot: u64,
}
#[event]
pub struct PaidOut {
    pub launch: Pubkey,
    pub total: u64,
}
