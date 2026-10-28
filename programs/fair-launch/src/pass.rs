//! Join passes. A launch may name a gatekeeper key at creation; from then on
//! a wallet can only take a slot with a pass from that key. A pass is an
//! ed25519 signature over `pass_message(launch, wallet, expiry)`, checked by
//! Solana's Ed25519 program in an instruction placed directly before `join`.
//! Passes cost nothing on-chain (no stored lists, no fees) and are tied to one
//! launch, one wallet and an expiry time, so they can't be reused or shared.

use anchor_lang::prelude::*;
use anchor_lang::solana_program::ed25519_program;
use anchor_lang::solana_program::sysvar::instructions::{load_current_index_checked, load_instruction_at_checked};

use crate::constants::PASS_TAG;
use crate::errors::FlpError;

/// The exact bytes a gatekeeper signs to let `wallet` join `launch` until `expiry`.
pub fn pass_message(launch: &Pubkey, wallet: &Pubkey, expiry: i64) -> Vec<u8> {
    let mut m = Vec::with_capacity(PASS_TAG.len() + 3 * 32 + 8);
    m.extend_from_slice(PASS_TAG);
    m.extend_from_slice(crate::ID.as_ref());
    m.extend_from_slice(launch.as_ref());
    m.extend_from_slice(wallet.as_ref());
    m.extend_from_slice(&expiry.to_le_bytes());
    m
}

/// Ed25519 instruction layout (Solana's ed25519 program):
/// [count u8][padding u8] then per signature 7 x u16:
/// signature offset, signature instruction, public key offset, public key
/// instruction, message offset, message size, message instruction.
/// An instruction index of u16::MAX means "inside this same instruction".
const HEADER: usize = 2;
const OFFSETS: usize = 14;
const SELF: u16 = u16::MAX;

/// Checks that the instruction directly before this one verified a pass by
/// `gatekeeper` for exactly (`launch`, `wallet`, `expiry`). The runtime has
/// already rejected the transaction if the signature itself is invalid; this
/// makes sure it is the right key over the right message.
pub fn verify_pass(ix_sysvar: &AccountInfo, gatekeeper: &Pubkey, launch: &Pubkey, wallet: &Pubkey, expiry: i64) -> Result<()> {
    let current = load_current_index_checked(ix_sysvar)? as usize;
    require!(current > 0, FlpError::PassMissing);
    let ix = load_instruction_at_checked(current - 1, ix_sysvar)?;
    require_keys_eq!(ix.program_id, ed25519_program::ID, FlpError::PassMissing);
    let d = &ix.data;
    require!(d.len() >= HEADER + OFFSETS && d[0] == 1, FlpError::PassMissing);
    let at = |i: usize| u16::from_le_bytes([d[HEADER + i], d[HEADER + i + 1]]);
    let (sig_off, sig_ix) = (at(0) as usize, at(2));
    let (key_off, key_ix) = (at(4) as usize, at(6));
    let (msg_off, msg_len, msg_ix) = (at(8) as usize, at(10) as usize, at(12));
    // Everything must live inside the verify instruction itself, so nobody can
    // point the check at bytes from another instruction.
    require!(sig_ix == SELF && key_ix == SELF && msg_ix == SELF, FlpError::PassMissing);
    require!(d.len() >= sig_off + 64 && d.len() >= key_off + 32 && d.len() >= msg_off + msg_len, FlpError::PassMissing);
    require!(&d[key_off..key_off + 32] == gatekeeper.as_ref(), FlpError::BadPass);
    require!(d[msg_off..msg_off + msg_len] == pass_message(launch, wallet, expiry)[..], FlpError::BadPass);
    Ok(())
}
