//! Meteora calls, for $SQUADZ (launch #1) only (founders 22 Oct 2026). $SQUADZ is created on Meteora's Dynamic
//! Bonding Curve (DBC) and graduates into a DAMM v2 pool with a flat 1.25% fee, its liquidity locked forever to the
//! launch's creator PDA. Every other coin launches on pump.fun exactly as before.
//! Like pump.rs: callers pass Meteora's accounts through `remaining_accounts` in Meteora's IDL order and check the ones
//! that decide ownership; Meteora validates everything else. Meteora's interface is the same on devnet and mainnet
//! (IDLs compared 22 Oct 2026: DBC 0.1.10, DAMM v2 0.2.0).

use anchor_lang::prelude::*;

use crate::constants::*;
use crate::errors::FlpError;

/// DBC `initialize_virtual_pool_with_token2022`: 14 accounts.
pub const INIT_LEN: usize = 14;
pub mod init_ix {
    pub const CONFIG: usize = 0;
    pub const CREATOR: usize = 2;
    pub const BASE_MINT: usize = 3;
    pub const QUOTE_MINT: usize = 4;
    pub const POOL: usize = 5;
    pub const PAYER: usize = 8;
    pub const PROGRAM: usize = 13;
}
/// DBC `swap2`: 15 accounts.
pub const SWAP_LEN: usize = 15;
pub mod swap_ix {
    pub const CONFIG: usize = 1;
    pub const POOL: usize = 2;
    pub const INPUT: usize = 3;
    pub const OUTPUT: usize = 4;
    pub const BASE_MINT: usize = 7;
    pub const QUOTE_MINT: usize = 8;
    pub const PAYER: usize = 9;
    pub const REFERRAL: usize = 12;
    pub const PROGRAM: usize = 14;
}

/// DBC `claim_creator_trading_fee`: 13 accounts (the coin creator's share of the curve's fees).
pub const DBC_CLAIM_LEN: usize = 13;
pub mod dbc_claim_ix {
    pub const TOKEN_A_ACCOUNT: usize = 2;
    pub const TOKEN_B_ACCOUNT: usize = 3;
    pub const BASE_MINT: usize = 6;
    pub const CREATOR: usize = 8;
    pub const TOKEN_BASE_PROGRAM: usize = 9;
    pub const PROGRAM: usize = 12;
}
/// DAMM v2 `claim_position_fee`: 15 accounts (the locked position's share of the pool's fees).
pub const DAMM_CLAIM_LEN: usize = 15;
pub mod damm_claim_ix {
    pub const TOKEN_A_ACCOUNT: usize = 3;
    pub const TOKEN_B_ACCOUNT: usize = 4;
    pub const MINT_A: usize = 7;
    pub const MINT_B: usize = 8;
    pub const OWNER: usize = 10;
    pub const TOKEN_A_PROGRAM: usize = 11;
    pub const PROGRAM: usize = 14;
}
/// DAMM v2 `swap`: 14 accounts.
pub const DAMM_SWAP_LEN: usize = 14;
pub mod damm_swap_ix {
    pub const POOL: usize = 1;
    pub const INPUT: usize = 2;
    pub const OUTPUT: usize = 3;
    pub const VAULT_A: usize = 4;
    pub const VAULT_B: usize = 5;
    pub const MINT_A: usize = 6;
    pub const MINT_B: usize = 7;
    pub const PAYER: usize = 8;
    pub const REFERRAL: usize = 11;
    pub const PROGRAM: usize = 13;
}

const INIT_T22: [u8; 8] = [169, 118, 51, 78, 145, 110, 220, 155];
const DBC_CLAIM: [u8; 8] = [82, 220, 250, 189, 3, 85, 107, 45];
const DAMM_CLAIM: [u8; 8] = [180, 38, 154, 17, 133, 33, 162, 211];
const DAMM_SWAP: [u8; 8] = [248, 198, 158, 145, 225, 117, 135, 200];
const DAMM_POOL_DISC: [u8; 8] = [241, 154, 109, 4, 17, 177, 109, 188];
const SWAP2: [u8; 8] = [65, 75, 63, 76, 235, 91, 91, 136];
/// swap2 mode: stop exactly where the curve completes; the rest of the input stays with the buyer.
const PARTIAL_FILL: u8 = 1;

