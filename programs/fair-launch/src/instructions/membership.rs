use anchor_lang::prelude::*;
use anchor_lang::solana_program::sysvar::instructions as ix_sysvar;
use anchor_lang::system_program;

use crate::constants::*;
use crate::errors::FlpError;
use crate::ops;
use crate::pass;
use crate::state::*;

// ---------------------------------------------------------------- join

#[derive(Accounts)]
pub struct Join<'info> {
    #[account(mut)]
    pub member: Signer<'info>,
    #[account(mut)]
    pub launch: AccountLoader<'info, Launch>,
    /// CHECK: launch vault PDA (system-owned), checked by seeds.
    #[account(mut, seeds = [VAULT_SEED, launch.key().as_ref()], bump)]
    pub vault: UncheckedAccount<'info>,
    /// CHECK: the instructions sysvar (fixed address); used to read the join pass.
    #[account(address = ix_sysvar::ID)]
    pub instructions: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

/// Takes one slot (see `handle_join_slots`). Kept as it was so older clients keep working: in a launch whose
/// creator allows 2 slots per wallet, a member calling it again takes their second slot.
pub fn handle_join(ctx: Context<Join>, donation: u64, pass_expiry: i64) -> Result<()> {
    handle_join_slots(ctx, donation, pass_expiry, 1)
}

/// Takes `count` slots (1 or 2; 17 Oct: one wallet, up to 2 slots): pays the slot price per slot plus an
/// optional donation (0 = none). A wallet that already holds a slot adds to it (its second slot), up to the
/// launch's per-wallet cap the creator set. Everything is held in the vault; the donation goes to the donation
/// wallet only if the launch happens, and is returned with the slots otherwise. If the launch has a gatekeeper,
/// the instruction before this one must verify the gatekeeper's pass for this wallet, valid until `pass_expiry`
/// (ignored for launches without a gatekeeper).
pub fn handle_join_slots(ctx: Context<Join>, donation: u64, pass_expiry: i64, count: u8) -> Result<()> {
    let member = ctx.accounts.member.key();
    let launch_key = ctx.accounts.launch.key();
    let now = Clock::get()?.unix_timestamp;
    require!(donation == 0 || donation >= MIN_DONATION, FlpError::BadDonation);
    require!(count >= 1 && count <= MAX_PER_WALLET, FlpError::BadSlotCount);
    let price;
    {
        let l = ctx.accounts.launch.load()?;
        require!(l.state == STATE_OPEN, FlpError::NotOpen);
        require!(now <= l.period_end, FlpError::PeriodEnded);
        require!(l.slots_taken() + count <= l.slots, FlpError::Full);
        let held = l.member_index(&member).map(|i| l.members[i].slot_count()).unwrap_or(0);
        // a 1-slot launch keeps its old answer for a second join
        require!(held == 0 || l.per_wallet_cap() > 1, FlpError::AlreadyMember);
        require!(held + count <= l.per_wallet_cap(), FlpError::TooManySlots);
        if l.has_gatekeeper() {
            require!(now <= pass_expiry, FlpError::PassExpired);
            pass::verify_pass(&ctx.accounts.instructions, &l.gatekeeper, &launch_key, &member, pass_expiry)?;
        }
        price = l.slot_price.checked_mul(count as u64).ok_or(FlpError::Overflow)?;
    }

    system_program::transfer(
        CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            system_program::Transfer { from: ctx.accounts.member.to_account_info(), to: ctx.accounts.vault.to_account_info() },
        ),
        price.checked_add(donation).ok_or(FlpError::Overflow)?,
    )?;

    let mut l = ctx.accounts.launch.load_mut()?;
    match l.member_index(&member) {
        Some(i) => {
            // their second slot: same record, same donation pot
            let m = &mut l.members[i];
            m.slots = m.slot_count() + count;
            m.donation = m.donation.checked_add(donation).ok_or(FlpError::Overflow)?;
        }
        None => {
            let i = l.filled as usize;
            l.members[i] = Member { owed: 0, donation, hodl_owed: 0, wallet: member, lowest_bps: BPS as u16, flags: 0, slots: count, _pad: [0; 4] };
            l.filled += 1;
        }
    }
    l.donations_total = l.donations_total.checked_add(donation).ok_or(FlpError::Overflow)?;
    emit!(Joined { launch: launch_key, member, filled: l.filled, donation });
    emit!(SlotsTaken { launch: launch_key, member, count, taken: l.slots_taken() });
    Ok(())
}

