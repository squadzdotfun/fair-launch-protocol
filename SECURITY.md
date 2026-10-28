# Security notes (self-review, pre-audit)

Not audited. This is the team's own review, to hand to auditors.

## Trust model

- **No pause, no fee switch.** Every rule is fixed in the program. The upgrade
  authority is held by the team's 2-of-4 Squads multisig for the first week
  after the mainnet deployment; we intend to remove it then
  (`solana program set-upgrade-authority --final`, as a multisig proposal).
  Until it is removed, the multisig could change the program.
- All SOL and tokens sit in program PDAs, none of which has a private key:
  - a per-launch **vault** (system-owned; signs the pump.fun buy),
  - a per-launch **creator PDA** (the coin's pump.fun creator; receives creator fees),
  - the **launch account** itself (holds fees until paid or swept),
  - a per-launch **operations budget** (system-owned; tips upkeep),
  - the protocol **burn jar**.
- Only the creator can `create_launch`, `set_fee_wallet` (before launch only),
  `cancel` and `launch_token`. Only a member can `join`, `leave`, `claim` and
  `claim_hodl` for themselves. Everything else is permissionless and can only
  move funds to addresses fixed in state.
- **Gatekeepers are per launch and optional.** The program has no built-in
  key. A gated launch trusts its gatekeeper's key: whoever holds it decides who
  may join that launch (and nothing else). A website using one key for all its
  launches must keep that key safe; a stolen key lets the thief let wallets into
  those launches, but cannot move any funds.

## Checks by instruction

| Instruction | Key checks |
| --- | --- |
| create_launch | Every setting range-checked; ending empty or 4 base58 characters and not `pump` / `bonk` / `bags`; raise covers pump.fun's live curve (pump Global read, owner + address checked) + 1.25% + the 5% launch fee, at most 3% above; donation wallet set; fee wallet not one of the launch's own accounts; creation fee by window (0.05 / 0.10 / 0.20 SOL) plus paid extras (approval-only, 2 slots per wallet), split into deposit + budget |
| set_fee_wallet | Creator only; launch still open (locked from launch onwards); not empty, not one of the launch's own accounts |
| join | Open, before deadline, not full, at most the launch's slots per wallet (1, or 2 if the creator chose it); donation 0 or >= 0.001 SOL; gated launches: pass not expired, and the instruction directly before verifies an ed25519 signature by the launch's gatekeeper over (tag, program, launch, wallet, expiry) with every offset inside that instruction |
| leave | Open, before deadline, member, not refunded (no double pay); returns slot + donation, minus 0.01 SOL per slot (to the burn jar) while the creator holds a seat |
| cancel / refund | Creator only / cancelled or expired only; each member refunded slot + donation once; refund tips come from the budget only for members actually refunded |
| launch_token | Creator, open, full, before deadline; coin address = PDA of (launch, coin number, bump) via `create_program_address`, and ends in the launch's ending; registered donation wallet; pump.fun program address hard-coded; mint, buyer, token accounts and bonding curve checked; curve SOL-quoted and not mayhem; after the buy: curve complete, zero reserves left, creator = our PDA, exact tokens received; 33% of supply held back; launch fee booked for the protocol |
| claim / claim_hodl | Launched; coin = the stored one; member; once (claim) / only what was released (claim_hodl, zeroed before transfer); token accounts address-checked |
| burn_hodl | Coin = the stored one; only tokens nobody qualified for; vault signs; vault token account checked |
| spot_check | Launched; each slot index in range; each token account = that member's for the stored coin; lowest holding only decreases; tip once per member, only on crossing below 75% while drops remain |
| collect_fees / sweep_fees | Launched; creator PDA and its wSOL account checked; pump.fun and PumpSwap program addresses checked; appended pump accounts pass through; tip at most hourly, only for >= 0.001 SOL, only while drops remain |
| payout_round | Launched; hourly; one account per member in slot order, each address-checked; lowest holding only decreases; each drop released once at fixed times, equally to members never below 75%; last drop releases exactly what remains; a paying round is reimbursed from the pot, a non-paying round is tipped from the budget while drops remain |
| pay_out | Owed amounts only; creator's share only to the fee wallet; wallets that can't receive (program addresses, or empty and below rent) are skipped and keep what they're owed; the launch account never drops below rent |
| sweep_protocol | Only the launch's booked protocol fees; launch account never below rent; only to the burn jar |
| buy_and_burn | Hourly; launch #1 read at its fixed address (a no-op until it has launched and graduated); pool = $SQUADZ's Meteora DAMM v2 pool, derived from the fixed Meteora config, the two mints and the given bump; mints, burn jar token accounts, payer and program address-checked, no referral; buy capped at 0.5% of pool SOL; min-out 2% under the fee-free price (11% after 3 days without a buyback) |
| close_ops | Only when the launch needs no more upkeep (every drop out, or failed and every member refunded); small tip, the rest to the burn jar |

All arithmetic is checked (release builds have overflow checks on); raise,
token split, fee and distribution maths are property-tested (550k+ randomized
cases).

## Known risks and limitations

1. **pump.fun admin powers.** pump.fun can reassign a coin's creator, change
   curve parameters, change fees, or disable PumpSwap pools. A reassigned
   creator redirects future creator fees away from this program.
2. **pump.fun instruction changes.** Appended accounts pass through (launch,
   collect, burn). A *replaced* launch instruction would stop new launches
   (refunds still work). A replaced collect instruction is covered by
   `sweep_fees`: anyone calls pump.fun's new collect directly (it pays the
   creator PDA), then sweeps. Not covered: pump.fun requiring the creator to
   sign to collect.