// Account discriminators (Meteora's IDL).
const POOL_CONFIG_DISC: [u8; 8] = [26, 108, 14, 123, 116, 230, 129, 43];
const VIRTUAL_POOL_DISC: [u8; 8] = [213, 224, 5, 209, 98, 69, 119, 92];

pub fn init_data(name: String, symbol: String, uri: String) -> Result<Vec<u8>> {
    let mut d = INIT_T22.to_vec();
    (name, symbol, uri).serialize(&mut d)?;
    Ok(d)
}

pub fn swap2_data(amount_in: u64, min_out: u64) -> Result<Vec<u8>> {
    let mut d = SWAP2.to_vec();
    (amount_in, min_out, PARTIAL_FILL).serialize(&mut d)?;
    Ok(d)
}

/// Claim everything: the creator's curve fees (DBC) take a max per side.
pub fn dbc_claim_data() -> Result<Vec<u8>> {
    let mut d = DBC_CLAIM.to_vec();
    (u64::MAX, u64::MAX).serialize(&mut d)?;
    Ok(d)
}

pub fn damm_claim_data() -> Vec<u8> {
    DAMM_CLAIM.to_vec()
}

pub fn damm_swap_data(amount_in: u64, min_out: u64) -> Result<Vec<u8>> {
    let mut d = DAMM_SWAP.to_vec();
    (amount_in, min_out).serialize(&mut d)?;
    Ok(d)
}

/// A coin's two mints in Meteora's pool-address order (larger first).
pub fn pool_mints(mint: &Pubkey) -> (Pubkey, Pubkey) {
    if mint.to_bytes() > WSOL_MINT.to_bytes() {
        (*mint, WSOL_MINT)
    } else {
        (WSOL_MINT, *mint)
    }
}

/// Fee-free tokens `lamports` buy at a DAMM v2 pool's current price: the price is (sqrt_price / 2^64)^2 lamports per
/// raw token, so tokens = lamports * 2^128 / sqrt_price^2. None if it doesn't fit.
pub fn tokens_for_sol(lamports: u64, sqrt_price: u128) -> Option<u64> {
    if sqrt_price == 0 {
        return None;
    }
    let t = ((lamports as u128) << 64) / sqrt_price;
    if t >> 64 != 0 {
        return None;
    }
    u64::try_from((t << 64) / sqrt_price).ok()
}

fn u64_at(d: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(d[o..o + 8].try_into().unwrap())
}
fn key_at(d: &[u8], o: usize) -> Pubkey {
    Pubkey::new_from_array(d[o..o + 32].try_into().unwrap())
}

