#!/usr/bin/env bash
# Upgrades the program on the LOCAL validator only (it was preloaded by
# scripts/local-validator.sh with the local test wallet as upgrade authority).
# Run from WSL.
set -euo pipefail
cd "$(dirname "$0")/.."
URL=http://127.0.0.1:8899
DEV=.keys/dev.json
PROGRAM_ID=7vGLH6t2HSWmLzk4E8GPF2EbnQPSwnA9n3sJ9GXTsaMw
if [ "$(solana balance -u $URL -k $DEV --lamports | cut -d' ' -f1)" -lt 10000000000 ]; then
  solana airdrop -u $URL -k $DEV 100 >/dev/null
fi
solana program deploy -u $URL -k $DEV --program-id $PROGRAM_ID --upgrade-authority $DEV target/deploy/fair_launch.so
