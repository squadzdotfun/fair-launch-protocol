//! $SQUADZ's creator fees from Meteora (founders 22 Oct 2026). Launch #1's creator PDA owns two fee sources:
//!   - its share of the bonding curve's fees (Meteora DBC, from the launch buy; claimable once the curve has them)
//!   - the locked pool position's share of the graduated pool's 1.25% fee (Meteora DAMM v2), forever
//! Both are claimed as SOL into the creator PDA's wSOL account, then booked by the SAME code as every launch's
//! pump.fun fees (`finish_fees`): 10% to the protocol (burn jar), the rest by the launch's split (launch #1: all to
//! the creator's fee wallet, no Fee Share). Only Meteora can pay these out, and only when our PDA signs, so this
//! instruction exists; anyone can run it (our bot does, hourly).

use anchor_lang::prelude::*;
use anchor_spl::associated_token::{self, get_associated_token_address_with_program_id};
use anchor_spl::{token, token_2022};

use crate::constants::*;
use crate::errors::FlpError;
use crate::instructions::fees::{finish_fees, CollectFees};
use crate::meteora::{self, damm_claim_ix, dbc_claim_ix, DAMM_CLAIM_LEN, DBC_CLAIM_LEN};
use crate::pump;
use crate::state::*;

/// `remaining_accounts`: Meteora DBC claim_creator_trading_fee (`dbc_len`, 13 today; 0 = skip), then DAMM v2
/// claim_position_fee (`damm_len`, 15 today; 0 = not graduated yet). Both pay SOL into `creator_wsol` and the coin side
/// (always 0: fees are taken in SOL only) into the creator PDA's coin account, opened here at the caller's cost if new.
pub fn handle_collect_fees_meteora<'info>(ctx: Context<'_, '_, 'info, 'info, CollectFees<'info>>, dbc_len: u8, damm_len: u8) -> Result<()> {
    let (dbc_len, damm_len) = (dbc_len as usize, damm_len as usize);
    require!((dbc_len == 0 || dbc_len >= DBC_CLAIM_LEN) && (damm_len == 0 || damm_len >= DAMM_CLAIM_LEN) && dbc_len + damm_len > 0, FlpError::AccountCount);
    require!(ctx.remaining_accounts.len() == dbc_len + damm_len, FlpError::AccountCount);
    let a = &ctx.accounts;
    let ra = ctx.remaining_accounts;
    let launch_key = a.launch.key();
    require_keys_eq!(launch_key, SQUADZ_LAUNCH, FlpError::WrongVenue);
    let mint = {
        let l = a.launch.load()?;
        require!(l.state == STATE_LAUNCHED, FlpError::NotLaunched);
        l.mint
    };
    let cp = a.creator_pda.key();
    let wsol = a.creator_wsol.key();
    require_keys_eq!(wsol, get_associated_token_address_with_program_id(&cp, &WSOL_MINT, &token::ID), FlpError::AccountMismatch);
    let coin_account = get_associated_token_address_with_program_id(&cp, &mint, &token_2022::ID);
    let seeds: &[&[u8]] = &[CREATOR_SEED, launch_key.as_ref(), &[ctx.bumps.creator_pda]];

    // Where the money goes is decided here: SOL to the creator PDA's wSOL account, coin side to its own coin account.
    let (dbc, damm) = ra.split_at(dbc_len);
    let (mint_info, t22_info) = if dbc_len > 0 {
        require_keys_eq!(dbc[dbc_claim_ix::CREATOR].key(), cp, FlpError::AccountMismatch);
        require_keys_eq!(dbc[dbc_claim_ix::TOKEN_A_ACCOUNT].key(), coin_account, FlpError::AccountMismatch);
        require_keys_eq!(dbc[dbc_claim_ix::TOKEN_B_ACCOUNT].key(), wsol, FlpError::AccountMismatch);
        require_keys_eq!(dbc[dbc_claim_ix::BASE_MINT].key(), mint, FlpError::AccountMismatch);
        require_keys_eq!(dbc[dbc_claim_ix::PROGRAM].key(), METEORA_DBC, FlpError::AccountMismatch);
        (&dbc[dbc_claim_ix::BASE_MINT], &dbc[dbc_claim_ix::TOKEN_BASE_PROGRAM])
    } else {
        (&damm[damm_claim_ix::MINT_A], &damm[damm_claim_ix::TOKEN_A_PROGRAM])
    };
    if damm_len > 0 {
        require_keys_eq!(damm[damm_claim_ix::OWNER].key(), cp, FlpError::AccountMismatch);
        require_keys_eq!(damm[damm_claim_ix::TOKEN_A_ACCOUNT].key(), coin_account, FlpError::AccountMismatch);
        require_keys_eq!(damm[damm_claim_ix::TOKEN_B_ACCOUNT].key(), wsol, FlpError::AccountMismatch);
        require_keys_eq!(damm[damm_claim_ix::MINT_A].key(), mint, FlpError::AccountMismatch);
        require_keys_eq!(damm[damm_claim_ix::MINT_B].key(), WSOL_MINT, FlpError::AccountMismatch);
        require_keys_eq!(damm[damm_claim_ix::PROGRAM].key(), METEORA_DAMM_V2, FlpError::AccountMismatch);
    }
    require_keys_eq!(*t22_info.key, token_2022::ID, FlpError::AccountMismatch);

    // The two receiving accounts: wSOL paid from the creator PDA's reserve (as in collect_fees), the coin account by
    // the caller (once).
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
    let coin_info = ra.iter().find(|i| *i.key == coin_account).ok_or(FlpError::AccountMismatch)?;
    associated_token::create_idempotent(CpiContext::new(
        a.associated_token_program.to_account_info(),
        associated_token::Create {
            payer: a.caller.to_account_info(),
            associated_token: coin_info.clone(),
            authority: a.creator_pda.to_account_info(),
            mint: mint_info.clone(),
            system_program: a.system_program.to_account_info(),
            token_program: t22_info.clone(),
        },
    ))?;

    // Claim: our PDA signs as the coin's creator (curve) and as the locked position's owner (pool).
    if dbc_len > 0 {
        pump::invoke(METEORA_DBC, dbc, meteora::dbc_claim_data()?, &[(dbc_claim_ix::CREATOR, cp)], &[seeds])?;
    }
    if damm_len > 0 {
        pump::invoke(METEORA_DAMM_V2, damm, meteora::damm_claim_data(), &[(damm_claim_ix::OWNER, cp)], &[seeds])?;
    }
    // Unwrap and book exactly like every launch's fees. (Fees are SOL only; the coin side stays empty.)
    finish_fees(a, seeds)
}
