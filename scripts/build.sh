#!/usr/bin/env bash
# Builds the on-chain program (real time rules). Run from WSL: bash -l scripts/build.sh
set -euo pipefail
cd "$(dirname "$0")/.."
anchor build "$@"
ls -la target/deploy/fair_launch.so
