//! Pure arithmetic. All in u128 internally, checked, no floats.

use crate::constants::*;

#[inline]
pub fn bps(amount: u64, bps: u64) -> u64 {
    ((amount as u128 * bps as u128) / BPS as u128) as u64
}

/// SOL (lamports) a constant-product curve charges for `tokens`, before fees:
/// ceil(v_sol * tokens / (v_tok - tokens)).
pub fn curve_cost(v_sol: u64, v_tok: u64, tokens: u64) -> Option<u64> {
    if tokens >= v_tok {
        return None;
    }
    let num = v_sol as u128 * tokens as u128;
    let den = (v_tok - tokens) as u128;
    u64::try_from(num.div_ceil(den)).ok()
}

/// SOL available for the launch buy: the raise minus the launch fee.
pub fn buy_budget(raise: u64) -> u64 {
    raise - bps(raise, LAUNCH_FEE_BPS)
}

/// The allowed raise for a curve: (min, max). The minimum buys the whole curve
/// plus pump.fun's 1.25% fee and our reserves after the 5% launch fee; the
/// maximum is 3% above it. None if the curve settings are unusable.
pub fn raise_range(v_sol: u64, v_tok: u64, real_tok: u64) -> Option<(u64, u64)> {
    let cost = curve_cost(v_sol, v_tok, real_tok)? as u128;
    let need = (cost * (BPS + MIN_BUY_MARGIN_BPS) as u128).div_ceil(BPS as u128) + (CREATOR_PDA_RESERVE + VAULT_BUY_RESERVE) as u128;
    let min = (need * BPS as u128).div_ceil((BPS - LAUNCH_FEE_BPS) as u128);
    let max = min * (BPS + MAX_RAISE_MARGIN_BPS) as u128 / BPS as u128;
    Some((u64::try_from(min).ok()?, u64::try_from(max).ok()?))
}

/// SOL (lamports) a Meteora curve takes to collect `threshold`, its fee included (the fee is charged on what goes
/// in): ceil(threshold / (1 - fee)).
pub fn meteora_curve_cost(threshold: u64, fee_numerator: u64) -> Option<u64> {
    let den = (METEORA_FEE_DENOMINATOR as u128).checked_sub(fee_numerator as u128).filter(|d| *d > 0)?;
    u64::try_from((threshold as u128 * METEORA_FEE_DENOMINATOR as u128).div_ceil(den)).ok()
}

/// The allowed raise for $SQUADZ on its Meteora curve: (min, max), like `raise_range`. The minimum buys the whole
/// curve with its fee, plus the Meteora launch's rents, our reserves and the flat `launch_fee`; the maximum is 3%
/// above it.
pub fn meteora_raise_range(threshold: u64, fee_numerator: u64, launch_fee: u64) -> Option<(u64, u64)> {
    let cost = meteora_curve_cost(threshold, fee_numerator)? as u128;
    let min = cost + (METEORA_LAUNCH_RESERVE + CREATOR_PDA_RESERVE + VAULT_BUY_RESERVE + launch_fee) as u128;
    let max = min * (BPS + MAX_RAISE_MARGIN_BPS) as u128 / BPS as u128;
    Some((u64::try_from(min).ok()?, u64::try_from(max).ok()?))
}

/// The buyback's price limit (bps below the fee-free price): 2%, or 11% once no buyback has succeeded for
/// BURN_STUCK_AFTER (the safety valve, founders 22 Oct). Before the first buyback ever the clock runs from $SQUADZ's
/// launch (audit run 6, 23 Oct), so the burn can't get stuck for good before it has ever run either.
pub fn burn_slippage_bps(last_burn_at: i64, launched_at: i64, now: i64) -> u64 {
    let since = if last_burn_at != 0 { last_burn_at } else { launched_at };
    if since != 0 && now >= since.saturating_add(BURN_STUCK_AFTER) {
        BURN_STUCK_SLIPPAGE_BPS
    } else {
        BURN_MAX_SLIPPAGE_BPS
    }
}

