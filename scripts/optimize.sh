#!/bin/bash
set -e
# Run from repository root (workspace Cargo.toml).
docker run --rm -v "$(pwd)":/code \
  --mount type=volume,source=ust1_window_optimizer_cache,target=/code/target \
  --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
  cosmwasm/workspace-optimizer:0.16.1

echo "Optimized wasm artifacts are under artifacts/"
