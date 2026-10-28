use anchor_lang::prelude::*;
use anchor_spl::associated_token::{self, get_associated_token_address_with_program_id, AssociatedToken};
use anchor_spl::token_2022;

use crate::constants::*;
use crate::errors::FlpError;
use crate::instructions::membership::pay_from_vault;
use crate::state::*;

#[derive(Accounts)]
pub struct Claim<'info> {
    #[account(mut)]
    pub member: Signer<'info>,
    #[account(mut)]
    pub launch: AccountLoader<'info, Launch>,
    /// CHECK: launch vault PDA, checked by seeds.
    #[account(mut, seeds = [VAULT_SEED, launch.key().as_ref()], bump)]
    pub vault: UncheckedAccount<'info>,
    /// CHECK: the launch's coin; must equal the mint stored at launch.
    pub mint: UncheckedAccount<'info>,
    /// CHECK: vault's token account; address checked.
    #[account(mut)]
    pub vault_token: UncheckedAccount<'info>,
    /// CHECK: member's token account (created if missing); address checked.
    #[account(mut)]
    pub member_token: UncheckedAccount<'info>,
    /// CHECK: Token-2022 program, fixed address.
    #[account(address = token_2022::ID)]
    pub token_2022_program: UncheckedAccount<'info>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

/// After launch, a member takes their equal share of tokens and of any SOL
/// the launch buy didn't need, per slot they hold. No deadline: unclaimed shares wait forever.
pub fn handle_claim(ctx: Context<Claim>) -> Result<()> {
    let a = &ctx.accounts;
    let member = a.member.key();
    let launch_key = a.launch.key();
    let (tokens, sol, vault_bump) = {
        let mut l = a.launch.load_mut()?;
        require!(l.state == STATE_LAUNCHED, FlpError::NotLaunched);
        require_keys_eq!(a.mint.key(), l.mint, FlpError::AccountMismatch);
        let i = l.member_index(&member).ok_or(FlpError::NotMember)?;
        require!(l.members[i].flags & FLAG_CLAIMED == 0, FlpError::AlreadyClaimed);
        l.members[i].flags |= FLAG_CLAIMED;
        // the per-member figures are per slot; a wallet holding 2 slots takes both shares
        let k = l.members[i].slot_count() as u64;
        (l.per_member_tokens.checked_mul(k).ok_or(FlpError::Overflow)?, l.leftover_per_member.checked_mul(k).ok_or(FlpError::Overflow)?, l.vault_bump)
    };
    move_tokens(a.vault.as_ref(), &a.vault_token, &a.member, &a.member_token, &a.mint, &a.member, &a.token_2022_program, &a.associated_token_program, &a.system_program, &launch_key, vault_bump, tokens)?;
    if sol > 0 {
        pay_from_vault(&a.vault, &a.member.to_account_info(), &a.system_program, &launch_key, vault_bump, sol)?;
    }
    emit!(Claimed { launch: launch_key, member, tokens, sol });
    Ok(())
}

/// A member takes the HODL Rewards released to them so far (any time, as
/// often as they like). Uses the same accounts as `claim`.
pub fn handle_claim_hodl(ctx: Context<Claim>) -> Result<()> {
    let a = &ctx.accounts;
    let member = a.member.key();
    let launch_key = a.launch.key();
    let (tokens, vault_bump) = {
        let mut l = a.launch.load_mut()?;
        require!(l.state == STATE_LAUNCHED, FlpError::NotLaunched);
        require_keys_eq!(a.mint.key(), l.mint, FlpError::AccountMismatch);
        let i = l.member_index(&member).ok_or(FlpError::NotMember)?;
        let owed = l.members[i].hodl_owed;
        require!(owed > 0, FlpError::NothingToClaim);
        l.members[i].hodl_owed = 0;
        (owed, l.vault_bump)
    };
    move_tokens(a.vault.as_ref(), &a.vault_token, &a.member, &a.member_token, &a.mint, &a.member, &a.token_2022_program, &a.associated_token_program, &a.system_program, &launch_key, vault_bump, tokens)?;
    emit!(HodlClaimed { launch: launch_key, member, tokens });
    Ok(())
}

