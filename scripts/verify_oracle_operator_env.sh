#!/usr/bin/env bash
# Preflight: required environment variables for ust1-oracle-service.
# See docs/DEPLOYMENT.md § Oracle service environment.

set -euo pipefail

required_vars=(
  BSC_RPC_URLS
  VENUS_VTOKEN_ADDRESS
  TERRA_LCD_URL
  TERRA_CHAIN_ID
  ORACLE_CONTRACT
  TERRA_MNEMONIC
)

missing=0
for v in "${required_vars[@]}"; do
  if [[ -z "${!v:-}" ]]; then
    echo "verify_oracle_operator_env: missing required env var: $v" >&2
    missing=1
  fi
done

if [[ "$missing" -ne 0 ]]; then
  echo "verify_oracle_operator_env: fix the above and retry (docs/DEPLOYMENT.md)." >&2
  exit 1
fi

# BSC_RPC_URLS must list at least two providers (matches parse_comma_separated_rpc_urls in evm_rpc.rs).
rpc_count="$(echo "$BSC_RPC_URLS" | awk -F',' '
  { n=0; for (i = 1; i <= NF; i++) {
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", $i); if (length($i)) n++
    } print n
  }')"
if [[ "${rpc_count:-0}" -lt 2 ]]; then
  echo "verify_oracle_operator_env: BSC_RPC_URLS must contain at least two comma-separated URLs" >&2
  exit 1
fi

# Advisories for H-3 / #24 (INV-ORACLE-OPS-POLL-001 / INV-ORACLE-OPS-SILENCE-001).
# Defaults match oracle-service/src/config.rs; window default age is 21600.
# Matches ust1_common::DEFAULT_MAX_ORACLE_AGE_SECS — keep in sync if that constant changes.
WINDOW_MAX_ORACLE_AGE_SECS_DEFAULT=21600
poll="${POLL_INTERVAL_SECS:-3600}"
silence="${ORACLE_MAX_SILENCE_SECS:-21600}"

if [[ "$poll" =~ ^[0-9]+$ ]] && [[ "$silence" =~ ^[0-9]+$ ]]; then
  if [[ "$poll" -eq 0 ]]; then
    echo "verify_oracle_operator_env: advisory: POLL_INTERVAL_SECS=0 tight-loops LCD/RPC (prefer 3600)" >&2
  elif [[ "$poll" -ge "$WINDOW_MAX_ORACLE_AGE_SECS_DEFAULT" ]]; then
    echo "verify_oracle_operator_env: advisory: POLL_INTERVAL_SECS ($poll) >= window max oracle age default ($WINDOW_MAX_ORACLE_AGE_SECS_DEFAULT); one missed tick can halt swaps (INV-ORACLE-OPS-POLL-001)" >&2
  fi
  if [[ "$silence" -gt "$WINDOW_MAX_ORACLE_AGE_SECS_DEFAULT" ]]; then
    echo "verify_oracle_operator_env: advisory: ORACLE_MAX_SILENCE_SECS ($silence) > window max oracle age default ($WINDOW_MAX_ORACLE_AGE_SECS_DEFAULT); alert may fire after user impact (INV-ORACLE-OPS-SILENCE-001)" >&2
  fi
  grace=$((WINDOW_MAX_ORACLE_AGE_SECS_DEFAULT + poll))
  if [[ "$silence" -gt "$grace" ]]; then
    echo "verify_oracle_operator_env: advisory: ORACLE_MAX_SILENCE_SECS ($silence) > max_age+poll ($grace); late paging footgun" >&2
  fi
else
  echo "verify_oracle_operator_env: advisory: POLL_INTERVAL_SECS / ORACLE_MAX_SILENCE_SECS should be positive integers when set" >&2
fi

# Optional hardening knobs (issue #25) — not required; printed when set for operator visibility.
optional_vars=(
  BSC_RPC_TIMEOUT_SECS
  TICK_TIMEOUT_SECS
  TERRA_GAS_PRICE
  TERRA_GAS_PRICE_ULUNA
  HEALTHZ_BIND
)
for v in "${optional_vars[@]}"; do
  if [[ -n "${!v:-}" ]]; then
    echo "verify_oracle_operator_env: optional $v=${!v}"
  fi
done

echo "verify_oracle_operator_env: required oracle operator variables are set (count BSC_RPC_URLS ok)."
