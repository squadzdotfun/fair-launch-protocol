//! $SQUADZ's launch on Meteora (founders 22 Oct 2026). The same launch as launch.rs, with the same checks, launch
//! fee, donations and equal shares, but the coin is created on Meteora's Dynamic Bonding Curve, with the launch's
//! creator PDA as its creator, and the pool buys that ENTIRE curve in the same instruction. Only launch #1 ($SQUADZ)
//! can use it; launch_token refuses launch #1. Any failure undoes everything.
//!
//! The coin address is the same kind of PDA as on pump.fun (launch + coin number, ending e.g. `...fair`): only this
//! program can sign for it, so nobody can create the coin first.

use anchor_lang::prelude::*;
use anchor_lang::system_program;
use anchor_spl::associated_token::{self, get_associated_token_address_with_program_id, AssociatedToken};
use anchor_spl::{token, token_2022};

use crate::constants::*;
use crate::errors::FlpError;
use crate::instructions::launch::Launched;
use crate::math;
use crate::meteora::{self, init_ix, swap_ix, INIT_LEN, SWAP_LEN};
use crate::pump;
use crate::state::*;

#[derive(Accounts)]
pub struct LaunchTokenMeteora<'info> {
    pub creator: Signer<'info>,
    #[account(mut)]
    pub launch: AccountLoader<'info, Launch>,
    /// CHECK: pool vault PDA (system-owned): pays Meteora's accounts and buys the curve.
    #[account(mut, seeds = [VAULT_SEED, launch.key().as_ref()], bump)]
    pub vault: UncheckedAccount<'info>,
    /// CHECK: this launch's creator PDA: the coin's creator on Meteora (curve fees, the locked pool position).
    #[account(mut, seeds = [CREATOR_SEED, launch.key().as_ref()], bump)]
    pub creator_pda: UncheckedAccount<'info>,
    /// CHECK: the coin's mint PDA (launch + coin number); checked in the handler, created by Meteora.
    #[account(mut)]
    pub mint: UncheckedAccount<'info>,
    /// CHECK: vault's Token-2022 account for the mint; address checked.
    #[account(mut)]
    pub vault_token: UncheckedAccount<'info>,
    /// CHECK: vault's wSOL account (the buy is paid in wSOL; closed again in the same instruction); address checked.
    #[account(mut)]
    pub vault_wsol: UncheckedAccount<'info>,
    /// CHECK: must equal the launch's donation wallet.
    #[account(mut)]
    pub donation_wallet: UncheckedAccount<'info>,
    /// CHECK: wSOL mint, fixed address.
    #[account(address = WSOL_MINT)]
    pub wsol_mint: UncheckedAccount<'info>,
    /// CHECK: Token-2022 program, fixed address.
    #[account(address = token_2022::ID)]
    pub token_2022_program: UncheckedAccount<'info>,
    pub token_program: Program<'info, token::Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