/// Splits the tokens the launch bought: 33% of total supply is held back for
/// HODL Rewards, the rest is shared equally between the slots. Returns
/// (per_member, reserve); rounding dust (< 1 unit per slot) joins the reserve.
/// None if the buy got less than the reserve (never on pump.fun's curve).
pub fn split_tokens(received: u64, total_supply: u64, slots: u8, treasury_bps: u16) -> Option<(u64, u64, u64)> {
    let target = bps(total_supply, HODL_RESERVE_BPS);
    // Project Treasury (docs/27): taken from the members' part only; the HODL pot is never touched
    let treasury = bps(total_supply, treasury_bps as u64);
    let for_members = received.checked_sub(target)?.checked_sub(treasury).filter(|t| *t > 0)?;
    let per_member = for_members / slots as u64;
    // rounding dust joins the HODL pot, as before
    Some((per_member, received - treasury - per_member * slots as u64, treasury))
}

/// The treasury's tokens, from what the launch stored (bought = members + HODL pot + treasury).
pub fn treasury_total(tokens_bought: u64, hodl_reserve: u64, per_member: u64, slots: u8) -> u64 {
    tokens_bought.saturating_sub(hodl_reserve).saturating_sub(per_member.saturating_mul(slots as u64))
}

/// Tokens unlocked at treasury release `k` (0-3). The last one releases everything left.
pub fn treasury_release_amount(total: u64, released: u64, k: usize) -> u64 {
    if k + 1 == TREASURY_RELEASE_BPS.len() {
        total - released
    } else {
        bps(total, TREASURY_RELEASE_BPS[k])
    }
}

/// Tokens released at HODL Rewards drop `k` (0-4). The last drop releases
/// everything left, so the whole reserve always goes out.
pub fn hodl_drop_amount(reserve: u64, released: u64, k: usize) -> u64 {
    if k + 1 == HODL_DROP_BPS.len() {
        reserve - released
    } else {
        bps(reserve, HODL_DROP_BPS[k])
    }
}

/// Splits a drop equally between qualified members. Returns (each, to_burn):
/// rounding dust, or the whole drop if nobody qualifies, is burned.
pub fn split_drop(amount: u64, qualified: u64) -> (u64, u64) {
    if qualified == 0 {
        return (0, amount);
    }
    (amount / qualified, amount % qualified)
}

#[derive(Debug, PartialEq, Eq)]
pub struct FeeSplit {
    pub protocol: u64,
    pub creator: u64,
    pub members: u64,
}

/// New creator fees: 10% to the protocol, the rest split creator / members.
pub fn split_fees(amount: u64, creator_fee_bps: u16) -> FeeSplit {
    let protocol = bps(amount, PROTOCOL_FEE_SHARE_BPS);
    let rest = amount - protocol;
    let creator = bps(rest, creator_fee_bps as u64);
    FeeSplit { protocol, creator, members: rest - creator }
}

/// Share of original tokens still held, in bps, capped at 100%.
pub fn held_bps(balance: u64, original: u64) -> u16 {
    if original == 0 {
        return 0;
    }
    (balance as u128 * BPS as u128 / original as u128).min(BPS as u128) as u16
}

/// Splits `pot` in proportion to `weights`. Returns each share and the total
/// actually distributed (rounding dust stays in the pot).
pub fn distribute(pot: u64, weights: &[u16], out: &mut [u64]) -> u64 {
    let total: u128 = weights.iter().map(|w| *w as u128).sum();
    if total == 0 {
        return 0;
    }
    let mut sum = 0u64;
    for (w, o) in weights.iter().zip(out.iter_mut()) {
        *o = (pot as u128 * *w as u128 / total) as u64;
        sum += *o;
    }
    sum
}

/// True if the base58 text of `key` ends with `suffix` (case-sensitive).
/// The last k base58 characters are the 32-byte number mod 58^k, so this needs
/// no string encoding: one pass over the bytes. Unknown characters never match.
pub fn ends_with_base58(key: &[u8; 32], suffix: &[u8]) -> bool {
    let mut m: u64 = 1;
    let mut target: u64 = 0;
    for c in suffix {
        let Some(d) = BASE58.iter().position(|x| x == c) else { return false };
        m *= 58;
        target = target * 58 + d as u64;
    }
    let mut r: u64 = 0;
    for b in key {
        r = (r * 256 + *b as u64) % m;
    }
    r == target
}

