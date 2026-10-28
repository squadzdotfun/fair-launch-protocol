#!/usr/bin/env bash
# TEST C (founders 27 Oct): closes the throwaway REHEARSAL program on mainnet after the test and sends its SOL (the
# program's storage deposit) back to the deployer. FOUNDERS run it. Refuses the real program address. Run the
# rehearsal tool's sell-all / sweep-members first: SOL left in the test launch's own accounts can't come out after.
#
#   bash -l scripts/close-rehearsal.sh <rehearsal-program-id> <deployer-wallet.json>
set -euo pipefail
cd "$(dirname "$0")/.."
U="usage: close-rehearsal.sh <rehearsal-program-id> <deployer-wallet.json>"
PROGRAM_ID="${1:?$U}"; DEPLOYER="${2:?$U}"
URL="${RPC_URL:-https://api.mainnet-beta.solana.com}"
die() { echo "STOP: $*" >&2; exit 1; }
[ "$(solana genesis-hash -u "$URL")" = "5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d" ] || die "the RPC is not mainnet"
[ "$PROGRAM_ID" != "BstpzKnft3hQNaae8TWL8KnUsff6wMZEtn5moEcD8FYR" ] || die "that is the REAL program: never close it"
DECLARED="$(grep -oP 'declare_id!\("\K[1-9A-HJ-NP-Za-km-z]+(?="\);\s*// TEST C REHEARSAL)' programs/fair-launch/src/lib.rs)"
[ "$PROGRAM_ID" = "$DECLARED" ] || die "$PROGRAM_ID is not the rehearsal program ($DECLARED)"
ME="$(solana-keygen pubkey "$DEPLOYER")"
AUTH="$(solana program show "$PROGRAM_ID" -u "$URL" | grep -oP 'Authority: \K\S+')"
[ "$AUTH" = "$ME" ] || die "the deployer ($ME) doesn't control $PROGRAM_ID (authority $AUTH)"
solana program show "$PROGRAM_ID" -u "$URL"
read -r -p "Type 'close TEST C' to close it for good: " ok
[ "$ok" = "close TEST C" ] || die "cancelled"
solana program close "$PROGRAM_ID" -u "$URL" -k "$DEPLOYER" --recipient "$ME" --bypass-warning
echo "closed; deployer now holds $(solana balance "$ME" -u "$URL")"
