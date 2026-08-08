#!/usr/bin/env bash
# TEST-16 / GitLab #28: LocalTerra-gated smoke for oracle-service fail-closed paths.
#
# Always-on CI already covers DeliverTx-reject → no liveness via wiremock:
#   cargo test -p ust1-oracle-service deliver_tx_failure_does_not_allow_liveness
#
# This script is the optional LCD-present gate:
#   - If LocalTerra LCD is unreachable → exit 0 with SKIP (does not fail CI).
#   - If reachable → re-run the wiremock DeliverTx-reject + skip-path suite as a
#     smoke that the oracle-service binary still builds/tests against this checkout,
#     and print the ops checklist for a full wasm pause/DeliverTx probe.
#
# Ownership: PlasticDigits (ust1-window). Prefer promoting to required only after
# `scripts/deploy_local.py` can store/instantiate optimized wasm automatically.
#
# Cross-links: docs/DEPLOYMENT.md (TEST-16), skills/oracle-liveness-confirm,
# skills/oracle-circuit-breaker, skills/audit-hardening-bundle.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

LCD_URL="${TERRA_LCD_URL:-http://127.0.0.1:1317}"
RPC_URL="${TERRA_RPC_URL:-http://127.0.0.1:26657}"

lcd_up() {
  curl -sf --max-time 2 "${LCD_URL}/cosmos/base/tendermint/v1beta1/node_info" >/dev/null 2>&1 \
    || curl -sf --max-time 2 "${RPC_URL}/status" >/dev/null 2>&1
}

if ! lcd_up; then
  echo "SKIP (TEST-16): LocalTerra LCD/RPC not reachable at ${LCD_URL} / ${RPC_URL}."
  echo "  Start with: make start && make wait-healthy"
  echo "  Always-on equivalent: cargo test -p ust1-oracle-service deliver_tx_failure"
  exit 0
fi

echo "LocalTerra reachable (${LCD_URL}). Running oracle-service DeliverTx-reject / skip-path smokes…"
cargo test -p ust1-oracle-service -- \
  deliver_tx_failure_does_not_allow_liveness \
  run_once_equal_rate_does_not_record_liveness \
  run_once_policy_throttle_does_not_record_liveness \
  run_once_mono_decrease_does_not_record_liveness \
  read_exchange_rate_stored_times_out

cat <<'EOF'

TEST-16 LocalTerra ops checklist (manual wasm path; deploy_local.py is still a helper stub):
  1. make build-optimized
  2. Store/instantiate oracle + window (see docs/DEPLOYMENT.md Phase 4–5)
  3. Broadcast UpdateRate with a force-fail DeliverTx condition (bad policy / paused)
     or trip SetPaused=true and confirm window deposit/withdraw revert OraclePaused
  4. Confirm oracle-service logs do NOT record liveness success on DeliverTx fail
     (INV-ORACLE-LIVENESS-001; skills/oracle-liveness-confirm)
EOF
