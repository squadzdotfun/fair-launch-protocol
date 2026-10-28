#!/usr/bin/env bash
# Runs the integration tests against the local validator (start it first with
# scripts/local-validator.sh). Usage: bash -l scripts/test.sh [test-name]
set -euo pipefail
cd "$(dirname "$0")/../tests"
if [ $# -gt 0 ]; then
  cargo test --quiet --test "$1" -- --nocapture --test-threads=1
else
  cargo test --quiet -- --nocapture --test-threads=1
fi