/// `init_len`: how many of `remaining_accounts` belong to Meteora's initialize_virtual_pool_with_token2022 (14 today);
/// the rest are swap2's (15 today). `coin_number` + `mint_bump` give the coin address, as in launch_token.
pub fn handle_launch_token_meteora<'info>(ctx: Context<'_, '_, 'info, 'info, LaunchTokenMeteora<'info>>, init_len: u8, coin_number: u64, mint_bump: u8) -> Result<()> {
    let ra = ctx.remaining_accounts;
    let init_len = init_len as usize;
    require!(init_len >= INIT_LEN && ra.len() >= init_len + SWAP_LEN, FlpError::AccountCount);
    let (c, s) = ra.split_at(init_len);
    let a = &ctx.accounts;
    let launch_key = a.launch.key();
    require_keys_eq!(launch_key, SQUADZ_LAUNCH, FlpError::WrongVenue);
    let now = Clock::get()?.unix_timestamp;

    let (raise, slots, donations, name, symbol, uri, ending, has_ending, treasury_bps);
    {
        let l = a.launch.load()?;
        require_keys_eq!(l.creator, a.creator.key(), FlpError::NotCreator);
        require!(l.state == STATE_OPEN, FlpError::NotOpen);
        require!(now <= l.period_end, FlpError::PeriodEnded);
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
        treasury_bps = l.treasury_bps;
    }

    // The coin address: our PDA for this number, with the launch's ending.
    let number = coin_number.to_le_bytes();
    let mint = Pubkey::create_program_address(&[MINT_SEED, launch_key.as_ref(), &number, &[mint_bump]], &crate::ID)
        .map_err(|_| error!(FlpError::AccountMismatch))?;
    require_keys_eq!(a.mint.key(), mint, FlpError::AccountMismatch);
    require!(!has_ending || math::ends_with_base58(&mint.to_bytes(), &ending), FlpError::WrongEnding);

    // Ownership-deciding accounts must be ours, the settings must be $SQUADZ's; Meteora checks the rest.
    let vault = a.vault.key();
    let cp = a.creator_pda.key();
    let burn_jar = Pubkey::find_program_address(&[BURN_JAR_SEED], &crate::ID).0;
    let config = meteora::read_config(&c[init_ix::CONFIG], &burn_jar)?;
    require!(config.migration_quote_threshold == SQUADZ_CURVE_THRESHOLD, FlpError::AccountMismatch);
    require_keys_eq!(c[init_ix::CREATOR].key(), cp, FlpError::AccountMismatch);
    require_keys_eq!(c[init_ix::BASE_MINT].key(), mint, FlpError::AccountMismatch);
    require_keys_eq!(c[init_ix::QUOTE_MINT].key(), WSOL_MINT, FlpError::AccountMismatch);
    require_keys_eq!(c[init_ix::PAYER].key(), vault, FlpError::AccountMismatch);
    require_keys_eq!(c[init_ix::PROGRAM].key(), METEORA_DBC, FlpError::AccountMismatch);
    require_keys_eq!(s[swap_ix::CONFIG].key(), SQUADZ_DBC_CONFIG, FlpError::AccountMismatch);
    require_keys_eq!(s[swap_ix::POOL].key(), c[init_ix::POOL].key(), FlpError::AccountMismatch);
    require_keys_eq!(s[swap_ix::INPUT].key(), a.vault_wsol.key(), FlpError::AccountMismatch);
    require_keys_eq!(s[swap_ix::OUTPUT].key(), a.vault_token.key(), FlpError::AccountMismatch);
    require_keys_eq!(s[swap_ix::BASE_MINT].key(), mint, FlpError::AccountMismatch);
    require_keys_eq!(s[swap_ix::QUOTE_MINT].key(), WSOL_MINT, FlpError::AccountMismatch);
    require_keys_eq!(s[swap_ix::PAYER].key(), vault, FlpError::AccountMismatch);
    // no referral on the squad's buy (passing the program itself means "none" to Meteora; audit 22 Oct)
    require_keys_eq!(s[swap_ix::REFERRAL].key(), METEORA_DBC, FlpError::AccountMismatch);
    require_keys_eq!(s[swap_ix::PROGRAM].key(), METEORA_DBC, FlpError::AccountMismatch);
    let t22 = token_2022::ID;
    require_keys_eq!(a.vault_token.key(), get_associated_token_address_with_program_id(&vault, &mint, &t22), FlpError::AccountMismatch);
    require_keys_eq!(a.vault_wsol.key(), get_associated_token_address_with_program_id(&vault, &WSOL_MINT, &token::ID), FlpError::AccountMismatch);

    let vault_seeds: &[&[u8]] = &[VAULT_SEED, launch_key.as_ref(), &[ctx.bumps.vault]];
    let creator_seeds: &[&[u8]] = &[CREATOR_SEED, launch_key.as_ref(), &[ctx.bumps.creator_pda]];
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
    let vault_ata = |ata: &AccountInfo<'info>, m: &AccountInfo<'info>, token_program: AccountInfo<'info>| -> Result<()> {
        associated_token::create_idempotent(CpiContext::new_with_signer(
            a.associated_token_program.to_account_info(),
            associated_token::Create {
                payer: a.vault.to_account_info(),
                associated_token: ata.clone(),
                authority: a.vault.to_account_info(),
                mint: m.clone(),
                system_program: a.system_program.to_account_info(),
                token_program,
            },
            &[vault_seeds],
        ))
    };

    // 1. Protocol launch fee (launch #1: a flat 2 SOL, founders 22 Oct), donations and the creator PDA's reserve,
    //    paid exactly as launch_token pays them.
    let launch_fee = SQUADZ_LAUNCH_FEE.min(raise);
    vault_transfer(&a.launch.to_account_info(), launch_fee)?;
    vault_transfer(&a.donation_wallet.to_account_info(), donations)?;
    vault_transfer(&a.creator_pda.to_account_info(), CREATOR_PDA_RESERVE.saturating_sub(a.creator_pda.lamports()))?;

    // 2. Create the coin and its curve on Meteora: vault pays, creator PDA is the creator, mint is our PDA.
    pump::invoke(METEORA_DBC, c, meteora::init_data(name, symbol, uri)?, &[(init_ix::PAYER, vault), (init_ix::CREATOR, cp), (init_ix::BASE_MINT, mint)], &[vault_seeds, creator_seeds, mint_seeds])?;
    vault_ata(&a.vault_token.to_account_info(), &a.mint.to_account_info(), a.token_2022_program.to_account_info())?;

    // 3. Buy the whole curve: wrap the pool's SOL, swap with "stop where the curve completes", unwrap what's left.
    let before = a.vault.lamports();
    vault_ata(&a.vault_wsol.to_account_info(), &a.wsol_mint.to_account_info(), a.token_program.to_account_info())?;
    vault_transfer(&a.vault_wsol.to_account_info(), a.vault.lamports().saturating_sub(VAULT_BUY_RESERVE))?;
    token::sync_native(CpiContext::new(a.token_program.to_account_info(), token::SyncNative { account: a.vault_wsol.to_account_info() }))?;
    let amount_in = pump::token_balance(&a.vault_wsol, &token::ID);
    let min_out = config.swap_base_amount.saturating_sub(METEORA_ROUNDING_DUST);
    pump::invoke(METEORA_DBC, s, meteora::swap2_data(amount_in, min_out)?, &[(swap_ix::PAYER, vault)], &[vault_seeds])?;
    token::close_account(CpiContext::new_with_signer(
        a.token_program.to_account_info(),
        token::CloseAccount { account: a.vault_wsol.to_account_info(), destination: a.vault.to_account_info(), authority: a.vault.to_account_info() },
        &[vault_seeds],
    ))?;
    let spent = before.saturating_sub(a.vault.lamports());

    // 4. Airtight checks: our curve, our creator, our coin, complete, and every token it sells (to Meteora's rounding)
    //    is in the vault.
    let curve = meteora::read_curve(&c[init_ix::POOL])?;
    require_keys_eq!(curve.config, SQUADZ_DBC_CONFIG, FlpError::AccountMismatch);
    require_keys_eq!(curve.creator, cp, FlpError::AccountMismatch);
    require_keys_eq!(curve.base_mint, mint, FlpError::AccountMismatch);
    require!(curve.quote_reserve >= config.migration_quote_threshold, FlpError::CurveNotComplete);
    let received = pump::token_balance(&a.vault_token, &t22);
    require!(received >= min_out && received <= config.swap_base_amount, FlpError::WrongTokenAmount);
    let total_supply = meteora::mint_supply(&a.mint)?;
    // the coin is fixed: 1B supply, nobody can mint more or freeze a wallet (audit 22 Oct)
    require!(total_supply == SQUADZ_SUPPLY && meteora::mint_is_fixed(&a.mint)?, FlpError::WrongMeteoraSettings);

    // 5. Same split as every launch: 33% of supply for HODL Rewards, the treasury, equal shares, leftover SOL back.
    let (per_member, hodl_reserve, _treasury) = math::split_tokens(received, total_supply, slots, treasury_bps).ok_or(FlpError::WrongTokenAmount)?;
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