/// A launch's chosen address ending is valid: none (all zeros), or 4 base58
/// characters that aren't another launchpad's ending.
pub fn ending_ok(e: &[u8; ENDING_LEN]) -> bool {
    if *e == [0u8; ENDING_LEN] {
        return true;
    }
    e.iter().all(|c| BASE58.contains(c)) && !RESERVED_ENDINGS.iter().any(|r| *r == e)
}

#[cfg(test)]
mod tests {
    use super::*;
    use anchor_lang::prelude::Pubkey;

    const V_SOL: u64 = 30_000_000_000; // mainnet: 30 SOL virtual
    const V_TOK: u64 = 1_073_000_000_000_000;
    const REAL: u64 = 793_100_000_000_000;
    const SUPPLY: u64 = 1_000_000_000_000_000;
    const RESERVE: u64 = SUPPLY / 10_000 * HODL_RESERVE_BPS; // 330M tokens

    #[test]
    fn mainnet_curve_costs_about_85_sol() {
        let c = curve_cost(V_SOL, V_TOK, REAL).unwrap();
        assert!((84_900_000_000..85_100_000_000).contains(&c), "{c}");
    }

    #[test]
    fn mainnet_raise_is_about_90_6_sol() {
        let (min, max) = raise_range(V_SOL, V_TOK, REAL).unwrap();
        assert!((90_400_000_000..90_800_000_000).contains(&min), "{min}");
        assert_eq!(max, min * 10_300 / 10_000);
        // the minimum really covers fee + curve + pump fee + reserves
        let cost = curve_cost(V_SOL, V_TOK, REAL).unwrap();
        assert!(buy_budget(min) >= cost + bps(cost, MIN_BUY_MARGIN_BPS) + CREATOR_PDA_RESERVE + VAULT_BUY_RESERVE);
        // a website's +2% fits inside the 3% window
        assert!(min * 102 / 100 <= max);
    }

    #[test]
    #[cfg(not(feature = "mainnet-rehearsal"))] // test C's build is 1 SOL on purpose (constants.rs); checked below instead
    fn squadz_meteora_raise_is_50_slots_of_2_sol() {
        // 85 SOL curve + 0.25% fee = 85.2130 SOL (measured locally 22 Oct: the squad paid 85.213033)
        assert_eq!(meteora_curve_cost(85_000_000_000, SQUADZ_CURVE_FEE_NUMERATOR).unwrap(), 85_213_032_582);
        // founders 22 Oct: 50 slots x 2 SOL = 100 SOL covers the 97.7 SOL curve + 0.25% + reserves + the flat 2 SOL fee
        let cost = meteora_curve_cost(SQUADZ_CURVE_THRESHOLD, SQUADZ_CURVE_FEE_NUMERATOR).unwrap();
        let (min, max) = meteora_raise_range(SQUADZ_CURVE_THRESHOLD, SQUADZ_CURVE_FEE_NUMERATOR, SQUADZ_LAUNCH_FEE).unwrap();
        assert_eq!(min, cost + METEORA_LAUNCH_RESERVE + CREATOR_PDA_RESERVE + VAULT_BUY_RESERVE + SQUADZ_LAUNCH_FEE);
        assert_eq!(max, min * 10_300 / 10_000);
        let raise = 50 * 2_000_000_000u64;
        assert!(min <= raise && raise <= max, "{min} <= {raise} <= {max}");
        assert!(raise - min < 20_000_000, "less than 0.02 SOL of the 100 SOL left unused: {}", raise - min);
        assert!(meteora_curve_cost(1, METEORA_FEE_DENOMINATOR).is_none(), "a 100% fee is refused");
    }

    /// Test C's build (27 Oct): 10 slots of the rehearsal tool's slot price fit its own 1 SOL range.
    #[test]
    #[cfg(feature = "mainnet-rehearsal")]
    fn rehearsal_raise_is_10_small_slots() {
        let (min, max) = meteora_raise_range(SQUADZ_CURVE_THRESHOLD, SQUADZ_CURVE_FEE_NUMERATOR, SQUADZ_LAUNCH_FEE).unwrap();
        let raise = (min * 1_005 / 1_000).div_ceil(10) * 10; // tests/src/client.rs squadz_meteora_args
        assert!(min <= raise && raise <= max, "{min} <= {raise} <= {max}");
        assert!(SQUADZ_CURVE_THRESHOLD == 1_000_000_000 && SQUADZ_LAUNCH_FEE == 20_000_000);
    }

