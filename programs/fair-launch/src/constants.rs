//! Protocol constants. None of these can change after deployment.

use anchor_lang::prelude::*;

// ---- Seeds -----------------------------------------------------------------
pub const LAUNCH_SEED: &[u8] = b"launch";
pub const VAULT_SEED: &[u8] = b"vault";
pub const CREATOR_SEED: &[u8] = b"creator";
pub const MINT_SEED: &[u8] = b"mint";
pub const OPS_SEED: &[u8] = b"ops";
pub const BURN_JAR_SEED: &[u8] = b"burn_jar";
pub const BURN_STATE_SEED: &[u8] = b"burn_state";

// ---- Launch settings ---------------------------------------------------------
pub const MIN_SLOTS: u8 = 10;
pub const MAX_SLOTS: u8 = 50;
/// Most slots one wallet can ever hold in a launch (the creator picks 1 or 2; 17 Oct).
pub const MAX_PER_WALLET: u8 = 2;

// Time rules. The `short-time` feature shrinks them to seconds for LOCAL
// TESTS ONLY; real builds never enable it.
/// Launch periods: 24 hours, 72 hours, 1 week.
#[cfg(not(feature = "short-time"))]
pub const PERIODS: [i64; 3] = [86_400, 259_200, 604_800];
#[cfg(not(feature = "short-time"))]
pub const ROUND_INTERVAL: i64 = 3_600;
#[cfg(not(feature = "short-time"))]
pub const BURN_INTERVAL: i64 = 3_600;

#[cfg(feature = "short-time")]
pub const PERIODS: [i64; 3] = [45, 90, 135];
#[cfg(feature = "short-time")]
pub const ROUND_INTERVAL: i64 = 4;
#[cfg(feature = "short-time")]
pub const BURN_INTERVAL: i64 = 4;

pub const BPS: u64 = 10_000;

pub const MAX_NAME_LEN: usize = 32; // pump.fun limit
pub const MAX_SYMBOL_LEN: usize = 13; // pump.fun limit
pub const MAX_URI_LEN: usize = 200; // pump.fun limit

// ---- Coin address ending -------------------------------------------------------
/// The base58 alphabet: the only characters a Solana address can contain.
pub const BASE58: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
/// A launch may require its coin address to end in 4 chosen characters
/// (for example `fair`). All zeros = no requirement.
pub const ENDING_LEN: usize = 4;
/// Endings other launchpads are known by. They can't be chosen, so a coin
/// launched here never looks like it came from somewhere else.
pub const RESERVED_ENDINGS: [&[u8; ENDING_LEN]; 3] = [b"pump", b"bonk", b"bags"];

// ---- Join passes ---------------------------------------------------------------
/// A lobby with a gatekeeper only accepts members holding a pass: the
/// gatekeeper's ed25519 signature over
/// `PASS_TAG || program id || launch || wallet || expiry (i64, little-endian)`.
pub const PASS_TAG: &[u8] = b"fair-launch-protocol:join:v1";

// ---- The raise: a group buy of pump.fun's whole curve --------------------------
/// The raise must cover the curve plus pump.fun's 1.25% buy fee (and the 5%
/// launch fee). pump.fun's live curve settings are read at creation.
pub const MIN_BUY_MARGIN_BPS: u64 = 125;
/// ...and may exceed that minimum by at most 3% (a website can add 2% as a
/// safety margin against small curve changes). Whatever the buy doesn't use is
/// returned to members at claim.
pub const MAX_RAISE_MARGIN_BPS: u64 = 300;

/// Smallest non-zero donation (0.001 SOL). Keeps any donation total above the
/// rent minimum, so an empty donation wallet can always receive it.
pub const MIN_DONATION: u64 = 1_000_000;

