#!/usr/bin/env bash
# MAINNET first deploy of the Fair Launch Protocol. FOUNDERS ONLY, on the deploy machine, keys by path. After it, the
# team vault holds control, so later upgrades are a vault proposal (Squads, with a buffer), not this script.
# Claude never runs this. Audit run 5 (M2): always a fresh mainnet build, checked from inside the file, then a
# typed confirmation. No test build can be deployed by mistake.
#
#   bash -l scripts/deploy-mainnet.sh <program-id-keypair.json> <deployer-wallet.json> <team-vault-address>
#
# RPC_URL=<mainnet RPC> (default api.mainnet-beta.solana.com). MAX_LEN=<bytes> for room to grow.
# <team-vault-address>: the founders' Squads vault (decision 2.1). Audit run 6 (23 Oct): right after the deploy is
# checked, the script itself hands the upgrade authority to that vault and reads it back, so the single deployer
# wallet never keeps control. Locking it (immutable) later is a vault proposal, after week 1.
set -euo pipefail
cd "$(dirname "$0")/.."

USAGE="usage: deploy-mainnet.sh <program-id-keypair.json> <deployer-wallet.json> <team-vault-address>"
PROGRAM_KEY="${1:?$USAGE}"
DEPLOYER="${2:?$USAGE}"
VAULT="${3:?$USAGE}"
URL="${RPC_URL:-https://api.mainnet-beta.solana.com}"
MAINNET_GENESIS=5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d
die() { echo "STOP: $*" >&2; exit 1; }

# ---- the right network, the right keys -------------------------------------------
[ "$(solana genesis-hash -u "$URL")" = "$MAINNET_GENESIS" ] || die "the RPC is not mainnet (genesis hash mismatch)"
for f in "$PROGRAM_KEY" "$DEPLOYER"; do [ -f "$f" ] || die "file not found: $f"; done
[ "$(realpath "$PROGRAM_KEY")" != "$(realpath "$DEPLOYER")" ] || die "program key and deployer must be different files"
[ "$(realpath "$DEPLOYER")" != "$(realpath "$HOME/.config/solana/id.json" 2>/dev/null || echo none)" ] || die "never the default Solana wallet"
case "$(realpath "$PROGRAM_KEY")" in "$(pwd)"/*) die "keep the program key outside this repo" ;; esac
case "$(realpath "$DEPLOYER")" in "$(pwd)"/*) die "keep the deployer wallet outside this repo" ;; esac
PROGRAM_ID="$(solana-keygen pubkey "$PROGRAM_KEY")"
DECLARED="$(grep -oP 'declare_id!\("\K[1-9A-HJ-NP-Za-km-z]+(?="\);\s*// MAINNET)' programs/fair-launch/src/lib.rs)"
[ -n "$DECLARED" ] || die "no declare_id marked MAINNET in lib.rs"
[ "$PROGRAM_ID" = "$DECLARED" ] || die "program key ($PROGRAM_ID) does not match the MAINNET declare_id in lib.rs ($DECLARED)"
[ "$PROGRAM_ID" != "7vGLH6t2HSWmLzk4E8GPF2EbnQPSwnA9n3sJ9GXTsaMw" ] || die "that is the DEVNET program address; mainnet uses a fresh key (decision 2.2)"
[ -z "$(git status --porcelain programs)" ] || die "uncommitted changes in programs/: deploy only committed code"
[[ "$VAULT" =~ ^[1-9A-HJ-NP-Za-km-z]{32,44}$ ]] || die "team vault address is not a Solana address: $VAULT"
[ "$VAULT" != "$(solana-keygen pubkey "$DEPLOYER")" ] || die "the team vault must not be the deployer wallet"
[ "$VAULT" != "$PROGRAM_ID" ] || die "the team vault must not be the program address"
solana account "$VAULT" -u "$URL" >/dev/null 2>&1 || die "team vault $VAULT does not exist on mainnet (fund it / check the address)"
# A6 (25 Oct): launch #1 can only be created by the wallet written into the contract; it must be this vault
CREATOR="$(grep -oP 'pub const SQUADZ_CREATOR: Pubkey = pubkey!\("\K[1-9A-HJ-NP-Za-km-z]+' programs/fair-launch/src/constants.rs | head -1)"
[ "$CREATOR" = "$VAULT" ] || die "constants.rs SQUADZ_CREATOR ($CREATOR) is not the team vault ($VAULT): set it, recompute SQUADZ_LAUNCH, commit, then run again"