    #[test]
    fn burn_limit_is_2_percent_and_opens_to_11_after_3_stuck_days() {
        let t = 1_800_000_000i64;
        let l = t - 86_400; // $SQUADZ launched a day before t
        assert_eq!(burn_slippage_bps(0, l, l + BURN_STUCK_AFTER - 1), 200, "first buyback ever, soon after launch: 2%");
        assert_eq!(burn_slippage_bps(0, l, l + BURN_STUCK_AFTER), 1_100, "no buyback ever since launch for the whole period: 11%");
        assert_eq!(burn_slippage_bps(0, 0, i64::MAX), 200, "launch time unknown: never opens on its own");
        assert_eq!(burn_slippage_bps(t, l, t + BURN_STUCK_AFTER / 2), 200, "halfway to the valve: 2%");
        assert_eq!(burn_slippage_bps(t, l, t + BURN_STUCK_AFTER - 1), 200, "just under the valve: 2% (last buyback counts, not launch)");
        assert_eq!(burn_slippage_bps(t, l, t + BURN_STUCK_AFTER), 1_100, "no buyback for the whole period: 11%");
        assert_eq!(burn_slippage_bps(t, l, i64::MAX), 1_100);
        #[cfg(not(feature = "short-time"))]
        assert_eq!(BURN_STUCK_AFTER, 3 * 86_400, "founders 22 Oct: 3 days");
    }

    #[test]
    fn stress_raise_range_always_covers_the_buy() {
        let mut r = Rng(0xe7c3);
        for _ in 0..100_000 {
            let v_sol = 1_000_000 + r.below(100_000_000_000);
            let v_tok = 1_000_000_000 + r.below(2_000_000_000_000_000);
            let real = r.below(v_tok);
            let Some((min, max)) = raise_range(v_sol, v_tok, real) else { continue };
            let cost = curve_cost(v_sol, v_tok, real).unwrap();
            assert!(buy_budget(min) >= cost + bps(cost, MIN_BUY_MARGIN_BPS) + CREATOR_PDA_RESERVE + VAULT_BUY_RESERVE);
            assert!(max >= min && max - min <= bps(min, MAX_RAISE_MARGIN_BPS));
        }
    }

    #[test]
    fn tokens_split_equally_with_hodl_reserve() {
        let mut r = Rng(0xa110_c8);
        for _ in 0..100_000 {
            let received = REAL - r.below(1_000_000);
            let slots = 10 + r.below(41) as u8;
            let (each, reserve, treasury) = split_tokens(received, SUPPLY, slots, 0).unwrap();
            assert_eq!(treasury, 0, "no treasury: exactly as before");
            assert_eq!(each * slots as u64 + reserve, received, "every token accounted for");
            assert!(reserve >= RESERVE && reserve - RESERVE < slots as u64, "reserve = 33% of supply + dust");
        }
        // 50 slots: 9.262M each at claim, 330M reserve (6.6M each if all hold)
        assert_eq!(split_tokens(REAL, SUPPLY, 50, 0), Some((9_262_000_000_000, 330_000_000_000_000, 0)));
        assert_eq!(split_tokens(RESERVE, SUPPLY, 10, 0), None, "nothing left for members");
    }

    // Project Treasury (docs/27): from the members' part only, HODL pot untouched, nothing lost, every % x slots
    #[test]
    fn treasury_comes_from_the_claim_only() {
        let mut r = Rng(0x7e45_0e5);
        for pct in 1..=10u16 {
            let bps_ = pct * TREASURY_STEP_BPS;
            for _ in 0..10_000 {
                let received = REAL - r.below(1_000_000);
                let slots = 10 + r.below(41) as u8;
                let (each, reserve, treasury) = split_tokens(received, SUPPLY, slots, bps_).unwrap();
                assert_eq!(treasury, SUPPLY / 10_000 * bps_ as u64, "treasury = exactly X% of supply");
                assert_eq!(each * slots as u64 + reserve + treasury, received, "every token accounted for");
                assert!(reserve >= RESERVE && reserve - RESERVE < slots as u64, "HODL pot untouched (33% + dust)");
                assert_eq!(treasury_total(received, reserve, each, slots), treasury, "release reads back the same total");
            }
        }
        // 50 slots, 10%: 7.262M each at claim (was 9.262M), HODL pot still 330M, treasury 100M
        assert_eq!(split_tokens(REAL, SUPPLY, 50, 1_000), Some((7_262_000_000_000, 330_000_000_000_000, 100_000_000_000_000)));
    }