/// $SQUADZ's DBC settings account (`PoolConfig`), owner and address checked.
pub struct CurveConfig {
    /// Tokens the curve sells, start to graduation.
    pub swap_base_amount: u64,
    /// SOL (lamports) the curve collects before it can graduate.
    pub migration_quote_threshold: u64,
}
/// Reads $SQUADZ's settings account and refuses any setting that isn't the decided one (audit 22 Oct): the account is
/// created once by the team and can't change, but if it were ever created wrong the launch must refuse rather than go
/// ahead. Offsets: Meteora DBC 0.1.10 `PoolConfig`.
pub fn read_config(info: &AccountInfo, burn_jar: &Pubkey) -> Result<CurveConfig> {
    require_keys_eq!(*info.key, SQUADZ_DBC_CONFIG, FlpError::AccountMismatch);
    require_keys_eq!(*info.owner, METEORA_DBC, FlpError::AccountMismatch);
    let d = info.try_borrow_data()?;
    require!(d.len() >= 392 && d[..8] == POOL_CONFIG_DISC, FlpError::AccountMismatch);
    let zero = |a: usize, b: usize| d[a..b].iter().all(|x| *x == 0);
    let u16_at = |o: usize| u16::from_le_bytes([d[o], d[o + 1]]);
    require!(
        key_at(&d, 8) == WSOL_MINT                                  // priced in SOL
            && key_at(&d, 72) == *burn_jar                          // the curve's leftover goes to the burn jar
            && u64_at(&d, 104) == SQUADZ_CURVE_FEE_NUMERATOR        // curve fee 0.25%...
            && u16_at(128) == 0 && d[130] == 0 && d[136] == 0       // ...flat: no fee schedule, no volatility fee
            && zero(184, 216)                                       // no liquidity vesting (partner, creator)
            && d[232] == 0                                          // curve fees in SOL
            && d[233] == 1                                          // graduates into DAMM v2
            && d[237] == 1                                          // Token-2022 coin
            && d[239] == 0 && d[240] == 0 && d[242] == 0            // no partner or unlocked creator liquidity...
            && d[241] == 100                                        // ...100% locked forever to the coin's creator
            && d[243] == 6                                          // our own pool fee (Customizable)
            && d[244] == 1                                          // fixed supply...
            && u64_at(&d, 344) == SQUADZ_SUPPLY && u64_at(&d, 352) == SQUADZ_SUPPLY
            && d[245] == 100                                        // the curve's trading fee share: all to the creator
            && d[246] == 1                                          // coin metadata can't be changed, no mint authority
            && d[247] == 0 && d[248] == 0                           // no graduation fee shares we couldn't claim
            && zero(296, 344)                                       // no locked vesting of supply
            && d[360] == 0 && d[361] == 0                           // pool fees in SOL, no volatility fee...
            && u16_at(362) == SQUADZ_POOL_FEE_BPS                   // ...a flat 1.25%
            && d[364] == 0 && u16_at(366) == 0                      // no market-cap fee schedule, no compounding
            && u64_at(&d, 368) == 0,                                // no pool creation fee
        FlpError::WrongMeteoraSettings
    );
    Ok(CurveConfig { swap_base_amount: u64_at(&d, 256), migration_quote_threshold: u64_at(&d, 264) })
}

/// A Token-2022 mint with no mint authority and no freeze authority: nobody can ever mint more or freeze a wallet.
pub fn mint_is_fixed(info: &AccountInfo) -> Result<bool> {
    require_keys_eq!(*info.owner, anchor_spl::token_2022::ID, FlpError::AccountMismatch);
    let d = info.try_borrow_data()?;
    Ok(d.len() >= 82 && d[0..4] == [0; 4] && d[46..50] == [0; 4])
}

/// A DBC curve (`VirtualPool`), owner checked.
pub struct Curve {
    pub config: Pubkey,
    pub creator: Pubkey,
    pub base_mint: Pubkey,
    pub quote_reserve: u64,
}
pub fn read_curve(info: &AccountInfo) -> Result<Curve> {
    require_keys_eq!(*info.owner, METEORA_DBC, FlpError::AccountMismatch);
    let d = info.try_borrow_data()?;
    require!(d.len() >= 248 && d[..8] == VIRTUAL_POOL_DISC, FlpError::AccountMismatch);
    Ok(Curve { config: key_at(&d, 72), creator: key_at(&d, 104), base_mint: key_at(&d, 136), quote_reserve: u64_at(&d, 240) })
}

