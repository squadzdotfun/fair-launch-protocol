//! The launch: one instruction that books the launch fee, pays donations,
//! creates the coin on pump.fun with a protocol PDA as creator, buys the
//! ENTIRE curve with the pool and checks the result. Members then claim equal
//! shares. Any failure undoes everything.
//!
//! The coin address is a PDA of (launch, coin number). The number is chosen
//! off-chain so the address ends in the launch's ending (e.g. `...fair`) and
//! is only revealed by this transaction. Only this program can sign for that
//! address, so nobody can create a coin there first.

use anchor_lang::prelude::*;
use anchor_lang::system_program;
use anchor_spl::associated_token::{self, get_associated_token_address_with_program_id, AssociatedToken};
use anchor_spl::token_2022;

use crate::constants::*;
use crate::errors::FlpError;
use crate::math;
use crate::pump::{self, buy_ix, create_ix, BUY_V2_LEN, CREATE_V2_LEN};
use crate::state::*;

#[derive(Accounts)]
pub struct LaunchToken<'info> {
    pub creator: Signer<'info>,
    #[account(mut)]
    pub launch: AccountLoader<'info, Launch>,
    /// CHECK: pool vault PDA (system-owned): pays and buys on pump.fun.
    #[account(mut, seeds = [VAULT_SEED, launch.key().as_ref()], bump)]
    pub vault: UncheckedAccount<'info>,
    /// CHECK: this launch's pump.fun creator PDA; receives its creator fees.
    #[account(mut, seeds = [CREATOR_SEED, launch.key().as_ref()], bump)]
    pub creator_pda: UncheckedAccount<'info>,
    /// CHECK: the coin's mint PDA (launch + coin number); checked in the handler, created by pump.fun.
    #[account(mut)]
    pub mint: UncheckedAccount<'info>,
    /// CHECK: vault's Token-2022 account for the mint; address checked.
    #[account(mut)]
    pub vault_token: UncheckedAccount<'info>,
    /// CHECK: must equal the launch's donation wallet.
    #[account(mut)]
    pub donation_wallet: UncheckedAccount<'info>,
    /// CHECK: pump.fun program, fixed address.
    #[account(address = PUMP_PROGRAM)]
    pub pump_program: UncheckedAccount<'info>,
    /// CHECK: Token-2022 program, fixed address.
    #[account(address = token_2022::ID)]
    pub token_2022_program: UncheckedAccount<'info>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