    #[test]
    fn treasury_releases_10_30_30_30_and_everything() {
        for total in [100_000_000_000_000u64, 10_000_000_000_000, 99_999_999_999_999, 1] {
            let mut released = 0;
            let mut parts = vec![];
            for k in 0..4 {
                let p = treasury_release_amount(total, released, k);
                released += p;
                parts.push(p);
            }
            assert_eq!(released, total, "the whole treasury goes out, nothing more");
            if total == 100_000_000_000_000 {
                assert_eq!(parts, [10_000_000_000_000, 30_000_000_000_000, 30_000_000_000_000, 30_000_000_000_000]);
            }
        }
        assert_eq!(TREASURY_RELEASE_TIMES[0], 0, "the first 10% unlocks at launch");
    }

    #[test]
    fn hodl_drops_ramp_and_release_everything() {
        let reserve = 330_000_000_000_000u64;
        let mut released = 0;
        let mut drops = vec![];
        for k in 0..5 {
            let d = hodl_drop_amount(reserve, released, k);
            released += d;
            drops.push(d);
        }
        // 33M / 49.5M / 66M / 82.5M / 99M tokens
        assert_eq!(drops, [33_000_000_000_000, 49_500_000_000_000, 66_000_000_000_000, 82_500_000_000_000, 99_000_000_000_000]);
        assert_eq!(released, reserve);
        // 50 qualified: 1.98M each at the last drop; 5 left: 19.8M each; none: all burned
        assert_eq!(split_drop(99_000_000_000_000, 50), (1_980_000_000_000, 0));
        assert_eq!(split_drop(99_000_000_000_000, 5), (19_800_000_000_000, 0));
        assert_eq!(split_drop(99_000_000_000_000, 0), (0, 99_000_000_000_000));
        let mut r = Rng(0xd20b);
        for _ in 0..100_000 {
            let reserve = RESERVE + r.below(50);
            let mut released = 0;
            for k in 0..5 {
                let d = hodl_drop_amount(reserve, released, k);
                let q = r.below(51);
                let (each, burn) = split_drop(d, q);
                assert_eq!(each * q + burn, d);
                released += d;
            }
            assert_eq!(released, reserve, "the whole reserve goes out");
        }
    }

    #[test]
    fn fee_split_protocol_always_10_percent() {
        for bps_c in [0u16, 5_000, 10_000] {
            let s = split_fees(1_000_000_000, bps_c);
            assert_eq!(s.protocol, 100_000_000);
            assert_eq!(s.protocol + s.creator + s.members, 1_000_000_000);
        }
        let s = split_fees(1_000_000_000, 10_000);
        assert_eq!((s.creator, s.members), (900_000_000, 0));
        let s = split_fees(1_000_000_000, 0);
        assert_eq!((s.creator, s.members), (0, 900_000_000));
    }

    #[test]
    fn loyalty_is_straight_line_and_capped() {
        assert_eq!(held_bps(100, 100), 10_000);
        assert_eq!(held_bps(75, 100), 7_500);
        assert_eq!(held_bps(50, 100), 5_000);
        assert_eq!(held_bps(0, 100), 0);
        assert_eq!(held_bps(500, 100), 10_000);
    }

