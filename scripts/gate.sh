#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

if [ "$(find target/debug/deps 2>/dev/null | wc -l | tr -d ' ')" -gt 50000 ]; then
  echo "target directory has accumulated; cleaning"
  cargo clean
fi

cargo fmt --check

CARGO_TARGET_DIR=target/lint cargo clippy --workspace --all-targets --locked -- -D warnings &
clippy=$!
trap 'kill "$clippy" 2>/dev/null || true' EXIT INT TERM

cargo test --workspace --locked

if ! wait "$clippy"; then
  echo "clippy failed" >&2
  exit 1
fi
trap - EXIT INT TERM

npm run test:schemas
npm run check:traceability

echo "gate passed"