// ---------------------------------------------------------------- per-wallet cap

#[derive(Accounts)]
pub struct SetMaxPerWallet<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,
    #[account(mut)]
    pub launch: AccountLoader<'info, Launch>,
    /// CHECK: this launch's operations budget PDA (system-owned); receives the two-slot fee.
    #[account(mut, seeds = [OPS_SEED, launch.key().as_ref()], bump)]
    pub ops: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

/// The creator sets how many slots one wallet may hold (1 or 2), at creation only: before anyone has joined (the
/// site does it in the creation transaction, before the creator's own seat). Audit run 5: raising it mid-lobby let a
/// creator take extra slots after people joined under "1 per person". Fix #18 (22 Oct): going to 2 costs +0.10 SOL
/// (to the operations budget), each time it goes from 1 to 2.
pub fn handle_set_max_per_wallet(ctx: Context<SetMaxPerWallet>, max: u8) -> Result<()> {
    require!(max >= 1 && max <= MAX_PER_WALLET, FlpError::BadSlotCount);
    let fee;
    {
        let mut l = ctx.accounts.launch.load_mut()?;
        require_keys_eq!(l.creator, ctx.accounts.creator.key(), FlpError::NotCreator);
        require!(l.state == STATE_OPEN, FlpError::NotOpen);
        require!(l.filled == 0, FlpError::SlotRuleLocked);
        fee = if max > 1 && l.per_wallet_cap() < 2 { TWO_SLOT_FEE } else { 0 };
        l.max_per_wallet = max;
    }
    if fee > 0 {
        system_program::transfer(
            CpiContext::new(
                ctx.accounts.system_program.to_account_info(),
                system_program::Transfer { from: ctx.accounts.creator.to_account_info(), to: ctx.accounts.ops.to_account_info() },
            ),
            fee,
        )?;
    }
    emit!(MaxPerWalletSet { launch: ctx.accounts.launch.key(), max });
    Ok(())
}

// ---------------------------------------------------------------- donate any time before launch

#[derive(Accounts)]
pub struct AddDonation<'info> {
    #[account(mut)]
    pub member: Signer<'info>,
    #[account(mut)]
    pub launch: AccountLoader<'info, Launch>,
    /// CHECK: launch vault PDA, checked by seeds.
    #[account(mut, seeds = [VAULT_SEED, launch.key().as_ref()], bump)]
    pub vault: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

/// A member adds to their donation while the lobby is open (fix #3, founders 20 Oct: "everyone chip in 0.2 for
/// Dex"). It joins the same pot as a donation made at join, so every existing path already handles it: refund
/// returns slot + donation, leaving every slot returns it, leaving one slot keeps it, launch pays the whole total to
/// the donation wallet before the buy. Refused after the window, after a refund, and below 0.001 SOL.
pub fn handle_add_donation(ctx: Context<AddDonation>, amount: u64) -> Result<()> {
    let member = ctx.accounts.member.key();
    let launch_key = ctx.accounts.launch.key();
    require!(amount >= MIN_DONATION, FlpError::BadDonation);
    {
        let l = ctx.accounts.launch.load()?;
        require!(l.state == STATE_OPEN, FlpError::NotOpen);
        require!(Clock::get()?.unix_timestamp <= l.period_end, FlpError::PeriodEnded);
        let i = l.member_index(&member).ok_or(FlpError::NotMember)?;
        // a refunded member's SOL has left the vault: a new donation would be stuck there
        require!(l.members[i].flags & FLAG_REFUNDED == 0, FlpError::AlreadyClaimed);
    }
    system_program::transfer(
        CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            system_program::Transfer { from: ctx.accounts.member.to_account_info(), to: ctx.accounts.vault.to_account_info() },
        ),
        amount,
    )?;
    let mut l = ctx.accounts.launch.load_mut()?;
    let i = l.member_index(&member).ok_or(FlpError::NotMember)?;
    l.members[i].donation = l.members[i].donation.checked_add(amount).ok_or(FlpError::Overflow)?;
    l.donations_total = l.donations_total.checked_add(amount).ok_or(FlpError::Overflow)?;
    let total = l.members[i].donation;
    emit!(DonationAdded { launch: launch_key, member, amount, total });
    Ok(())
}

