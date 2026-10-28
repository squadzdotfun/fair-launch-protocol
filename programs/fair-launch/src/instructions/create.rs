use anchor_lang::prelude::*;
use anchor_lang::system_program;

use crate::constants::*;
use crate::errors::FlpError;
use crate::math;
use crate::pump;
use crate::state::*;

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct CreateLaunchArgs {
    pub nonce: u64,
    pub slots: u8,
    /// Index into PERIODS: 0 = 24h, 1 = 72h, 2 = 1 week.
    pub period: u8,
    /// Lamports per slot. Raise = slot_price x slots: what pump.fun's whole
    /// curve costs (read live) + fees, up to 3% more.
    pub slot_price: u64,
    /// Creator's share of creator fees (after the protocol's 10%); members get
    /// the rest as the Fee Share. 10000 = creator keeps all of it.
    pub creator_fee_bps: u16,
    /// Receives members' optional donations at launch.
    pub donation_wallet: Pubkey,
    /// Receives the creator's share of creator fees. All zeros = the creator.
    pub fee_wallet: Pubkey,
    /// Key that signs join passes. All zeros = anyone can join.
    pub gatekeeper: Pubkey,
    /// Required ending of the coin address (4 base58 characters, e.g. `fair`).
    /// All zeros = no requirement.
    pub ending: [u8; ENDING_LEN],
    pub name: String,
    pub symbol: String,
    pub uri: String,
}

#[derive(Accounts)]
#[instruction(args: CreateLaunchArgs)]
pub struct CreateLaunch<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,
    #[account(
        init,
        payer = creator,
        space = Launch::SPACE,
        seeds = [LAUNCH_SEED, creator.key().as_ref(), &args.nonce.to_le_bytes()],
        bump
    )]
    pub launch: AccountLoader<'info, Launch>,
    /// CHECK: this launch's operations budget PDA (system-owned); funded here.
    #[account(mut, seeds = [OPS_SEED, launch.key().as_ref()], bump)]
    pub ops: UncheckedAccount<'info>,
    /// CHECK: pump.fun Global, owner and address checked when read.
    pub pump_global: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