# ---- fresh MAINNET build (no features), checked from inside the file -------------
echo "Unit test: launch #1 address is derived from SQUADZ_CREATOR (= the team vault, checked above)..."
cargo test -p fair-launch --lib squadz_launch 2>&1 | grep -q 'test result: ok. 1 passed' || die "squadz_launch unit test failed"
echo "Building the mainnet program (no features)..."
anchor build >/dev/null
# audit run 5b: check, hash and deploy one private copy, so nothing (e.g. a test build) can replace the file in between
SO="$(mktemp -d)/fair_launch.so"
cp target/deploy/fair_launch.so "$SO"
bash scripts/check-build.sh mainnet "$SO"
SIZE=$(stat -c %s "$SO"); LEN="${MAX_LEN:-$SIZE}"
[ "$LEN" -ge "$SIZE" ] || die "MAX_LEN ($LEN) is smaller than the program ($SIZE bytes)"
SHA=$(sha256sum "$SO" | cut -d' ' -f1)

# ---- what it costs, then a typed yes ---------------------------------------------
RENT=$(solana rent "$LEN" -u "$URL" --lamports | grep -oP '\d+')
NEED=$((RENT * 2 + 50000000))
BAL=$(solana balance "$(solana-keygen pubkey "$DEPLOYER")" -u "$URL" --lamports | grep -oP '^\d+')
printf '\nMAINNET\nProgram id: %s\nCommit:     %s\nBuild:      mainnet (checked), %s bytes, room %s\nsha256:     %s\nDeployer:   %s SOL (need ~%s)\nControl to: %s (team vault, set right after the deploy)\n\n' \
  "$PROGRAM_ID" "$(git rev-parse --short HEAD)" "$SIZE" "$LEN" "$SHA" "$(awk -v l=$BAL 'BEGIN{printf "%.4f", l/1e9}')" "$(awk -v l=$NEED 'BEGIN{printf "%.4f", l/1e9}')" "$VAULT"
[ "$BAL" -ge "$NEED" ] || die "not enough SOL in the deployer wallet"
read -r -p "Both founders agree. Type 'deploy MAINNET' to continue: " ok
[ "$ok" = "deploy MAINNET" ] || die "cancelled"
[ "$(sha256sum "$SO" | cut -d' ' -f1)" = "$SHA" ] || die "the build file changed since it was checked"

ARGS=(-u "$URL" -k "$DEPLOYER" --upgrade-authority "$DEPLOYER" --program-id "$PROGRAM_KEY")
[ -n "${MAX_LEN:-}" ] && ARGS+=(--max-len "$MAX_LEN")
# A5 (25 Oct): if anything stops the script from here on, say exactly what to do. The deployer wallet must never keep
# control of a live program, and a deploy that stopped half-way leaves a buffer that holds SOL.
STAGE=deploy
recover() {
  case "$STAGE" in
    deploy) echo "RECOVERY: the deploy did not finish. Get the SOL back from any half-written buffer:" >&2
            echo "  solana program close --buffers -u $URL -k $DEPLOYER" >&2 ;;
    handoff) echo "RECOVERY: the program is LIVE and the deployer wallet still controls it. Run this NOW:" >&2
             echo "  solana program set-upgrade-authority $PROGRAM_ID -u $URL -k $DEPLOYER --upgrade-authority $DEPLOYER --new-upgrade-authority $VAULT --skip-new-upgrade-authority-signer-check" >&2
             echo "  solana program show $PROGRAM_ID -u $URL   (Authority must be $VAULT)" >&2 ;;
    verify) echo "RECOVERY: control is with the team vault. The on-chain check did not finish: run it by hand:" >&2
            echo "  solana program dump $PROGRAM_ID /tmp/flp-mainnet-onchain.so -u $URL && head -c $SIZE /tmp/flp-mainnet-onchain.so | sha256sum   (must be $SHA)" >&2 ;;
  esac
}
trap 'recover' ERR
solana program deploy "${ARGS[@]}" "$SO"

# ---- hand control to the team vault FIRST, then read it back (audit run 6, A5) ----
STAGE=handoff
solana program set-upgrade-authority "$PROGRAM_ID" -u "$URL" -k "$DEPLOYER" --upgrade-authority "$DEPLOYER" \
  --new-upgrade-authority "$VAULT" --skip-new-upgrade-authority-signer-check
NOW_AUTH="$(solana program show "$PROGRAM_ID" -u "$URL" | grep -oP 'Authority: \K\S+')"
[ "$NOW_AUTH" = "$VAULT" ] || { recover; die "upgrade authority is $NOW_AUTH, not the team vault $VAULT"; }
echo "upgrade authority = team vault $VAULT (checked on chain); the deployer wallet has no control any more"

# ---- the program on mainnet is exactly this build -------------------------------
STAGE=verify
solana program dump "$PROGRAM_ID" /tmp/flp-mainnet-onchain.so -u "$URL" >/dev/null
[ "$(head -c "$SIZE" /tmp/flp-mainnet-onchain.so | sha256sum | cut -d' ' -f1)" = "$SHA" ] || die "the program on mainnet does NOT match this build. Control is with the team vault: fix it with a vault upgrade proposal before anything else"
echo "on-chain program = this build (sha256 matches)"
trap - ERR
echo "Next: record program id, commit, sha256 and the authority in the workspace. Lock (immutable) after week 1 via a vault proposal."