#[derive(Accounts)]
pub struct BurnHodl<'info> {
    #[account(mut)]
    pub launch: AccountLoader<'info, Launch>,
    /// CHECK: launch vault PDA, checked by seeds; signs the burn.
    #[account(seeds = [VAULT_SEED, launch.key().as_ref()], bump)]
    pub vault: UncheckedAccount<'info>,
    /// CHECK: the launch's coin; must equal the mint stored at launch.
    #[account(mut)]
    pub mint: UncheckedAccount<'info>,
    /// CHECK: vault's token account; address checked.
    #[account(mut)]
    pub vault_token: UncheckedAccount<'info>,
    /// CHECK: Token-2022 program, fixed address.
    #[account(address = token_2022::ID)]
    pub token_2022_program: UncheckedAccount<'info>,
}

/// Anyone burns the HODL Rewards nobody qualified for (drops when every
/// member had sold, plus rounding dust). The coin's supply shrinks.
pub fn handle_burn_hodl(ctx: Context<BurnHodl>) -> Result<()> {
    let a = &ctx.accounts;
    let launch_key = a.launch.key();
    require_keys_eq!(a.vault_token.key(), get_associated_token_address_with_program_id(&a.vault.key(), &a.mint.key(), &token_2022::ID), FlpError::AccountMismatch);
    let amount = {
        let mut l = a.launch.load_mut()?;
        require_keys_eq!(a.mint.key(), l.mint, FlpError::AccountMismatch);
        let amount = l.hodl_burn_pending;
        require!(amount > 0, FlpError::NothingToClaim);
        l.hodl_burn_pending = 0;
        l.hodl_burned_total += amount;
        amount
    };
    token_2022::burn(
        CpiContext::new_with_signer(
            a.token_2022_program.to_account_info(),
            token_2022::Burn { mint: a.mint.to_account_info(), from: a.vault_token.to_account_info(), authority: a.vault.to_account_info() },
            &[&[VAULT_SEED, launch_key.as_ref(), &[ctx.bumps.vault]]],
        ),
        amount,
    )?;
    emit!(HodlBurned { launch: launch_key, tokens: amount });
    Ok(())
}

/// Vault -> `owner`'s Token-2022 account (created idempotently, `payer` pays rent).
#[allow(clippy::too_many_arguments)]
pub(crate) fn move_tokens<'info>(
    vault: &AccountInfo<'info>,
    vault_token: &AccountInfo<'info>,
    owner: &AccountInfo<'info>,
    owner_token: &AccountInfo<'info>,
    mint: &AccountInfo<'info>,
    payer: &AccountInfo<'info>,
    token_program: &AccountInfo<'info>,
    ata_program: &AccountInfo<'info>,
    system: &AccountInfo<'info>,
    launch: &Pubkey,
    vault_bump: u8,
    amount: u64,
) -> Result<()> {
    let t22 = token_2022::ID;
    require_keys_eq!(*vault_token.key, get_associated_token_address_with_program_id(vault.key, mint.key, &t22), FlpError::AccountMismatch);
    require_keys_eq!(*owner_token.key, get_associated_token_address_with_program_id(owner.key, mint.key, &t22), FlpError::AccountMismatch);
    associated_token::create_idempotent(CpiContext::new(
        ata_program.clone(),
        associated_token::Create {
            payer: payer.clone(),
            associated_token: owner_token.clone(),
            authority: owner.clone(),
            mint: mint.clone(),
            system_program: system.clone(),
            token_program: token_program.clone(),
        },
    ))?;
    token_2022::transfer_checked(
        CpiContext::new_with_signer(
            token_program.clone(),
            token_2022::TransferChecked { from: vault_token.clone(), mint: mint.clone(), to: owner_token.clone(), authority: vault.clone() },
            &[&[VAULT_SEED, launch.as_ref(), &[vault_bump]]],
        ),
        amount,
        TOKEN_DECIMALS,
    )
}

#[event]
pub struct HodlClaimed {
    pub launch: Pubkey,
    pub member: Pubkey,
    pub tokens: u64,
}
#[event]
pub struct HodlBurned {
    pub launch: Pubkey,
    pub tokens: u64,
}
#[event]
pub struct Claimed {
    pub launch: Pubkey,
    pub member: Pubkey,
    pub tokens: u64,
    pub sol: u64,
}