#[event]
pub struct DonationAdded {
    pub launch: Pubkey,
    pub member: Pubkey,
    pub amount: u64,
    /// This member's donation after it.
    pub total: u64,
}

// ---------------------------------------------------------------- leave

#[derive(Accounts)]
pub struct Leave<'info> {
    #[account(mut)]
    pub member: Signer<'info>,
    #[account(mut)]
    pub launch: AccountLoader<'info, Launch>,
    /// CHECK: launch vault PDA, checked by seeds.
    #[account(mut, seeds = [VAULT_SEED, launch.key().as_ref()], bump)]
    pub vault: UncheckedAccount<'info>,
    /// CHECK: protocol burn jar PDA (receives the leave fee).
    #[account(mut, seeds = [BURN_JAR_SEED], bump)]
    pub burn_jar: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

/// Leaves every slot this wallet holds (see `handle_leave_slots`). Kept as it was so older clients keep working.
pub fn handle_leave(ctx: Context<Leave>) -> Result<()> {
    handle_leave_slots(ctx, 0)
}

/// Before launch, a member takes back `count` of their slots (0 = all of them; 17 Oct: a 2-slot member may leave
/// one or both) and those slots reopen. Leaving every slot also returns the donation; leaving one keeps it with
/// the slot that stays. The anti-griefing leave fee (to the burn jar) is charged per slot left, and only while
/// the creator holds a seat (decided 12 Oct): a creator with nothing at stake, or one who left, can't make
/// members pay to get out. The creator's own leave pays it too. Rejoining a gated launch needs a valid pass again.
pub fn handle_leave_slots(ctx: Context<Leave>, count: u8) -> Result<()> {
    let member = ctx.accounts.member.key();
    let launch_key = ctx.accounts.launch.key();
    let (amount, fee, bump);
    {
        let mut l = ctx.accounts.launch.load_mut()?;
        require!(l.state == STATE_OPEN, FlpError::NotOpen);
        let i = l.member_index(&member).ok_or(FlpError::NotMember)?;
        // After the deadline refunds take over; never pay a slot twice.
        require!(l.members[i].flags & FLAG_REFUNDED == 0, FlpError::AlreadyClaimed);
        require!(Clock::get()?.unix_timestamp <= l.period_end, FlpError::PeriodEnded);
        let held = l.members[i].slot_count();
        let count = if count == 0 { held } else { count };
        require!(count <= held, FlpError::BadSlotCount);
        // checked before the seat is removed, so a seated creator's own leave pays it
        let creator = l.creator;
        let creator_seated = l.member_index(&creator).is_some();
        let donation = if count == held {
            // every slot goes: the record goes, and the donation comes back
            let donation = l.members[i].donation;
            let last = l.filled as usize - 1;
            l.members[i] = l.members[last];
            l.members[last] = Member::default();
            l.filled -= 1;
            l.donations_total -= donation;
            donation
        } else {
            l.members[i].slots = held - count;
            0
        };
        fee = if creator_seated { LEAVE_FEE.min(l.slot_price) * count as u64 } else { 0 };
        amount = l.slot_price * count as u64 + donation - fee;
        bump = l.vault_bump;
    }
    pay_from_vault(&ctx.accounts.vault, &ctx.accounts.member.to_account_info(), &ctx.accounts.system_program, &launch_key, bump, amount)?;
    if fee > 0 {
        pay_from_vault(&ctx.accounts.vault, &ctx.accounts.burn_jar.to_account_info(), &ctx.accounts.system_program, &launch_key, bump, fee)?;
    }
    emit!(Left { launch: launch_key, member, fee });
    Ok(())
}

// ---------------------------------------------------------------- cancel / refund

#[derive(Accounts)]
pub struct Cancel<'info> {
    pub creator: Signer<'info>,
    #[account(mut)]
    pub launch: AccountLoader<'info, Launch>,
}

/// Creator cancels before launch. Refunds open immediately; it can never restart.
pub fn handle_cancel(ctx: Context<Cancel>) -> Result<()> {
    let mut l = ctx.accounts.launch.load_mut()?;
    require_keys_eq!(l.creator, ctx.accounts.creator.key(), FlpError::NotCreator);
    require!(l.state == STATE_OPEN, FlpError::NotOpen);
    l.state = STATE_CANCELLED;
    emit!(Cancelled { launch: ctx.accounts.launch.key() });
    Ok(())
}