3. **Buyback sandwiching.** The price floor is computed inside the
   transaction, so a bot trading just before it in the same block can extract
   up to ~2% of a buyback. Bounded by the 0.5%-of-pool cap. Keepers should
   submit buybacks privately (e.g. Jito bundles).
4. **Holdings are sampled.** Rounds read balances hourly and spot checks
   whenever someone runs them. A sell and buy-back that nobody checks in
   between is not seen. Websites should run a spot check on every member sale
   they detect.
5. **Sybils.** One person can join with several wallets. Gatekeepers decide
   who gets passes (for example one account, two wallets).
6. **Gatekeeper availability.** A gated launch can only be joined while its
   gatekeeper issues passes. Leaving, refunds, launch, claims and everything
   after work without it.
7. **Lookup tables required for the big transactions.** Launch, graduation,
   rounds, payouts and spot checks of many members at once exceed 1232 bytes
   without address lookup tables. Fee collection and a spot check of a single
   member fit without one (measured). Rounds, spot checks and payouts at 50
   members use up to 57 of 64 transaction account locks.
8. **Launch #1 creator.** Real builds set `SQUADZ_CREATOR` to the team's
   multisig vault; `short-time` (local test) builds use a local test wallet so
   tests can create launch #1. If the creator ever changes, recompute
   `SQUADZ_LAUNCH` (the unit test `squadz_launch_matches_derivation` fails until
   you do).
9. **Dust.** Rounding dust (< 1 lamport per member per round; < 1 token unit
   and < 1 lamport per slot at launch) stays in the pot / vault.
10. **Everyone sold.** If no member holds any tokens, that round's members' pot
    is booked for the protocol (and so burned). If nobody qualifies for a HODL
    Rewards drop, that drop is burned (anyone can call `burn_hodl`).
11. **HODL Rewards timing.** Drops are released by the first round after each
    drop time, so they depend on someone running rounds (anyone can).
12. **Donation wallet.** Set by the creator. A wallet that can't receive SOL
    blocks that launch only; members are refunded when the window ends.
13. **Operations budget.** Tips are fixed small amounts for verifiable work and
    can't exceed the budget; the budget never pays for anything outside its
    own launch. Network costs that the program can't verify (address lookup
    tables, triggering graduation) are paid by whoever runs the bot.

## Test coverage

Local validator with pump.fun, PumpSwap, pump fees and mayhem copied from
devnet (read-only). `short-time` builds (hours -> seconds, payout threshold
0.001 SOL) are for local tests only and are never deployed elsewhere.

| Test | Covers |
|---|---|
| phase0 | pump.fun works locally |
| phase2 | creation rules, the 0.05 SOL split, open lifecycle, claims, cancel / expiry refunds with tips |
| phase3 | creator fees (curve + PumpSwap + direct collect), 10% protocol share, holdings, payouts, fee wallet lock, protocol sweep |
| phase4 | $SQUADZ buy and burn: no-op before launch #1, canonical pool only, caps, hourly limit |
| phase5 | 50 members with passes and a `...fair` coin through every step within Solana's limits |
| phase6 | HODL Rewards: ramp, pass / fail at 75%, buy-back doesn't restore, burn when everyone sold |
| phase7 | join passes (wrong key / wallet / launch / expiry, misplaced, foreign bytes) and coin endings (default + custom, wrong number / bump, pre-funding, hijack attempt) |
| phase8 | operations budget: every tip, spot checks, sell + buy-back caught, close after success / failure |
