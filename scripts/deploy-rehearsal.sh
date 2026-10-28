#!/usr/bin/env bash
# TEST C (founders 27 Oct): deploys the throwaway REHEARSAL program to mainnet. FOUNDERS run it (keys by path).
# Unlike deploy-mainnet.sh the deployer KEEPS control, so close-rehearsal.sh can close the program afterwards and get its
# SOL back. Fresh rehearsal build, checked from inside the file, typed confirmation; then the real mainnet build is put
# back in target/ so nothing else can pick up the rehearsal file.
#
#   bash -l scripts/deploy-rehearsal.sh <rehearsal-program-key.json> <deployer-wallet.json>
set -euo pipefail
cd "$(dirname "$0")/.."
U="usage: deploy-rehearsal.sh <rehearsal-program-key.json> <deployer-wallet.json>"
PROGRAM_KEY="${1:?$U}"; DEPLOYER="${2:?$U}"
URL="${RPC_URL:-https://api.mainnet-beta.solana.com}"
die() { echo "STOP: $*" >&2; exit 1; }

[ "$(solana genesis-hash -u "$URL")" = "5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d" ] || die "the RPC is not mainnet"
for f in "$PROGRAM_KEY" "$DEPLOYER"; do [ -f "$f" ] || die "file not found: $f"; done
[ "$(realpath "$DEPLOYER")" != "$(realpath "$HOME/.config/solana/id.json" 2>/dev/null || echo none)" ] || die "never the default Solana wallet"
case "$(realpath "$PROGRAM_KEY")$(realpath "$DEPLOYER")" in *"$(pwd)"/*) die "keep the keys outside this repo" ;; esac
PROGRAM_ID="$(solana-keygen pubkey "$PROGRAM_KEY")"
DECLARED="$(grep -oP 'declare_id!\("\K[1-9A-HJ-NP-Za-km-z]+(?="\);\s*// TEST C REHEARSAL)' programs/fair-launch/src/lib.rs)"
[ "$PROGRAM_ID" = "$DECLARED" ] || die "program key ($PROGRAM_ID) is not the rehearsal declare_id ($DECLARED): run set-rehearsal.sh"
[ "$PROGRAM_ID" != "BstpzKnft3hQNaae8TWL8KnUsff6wMZEtn5moEcD8FYR" ] || die "that is the REAL program address"
[ -z "$(git status --porcelain programs scripts)" ] || die "uncommitted changes: deploy only committed code"

echo "Unit test: the test launch #1 is derived from the test creator..."
cargo test -p fair-launch --lib --features mainnet-rehearsal squadz_launch 2>&1 | grep -q 'test result: ok. 1 passed' || die "squadz_launch unit test failed (rehearsal build)"
echo "Building the REHEARSAL program (--features mainnet-rehearsal)..."
anchor build -- --features mainnet-rehearsal >/dev/null
SO="$(mktemp -d)/fair_launch_rehearsal.so"; cp target/deploy/fair_launch.so "$SO"
echo "Putting the real mainnet build back in target/..."
anchor build >/dev/null
bash scripts/check-build.sh rehearsal "$SO"
SIZE=$(stat -c %s "$SO"); SHA=$(sha256sum "$SO" | cut -d' ' -f1)
RENT=$(solana rent "$SIZE" -u "$URL" --lamports | grep -oP '\d+'); NEED=$((RENT * 2 + 50000000))
BAL=$(solana balance "$(solana-keygen pubkey "$DEPLOYER")" -u "$URL" --lamports | grep -oP '^\d+')
printf '\nTEST C REHEARSAL (throwaway, closed after)\nProgram id: %s\nCommit:     %s\nBuild:      rehearsal (checked), %s bytes\nsha256:     %s\nDeployer:   %s SOL (need ~%s; ~half comes straight back, the rest when closed)\nControl:    stays with the deployer (so it can be closed)\n\n' \
  "$PROGRAM_ID" "$(git rev-parse --short HEAD)" "$SIZE" "$SHA" "$(awk -v l=$BAL 'BEGIN{printf "%.4f", l/1e9}')" "$(awk -v l=$NEED 'BEGIN{printf "%.4f", l/1e9}')"
[ "$BAL" -ge "$NEED" ] || die "not enough SOL in the deployer wallet"
read -r -p "Type 'deploy TEST C' to continue: " ok
[ "$ok" = "deploy TEST C" ] || die "cancelled"
[ "$(sha256sum "$SO" | cut -d' ' -f1)" = "$SHA" ] || die "the build file changed since it was checked"
trap 'echo "RECOVERY: get the SOL back from a half-written buffer: solana program close --buffers -u $URL -k $DEPLOYER" >&2' ERR
solana program deploy -u "$URL" -k "$DEPLOYER" --upgrade-authority "$DEPLOYER" --program-id "$PROGRAM_KEY" "$SO"
trap - ERR
solana program dump "$PROGRAM_ID" /tmp/flp-rehearsal-onchain.so -u "$URL" >/dev/null
[ "$(head -c "$SIZE" /tmp/flp-rehearsal-onchain.so | sha256sum | cut -d' ' -f1)" = "$SHA" ] || die "the program on mainnet does NOT match this build"
echo "on-chain rehearsal program = this build (sha256 matches). Close it after the test: scripts/close-rehearsal.sh"
