#!/usr/bin/env bash
# Verify / refresh InstantWithdrawCw20 golden JSON against pinned ustr-cmm treasury.
#
# Usage:
#   scripts/verify_treasury_wire_schema.sh           # fetch pin + compare (CI)
#   scripts/verify_treasury_wire_schema.sh --regen   # rewrite golden from pin
#
# Pin source of truth: ust1_window::treasury::USTR_CMM_TREASURY_SCHEMA_REV
# Cross-links: skills/window-instant-withdraw-cw20/SKILL.md, docs/DEPLOYMENT.md, issue #21

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

PIN_RS="$(sed -n 's/^pub const USTR_CMM_TREASURY_SCHEMA_REV: \&str = "\([a-f0-9]*\)";/\1/p' contracts/ust1-window/src/treasury.rs)"
if [[ -z "$PIN_RS" ]]; then
  echo "ERROR: could not parse USTR_CMM_TREASURY_SCHEMA_REV from treasury.rs" >&2
  exit 1
fi
REPO="https://gitlab.com/PlasticDigits2/ustr-cmm.git"
GOLDEN="contracts/ust1-window/testdata/instant_withdraw_cw20_golden.json"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

REGEN=0
if [[ "${1:-}" == "--regen" ]]; then
  REGEN=1
fi

echo "Schema pin: $PIN_RS"
echo "Cloning $REPO @$PIN_RS ..."
git clone --depth 1 --filter=blob:none --sparse "$REPO" "$WORK/ustr-cmm" >/dev/null 2>&1 || \
  git clone --depth 1 "$REPO" "$WORK/ustr-cmm"
cd "$WORK/ustr-cmm"
git fetch --depth 1 origin "$PIN_RS" >/dev/null 2>&1 || true
git checkout "$PIN_RS" >/dev/null 2>&1
cd "$ROOT"

PROBE="$WORK/probe"
mkdir -p "$PROBE/src"
cat > "$PROBE/Cargo.toml" <<EOF
[package]
name = "ustr_cmm_schema_probe"
version = "0.0.0"
edition = "2021"

[dependencies]
cmm-treasury = { package = "treasury", git = "$REPO", rev = "$PIN_RS", features = ["library"] }
cosmwasm-std = { version = "1.5.11", features = ["staking"] }
serde_json = "1"
EOF

cat > "$PROBE/src/main.rs" <<'EOF'
use cosmwasm_std::{to_json_binary, Uint128};
use cmm_treasury::msg::ExecuteMsg;

fn main() {
    let cases = [
        ("representative", Uint128::from(42u128)),
        ("max_uint128", Uint128::MAX),
    ];
    let mut out = serde_json::json!({
        "ustr_cmm_repo": "https://gitlab.com/PlasticDigits2/ustr-cmm.git",
        "ustr_cmm_rev": std::env::var("PIN").unwrap(),
        "notes": "Canonical InstantWithdrawCw20 wire JSON produced by ustr-cmm treasury ExecuteMsg at the pinned rev (and by this window's TreasuryExecuteMsg subset). Refresh via scripts/verify_treasury_wire_schema.sh --regen after intentional pin bumps. Field order is recipient, token, amount (Rust struct declaration order).",
        "cases": [],
        "negative_fixtures": [
            {"name":"renamed_field_receiver","json":"{\"instant_withdraw_cw20\":{\"receiver\":\"terra1user\",\"token\":\"terra1vfdusd\",\"amount\":\"42\"}}"},
            {"name":"wrong_casing_variant","json":"{\"InstantWithdrawCw20\":{\"recipient\":\"terra1user\",\"token\":\"terra1vfdusd\",\"amount\":\"42\"}}"},
            {"name":"unknown_extra_field","json":"{\"instant_withdraw_cw20\":{\"recipient\":\"terra1user\",\"token\":\"terra1vfdusd\",\"amount\":\"42\",\"memo\":\"x\"}}"},
            {"name":"amount_as_number","json":"{\"instant_withdraw_cw20\":{\"recipient\":\"terra1user\",\"token\":\"terra1vfdusd\",\"amount\":42}}"},
            {"name":"missing_token","json":"{\"instant_withdraw_cw20\":{\"recipient\":\"terra1user\",\"amount\":\"42\"}}"}
        ]
    });
    let arr = out["cases"].as_array_mut().unwrap();
    for (name, amount) in cases {
        let msg = ExecuteMsg::InstantWithdrawCw20 {
            recipient: "terra1user".into(),
            token: "terra1vfdusd".into(),
            amount,
        };
        let canonical = String::from_utf8(to_json_binary(&msg).unwrap().to_vec()).unwrap();
        arr.push(serde_json::json!({"name": name, "canonical_json": canonical}));
    }
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}
EOF

export PIN="$PIN_RS"
(cd "$PROBE" && PIN="$PIN_RS" cargo run -q) > "$WORK/golden.new.json"

# Normalize: cargo run prints JSON; ensure pin env injected
python3 - <<PY
import json, os
path = "$WORK/golden.new.json"
with open(path) as f:
    data = json.load(f)
data["ustr_cmm_rev"] = os.environ["PIN"]
with open(path, "w") as f:
    json.dump(data, f, indent=2)
    f.write("\n")
PY

if [[ "$REGEN" -eq 1 ]]; then
  cp "$WORK/golden.new.json" "$GOLDEN"
  echo "Wrote $GOLDEN"
  exit 0
fi

python3 - <<PY
import json, sys
with open("$GOLDEN") as f:
    old = json.load(f)
with open("$WORK/golden.new.json") as f:
    new = json.load(f)
# Compare cases + rev only (notes may drift)
if old.get("ustr_cmm_rev") != new.get("ustr_cmm_rev"):
    print("REV MISMATCH", old.get("ustr_cmm_rev"), new.get("ustr_cmm_rev"), file=sys.stderr)
    sys.exit(1)
if old.get("cases") != new.get("cases"):
    print("CASE MISMATCH", file=sys.stderr)
    print("committed:", json.dumps(old.get("cases"), indent=2), file=sys.stderr)
    print("from pin:", json.dumps(new.get("cases"), indent=2), file=sys.stderr)
    sys.exit(1)
print("OK: golden InstantWithdrawCw20 matches ustr-cmm @$PIN")
PY

# Also run Rust conformance tests that use the git dep directly.
cargo test -p ust1-window --lib treasury::tests
cargo test -p ust1-integration-tests --lib treasury_schema
