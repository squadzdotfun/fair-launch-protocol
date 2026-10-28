#!/usr/bin/env bash
# Starts a local Solana validator with pump.fun, PumpSwap, pump fees and
# mayhem programs, Meteora's bonding curve (DBC) and DAMM v2 pool programs
# ($SQUADZ), + their config accounts copied from DEVNET (read-only), and
# this program preloaded from target/deploy (build it first with
# scripts/build-test.sh). Nothing is sent to devnet or mainnet.
# Run from WSL:  bash -l scripts/local-validator.sh
set -euo pipefail
cd "$(dirname "$0")/.."

LEDGER="${LEDGER:-$HOME/.cache/flp-ledger}"
LIST=scripts/clone-list.txt
PROGRAM_ID=7vGLH6t2HSWmLzk4E8GPF2EbnQPSwnA9n3sJ9GXTsaMw
SO=target/deploy/fair_launch.so
DEV=.keys/dev.json

[ -f "$SO" ] || { echo "build first: bash -l scripts/build-test.sh"; exit 1; }
if [ ! -f "$DEV" ]; then
  # Local test wallet: generated per machine, never committed, never funded on a real network.
  mkdir -p .keys
  solana-keygen new --no-bip39-passphrase --silent -o "$DEV"
fi

PROGRAMS=(
  6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P   # pump
  pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA   # pump_amm (PumpSwap)
  pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ   # pump_fees
  MAyhSmzXzV1pTf7LsNkrNwkWKTo4ougAJ1PPg47MD4e   # mayhem
  dbcij3LWUppWqq96dh6gJWwBifmcGfLSB5D4DuSMaqN   # Meteora Dynamic Bonding Curve
  cpamdpZCGKUy5JxQXB4dcpGPiikHawvSWAd6mEn1sGG   # Meteora DAMM v2
)

if [ ! -s "$LIST" ] || [ "${REFRESH:-0}" = 1 ]; then
  echo "Refreshing clone list from devnet (read-only)..."
  (cd tests && cargo run --quiet --bin clone-list) > "$LIST"
fi

ARGS=(--reset --ledger "$LEDGER" --url devnet --quiet)
for p in "${PROGRAMS[@]}"; do ARGS+=(--clone-upgradeable-program "$p"); done
while read -r acct; do [ -n "$acct" ] && ARGS+=(--maybe-clone "$acct"); done < "$LIST"
# This program, upgradeable by the local test wallet (no deploy key needed).
ARGS+=(--upgradeable-program "$PROGRAM_ID" "$SO" "$(solana-keygen pubkey "$DEV")")

echo "Starting local validator: ${#PROGRAMS[@]} pump + Meteora programs, $(wc -l < "$LIST") accounts, fair_launch preloaded. Ledger: $LEDGER"
exec solana-test-validator "${ARGS[@]}"
