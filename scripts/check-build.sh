#!/usr/bin/env bash
# Checks WHICH build a fair_launch.so is, from inside the file (audit run 5, M2): every build carries exactly one
# launch #1 address (constants.rs SQUADZ_LAUNCH), so the bytes tell the builds apart. If SQUADZ_LAUNCH ever changes
# (new mainnet program id or vault), update the table below too (the mainnet deploy then refuses until it matches).
#
#   bash scripts/check-build.sh mainnet|devnet|local [path/to/fair_launch.so]
#
# Exit 0 only if the file holds the expected address and none of the other two.
set -euo pipefail
cd "$(dirname "$0")/.."
WANT="${1:?usage: check-build.sh mainnet|devnet|local [file.so]}"
SO="${2:-target/deploy/fair_launch.so}"
[ -f "$SO" ] || { echo "STOP: $SO not found"; exit 1; }
python3 - "$WANT" "$SO" <<'PY'
import sys
A = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
def b58(s):
    n = 0
    for c in s: n = n * 58 + A.index(c)
    return n.to_bytes(32, "big")
flavors = {  # constants.rs SQUADZ_LAUNCH per build (launch #1's address; the code stores this one, not the creator)
    "mainnet": "AGKpwt6KFYAjDf5iqMsWRHqDYHfk2RVXZkJyX9xNBVZ3",  # team Squads vault's launch #1 (no features; vault 4pki…, 27 Oct)
    "devnet":  "WFwhb8BDMzzkbSYUgXTzcnceJWBxMbAUypMpqJvRzws",  # devnet-squadz (nonce 1 since 22 Oct: $SQUADZ on Meteora)
    "local":   "EsfgcnFwc7hzqj8XMmP91t17MCNrsdRAgCmu41PmfQLi",  # short-time
    "rehearsal": "9k89m9bMzMiFw8TosfFS6ovrMZEkoy3WD2B398Cwohn9",  # mainnet-rehearsal (test C, 27 Oct): its test launch #1, written by scripts/set-rehearsal.sh
}
want, path = sys.argv[1], sys.argv[2]
if want not in flavors: sys.exit(f"STOP: unknown build '{want}'")
if not flavors[want]: sys.exit(f"STOP: the {want} build's address isn't set yet (scripts/set-rehearsal.sh)")
flavors = {k: v for k, v in flavors.items() if v}  # an unset entry is never "found"
data = open(path, "rb").read()
# the compiler may store a 32-byte constant whole (read-only data) or split into 4-byte immediates inside
# instructions (lddw): a build "has" an address when all eight 4-byte pieces of it are in the file
def has(key):
    k = b58(key)
    return k in data or all(k[i:i + 4] in data for i in range(0, 32, 4))
found = [k for k, v in flavors.items() if has(v)]
if found != [want]:
    sys.exit(f"STOP: {path} is the {'+'.join(found) or 'UNKNOWN'} build, not {want}. Do not deploy it.")
# fix #5 (22 Oct): repay_shield pays part of the shield rent to constants.rs KEEPER, the upkeep bot. Until the
# mainnet bot key exists, the mainnet line holds the devnet bot: a mainnet build with it must never be deployed (B3).
if want == "mainnet" and has("3fEMMpeYZNWjcwJhhA9sugVpS4RCnvsep6rgHhNFPjqu"):
    sys.exit("STOP: constants.rs KEEPER is still the devnet bot. Set it to the mainnet bot's address (B3), rebuild.")
# $SQUADZ on Meteora (22 Oct): constants.rs SQUADZ_DBC_CONFIG is $SQUADZ's Meteora settings account. Until the team
# creates it on a network, that network's build holds the local-tests key: such a build must never be deployed.
if want != "local" and has("AAbg41hHpnptEohXspyUc6aJpuaERmb5cc6C42iqdVxZ"):
    sys.exit(f"STOP: constants.rs SQUADZ_DBC_CONFIG is still the local-tests key. Create $SQUADZ's Meteora settings on {want}, set its address, rebuild.")
print(f"ok: {path} is the {want} build")
PY
