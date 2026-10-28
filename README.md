# Fair Launch Protocol

Fair group launches on [pump.fun](https://pump.fun), on Solana.

A group of 10 to 50 people pools SOL into equal slots. When every slot is
filled, the coin is created on pump.fun and the pool buys the **entire bonding
curve in the same transaction**. Nobody can buy ahead of the group, everyone
pays the same price, and everyone gets the same share. Creator fees then flow
back through the protocol forever.

There is no pause switch and no team allocation in other people's coins. The
upgrade key is held by the team's 2-of-4 multisig for the first week after the
mainnet deployment; we intend to remove it then, and after that nobody can change
the program. [Squadz](https://squadz.fun) is the first website built on the
protocol; anyone can build their own.

> **Status:** deployed on Solana mainnet at
> `BstpzKnft3hQNaae8TWL8KnUsff6wMZEtn5moEcD8FYR` (October 2026). Tested on a
> local validator with pump.fun's and Meteora's programs copied from devnet, on
> devnet, and in a mainnet rehearsal. **Not independently audited. Use at your
> own risk.**

## How a launch works

1. **Create.** A creator opens a launch: name, ticker, metadata link, 10-50 slots, a
   time window (24 hours, 72 hours or 1 week) and the slot price. The raise
   must cover what pump.fun's curve costs at that moment (read on-chain), plus
   fees, and can be at most 3% above it. The creation fee is 0.05, 0.10 or
   0.20 SOL depending on the window, plus 0.05 SOL for approval-only and
   0.10 SOL for up to 2 slots per wallet.
2. **Join.** Members take a slot (or two, if the creator allows it) by paying
   the slot price, plus an optional donation to the creator. Before launch,
   anyone can leave and get their slot and donation back, minus a 0.01 SOL
   anti-griefing fee per slot while the creator holds a seat.
3. **Launch.** When every slot is filled, the creator launches. One
   transaction creates the coin with a protocol address as its pump.fun
   creator, buys the whole curve, and checks the result. If anything is off,
   nothing happens.
4. **Claim.** Each member claims an equal share of the tokens, plus their
   share of any SOL the buy didn't need. There is no deadline.
5. **Hold.** 33% of the coin's total supply is held back as **HODL Rewards**
   and released in 5 drops over 2 weeks (10/15/20/25/30%) to members who never
   fall below 75% of the tokens they claimed. If nobody qualifies for a drop,
   it is burned.
6. **Fees, forever.** pump.fun pays creator fees to the protocol address. The
   protocol takes 10%; the rest goes to the creator, or is shared with members
   as a **Fee Share** if the creator chose one. Members' shares follow how
   much they kept holding, so sellers' shares go to holders.

If a launch is cancelled by its creator, or its window ends before it fills,
every member is refunded their slot and donation in full.

A creator can also set aside a **Project Treasury** (1-10% of supply, shown before
anyone joins, released at launch and after 30, 60 and 90 days).

**$SQUADZ** is launch #1. It uses the same fair start but launches on
[Meteora](https://meteora.ag)'s Dynamic Bonding Curve, graduates into a Meteora
pool with a flat 1.25% fee and liquidity locked forever, pays a flat 2 SOL launch
fee, and has a 15% treasury held by the team's multisig.

## Design

| | |
|---|---|
| **Gatekeeper per launch** | A launch can name a key (a website, the creator, anyone) that must sign a *join pass* for each wallet. Passes are ed25519 signatures checked on-chain, tied to one launch, one wallet and an expiry time. A launch without a gatekeeper is open to anyone. The protocol has no built-in key of its own. |
| **Coin address endings** | A launch can require its coin address to end in 4 chosen characters (`...fair` by default on Squadz). The address is derived from the launch and a number found off-chain, is only revealed by the launch transaction, and only this program can sign for it, so nobody can create a coin there first or block it. |
| **Fixed rules** | Every rule is fixed in the program. Permissionless steps can only move funds to addresses fixed in each launch's state. |
| **Operations budget** | Each launch's creation fee funds its own small budget, which tips whoever does verifiable upkeep (payout rounds, spot checks, fee collection, refunds). Anyone can run a bot; when a launch needs no more upkeep, what is left goes to the burn jar. |
| **Spot checks** | Besides hourly rounds, anyone can re-read members' holdings at any time (for example right after a sale), so selling and buying back between rounds is caught. |
| **Protocol fees burn $SQUADZ** | The launch fee, 10% of creator fees and what operations budgets don't use collect in a burn jar (a program address with no private key), which buys $SQUADZ (launch #1) on its Meteora pool and burns it. |

### Numbers

| | |
|---|---|
| Slots | 10-50 (up to 2 per wallet if the creator allows it) |
| Creation fee | 0.05 / 0.10 / 0.20 SOL (24 h / 72 h / 1 week window) + 0.05 approval-only + 0.10 two slots per wallet; pays the launch's storage deposit and its operations budget |
| Leave fee | 0.01 SOL per slot, only while the creator holds a seat; refunds of cancelled or expired launches are always in full |
| Launch fee | 5% of the raise, only when the launch succeeds (a flat 2 SOL for $SQUADZ) |
| Protocol share of creator fees | 10% |
| HODL Rewards | 33% of supply, 5 drops over 2 weeks, hold line 75% |
| Fee Share payout threshold | 0.25 SOL per member |
| $SQUADZ buyback | at most hourly, at most 0.5% of the pool's SOL, 2% price limit (11% after 3 days without a buyback) |

## Instructions

| Instruction | Who | What |
|---|---|---|
| `create_launch` | creator | Opens a launch; funds its operations budget |
| `add_treasury` / `set_max_per_wallet` / `set_approval` | creator | Options only possible at creation: Project Treasury, 2 slots per wallet, approval-only |
| `set_fee_wallet` | creator | Redirects the creator's fee share; only before launch |
| `join` / `join_slots` | member | Takes a slot or two (with a pass if the launch has a gatekeeper) |
| `add_donation` | member | Adds to the member's donation before launch |
| `leave` / `leave_slots` | member | Leaves before launch; refunded minus the leave fee while the creator holds a seat |
| `cancel` | creator | Cancels before launch; refunds open |
| `refund` | anyone | Refunds members of a cancelled or expired launch, in full |
| `launch_token` | creator | Creates the coin on pump.fun and buys the whole curve |
| `launch_token_meteora` | creator | Launch #1 ($SQUADZ) only: creates the coin on Meteora and buys the whole curve |
| `repay_shield` | anyone | After launch: pays back the graduation account deposits |
| `settle_treasury_fee` / `release_treasury` | anyone | Settles a Project Treasury's fee; releases its parts on schedule |
| `claim` | member | Takes the member's tokens and unused SOL |
| `claim_hodl` | member | Takes released HODL Rewards |
| `burn_hodl` | anyone | Burns HODL Rewards nobody qualified for |
| `spot_check` | anyone | Re-reads chosen members' holdings now |
| `collect_fees` / `sweep_fees` / `collect_fees_meteora` | anyone | Books creator fees into the launch (pump.fun / PumpSwap; Meteora for $SQUADZ) |
| `payout_round` | anyone | Hourly: holdings snapshot, HODL drops, Fee Share split |
| `pay_out` | anyone | Sends what members and the fee wallet are owed |
| `sweep_protocol` | anyone | Moves a launch's protocol fees to the burn jar |
| `buy_and_burn` | anyone | Buys $SQUADZ with the burn jar and burns it |
| `close_ops` | anyone | Empties a finished launch's operations budget into the burn jar |

## Layout

| Path | What |
|---|---|
| `programs/fair-launch` | The on-chain program (Anchor 0.31, Solana 2.3) |
| `programs/fair-launch/src/math.rs` | All arithmetic, with unit and randomized stress tests |
| `idls/` | pump.fun, PumpSwap and pump fees interface files, pinned (`idls/SOURCE.txt`) |
| `scripts/` | Local validator, builds, tests, deploy scripts (with their safety checks) |

The integration tests and the command-line tool are kept separately for now; a
public command-line tool is planned.

## Running locally (Linux / WSL)

```bash
bash -l scripts/build-test.sh          # LOCAL test build (hours and days shrunk to seconds)
bash -l scripts/local-validator.sh     # in a second terminal: pump.fun copied from devnet (read-only) + this program
cargo test -p fair-launch --lib        # unit and stress tests (real constants)
```

`anchor build` alone produces the real build. The `short-time` build is for
local tests only and must never be deployed anywhere else. The scripts create
a local test wallet in `.keys/` (never committed, never funded on a real
network).

## Devnet

```bash
bash -l scripts/deploy-devnet.sh <program-id-keypair.json> <deployer-wallet.json>
```

Devnet only. It refuses any other network, refuses the default Solana wallet
and keys stored inside this repo, checks the program key matches
`declare_id!`, always does a fresh real build, shows the cost, and asks for
confirmation before sending anything. Program address:
`7vGLH6t2HSWmLzk4E8GPF2EbnQPSwnA9n3sJ9GXTsaMw`.

## Using it without squadz.fun

Every instruction is public: anyone can call the program directly (`anchor
build` writes its interface file to `target/idl/`; see the instruction list above)
or build their own website and bots.
`docs/Using-the-protocol.md` explains the flow. A public command-line tool is
planned.

## Security

See [SECURITY.md](SECURITY.md) for the trust model, the checks each
instruction makes and the known risks.
