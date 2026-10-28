//! pump.fun / PumpSwap calls. Built by hand from pump's own account order
//! (Anchor's generated CPI helpers referenced an extra account and failed).
//! Callers pass pump's accounts through `remaining_accounts` in IDL order and
//! check the ones that decide ownership; pump validates everything else.

use anchor_lang::prelude::*;
use anchor_lang::solana_program::instruction::{AccountMeta, Instruction};
use anchor_lang::solana_program::program::invoke_signed;

use crate::constants::*;
use crate::errors::FlpError;

anchor_lang::declare_program!(pump);
anchor_lang::declare_program!(pump_amm);

/// pump.fun `create_v2`: 16 accounts.
pub const CREATE_V2_LEN: usize = 16;
pub mod create_ix {
    pub const MINT: usize = 0;
    pub const BONDING_CURVE: usize = 2;
    pub const USER: usize = 5;
    pub const PROGRAM: usize = 15;
}
/// pump.fun `buy_v2`: 27 accounts.
pub const BUY_V2_LEN: usize = 27;
pub mod buy_ix {
    pub const BASE_MINT: usize = 1;
    pub const BONDING_CURVE: usize = 10;
    pub const USER: usize = 13;
    pub const ASSOCIATED_BASE_USER: usize = 14;
    pub const PROGRAM: usize = 26;
}
/// pump.fun `collect_creator_fee_v2`: 10 accounts.
pub const COLLECT_CURVE_LEN: usize = 10;
pub mod collect_curve_ix {
    pub const CREATOR: usize = 0;
    pub const PROGRAM: usize = 9;
}
/// PumpSwap `collect_coin_creator_fee`: 8 accounts.
pub const COLLECT_AMM_LEN: usize = 8;
pub mod collect_amm_ix {
    pub const COIN_CREATOR: usize = 2;
    pub const COIN_CREATOR_TOKEN_ACCOUNT: usize = 5;
    pub const PROGRAM: usize = 7;
}

/// Calls `program` with exactly `accounts`, in order. Each of our PDAs in
/// `pda_signers` comes with the one slot it belongs in and is marked as signer
/// there; `seeds` signs for them. Audit run 6, option C (founders 23 Oct): one of
/// those PDAs anywhere else in the list (e.g. an extra trailing account) refuses
/// the whole call, so our signature only ever covers the role we checked. Extra
/// accounts that aren't our PDAs still pass through (a callee may add accounts).
pub fn invoke(program: Pubkey, accounts: &[AccountInfo], data: Vec<u8>, pda_signers: &[(usize, Pubkey)], seeds: &[&[&[u8]]]) -> Result<()> {
    for (slot, key) in pda_signers {
        require!(accounts.get(*slot).map(|ai| ai.key) == Some(key), FlpError::AccountMismatch);
    }
    let mut metas = Vec::with_capacity(accounts.len());
    for (i, ai) in accounts.iter().enumerate() {
        let ours = pda_signers.iter().any(|(_, k)| k == ai.key);
        let in_place = pda_signers.iter().any(|(s, k)| *s == i && k == ai.key);
        require!(!ours || in_place, FlpError::AccountMismatch);
        metas.push(AccountMeta { pubkey: *ai.key, is_signer: ai.is_signer || in_place, is_writable: ai.is_writable });
    }
    invoke_signed(&Instruction { program_id: program, accounts: metas, data }, accounts, seeds)?;
    Ok(())
}

pub fn create_v2_data(name: String, symbol: String, uri: String, creator: Pubkey) -> Vec<u8> {
    use anchor_lang::InstructionData;
    pump::client::args::CreateV2 {
        name,
        symbol,
        uri,
        creator,
        is_mayhem_mode: false,
        is_cashback_enabled: pump::types::OptionBool(false),
        creator_fee_bps: pump::types::OptionU64(0),
        is_holder_reward: pump::types::OptionBool(false),
    }
    .data()
}

pub fn buy_v2_data(amount: u64, max_sol_cost: u64) -> Vec<u8> {
    use anchor_lang::InstructionData;
    pump::client::args::BuyV2 { amount, max_sol_cost }.data()
}

pub fn collect_curve_data() -> Vec<u8> {
    use anchor_lang::InstructionData;
    pump::client::args::CollectCreatorFeeV2 {}.data()
}

pub fn collect_amm_data() -> Vec<u8> {
    use anchor_lang::InstructionData;
    pump_amm::client::args::CollectCoinCreatorFee {}.data()
}

/// Reads a pump.fun bonding curve, checking it is owned by pump.fun.
pub fn read_curve(info: &AccountInfo) -> Result<pump::accounts::BondingCurve> {
    require_keys_eq!(*info.owner, PUMP_PROGRAM, FlpError::AccountMismatch);
    let data = info.try_borrow_data()?;
    pump::accounts::BondingCurve::try_deserialize(&mut &data[..])
}

/// Reads pump.fun's Global settings, checking owner and address.
pub fn read_global(info: &AccountInfo) -> Result<pump::accounts::Global> {
    require_keys_eq!(*info.owner, PUMP_PROGRAM, FlpError::AccountMismatch);
    let (expected, _) = Pubkey::find_program_address(&[b"global"], &PUMP_PROGRAM);
    require_keys_eq!(*info.key, expected, FlpError::AccountMismatch);
    let data = info.try_borrow_data()?;
    pump::accounts::Global::try_deserialize(&mut &data[..])
}

/// Token account balance (SPL and Token-2022 share the layout: amount at 64..72).
/// A missing, closed or foreign-owned account counts as zero.
pub fn token_balance(info: &AccountInfo, token_program: &Pubkey) -> u64 {
    if info.owner != token_program {
        return 0;
    }
    let Ok(data) = info.try_borrow_data() else { return 0 };
    if data.len() < 72 {
        return 0;
    }
    u64::from_le_bytes(data[64..72].try_into().unwrap())
}