    /// Deterministic xorshift so failures reproduce.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }
    }

    #[test]
    fn stress_fee_split_conserves_every_lamport() {
        let mut r = Rng(0x5eed_1234);
        for _ in 0..200_000 {
            let amount = r.below(1_000_000_000_000_000);
            let c = r.below(10_001) as u16;
            let s = split_fees(amount, c);
            assert_eq!(s.protocol + s.creator + s.members, amount);
            assert_eq!(s.protocol, bps(amount, 1_000));
        }
    }

    #[test]
    fn stress_distribution_is_exact_proportional_and_bounded() {
        let mut r = Rng(0xd15_7);
        for _ in 0..50_000 {
            let n = 10 + r.below(41) as usize;
            let pot = r.below(1_000_000_000_000);
            let w: Vec<u16> = (0..n).map(|_| if r.below(4) == 0 { 0 } else { r.below(10_001) as u16 }).collect();
            let mut out = vec![0u64; n];
            let paid = distribute(pot, &w, &mut out);
            assert_eq!(out.iter().sum::<u64>(), paid);
            assert!(paid <= pot);
            if w.iter().any(|x| *x > 0) {
                assert!(pot - paid < n as u64, "rounding dust below one lamport per member");
            }
            for i in 0..n {
                for j in 0..n {
                    if w[i] <= w[j] {
                        assert!(out[i] <= out[j], "a bigger holder never gets less");
                    }
                }
                if w[i] == 0 {
                    assert_eq!(out[i], 0, "a full seller gets nothing");
                }
            }
        }
    }

    #[test]
    fn stress_curve_cost_never_undercharges() {
        let mut r = Rng(0xc0_57);
        for _ in 0..100_000 {
            let v_sol = 1 + r.below(100_000_000_000);
            let v_tok = 1_000_000 + r.below(2_000_000_000_000_000);
            let t = r.below(v_tok);
            let cost = curve_cost(v_sol, v_tok, t).unwrap();
            // constant product must not shrink: (v_sol + cost) * (v_tok - t) >= v_sol * v_tok
            assert!((v_sol as u128 + cost as u128) * (v_tok - t) as u128 >= v_sol as u128 * v_tok as u128);
        }
    }

    #[test]
    fn stress_loyalty_bounded() {
        let mut r = Rng(0x10_7a1);
        for _ in 0..100_000 {
            let orig = 1 + r.below(u64::MAX / 20_000);
            let bal = r.below(orig * 3);
            let h = held_bps(bal, orig);
            assert!(h <= 10_000);
            if bal >= orig {
                assert_eq!(h, 10_000);
            }
        }
    }

    #[test]
    fn squadz_launch_matches_derivation() {
        let derived = Pubkey::find_program_address(&[LAUNCH_SEED, SQUADZ_CREATOR.as_ref(), &SQUADZ_NONCE.to_le_bytes()], &crate::ID).0;
        assert_eq!(derived, SQUADZ_LAUNCH, "recompute SQUADZ_LAUNCH: SQUADZ_CREATOR or SQUADZ_NONCE changed");
    }

    #[test]
    fn sellers_share_goes_to_holders() {
        // three full holders, one sold half: pot 7 SOL -> 2/2/2/1
        let mut out = [0u64; 4];
        let paid = distribute(7_000_000_000, &[10_000, 10_000, 10_000, 5_000], &mut out);
        assert_eq!(out, [2_000_000_000, 2_000_000_000, 2_000_000_000, 1_000_000_000]);
        assert_eq!(paid, 7_000_000_000);
        // nobody holds: nothing distributed
        assert_eq!(distribute(1_000, &[0, 0], &mut [0, 0]), 0);
    }

    #[test]
    fn base58_suffix_matches_string_encoding() {
        let mut seed = 7u64;
        for _ in 0..20_000 {
            let mut k = [0u8; 32];
            for b in k.iter_mut() {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                *b = (seed >> 33) as u8;
            }
            let s = Pubkey::new_from_array(k).to_string();
            let tail = &s.as_bytes()[s.len() - ENDING_LEN..];
            assert!(ends_with_base58(&k, tail), "{s}");
            assert_eq!(ends_with_base58(&k, b"fair"), s.ends_with("fair"));
        }
        assert!(!ends_with_base58(&[0u8; 32], b"0OIl"), "characters outside base58 never match");
    }

    #[test]
    fn endings_valid_reserved_and_none() {
        assert!(ending_ok(b"fair"));
        assert!(ending_ok(b"SQDZ"));
        assert!(ending_ok(&[0; 4]), "no ending");
        for r in [b"pump", b"bonk", b"bags"] {
            assert!(!ending_ok(r), "reserved");
        }
        for bad in [b"fa0r", b"faIr", b"fOir", b"fal ", b"fai\0"] {
            assert!(!ending_ok(bad), "{:?}", std::str::from_utf8(bad));
        }
    }
}
