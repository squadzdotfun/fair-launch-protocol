#!/usr/bin/env bash
# TEST C (founders 27 Oct): writes the three throwaway test keys' PUBLIC addresses into the mainnet-rehearsal lines
# (lib.rs declare_id, constants.rs SQUADZ_CREATOR / SQUADZ_DBC_CONFIG), derives the test launch #1 into constants.rs
# SQUADZ_LAUNCH and the check-build.sh table. Keys by path only: nothing but their public addresses is read.
#
#   bash -l scripts/set-rehearsal.sh <rehearsal-program-key.json> <rehearsal-settings-key.json> <rehearsal-creator-key.json>
set -euo pipefail
cd "$(dirname "$0")/.."
U="usage: set-rehearsal.sh <program-key.json> <settings-key.json> <creator-key.json>"
die() { echo "STOP: $*" >&2; exit 1; }
for f in "${1:?$U}" "${2:?$U}" "${3:?$U}"; do [ -f "$f" ] || die "file not found: $f"; done
P=$(solana-keygen pubkey "$1"); S=$(solana-keygen pubkey "$2"); C=$(solana-keygen pubkey "$3")
[ "$P" != "$S" ] && [ "$P" != "$C" ] && [ "$S" != "$C" ] || die "three different keys, please"
# never a real address (program, vault, multisig, devnet program, mainnet bot / gatekeeper / deployer)
for k in BstpzKnft3hQNaae8TWL8KnUsff6wMZEtn5moEcD8FYR 4pki8NpwGpGZf3SUF7XZ12u2xiZfhdfX4vd8KQNbMHRC 3AY6F8wNDrQP4FBmKHffrXu63hFkCFsYnDaSUv1TpDGz \
         7vGLH6t2HSWmLzk4E8GPF2EbnQPSwnA9n3sJ9GXTsaMw EeWNc7mQfuyTtE1vcoWpnfNVHKUeuwzZEj3heCAjAQVB 855ttR4seXn83HKfVgbzUwknKc3yyDmgfXMDGbyKVtid; do
  [ "$P" != "$k" ] && [ "$S" != "$k" ] && [ "$C" != "$k" ] || die "a real address can't be a test key ($k)"
done
L=$(solana find-program-derived-address "$P" string:launch pubkey:"$C" u64le:0)
[[ "$L" =~ ^[1-9A-HJ-NP-Za-km-z]{32,44}$ ]] || die "couldn't derive the test launch #1"

LIB=programs/fair-launch/src/lib.rs; K=programs/fair-launch/src/constants.rs
sed -i -E "s|declare_id!\(\"[1-9A-HJ-NP-Za-km-z]+\"\); // TEST C REHEARSAL|declare_id!(\"$P\"); // TEST C REHEARSAL|" "$LIB"
# the line right after each `#[cfg(feature = "mainnet-rehearsal")]`
sed -i -E "/^#\[cfg\(feature = \"mainnet-rehearsal\"\)\]$/{n;s|^(pub const SQUADZ_CREATOR: Pubkey = pubkey!\(\")[^\"]+|\1$C|;s|^(pub const SQUADZ_LAUNCH: Pubkey = pubkey!\(\")[^\"]+|\1$L|;s|^(pub const SQUADZ_DBC_CONFIG: Pubkey = pubkey!\(\")[^\"]+|\1$S|}" "$K"
sed -i -E "s|^(    \"rehearsal\": )\"[^\"]*\"|\1\"$L\"|" scripts/check-build.sh

grep -q "declare_id!(\"$P\"); // TEST C REHEARSAL" "$LIB" || die "lib.rs not updated"
for v in "$C" "$L" "$S"; do grep -q "pubkey!(\"$v\")" "$K" || die "constants.rs missing $v"; done
grep -q "\"rehearsal\": \"$L\"" scripts/check-build.sh || die "check-build.sh not updated"
printf 'TEST C rehearsal set:\n  program    %s\n  settings   %s\n  creator    %s\n  launch #1  %s (derived)\n' "$P" "$S" "$C" "$L"
echo "Next: cargo test -p fair-launch --lib --features mainnet-rehearsal squadz_launch  (must pass), then commit."
