#!/usr/bin/env python3
"""
Deploy orchestration for LocalTerra (ust1-window).

This script does not encode business rules; it shells out to `terrad` when available.
Install the Terra Classic CLI and run inside the LocalTerra container or against localhost:26657.

For **mainnet / testnet** checklists, address registry, BSC integration notes, and oracle env
semantics, see ``docs/DEPLOYMENT.md`` (GitLab issue #15).

TEST-16 / LocalTerra gated smoke (DeliverTx-reject / skip-path; skip-clean without LCD):
``make test-localterra-smoke`` / ``scripts/localterra_e2e_smoke.sh`` (GitLab issue #28).

Environment:
  CHAIN_ID (default: localterra)
  HOME / keyring (configure per your terrad setup)
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys


def main() -> int:
    chain_id = os.environ.get("CHAIN_ID", "localterra")
    terrad = shutil.which("terrad")
    if terrad is None:
        print(
            "terrad not found on PATH. Install Terra Classic CLI or run deploy commands manually.",
            file=sys.stderr,
        )
        print("\nExample sequence after `cargo build` wasm artifacts:")
        print("  terrad tx wasm store artifacts/ust1_oracle.wasm ...")
        print("  terrad tx wasm instantiate <code_id> '{...}' ...")
        print("\nFull operator runbook: docs/DEPLOYMENT.md")
        print("\nOracle env for oracle-service (after deploy):")
        print("  export TERRA_LCD_URL=http://localhost:1317")
        print(f"  export TERRA_CHAIN_ID={chain_id}")
        print("  export ORACLE_CONTRACT=<addr>")
        print(
            "  export BSC_RPC_URLS=https://bsc-dataseed1.binance.org,https://bsc-dataseed2.binance.org"
        )
        print("  export VENUS_VTOKEN_ADDRESS=<bsc_vtoken_address>")
        print("  export TERRA_MNEMONIC=<oracle_operator_mnemonic>")
        return 0

    print(f"Using terrad at {terrad}, chain-id={chain_id}")
    subprocess.run([terrad, "status"], check=False)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
