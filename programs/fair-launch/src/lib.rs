//! Fair Launch Protocol: fair group launches on pump.fun.
//!
//! People pool SOL in 10-50 equal slots. When every slot is filled, the creator
//! launches: the coin is created on pump.fun with a protocol address as its
//! creator, and the pool buys the entire bonding curve in the same
//! transaction, so nobody buys ahead of the group. Every member gets the same
//! share at the same price. Creator fees then flow through this program
//! forever. There are no admin keys.

use anchor_lang::prelude::*;
#[cfg(not(feature = "no-entrypoint"))]
use solana_security_txt::security_txt;

// Founders 28 Oct (option A): who to contact, stored inside the program. Links only: it can't change after the lock.
#[cfg(not(feature = "no-entrypoint"))]
security_txt! {
    name: "Squadz Fair Launch Protocol",
    project_url: "https://squadz.fun",
    contacts: "email:security@squadz.fun,link:https://github.com/squadzdotfun/fair-launch-protocol/blob/main/SECURITY.md",
    policy: "https://github.com/squadzdotfun/fair-launch-protocol/blob/main/SECURITY.md",
    preferred_languages: "en",
    source_code: "https://github.com/squadzdotfun/fair-launch-protocol",
    auditors: "None"
}

pub mod constants;
pub mod errors;
pub mod instructions;
pub mod math;
pub mod meteora;
pub mod ops;
pub mod pass;
pub mod pump;
pub mod state;

pub use instructions::*;

// B5 (25 Oct): the program address by build type, so devnet and local test builds keep working once the mainnet
// address is set. Mainnet builds (no features) use the line marked MAINNET: set it to the mainnet program key at step
// 1.3, then recompute SQUADZ_LAUNCH (its unit test fails until it is). deploy-mainnet.sh reads that line and refuses
// the devnet address; deploy-devnet.sh reads the DEVNET line.
#[cfg(not(any(feature = "short-time", feature = "devnet-squadz", feature = "mainnet-rehearsal")))]
declare_id!("BstpzKnft3hQNaae8TWL8KnUsff6wMZEtn5moEcD8FYR"); // MAINNET (founders' program key, made 26 Oct; step C 27 Oct)
#[cfg(any(feature = "short-time", feature = "devnet-squadz"))]
declare_id!("7vGLH6t2HSWmLzk4E8GPF2EbnQPSwnA9n3sJ9GXTsaMw"); // DEVNET
// Test C (27 Oct): the throwaway rehearsal program on mainnet (scripts/set-rehearsal.sh writes this line; closed after)
#[cfg(all(feature = "mainnet-rehearsal", not(any(feature = "short-time", feature = "devnet-squadz"))))]
declare_id!("7u76pwq75UANLU7TxvXEq93ppSFqYwYc9kcyijf9sxGu"); // TEST C REHEARSAL (deploy-mainnet.sh only reads the line marked MAINNET)

#[program]
pub mod fair_launch {
    use super::*;

    // ---- creation ----
    pub fn create_launch(ctx: Context<CreateLaunch>, args: CreateLaunchArgs) -> Result<()> {
        instructions::create::handle_create_launch(ctx, args)
    }
    pub fn set_fee_wallet(ctx: Context<SetFeeWallet>, fee_wallet: Pubkey) -> Result<()> {
        instructions::create::handle_set_fee_wallet(ctx, fee_wallet)
    }