// ---- HODL Rewards --------------------------------------------------------------
/// Share of TOTAL supply held back from the launch buy and released to members
/// who keep holding: 33% (members get the rest at claim). Was 20% until 14 Oct 2026.
pub const HODL_RESERVE_BPS: u64 = 3_300;
/// A member qualifies while they have never held less than 75% of the tokens
/// they got at claim (buying back doesn't restore it).
pub const HODL_MIN_HELD_BPS: u16 = 7_500;
/// Share of the reserve released at each drop (ramps up; sums to 100%).
pub const HODL_DROP_BPS: [u64; 5] = [1_000, 1_500, 2_000, 2_500, 3_000];
/// When each drop is released, after launch: 24h, 48h, 72h, 1 week, 2 weeks.
#[cfg(not(feature = "short-time"))]
pub const HODL_DROP_TIMES: [i64; 5] = [86_400, 172_800, 259_200, 604_800, 1_209_600];
// LOCAL TESTS ONLY.
#[cfg(feature = "short-time")]
pub const HODL_DROP_TIMES: [i64; 5] = [10, 20, 30, 40, 50];

// ---- Protocol fees -------------------------------------------------------------
/// 5% of each raise, taken from the pool only when the launch succeeds.
pub const LAUNCH_FEE_BPS: u64 = 500;
/// Creation fee by launch window (index = period: 24h, 72h, 1 week): 0.05 / 0.10 / 0.20 SOL, not refundable
/// (founders 22 Oct, fix #18: a longer window is longer on our servers; filling it is the creator's job). It pays the
/// launch account's storage deposit; the rest funds the launch's operations budget (tips for upkeep; leftovers go
/// to the burn jar, which buys and burns $SQUADZ).
pub const CREATION_FEES: [u64; 3] = [50_000_000, 100_000_000, 200_000_000];
/// The baseline (24h window).
pub const CREATION_FEE: u64 = CREATION_FEES[0];
/// Approval-only lobby (the gatekeeper only lets in people the creator approved): +0.05 SOL, at creation only.
pub const APPROVAL_FEE: u64 = 50_000_000;
/// Two slots per person: +0.10 SOL, at creation only. Both extras go to the operations budget like the creation fee.
pub const TWO_SLOT_FEE: u64 = 100_000_000;
/// 10% of every coin's creator fees, forever.
pub const PROTOCOL_FEE_SHARE_BPS: u64 = 1_000;

// ---- Payouts -------------------------------------------------------------------
/// A round pays the Fee Share out only when the members' pot reaches 0.25 SOL
/// per member.
#[cfg(not(feature = "short-time"))]
pub const MIN_PAYOUT_PER_MEMBER: u64 = 250_000_000;
/// Reimbursed to whoever runs a paying round, from the pot. Covers the
/// round's collect + snapshot + payout transactions (~0.0005 SOL).
#[cfg(not(feature = "short-time"))]
pub const ROUND_REIMBURSEMENT: u64 = 500_000;
// LOCAL TESTS ONLY: small enough that local trading can trigger payouts.
#[cfg(feature = "short-time")]
pub const MIN_PAYOUT_PER_MEMBER: u64 = 1_000_000;
#[cfg(feature = "short-time")]
pub const ROUND_REIMBURSEMENT: u64 = 5_000;

// ---- Operations budget ------------------------------------------------------
// Each launch's budget tips whoever does useful upkeep. Tips are small (about
// two network fees) and paid only for work the program can verify.
/// A payout round while HODL Rewards drops remain (rounds are at most hourly).
pub const ROUND_TIP: u64 = 10_000;
/// A spot check that catches a member falling below the 75% hold line (once per member).
pub const SPOT_TIP: u64 = 10_000;
/// Collecting at least COLLECT_TIP_MIN_FEES of fees, at most hourly, while drops remain.
pub const COLLECT_TIP: u64 = 10_000;
pub const COLLECT_TIP_MIN_FEES: u64 = 1_000_000;
/// Each member refunded from a cancelled or expired launch.
pub const REFUND_TIP: u64 = 5_000;
/// Closing the budget when the launch's active period is over.
pub const CLOSE_TIP: u64 = 10_000;

// ---- Account reserves ------------------------------------------------------
/// System accounts with no data must keep this to stay rent-exempt.
pub const RENT_EXEMPT_ZERO: u64 = 890_880;
/// Rent for a 165-byte SPL token account (the creator PDA's wSOL account).
pub const RENT_EXEMPT_TOKEN_ACCOUNT: u64 = 2_039_280;
/// Kept in each launch's creator PDA: its own rent + recreating its wSOL account.
pub const CREATOR_PDA_RESERVE: u64 = RENT_EXEMPT_ZERO + RENT_EXEMPT_TOKEN_ACCOUNT;
/// Kept in the vault through launch: own rent + pump.fun's per-buyer account + margin.
pub const VAULT_BUY_RESERVE: u64 = 10_000_000;