/// A DAMM v2 pool (the graduated $SQUADZ pool), owner checked.
pub struct DammPool {
    pub token_a_mint: Pubkey,
    pub token_b_mint: Pubkey,
    pub token_a_vault: Pubkey,
    pub token_b_vault: Pubkey,
    /// Meteora's share of SOL fees not yet taken out of the SOL vault.
    pub protocol_b_fee: u64,
    pub sqrt_price: u128,
    /// false while Meteora has the pool paused.
    pub trading: bool,
}
pub fn read_damm_pool(info: &AccountInfo) -> Result<DammPool> {
    require_keys_eq!(*info.owner, METEORA_DAMM_V2, FlpError::AccountMismatch);
    let d = info.try_borrow_data()?;
    require!(d.len() >= 488 && d[..8] == DAMM_POOL_DISC, FlpError::AccountMismatch);
    Ok(DammPool {
        token_a_mint: key_at(&d, 168),
        token_b_mint: key_at(&d, 200),
        token_a_vault: key_at(&d, 232),
        token_b_vault: key_at(&d, 264),
        protocol_b_fee: u64_at(&d, 400),
        sqrt_price: u128::from_le_bytes(d[456..472].try_into().unwrap()),
        trading: d[481] == 0,
    })
}

/// Total supply of a Token-2022 mint (base mint layout: supply at 36..44).
pub fn mint_supply(info: &AccountInfo) -> Result<u64> {
    require_keys_eq!(*info.owner, anchor_spl::token_2022::ID, FlpError::AccountMismatch);
    let d = info.try_borrow_data()?;
    require!(d.len() >= 44, FlpError::AccountMismatch);
    Ok(u64_at(&d, 36))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// $SQUADZ's settings as decided (the same bytes the devnet settings account holds, checked 22 Oct).
    fn good(jar: &Pubkey) -> Vec<u8> {
        let mut d = vec![0u8; 1048];
        d[..8].copy_from_slice(&POOL_CONFIG_DISC);
        d[8..40].copy_from_slice(WSOL_MINT.as_ref());
        d[72..104].copy_from_slice(jar.as_ref());
        d[104..112].copy_from_slice(&SQUADZ_CURVE_FEE_NUMERATOR.to_le_bytes());
        for (o, v) in [(233, 1u8), (237, 1), (241, 100), (243, 6), (244, 1), (245, 100), (246, 1)] {
            d[o] = v;
        }
        d[256..264].copy_from_slice(&850_000_000_000_000u64.to_le_bytes());
        d[264..272].copy_from_slice(&SQUADZ_CURVE_THRESHOLD.to_le_bytes());
        d[344..352].copy_from_slice(&SQUADZ_SUPPLY.to_le_bytes());
        d[352..360].copy_from_slice(&SQUADZ_SUPPLY.to_le_bytes());
        d[362..364].copy_from_slice(&SQUADZ_POOL_FEE_BPS.to_le_bytes());
        d
    }

    fn check(d: &mut [u8], jar: &Pubkey) -> Result<CurveConfig> {
        let (key, owner) = (SQUADZ_DBC_CONFIG, METEORA_DBC);
        let mut lamports = 1u64;
        let info = AccountInfo::new(&key, false, false, &mut lamports, d, &owner, false, 0);
        read_config(&info, jar)
    }

    #[test]
    fn settings_check_accepts_ours_and_refuses_every_wrong_field() {
        let jar = Pubkey::new_unique();
        let c = check(&mut good(&jar), &jar).expect("the decided settings pass");
        assert_eq!((c.migration_quote_threshold, c.swap_base_amount), (SQUADZ_CURVE_THRESHOLD, 850_000_000_000_000));
        assert!(check(&mut good(&jar), &Pubkey::new_unique()).is_err(), "leftover to someone else than the burn jar");
        // one byte changed anywhere a decided setting lives -> refused
        let fields = [8, 72, 104, 128, 130, 136, 184, 200, 232, 233, 237, 239, 240, 241, 242, 243, 244, 245, 246, 247, 248, 296, 340,
            344, 352, 360, 361, 362, 364, 366, 368];
        for o in fields {
            let mut d = good(&jar);
            d[o] ^= 0x11;
            assert!(check(&mut d, &jar).is_err(), "byte {o} changed but accepted");
        }
        let mut short = good(&jar);
        assert!(check(&mut short[..300], &jar).is_err(), "too short");
    }
}
