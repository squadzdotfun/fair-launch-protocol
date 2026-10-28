//! The operations budget: one system-owned PDA per launch, funded from the
//! creation fee. It tips whoever does verifiable upkeep for that launch (payout
//! rounds, spot checks, fee collection, refunds), so the protocol runs without
//! any keyed wallet and anyone can do the work. When the launch needs no more
//! upkeep, `close_ops` sends what is left to the burn jar.

use anchor_lang::prelude::*;
use anchor_lang::system_program;

use crate::constants::*;

/// Pays `amount` from the launch's budget to `to`, if the budget can afford it
/// while keeping its own rent. Returns what was paid (0 if not affordable).
pub fn tip<'info>(ops: &AccountInfo<'info>, to: &AccountInfo<'info>, system: &AccountInfo<'info>, launch: &Pubkey, bump: u8, amount: u64) -> Result<u64> {
    if amount == 0 || ops.lamports() < RENT_EXEMPT_ZERO.saturating_add(amount) {
        return Ok(0);
    }
    system_program::transfer(
        CpiContext::new_with_signer(
            system.clone(),
            system_program::Transfer { from: ops.clone(), to: to.clone() },
            &[&[OPS_SEED, launch.as_ref(), &[bump]]],
        ),
        amount,
    )?;
    Ok(amount)
}