// ---- External programs (hard-coded: nobody can pass a look-alike) -----------
pub const PUMP_PROGRAM: Pubkey = pubkey!("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P");
pub const PUMP_AMM_PROGRAM: Pubkey = pubkey!("pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA");
pub const WSOL_MINT: Pubkey = pubkey!("So11111111111111111111111111111111111111112");
pub const TOKEN_DECIMALS: u8 = 6;

// Audit run 5 (M2): the two test-only builds are never combined. A mainnet build has neither feature; the deploy
// scripts check the built file itself (scripts/check-build.sh) before anything is deployed.
#[cfg(all(feature = "short-time", feature = "devnet-squadz"))]
compile_error!("short-time (local tests) and devnet-squadz (devnet rehearsal) are never built together");
#[cfg(all(feature = "mainnet-rehearsal", any(feature = "short-time", feature = "devnet-squadz")))]
compile_error!("mainnet-rehearsal (test C) is built on its own: the mainnet build with its test values, no other feature");

// ---- $SQUADZ (launch #1) ---------------------------------------------------------
/// Wallet that creates launch #1 ($SQUADZ) with nonce 0. All protocol fees buy
/// $SQUADZ on its PumpSwap pool and burn it.
/// Real builds: the team's Squads multisig vault (created on mainnet; it does
/// not exist on devnet, so launch #1 can't be created there). If it ever changes,
/// SQUADZ_LAUNCH must be recomputed (a unit test fails until it is).
/// Mainnet (27 Oct): vault #0 of the founders' new Squads v4 multisig 3AY6…pDGz (2 of 4, no config authority).
#[cfg(not(any(feature = "short-time", feature = "devnet-squadz", feature = "mainnet-rehearsal")))]
pub const SQUADZ_CREATOR: Pubkey = pubkey!("4pki8NpwGpGZf3SUF7XZ12u2xiZfhdfX4vd8KQNbMHRC");
/// MAINNET REHEARSAL ONLY (test C): a throwaway test wallet creates the test launch #1 (scripts/set-rehearsal.sh).
#[cfg(feature = "mainnet-rehearsal")]
pub const SQUADZ_CREATOR: Pubkey = pubkey!("CiHEUndX917HY7NUrjV9N8p6KkMyUdqaJz2HW57YyvbF");
/// LOCAL TESTS ONLY: the local test wallet, so tests can create launch #1.
#[cfg(feature = "short-time")]
pub const SQUADZ_CREATOR: Pubkey = pubkey!("7BraH9py4siDyWhL3y2FYGWPz4fnZCoQ2BpQsKTcKhHA");
/// DEVNET REHEARSAL ONLY: a 2-of-3 Squads v4 vault on devnet (test member keys
/// in the workspace `keys/devnet-test-wallets/squadz-ms-*`), so launch #1,
/// buy-and-burn and Spotlight can be rehearsed with a multisig creator.
#[cfg(all(feature = "devnet-squadz", not(feature = "short-time")))]
pub const SQUADZ_CREATOR: Pubkey = pubkey!("25JXZmHvep719gjJ6u6vGgtUKJpXBYksBTz299Ebduj2");
/// Round 2 (founders 1 Nov): the first $SQUADZ lobby (nonce 0, `AGKpwt6K…xBVZ3`) expired unfilled on 31 Oct and everyone
/// was refunded. Its account stays on chain, so the relaunch is a new lobby with nonce 1.
#[cfg(not(any(feature = "short-time", feature = "devnet-squadz", feature = "mainnet-rehearsal")))]
pub const SQUADZ_NONCE: u64 = 1;
#[cfg(any(feature = "short-time", feature = "mainnet-rehearsal"))]
pub const SQUADZ_NONCE: u64 = 0;
/// DEVNET REHEARSAL ONLY: nonce 1 (22 Oct). Devnet's nonce-0 launch #1 already launched $SQUADZ on pump.fun (8 Oct);
/// $SQUADZ on Meteora is rehearsed as a fresh launch #1.
#[cfg(all(feature = "devnet-squadz", not(feature = "short-time")))]
pub const SQUADZ_NONCE: u64 = 1;
/// Launch #1's address, derived from SQUADZ_CREATOR + SQUADZ_NONCE. The
/// $SQUADZ coin address is read from it once it has launched.
#[cfg(not(any(feature = "short-time", feature = "devnet-squadz", feature = "mainnet-rehearsal")))]
pub const SQUADZ_LAUNCH: Pubkey = pubkey!("EcsceNEby1rixPAaS9fpzQPEaXSniiw1QA6ErFNsj8sN");
#[cfg(feature = "mainnet-rehearsal")]
pub const SQUADZ_LAUNCH: Pubkey = pubkey!("9k89m9bMzMiFw8TosfFS6ovrMZEkoy3WD2B398Cwohn9");
#[cfg(feature = "short-time")]
pub const SQUADZ_LAUNCH: Pubkey = pubkey!("EsfgcnFwc7hzqj8XMmP91t17MCNrsdRAgCmu41PmfQLi");
#[cfg(all(feature = "devnet-squadz", not(feature = "short-time")))]
pub const SQUADZ_LAUNCH: Pubkey = pubkey!("WFwhb8BDMzzkbSYUgXTzcnceJWBxMbAUypMpqJvRzws");
/// The protocol's upkeep bot (fix #5, A2, founders 21 Oct): it opens a launch's 4 graduation wSOL accounts before
/// the creator presses Launch (one-click launch), so `repay_shield` pays that part of the shield rent back to it; the
/// 3 coin accounts opened inside the launch are paid back to the creator. Only a receiving address: it controls
/// nothing. Mainnet: the founders' mainnet bot key (made 26 Oct, set at step C 27 Oct); scripts/check-build.sh refuses
/// a mainnet build that still holds the devnet bot.
#[cfg(not(any(feature = "short-time", feature = "devnet-squadz")))]
pub const KEEPER: Pubkey = pubkey!("EeWNc7mQfuyTtE1vcoWpnfNVHKUeuwzZEj3heCAjAQVB");
#[cfg(feature = "short-time")]
pub const KEEPER: Pubkey = pubkey!("7BraH9py4siDyWhL3y2FYGWPz4fnZCoQ2BpQsKTcKhHA");
#[cfg(all(feature = "devnet-squadz", not(feature = "short-time")))]
pub const KEEPER: Pubkey = pubkey!("3fEMMpeYZNWjcwJhhA9sugVpS4RCnvsep6rgHhNFPjqu");
// ---- $SQUADZ on Meteora (founders 22 Oct 2026) ---------------------------------------------------------------
// $SQUADZ (and only $SQUADZ) launches on Meteora's Dynamic Bonding Curve and graduates into a DAMM v2 pool with a flat
// 1.25% fee paid in SOL, its liquidity locked forever to launch #1's creator PDA. Every other coin: pump.fun as before.
pub const METEORA_DBC: Pubkey = pubkey!("dbcij3LWUppWqq96dh6gJWwBifmcGfLSB5D4DuSMaqN");
pub const METEORA_DAMM_V2: Pubkey = pubkey!("cpamdpZCGKUy5JxQXB4dcpGPiikHawvSWAd6mEn1sGG");
/// Meteora's "Customizable" DAMM v2 key: the graduated $SQUADZ pool is created under it (its address derives from it).
pub const METEORA_DAMM_CONFIG: Pubkey = pubkey!("A8gMrEPJkacWkcb3DGwtJwTe16HktSEfvwtuDh2MCtck");
/// $SQUADZ's DBC settings account, created once by the team before launch #1 (Meteora has no way to change it):
/// fixed 1B supply, ~85% sold on the curve (the pool gets 15%, founders 22 Oct), 0.25% curve fee, graduation
/// threshold SQUADZ_CURVE_THRESHOLD, then a flat 1.25% DAMM v2 pool with SOL-only fees, 100% of the liquidity locked to
/// the coin's creator, leftover tokens to the burn jar, launchpad name "Squadz". LOCAL TESTS: a local-only key
/// (.keys/squadz-dbc-config.json). Devnet: created 22 Oct (fee claimer = the devnet vault). Mainnet (27 Oct): the
/// founders' settings key (made offline, phrase on paper); the account itself is created with `squadz-mainnet config`.
/// scripts/check-build.sh refuses any non-local build that holds the local key.
#[cfg(not(any(feature = "short-time", feature = "devnet-squadz", feature = "mainnet-rehearsal")))]
pub const SQUADZ_DBC_CONFIG: Pubkey = pubkey!("5BVE2iim2f5XvdnGp9Q4GCNS2YwMkFWwcQztyYb83Nb4");
/// LOCAL TESTS ONLY: a local-only key, so the tests can create the settings account.
#[cfg(feature = "short-time")]
pub const SQUADZ_DBC_CONFIG: Pubkey = pubkey!("AAbg41hHpnptEohXspyUc6aJpuaERmb5cc6C42iqdVxZ");
/// MAINNET REHEARSAL ONLY (test C): the test's own Meteora settings (1 SOL graduation), made with a throwaway key.
#[cfg(feature = "mainnet-rehearsal")]
pub const SQUADZ_DBC_CONFIG: Pubkey = pubkey!("F4rrVnu2zHG4i9yUozyB6hyj984dWMPrzZqSFMy9Vncn");
#[cfg(all(feature = "devnet-squadz", not(feature = "short-time")))]
pub const SQUADZ_DBC_CONFIG: Pubkey = pubkey!("ExotY9KHtA8HyJC7SThrwddhCp9DNiPRySJqfxtgeC4H");
/// SOL the $SQUADZ curve collects before it graduates (founders 22 Oct): 97.7 SOL, what's left of 50 slots × 2 SOL
/// after the flat 2 SOL launch fee, the 0.25% curve fee and the reserves. Must equal the settings account's threshold
/// (checked at launch).
#[cfg(not(feature = "mainnet-rehearsal"))]
pub const SQUADZ_CURVE_THRESHOLD: u64 = 97_700_000_000;
/// MAINNET REHEARSAL ONLY (test C): 1 SOL (10 slots × 0.107 SOL with the 0.02 SOL launch fee below).
#[cfg(feature = "mainnet-rehearsal")]
pub const SQUADZ_CURVE_THRESHOLD: u64 = 1_000_000_000;
/// Launch #1's launch fee: a flat 2 SOL instead of 5% of the raise (founders 22 Oct). To the burn jar like every
/// launch fee, so it buys and burns $SQUADZ right after launch.
#[cfg(not(feature = "mainnet-rehearsal"))]
pub const SQUADZ_LAUNCH_FEE: u64 = 2_000_000_000;
/// MAINNET REHEARSAL ONLY (test C): 0.02 SOL.
#[cfg(feature = "mainnet-rehearsal")]
pub const SQUADZ_LAUNCH_FEE: u64 = 20_000_000;
/// Launch #1's Project Treasury may be up to 15% of supply (every other launch: 10%), with no treasury fee (founders
/// 22 Oct: the fee would be the team paying itself).
pub const SQUADZ_MAX_TREASURY_BPS: u16 = 1_500;
/// The curve fee, Meteora's minimum: 0.25% (as Meteora writes it: 2,500,000 per 1,000,000,000), charged on the
/// squad's buy; 80% of it comes back to launch #1 as creator fees.
pub const SQUADZ_CURVE_FEE_NUMERATOR: u64 = 2_500_000;
pub const METEORA_FEE_DENOMINATOR: u64 = 1_000_000_000;
/// Kept in the vault for the Meteora launch's account rents (the curve, its two vaults, the coin with its metadata,
/// the vault's two token accounts), on top of VAULT_BUY_RESERVE. Whatever isn't used goes back to members at claim.
pub const METEORA_LAUNCH_RESERVE: u64 = 30_000_000;
/// Meteora's curve math rounds down: buying the whole curve can deliver a hair less than its settings say (measured
/// 22 Oct: 1 raw unit). Accepted shortfall: at most 0.001 $SQUADZ (1,000 raw units).
pub const METEORA_ROUNDING_DUST: u64 = 1_000;
/// $SQUADZ's fixed supply: 1,000,000,000 tokens with 6 decimals.
pub const SQUADZ_SUPPLY: u64 = 1_000_000_000_000_000;
/// The graduated pool's fee: a flat 1.25% (founders: 1.2% on 22 Oct, 1.25% since 27 Oct), as Meteora's settings write it (basis points).
#[cfg(not(all(feature = "devnet-squadz", not(feature = "short-time"))))]
pub const SQUADZ_POOL_FEE_BPS: u16 = 125;
/// DEVNET REHEARSAL ONLY: devnet's settings account was made on 22 Oct at 1.2% and can never change.
#[cfg(all(feature = "devnet-squadz", not(feature = "short-time")))]
pub const SQUADZ_POOL_FEE_BPS: u16 = 120;