/// `create_len`: how many of `remaining_accounts` belong to pump.fun's
/// create_v2 (16 today); the rest are buy_v2's (27 today). pump.fun appends
/// new accounts at the end, so later additions pass through unchanged while
/// every ownership check below stays at a fixed position.
/// `coin_number` + `mint_bump` give the coin address: PDA of
/// (MINT_SEED, launch, coin_number).
pub fn handle_launch_token<'info>(ctx: Context<'_, '_, 'info, 'info, LaunchToken<'info>>, create_len: u8, coin_number: u64, mint_bump: u8) -> Result<()> {
    let ra = ctx.remaining_accounts;
    let create_len = create_len as usize;
    require!(create_len >= CREATE_V2_LEN && ra.len() >= create_len + BUY_V2_LEN, FlpError::AccountCount);
    let (c, b) = ra.split_at(create_len);
    let a = &ctx.accounts;
    let launch_key = a.launch.key();
    // $SQUADZ launches on Meteora only (launch_token_meteora).
    require_keys_neq!(launch_key, SQUADZ_LAUNCH, FlpError::WrongVenue);
    let now = Clock::get()?.unix_timestamp;

    let (raise, slots, donations, name, symbol, uri, ending, has_ending);
    {
        let l = a.launch.load()?;
        require_keys_eq!(l.creator, a.creator.key(), FlpError::NotCreator);
        require!(l.state == STATE_OPEN, FlpError::NotOpen);
        require!(now <= l.period_end, FlpError::PeriodEnded);
        // slots, not wallets: a wallet can hold 2 (17 Oct)
        require!(l.slots_taken() == l.slots, FlpError::NotFull);
        require_keys_eq!(l.donation_wallet, a.donation_wallet.key(), FlpError::AccountMismatch);
        raise = l.raise();
        slots = l.slots;
        donations = l.donations_total;
        name = l.name();
        symbol = l.symbol();
        uri = l.uri();
        ending = l.ending;
        has_ending = l.has_ending();
    }

    // The coin address: our PDA for this number, with the launch's ending.
    let number = coin_number.to_le_bytes();
    let mint = Pubkey::create_program_address(&[MINT_SEED, launch_key.as_ref(), &number, &[mint_bump]], &crate::ID)
        .map_err(|_| error!(FlpError::AccountMismatch))?;
    require_keys_eq!(a.mint.key(), mint, FlpError::AccountMismatch);
    require!(!has_ending || math::ends_with_base58(&mint.to_bytes(), &ending), FlpError::WrongEnding);

    // Ownership-deciding accounts must be ours; pump.fun checks the rest.
    let vault = a.vault.key();
    let t22 = token_2022::ID;
    require_keys_eq!(c[create_ix::MINT].key(), mint, FlpError::AccountMismatch);
    require_keys_eq!(c[create_ix::USER].key(), vault, FlpError::AccountMismatch);
    require_keys_eq!(c[create_ix::PROGRAM].key(), PUMP_PROGRAM, FlpError::AccountMismatch);
    require_keys_eq!(b[buy_ix::BASE_MINT].key(), mint, FlpError::AccountMismatch);
    require_keys_eq!(b[buy_ix::USER].key(), vault, FlpError::AccountMismatch);
    require_keys_eq!(b[buy_ix::ASSOCIATED_BASE_USER].key(), a.vault_token.key(), FlpError::AccountMismatch);
    require_keys_eq!(b[buy_ix::PROGRAM].key(), PUMP_PROGRAM, FlpError::AccountMismatch);
    require_keys_eq!(b[buy_ix::BONDING_CURVE].key(), c[create_ix::BONDING_CURVE].key(), FlpError::AccountMismatch);
    require_keys_eq!(a.vault_token.key(), get_associated_token_address_with_program_id(&vault, &mint, &t22), FlpError::AccountMismatch);

    let vault_seeds: &[&[u8]] = &[VAULT_SEED, launch_key.as_ref(), &[ctx.bumps.vault]];
    let mint_seeds: &[&[u8]] = &[MINT_SEED, launch_key.as_ref(), &number, &[mint_bump]];
    let vault_transfer = |to: &AccountInfo<'info>, amount: u64| -> Result<()> {
        if amount == 0 {
            return Ok(());
        }
        system_program::transfer(
            CpiContext::new_with_signer(
                a.system_program.to_account_info(),
                system_program::Transfer { from: a.vault.to_account_info(), to: to.clone() },
                &[vault_seeds],
            ),
            amount,
        )
    };

    // 1. Protocol launch fee (held in the launch account until swept to the
    //    burn jar), donations (0 or >= 0.001 SOL, so always receivable) and
    //    the creator PDA's reserve.
    let launch_fee = math::bps(raise, LAUNCH_FEE_BPS);
    vault_transfer(&a.launch.to_account_info(), launch_fee)?;
    vault_transfer(&a.donation_wallet.to_account_info(), donations)?;
    vault_transfer(&a.creator_pda.to_account_info(), CREATOR_PDA_RESERVE.saturating_sub(a.creator_pda.lamports()))?;

    // 2. Create the coin on pump.fun: vault pays, mint + vault are our PDAs.
    pump::invoke(
        PUMP_PROGRAM,
        c,
        pump::create_v2_data(name, symbol, uri, a.creator_pda.key()),
        &[(create_ix::USER, vault), (create_ix::MINT, mint)],
        &[vault_seeds, mint_seeds],
    )?;
    associated_token::create_idempotent(CpiContext::new_with_signer(
        a.associated_token_program.to_account_info(),
        associated_token::Create {
            payer: a.vault.to_account_info(),
            associated_token: a.vault_token.to_account_info(),
            authority: a.vault.to_account_info(),
            mint: a.mint.to_account_info(),
            system_program: a.system_program.to_account_info(),
            token_program: a.token_2022_program.to_account_info(),
        },
        &[vault_seeds],
    ))?;

    // 3. Buy everything the new curve has for sale; pump.fun prices it exactly.
    let fresh = pump::read_curve(&c[create_ix::BONDING_CURVE])?;
    // SOL-priced, standard coin only (extra create accounts could select another quote asset).
    require!(fresh.quote_mint == Pubkey::default() && !fresh.is_mayhem_mode, FlpError::AccountMismatch);
    let for_sale = fresh.real_token_reserves;
    let before = a.vault.lamports();
    let max_sol = before.saturating_sub(VAULT_BUY_RESERVE);
    pump::invoke(PUMP_PROGRAM, b, pump::buy_v2_data(for_sale, max_sol), &[(buy_ix::USER, vault)], &[vault_seeds])?;

    // 4. Airtight checks.
    let curve = pump::read_curve(&c[create_ix::BONDING_CURVE])?;
    require!(curve.complete && curve.real_token_reserves == 0, FlpError::CurveNotComplete);
    require_keys_eq!(curve.creator, a.creator_pda.key(), FlpError::AccountMismatch);
    let received = pump::token_balance(&a.vault_token, &t22);
    require!(received == for_sale, FlpError::WrongTokenAmount);
    let spent = before - a.vault.lamports();

    // 5. 33% of total supply is held back for HODL Rewards; members share the
    //    rest equally, and every lamport the buy didn't need (the vault keeps
    //    only its rent). Rounding dust joins the reserve / stays in the vault.
    let treasury_bps = a.launch.load()?.treasury_bps;
    let (per_member, hodl_reserve, _treasury) = math::split_tokens(received, curve.token_total_supply, slots, treasury_bps).ok_or(FlpError::WrongTokenAmount)?;
    let leftover = a.vault.lamports().saturating_sub(RENT_EXEMPT_ZERO) / slots as u64;

    let mut l = a.launch.load_mut()?;
    l.state = STATE_LAUNCHED;
    l.launched_at = now;
    l.last_round_at = now;
    l.mint = mint;
    l.mint_bump = mint_bump;
    l.coin_number = coin_number;
    l.tokens_bought = received;
    l.per_member_tokens = per_member;
    l.hodl_reserve = hodl_reserve;
    l.leftover_per_member = leftover;
    l.sol_spent = spent;
    l.protocol_owed += launch_fee;
    l.protocol_fees_total += launch_fee;
    emit!(Launched {
        launch: launch_key,
        mint,
        tokens_bought: received,
        sol_spent: spent,
        per_member_tokens: per_member,
        hodl_reserve,
        leftover_per_member: leftover,
        donations,
        launch_fee,
    });
    Ok(())
}

#[event]
pub struct Launched {
    pub launch: Pubkey,
    pub mint: Pubkey,
    pub tokens_bought: u64,
    pub sol_spent: u64,
    pub per_member_tokens: u64,
    pub hodl_reserve: u64,
    pub leftover_per_member: u64,
    pub donations: u64,
    pub launch_fee: u64,
}