#[derive(Accounts)]
pub struct Refund<'info> {
    /// Whoever pushes the refunds; tipped from the operations budget.
    #[account(mut)]
    pub caller: Signer<'info>,
    #[account(mut)]
    pub launch: AccountLoader<'info, Launch>,
    /// CHECK: launch vault PDA, checked by seeds.
    #[account(mut, seeds = [VAULT_SEED, launch.key().as_ref()], bump)]
    pub vault: UncheckedAccount<'info>,
    /// CHECK: this launch's operations budget PDA, checked by seeds.
    #[account(mut, seeds = [OPS_SEED, launch.key().as_ref()], bump)]
    pub ops: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

/// Anyone refunds members of a cancelled or expired launch. `remaining_accounts`
/// are member wallets (writable), any order; already-refunded ones are skipped.
pub fn handle_refund<'info>(ctx: Context<'_, '_, 'info, 'info, Refund<'info>>) -> Result<()> {
    let launch_key = ctx.accounts.launch.key();
    let now = Clock::get()?.unix_timestamp;
    let mut refunded = 0u64;
    let mut ops_bump = 0u8;
    for wallet in ctx.remaining_accounts {
        let (amount, bump) = {
            let mut l = ctx.accounts.launch.load_mut()?;
            require!(l.refunds_open(now), FlpError::RefundsNotOpen);
            ops_bump = l.ops_bump;
            let Some(i) = l.member_index(wallet.key) else { continue };
            if l.members[i].flags & FLAG_REFUNDED != 0 {
                continue;
            }
            l.members[i].flags |= FLAG_REFUNDED;
            (l.slot_price * l.members[i].slot_count() as u64 + l.members[i].donation, l.vault_bump)
        };
        pay_from_vault(&ctx.accounts.vault, wallet, &ctx.accounts.system_program, &launch_key, bump, amount)?;
        refunded += 1;
        emit!(Refunded { launch: launch_key, member: *wallet.key });
    }
    let a = &ctx.accounts;
    ops::tip(&a.ops.to_account_info(), &a.caller.to_account_info(), &a.system_program.to_account_info(), &launch_key, ops_bump, REFUND_TIP * refunded)?;
    Ok(())
}

pub fn pay_from_vault<'info>(
    vault: &UncheckedAccount<'info>,
    to: &AccountInfo<'info>,
    system: &Program<'info, System>,
    launch: &Pubkey,
    bump: u8,
    amount: u64,
) -> Result<()> {
    // Audit run 5: anyone can send a few lamports to the vault. A payout that would leave less than an empty
    // account's rent minimum (but more than 0) is refused by Solana, which froze the last leave / refund / claim.
    // Then the whole remainder goes with it (at most ~0.00089 SOL of dust, which nobody is owed).
    let bal = vault.lamports();
    let rest = bal.saturating_sub(amount);
    let amount = if rest > 0 && rest < Rent::get()?.minimum_balance(0) { bal } else { amount };
    system_program::transfer(
        CpiContext::new_with_signer(
            system.to_account_info(),
            system_program::Transfer { from: vault.to_account_info(), to: to.clone() },
            &[&[VAULT_SEED, launch.as_ref(), &[bump]]],
        ),
        amount,
    )
}

#[event]
pub struct Joined {
    pub launch: Pubkey,
    pub member: Pubkey,
    pub filled: u8,
    pub donation: u64,
}
/// Slots a wallet took in one join (1 or 2) and the launch's slots taken after it (a wallet can hold 2).
#[event]
pub struct SlotsTaken {
    pub launch: Pubkey,
    pub member: Pubkey,
    pub count: u8,
    pub taken: u8,
}
#[event]
pub struct MaxPerWalletSet {
    pub launch: Pubkey,
    pub max: u8,
}
#[event]
pub struct Left {
    pub launch: Pubkey,
    pub member: Pubkey,
    /// Anti-griefing fee kept and sent to the burn jar.
    pub fee: u64,
}
#[event]
pub struct Cancelled {
    pub launch: Pubkey,
}
#[event]
pub struct Refunded {
    pub launch: Pubkey,
    pub member: Pubkey,
}