/// Max share of the $SQUADZ pool's SOL side spent in one buyback (0.5%).
pub const BURN_MAX_POOL_BPS: u64 = 50;
/// Buybacks below this are skipped (not worth the fees).
pub const BURN_MIN_AMOUNT: u64 = 1_000_000;
/// Kept in the burn jar: its own rent + its wSOL and $SQUADZ token accounts.
pub const BURN_JAR_RESERVE: u64 = 6_000_000;
/// Max accepted price impact + fees on a buyback (2%).
pub const BURN_MAX_SLIPPAGE_BPS: u64 = 200;
/// Safety valve (founders 22 Oct, audit, option D): if no buyback has succeeded for 3 days (e.g. Meteora raised the
/// $SQUADZ pool's fee past what 2% allows), the limit opens to 11% (Meteora's 10% maximum fee + the buy's own price
/// impact), so the burn can never get stuck for good once the upgrade key is gone. The next buyback that succeeds puts
/// it back to 2%. A normal buyback pays what the pool charges (~1.75%) either way.
pub const BURN_STUCK_SLIPPAGE_BPS: u64 = 1_100;
#[cfg(not(feature = "short-time"))]
pub const BURN_STUCK_AFTER: i64 = 3 * 86_400;
// LOCAL TESTS ONLY.
#[cfg(feature = "short-time")]
pub const BURN_STUCK_AFTER: i64 = 20;
/// Anti-griefing: kept from a member's refund when they choose to leave a
/// lobby (0.01 SOL), sent to the burn jar, only while the creator holds a seat
/// (12 Oct). Cancel / failed-lobby refunds are always in full.
pub const LEAVE_FEE: u64 = 10_000_000;