pub fn handle_create_launch(ctx: Context<CreateLaunch>, args: CreateLaunchArgs) -> Result<()> {
    let a = &args;
    require!((MIN_SLOTS..=MAX_SLOTS).contains(&a.slots), FlpError::BadSlots);
    require!((a.period as usize) < PERIODS.len(), FlpError::BadPeriod);
    require!(a.creator_fee_bps as u64 <= BPS, FlpError::BadFeeSplit);
    require!(
        (1..=MAX_NAME_LEN).contains(&a.name.len())
            && (1..=MAX_SYMBOL_LEN).contains(&a.symbol.len())
            && a.uri.len() <= MAX_URI_LEN,
        FlpError::BadMetadata
    );
    require!(math::ending_ok(&a.ending), FlpError::BadEnding);
    let creator = ctx.accounts.creator.key();
    require!(a.donation_wallet != Pubkey::default(), FlpError::BadWallets);

    // The raise is what pump.fun's whole curve costs (read live) + fees, up to 3% more. $SQUADZ (launch #1) launches
    // on its fixed Meteora curve instead, with a flat launch fee (22 Oct): same rule, that curve's numbers.
    let raise = a.slot_price.checked_mul(a.slots as u64).ok_or(FlpError::Overflow)?;
    let (min, max) = if ctx.accounts.launch.key() == SQUADZ_LAUNCH {
        math::meteora_raise_range(SQUADZ_CURVE_THRESHOLD, SQUADZ_CURVE_FEE_NUMERATOR, SQUADZ_LAUNCH_FEE)
    } else {
        let g = pump::read_global(&ctx.accounts.pump_global)?;
        math::raise_range(g.initial_virtual_sol_reserves, g.initial_virtual_token_reserves, g.initial_real_token_reserves)
    }
    .ok_or(FlpError::RaiseTooSmall)?;
    require!(raise >= min, FlpError::RaiseTooSmall);
    require!(raise <= max, FlpError::RaiseTooLarge);

    // The creation fee for this window (0.05 / 0.10 / 0.20 SOL, fix #18): the launch account's storage deposit
    // (already paid by `init`) plus the rest to this launch's operations budget.
    let deposit = Rent::get()?.minimum_balance(Launch::SPACE);
    let budget = CREATION_FEES[a.period as usize].checked_sub(deposit).filter(|b| *b >= RENT_EXEMPT_ZERO).ok_or(FlpError::Overflow)?;
    system_program::transfer(
        CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            system_program::Transfer { from: ctx.accounts.creator.to_account_info(), to: ctx.accounts.ops.to_account_info() },
        ),
        budget,
    )?;

    let key = ctx.accounts.launch.key();
    require!(a.fee_wallet == Pubkey::default() || fee_wallet_ok(&a.fee_wallet, &key), FlpError::BadWallets);
    // Audit run 6 (variant run): donations may not go to one of this lobby's own accounts either: in the launch
    // account they'd be stranded (never booked), in the creator PDA they'd be re-booked as creator fees.
    require!(fee_wallet_ok(&a.donation_wallet, &key), FlpError::BadWallets);
    let now = Clock::get()?.unix_timestamp;
    let mut l = ctx.accounts.launch.load_init()?;
    l.nonce = a.nonce;
    l.created_at = now;
    l.period_end = now + PERIODS[a.period as usize];
    l.slot_price = a.slot_price;
    l.creator = creator;
    l.donation_wallet = a.donation_wallet;
    l.fee_wallet = if a.fee_wallet == Pubkey::default() { creator } else { a.fee_wallet };
    l.gatekeeper = a.gatekeeper;
    l.ending = a.ending;
    l.creator_fee_bps = a.creator_fee_bps;
    l.state = STATE_OPEN;
    l.slots = a.slots;
    l.bump = ctx.bumps.launch;
    l.ops_bump = ctx.bumps.ops;
    l.vault_bump = Pubkey::find_program_address(&[VAULT_SEED, key.as_ref()], &crate::ID).1;
    l.creator_bump = Pubkey::find_program_address(&[CREATOR_SEED, key.as_ref()], &crate::ID).1;
    l.name_len = a.name.len() as u8;
    l.symbol_len = a.symbol.len() as u8;
    l.uri_len = a.uri.len() as u8;
    l.name[..a.name.len()].copy_from_slice(a.name.as_bytes());
    l.symbol[..a.symbol.len()].copy_from_slice(a.symbol.as_bytes());
    l.uri[..a.uri.len()].copy_from_slice(a.uri.as_bytes());

    emit!(LaunchCreated {
        launch: key,
        creator,
        slots: a.slots,
        slot_price: a.slot_price,
        period_end: l.period_end,
        gatekeeper: a.gatekeeper,
        ending: a.ending,
        ops_budget: budget,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct SetFeeWallet<'info> {
    pub creator: Signer<'info>,
    #[account(mut)]
    pub launch: AccountLoader<'info, Launch>,
}

/// Creator points their share of creator fees at another wallet (e.g. a
/// rewards distributor). Only while the launch is open: from launch onwards
/// the fee wallet is locked forever. Members' shares are unaffected.
pub fn handle_set_fee_wallet(ctx: Context<SetFeeWallet>, fee_wallet: Pubkey) -> Result<()> {
    require!(fee_wallet_ok(&fee_wallet, &ctx.accounts.launch.key()), FlpError::BadWallets);
    let mut l = ctx.accounts.launch.load_mut()?;
    require_keys_eq!(l.creator, ctx.accounts.creator.key(), FlpError::NotCreator);
    require!(l.state == STATE_OPEN, FlpError::FeeWalletLocked);
    l.fee_wallet = fee_wallet;
    emit!(FeeWalletSet { launch: ctx.accounts.launch.key(), fee_wallet });
    Ok(())
}

#[derive(Accounts)]
pub struct SetApproval<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,
    #[account(mut)]
    pub launch: AccountLoader<'info, Launch>,
    /// CHECK: this launch's operations budget PDA (system-owned); receives the fee.
    #[account(mut, seeds = [OPS_SEED, launch.key().as_ref()], bump)]
    pub ops: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

/// Approval-only lobby (fix #18, founders 22 Oct): +0.05 SOL to the operations budget, in the creation transaction
/// only (before anyone has joined, in the same second the launch was created), like `add_treasury`. Needs a
/// gatekeeper: approval is the gatekeeper only signing passes for people the creator approved. Can't be undone.
pub fn handle_set_approval(ctx: Context<SetApproval>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    {
        let mut l = ctx.accounts.launch.load_mut()?;
        require_keys_eq!(l.creator, ctx.accounts.creator.key(), FlpError::NotCreator);
        require!(l.state == STATE_OPEN, FlpError::NotOpen);
        require!(l.has_gatekeeper(), FlpError::NoGatekeeper);
        require!(l.filled == 0 && now == l.created_at && !l.approval_only(), FlpError::SettingLocked);
        l.launch_flags |= LF_APPROVAL;
    }
    system_program::transfer(
        CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            system_program::Transfer { from: ctx.accounts.creator.to_account_info(), to: ctx.accounts.ops.to_account_info() },
        ),
        APPROVAL_FEE,
    )?;
    emit!(ApprovalSet { launch: ctx.accounts.launch.key(), fee: APPROVAL_FEE });
    Ok(())
}

#[event]
pub struct ApprovalSet {
    pub launch: Pubkey,
    pub fee: u64,
}

/// A fee wallet must be a real destination: not empty, and not one of this
/// launch's own accounts (paying the launch account, vault or budget would
/// strand the SOL; paying the creator PDA would re-count it as new fees).
fn fee_wallet_ok(w: &Pubkey, launch: &Pubkey) -> bool {
    let own = |seed: &[u8]| Pubkey::find_program_address(&[seed, launch.as_ref()], &crate::ID).0;
    *w != Pubkey::default() && w != launch && *w != own(VAULT_SEED) && *w != own(CREATOR_SEED) && *w != own(OPS_SEED)
}

#[event]
pub struct FeeWalletSet {
    pub launch: Pubkey,
    pub fee_wallet: Pubkey,
}

#[event]
pub struct LaunchCreated {
    pub launch: Pubkey,
    pub creator: Pubkey,
    pub slots: u8,
    pub slot_price: u64,
    pub period_end: i64,
    pub gatekeeper: Pubkey,
    pub ending: [u8; ENDING_LEN],
    pub ops_budget: u64,
}
