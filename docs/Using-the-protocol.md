# Using the Fair Launch Protocol without squadz.fun

> The command-line tool (`flp`) shown below is not public yet; it is planned. Until then the same steps can be taken
> by calling the program's instructions directly (`anchor build` writes its interface file to `target/idl/`).

The Fair Launch Protocol is an open Solana program. squadz.fun is one website built on it; nothing in the program
needs that website. Anyone can create a lobby, join, launch, claim and run the upkeep from a terminal, or build their
own site and bots.

## What you need
- A Solana wallet file (a keypair JSON) with some SOL, and an RPC address (any provider).
- The `flp` tool (planned, see above). Every command takes `--url <rpc>` and `--keypair <file>`;
  `flp <command> --help` lists the options.
- Program address: `BstpzKnft3hQNaae8TWL8KnUsff6wMZEtn5moEcD8FYR` (mainnet);
  `7vGLH6t2HSWmLzk4E8GPF2EbnQPSwnA9n3sJ9GXTsaMw` (devnet, for testing).

## Fees (fixed in the program)
| What | Amount | Where it goes |
|---|---|---|
| Create a lobby | 0.05 SOL (24 h window), 0.10 SOL (72 h), 0.20 SOL (1 week) | the lobby's upkeep budget |
| Approval-only | +0.05 SOL | upkeep budget |
| Up to 2 slots per wallet | +0.10 SOL | upkeep budget |
| Project Treasury (optional) | per % of supply, burned via $SQUADZ at launch | burn jar |
| Launch fee | 5% of the raise, only if it launches | burn jar |
| Creator fees | 10% of every coin's creator fees, forever | burn jar |

None of it is refundable. Upkeep pays whoever runs a lobby's routine transactions (refunds, payout rounds, fee
collection); what it doesn't use goes to the burn jar when the lobby's budget closes. The burn jar buys and burns
$SQUADZ. None of these protocol fees goes to a team wallet.

## A lobby from start to finish
```bash
# create: 30 slots, 72 h window, keep 50% of creator fees, coin address ending in "fair"
flp create --name "Moss Squad" --symbol MOSS --uri https://example.com/meta.json --slots 30 --period 1 \
  --creator-fee-bps 5000 --ending fair
#   options only possible at creation: --gatekeeper <key>, --approval, --two-slots,
#   --treasury-pct <1-10> --treasury-wallet <addr>, --fee-wallet, --donation-wallet

flp status --launch <lobby>            # slots, price, budget, coin, drops, fees
flp join --launch <lobby> [--donation 0.1]
flp donate --launch <lobby> --amount 0.2   # members: add to your donation any time before launch
flp leave --launch <lobby>             # before launch; 0.01 SOL kept if the creator holds a seat
flp launch --launch <lobby>            # creator, once every slot is filled (finds the coin number for an ending)
flp claim --launch <lobby>             # members: your equal share + unused SOL
flp claim-hodl --launch <lobby>        # members: HODL drops you qualified for
flp cancel --launch <lobby>            # creator, before launch
flp refund --launch <lobby>            # anyone, once cancelled or out of time: everyone gets slot + donation back
```

## Gatekeepers and join passes
A lobby can name a **gatekeeper** key at creation. Then every join needs a pass signed by that key for that lobby,
that wallet and an expiry time (`flp sign-pass --launch <lobby> --wallet <addr>` prints one; `flp join ...
--pass-signature ... --pass-expiry ...` uses it). squadz.fun's gatekeeper only signs for people signed in with X; your
own site can use any rule, or no gatekeeper at all (anyone can join). **Approval-only** (`--approval`) is a promise
that the gatekeeper only signs passes for people the creator approved; it needs a gatekeeper.

## Upkeep anyone can run
These are permissionless and tipped from the lobby's budget, so any bot can run them:
```bash
flp round --launch <lobby>          # hourly: snapshot holdings, release due HODL drops, split the Fee Share pot
flp payout --launch <lobby>         # send members the Fee Share SOL they are owed
flp collect --launch <lobby>        # pull pump.fun / PumpSwap creator fees into the lobby
flp sweep --launch <lobby>          # protocol share to the burn jar
flp spot-check --launch <lobby> --slots 0,3   # record a member's lower holding at once
flp repay-shield --launch <lobby>   # after launch: pay back the graduation shield deposits
flp burn-hodl --launch <lobby>      # after the 2 weeks: burn drops nobody qualified for
flp sweep-protocol --launch <lobby>; flp close-ops --launch <lobby>
flp burn                            # the burn jar buys $SQUADZ and burns it (2% price limit)
```

## Rules worth knowing
- Every member pays the same slot price and gets the same share; nobody buys before the launch transaction.
- One transaction creates the coin on pump.fun and buys the whole curve; if any check fails, nothing happens.
- HODL drops at 24 h, 48 h, 72 h, 1 week and 2 weeks go to members who never held less than 75% of their claim.
- Fee Share follows each member's lowest holding ever; selling HODL drops doesn't lower it.
- Times are the chain's clock. With Alpenglow (Solana's new consensus) that clock is set by the block producer and can
  be a few seconds off; every rule is in hours or days, so this never matters in practice.
- The upgrade key is held by the team's 2-of-4 multisig for the first week after the mainnet deployment; we intend to
  remove it then and will announce it. After that nobody can change the program.
