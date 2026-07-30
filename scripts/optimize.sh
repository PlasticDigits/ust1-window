#!/bin/bash
set -euo pipefail
# Run from repository root (workspace Cargo.toml).
#
# CosmWasm `bob` only builds workspace members whose path starts with
# `contracts/` (see bob_the_builder PACKAGE_PREFIX).
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# Optimizer ≥0.15 writes release wasm to /target (not /code/target).
IMAGE="${OPTIMIZER_IMAGE:-cosmwasm/optimizer:0.16.1}"

docker run --rm -v "$ROOT":/code \
  --mount type=volume,source=ust1_window_optimizer_cache,target=/target \
  --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
  "$IMAGE"

echo "Optimized wasm artifacts are under artifacts/"
ls -l artifacts/*.wasm