/// Graduation shield (11 Oct): the creator's launch flow opens the PumpSwap
/// pool's token accounts (4 wSOL before launch, 3 coin inside the launch) so
/// nobody can block graduation. `repay_shield` pays that rent back once, from
/// the launch's operations budget (current rent, read on-chain).
pub const SHIELD_WSOL_ACCOUNTS: u64 = 4;
pub const SHIELD_COIN_ACCOUNTS: u64 = 3;
pub const TOKEN_ACCOUNT_LEN: usize = 165;
pub const TOKEN_2022_ACCOUNT_LEN: usize = 170;
/// Paid from the burn jar to whoever runs a successful buy_and_burn: covers
/// the network fee and a private-relay (Jito) tip, so the bot pays itself back.
pub const BURN_TIP: u64 = 50_000;

// ---- Project Treasury (founders 20 Oct 2026, docs/27) -------------------------------------------------------
/// Seed of a lobby's treasury record (created only when the creator adds a treasury).
pub const TREASURY_SEED: &[u8] = b"treasury";
/// Treasury size: 0 (none) or a whole percent from 1% to 10% of total supply.
pub const TREASURY_STEP_BPS: u16 = 100;
pub const MAX_TREASURY_BPS: u16 = 1_000;
/// Fee: 1 SOL per 1% of supply (per 100 bps). Held until launch, then burned via $SQUADZ; refunded if no launch.
pub const TREASURY_FEE_PER_BPS: u64 = 10_000_000;
/// Release schedule after launch: 10% at launch, then 30% at 30, 60 and 90 days (the last takes the rest).
pub const TREASURY_RELEASE_BPS: [u64; 4] = [1_000, 3_000, 3_000, 3_000];
#[cfg(not(feature = "short-time"))]
pub const TREASURY_RELEASE_TIMES: [i64; 4] = [0, 2_592_000, 5_184_000, 7_776_000];
#[cfg(feature = "short-time")]
pub const TREASURY_RELEASE_TIMES: [i64; 4] = [0, 20, 40, 60];