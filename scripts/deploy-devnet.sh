#!/usr/bin/env bash
# First deploy (or upgrade) of the Fair Launch Protocol to DEVNET ONLY.
# Run by a founder from WSL, with keys that stay on their own machine:
#
#   bash -l scripts/deploy-devnet.sh <program-id-keypair.json> <deployer-wallet.json>
#
#   program-id-keypair.json  the key that owns the program address (first deploy only)
#   deployer-wallet.json     a devnet wallet that pays and becomes the upgrade authority
#
# Optional: RPC_URL=<devnet RPC, e.g. Helius devnet>  MAX_LEN=<bytes> (room for bigger upgrades)
#           FEATURES=<build features>, default devnet-squadz: devnet's launch #1 creator is the devnet
#           Squads vault. A build without it points launch #1 at the MAINNET vault and devnet's
#           buy-and-burn stops working (happened 14 Oct). FEATURES= (empty) builds the plain mainnet build.
set -euo pipefail
cd "$(dirname "$0")/.."

PROGRAM_KEY="${1:?usage: deploy-devnet.sh <program-id-keypair.json> <deployer-wallet.json>}"
DEPLOYER="${2:?usage: deploy-devnet.sh <program-id-keypair.json> <deployer-wallet.json>}"
URL="${RPC_URL:-https://api.devnet.solana.com}"

die() { echo "STOP: $*" >&2; exit 1; }
sol() { awk -v l="$1" 'BEGIN { printf "%.4f", l / 1e9 }'; }

# ---- safety checks --------------------------------------------------------------
case "$URL" in *devnet*) ;; *) die "RPC_URL must be a devnet endpoint (got: $URL). This script never touches mainnet." ;; esac
[ "$(solana genesis-hash -u "$URL")" = "EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG" ] || die "the RPC is not devnet (genesis hash mismatch)"
for f in "$PROGRAM_KEY" "$DEPLOYER"; do [ -f "$f" ] || die "file not found: $f"; done
[ "$(realpath "$PROGRAM_KEY")" != "$(realpath "$DEPLOYER")" ] || die "the program key and the deployer wallet must be different files"
[ "$(realpath "$DEPLOYER")" != "$(realpath "$HOME/.config/solana/id.json" 2>/dev/null || echo none)" ] || die "do not use the default Solana wallet (~/.config/solana/id.json); use a dedicated devnet deployer"
case "$(realpath "$PROGRAM_KEY")" in "$(pwd)"/*) die "keep the program key outside this repo" ;; esac
case "$(realpath "$DEPLOYER")" in "$(pwd)"/*) die "keep the deployer wallet outside this repo" ;; esac

PROGRAM_ID="$(solana-keygen pubkey "$PROGRAM_KEY")"
DEPLOYER_PK="$(solana-keygen pubkey "$DEPLOYER")"

# ---- fresh REAL build (never the short-time local-test build) ---------------------
FEATURES="${FEATURES-devnet-squadz}"
case " $FEATURES " in *short-time*) die "never deploy the short-time (local test) build" ;; esac
# B5 (25 Oct): lib.rs has one declare_id per build type; compare with the one this build uses
if [ -n "$FEATURES" ]; then MARK=DEVNET; else MARK=MAINNET; fi
DECLARED="$(grep -oP "declare_id!\(\"\K[1-9A-HJ-NP-Za-km-z]+(?=\"\);\s*// $MARK)" programs/fair-launch/src/lib.rs)"
[ -n "$DECLARED" ] || die "no declare_id marked $MARK in lib.rs"
[ "$PROGRAM_ID" = "$DECLARED" ] || die "program key ($PROGRAM_ID) does not match the $MARK declare_id in lib.rs ($DECLARED)"
echo "Building the real program (features: ${FEATURES:-none})..."
if [ -n "$FEATURES" ]; then anchor build -- --features "$FEATURES" >/dev/null; else anchor build >/dev/null; fi
SO=target/deploy/fair_launch.so
# audit run 5 (M2): the file itself must be the build asked for (never the local short-time build)
if [ "$FEATURES" = "devnet-squadz" ]; then bash scripts/check-build.sh devnet "$SO"
elif [ -z "$FEATURES" ]; then bash scripts/check-build.sh mainnet "$SO"
else die "unknown FEATURES: $FEATURES"; fi
SIZE=$(stat -c %s "$SO")
LEN="${MAX_LEN:-$SIZE}"
[ "$LEN" -ge "$SIZE" ] || die "MAX_LEN ($LEN) is smaller than the program ($SIZE bytes)"

# ---- cost check -----------------------------------------------------------------
RENT_LAMPORTS=$(solana rent "$LEN" -u "$URL" --lamports | grep -oP '\d+')
NEED=$((RENT_LAMPORTS * 2 + 50000000))   # program account + temporary write buffer (returned) + fees margin
BAL=$(solana balance "$DEPLOYER_PK" -u "$URL" --lamports | grep -oP '^\d+')
printf 'Features:        %s\nProgram id:      %s\nDeployer:        %s (becomes the upgrade authority)\nProgram size:    %s bytes (room: %s bytes)\n' "${FEATURES:-none}" "$PROGRAM_ID" "$DEPLOYER_PK" "$SIZE" "$LEN"
printf 'Locked in program: %s SOL; temporary buffer (returned): %s SOL\n' "$(sol "$RENT_LAMPORTS")" "$(sol "$RENT_LAMPORTS")"
printf 'Deployer balance:  %s SOL (need about %s SOL)\n' "$(sol "$BAL")" "$(sol "$NEED")"
[ "$BAL" -ge "$NEED" ] || die "not enough devnet SOL in the deployer wallet"

read -r -p "Deploy to DEVNET now? Type 'deploy devnet' to continue: " ok
[ "$ok" = "deploy devnet" ] || die "cancelled"

# ---- deploy ---------------------------------------------------------------------
ARGS=(-u "$URL" -k "$DEPLOYER" --upgrade-authority "$DEPLOYER" --program-id "$PROGRAM_KEY")
[ -n "${MAX_LEN:-}" ] && ARGS+=(--max-len "$MAX_LEN")
solana program deploy "${ARGS[@]}" "$SO"

echo
solana program show "$PROGRAM_ID" -u "$URL"
echo
echo "Done. Explorer: https://explorer.solana.com/address/$PROGRAM_ID?cluster=devnet"
echo "If a later build is bigger than the room, run: solana program extend $PROGRAM_ID <extra bytes> -u $URL -k $DEPLOYER"