    // ---- membership ----
    pub fn join(ctx: Context<Join>, donation: u64, pass_expiry: i64) -> Result<()> {
        instructions::membership::handle_join(ctx, donation, pass_expiry)
    }
    pub fn leave(ctx: Context<Leave>) -> Result<()> {
        instructions::membership::handle_leave(ctx)
    }
    // ---- one wallet, up to 2 slots (17 Oct) ----
    pub fn join_slots(ctx: Context<Join>, donation: u64, pass_expiry: i64, count: u8) -> Result<()> {
        instructions::membership::handle_join_slots(ctx, donation, pass_expiry, count)
    }
    pub fn leave_slots(ctx: Context<Leave>, count: u8) -> Result<()> {
        instructions::membership::handle_leave_slots(ctx, count)
    }
    pub fn set_max_per_wallet(ctx: Context<SetMaxPerWallet>, max: u8) -> Result<()> {
        instructions::membership::handle_set_max_per_wallet(ctx, max)
    }
    pub fn cancel(ctx: Context<Cancel>) -> Result<()> {
        instructions::membership::handle_cancel(ctx)
    }
    pub fn refund<'info>(ctx: Context<'_, '_, 'info, 'info, Refund<'info>>) -> Result<()> {
        instructions::membership::handle_refund(ctx)
    }

    // ---- launch and claims ----
    pub fn launch_token<'info>(ctx: Context<'_, '_, 'info, 'info, LaunchToken<'info>>, create_len: u8, coin_number: u64, mint_bump: u8) -> Result<()> {
        instructions::launch::handle_launch_token(ctx, create_len, coin_number, mint_bump)
    }
    pub fn claim(ctx: Context<Claim>) -> Result<()> {
        instructions::claims::handle_claim(ctx)
    }

    // ---- HODL Rewards ----
    pub fn claim_hodl(ctx: Context<Claim>) -> Result<()> {
        instructions::claims::handle_claim_hodl(ctx)
    }
    pub fn burn_hodl(ctx: Context<BurnHodl>) -> Result<()> {
        instructions::claims::handle_burn_hodl(ctx)
    }
    pub fn spot_check(ctx: Context<SpotCheck>, members: Vec<u8>) -> Result<()> {
        instructions::fees::handle_spot_check(ctx, members)
    }

    // ---- creator fees, forever ----
    pub fn collect_fees<'info>(ctx: Context<'_, '_, 'info, 'info, CollectFees<'info>>, curve_len: u8, amm_len: u8) -> Result<()> {
        instructions::fees::handle_collect_fees(ctx, curve_len, amm_len)
    }
    pub fn sweep_fees(ctx: Context<CollectFees>) -> Result<()> {
        instructions::fees::handle_sweep_fees(ctx)
    }
    pub fn payout_round(ctx: Context<PayoutRound>) -> Result<()> {
        instructions::fees::handle_payout_round(ctx)
    }
    pub fn pay_out(ctx: Context<PayOut>) -> Result<()> {
        instructions::fees::handle_pay_out(ctx)
    }

    // ---- protocol fees -> $SQUADZ buy and burn ----
    pub fn sweep_protocol(ctx: Context<SweepProtocol>) -> Result<()> {
        instructions::burn::handle_sweep_protocol(ctx)
    }
    /// Buys $SQUADZ on its Meteora pool with the burn jar and burns it (hourly). Replaced the PumpSwap version 22 Oct.
    pub fn buy_and_burn<'info>(ctx: Context<'_, '_, 'info, 'info, BuyAndBurn<'info>>, pool_bump: u8) -> Result<()> {
        instructions::burn::handle_buy_and_burn(ctx, pool_bump)
    }

    // ---- operations budget ----
    pub fn close_ops(ctx: Context<CloseOps>) -> Result<()> {
        instructions::burn::handle_close_ops(ctx)
    }

    // ---- Project Treasury (20 Oct, docs/27) ----
    pub fn add_treasury(ctx: Context<AddTreasury>, bps: u16, wallet: Pubkey) -> Result<()> {
        instructions::treasury::handle_add_treasury(ctx, bps, wallet)
    }
    pub fn settle_treasury_fee(ctx: Context<SettleTreasuryFee>) -> Result<()> {
        instructions::treasury::handle_settle_treasury_fee(ctx)
    }
    pub fn release_treasury(ctx: Context<ReleaseTreasury>) -> Result<()> {
        instructions::treasury::handle_release_treasury(ctx)
    }

    /// Graduation shield: pays back, once, the rent of the pool's token accounts opened for the launch (coin
    /// accounts to the creator, wSOL accounts to the upkeep bot), from the operations budget.
    pub fn repay_shield(ctx: Context<RepayShield>) -> Result<()> {
        instructions::burn::handle_repay_shield(ctx)
    }

    // ---- fixes #3 / #18 (22 Oct) ----
    /// A member adds to their donation while the lobby is open.
    pub fn add_donation(ctx: Context<AddDonation>, amount: u64) -> Result<()> {
        instructions::membership::handle_add_donation(ctx, amount)
    }
    /// Approval-only lobby, chosen and paid for (+0.05 SOL) in the creation transaction.
    pub fn set_approval(ctx: Context<SetApproval>) -> Result<()> {
        instructions::create::handle_set_approval(ctx)
    }

    // ---- $SQUADZ on Meteora (22 Oct) ----
    /// Launch #1 ($SQUADZ) only: like launch_token, but the coin is created on Meteora's bonding curve and the pool
    /// buys that whole curve.
    pub fn launch_token_meteora<'info>(ctx: Context<'_, '_, 'info, 'info, LaunchTokenMeteora<'info>>, init_len: u8, coin_number: u64, mint_bump: u8) -> Result<()> {
        instructions::launch_meteora::handle_launch_token_meteora(ctx, init_len, coin_number, mint_bump)
    }
    /// Launch #1 only: claims its Meteora fees (curve share and/or the locked pool position's share) and books them
    /// like every launch's creator fees.
    pub fn collect_fees_meteora<'info>(ctx: Context<'_, '_, 'info, 'info, CollectFees<'info>>, dbc_len: u8, damm_len: u8) -> Result<()> {
        instructions::fees_meteora::handle_collect_fees_meteora(ctx, dbc_len, damm_len)
    }
}
