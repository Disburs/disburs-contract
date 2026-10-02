#!/bin/bash
# Deploy a fresh factory (and the payroll code it deploys from) to a network.
#
#   ./scripts/deploy-factory.sh <identity> [network]
#
# <identity> is a `stellar keys` identity on this machine; it signs, pays the
# fees, and becomes the factory's owner (the key the backend uses as
# WALLET_SPONSOR_SECRET_KEY). Prints the two values the backend needs.
set -euo pipefail

IDENTITY=${1:?usage: $0 <identity> [network]}
NETWORK=${2:-testnet}
cd "$(dirname "$0")/.."

echo "→ building the contracts"
stellar contract build >/dev/null 2>&1
PAYROLL=target/wasm32v1-none/release/disburs_payroll.wasm
FACTORY_WASM=target/wasm32v1-none/release/disburs_factory.wasm
# Newer CLIs also write an .optimized.wasm; use it when present.
[ -f "${PAYROLL%.wasm}.optimized.wasm" ] && PAYROLL="${PAYROLL%.wasm}.optimized.wasm"
[ -f "${FACTORY_WASM%.wasm}.optimized.wasm" ] && FACTORY_WASM="${FACTORY_WASM%.wasm}.optimized.wasm"

# The hash the network records for a wasm is its sha256, so compute it here
# rather than parsing the CLI's output, which differs between versions.
WASM_HASH=$(shasum -a 256 "$PAYROLL" | cut -c1-64)
echo "→ uploading the payroll code ($WASM_HASH)"
stellar contract upload --source "$IDENTITY" --network "$NETWORK" --wasm "$PAYROLL" 2> >(grep -v "new release" >&2) >/dev/null

echo "→ deploying the factory"
OWNER=$(stellar keys public-key "$IDENTITY" 2>/dev/null)
FACTORY=$(stellar contract deploy --source "$IDENTITY" --network "$NETWORK" --wasm "$FACTORY_WASM" \
  -- --owner "$OWNER" --payroll_wasm_hash "$WASM_HASH" 2> >(grep -v "new release" >&2) | tail -1)
[ -n "$FACTORY" ] || { echo "deploy failed (see the error above)"; exit 1; }

echo "→ checking"
STORED=$(stellar contract invoke --id "$FACTORY" --source "$IDENTITY" --network "$NETWORK" -- wasm_hash 2> >(grep -v "new release" >&2) | tail -1 | tr -d '"')
[ "$STORED" = "$WASM_HASH" ] || { echo "the factory stores $STORED, expected $WASM_HASH"; exit 1; }

cat <<OUT

Done on $NETWORK. Factory owned by $OWNER, deploying payroll $WASM_HASH.

  FACTORY_CONTRACT_ID=$FACTORY
  payroll wasm hash:  $WASM_HASH
OUT
