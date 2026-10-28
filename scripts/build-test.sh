#!/usr/bin/env bash
# LOCAL TESTING ONLY: builds with hours/days shrunk to seconds. If the local
# validator is running, the new build is deployed to it; otherwise start the
# validator next (it preloads this build). Never deploy this build anywhere else.
set -euo pipefail
cd "$(dirname "$0")/.."
anchor build -- --features short-time
if solana cluster-version -u http://127.0.0.1:8899 >/dev/null 2>&1; then
  bash -l scripts/deploy-local.sh
fi
