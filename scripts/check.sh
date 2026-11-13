#!/usr/bin/env bash
# The builder's check: formatting, clippy, Rust tests, headless Swift tests, and the
# repository checks, including the standalone CLI installer. No native app build or
# signing; scripts/gate.sh adds those.
#
#   scripts/check.sh            everything above
#   scripts/check.sh FILTER...  only the tests whose names match FILTER
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"

if [ "$(find target/debug/deps 2>/dev/null | wc -l | tr -d ' ')" -gt 50000 ]; then
  echo "target directory has accumulated; cleaning"
  cargo clean
fi

swift_tests() {
  [[ "$(uname -s)" == Darwin ]] || return 0
  cargo build -p spielorders --locked
  swift test --package-path apps/macos -Xlinker "$PWD/target/debug/libspielorders.a" "$@"
}

if (( $# > 0 )); then
  cargo test --workspace --all-features --locked -- "$@"
  for filter in "$@"; do swift_tests --filter "$filter"; done
  if [[ "$(uname -s)" == Darwin ]]; then
    for filter in "$@"; do
      # unittest exits 5 when the filter matches no installer test.
      python3 -m unittest discover -s scripts/tests -p test_cli_install.py -k "$filter" || [[ $? == 5 ]]
    done
  fi
  exit 0
fi

cargo fmt --check

CARGO_TARGET_DIR=target/lint cargo clippy --workspace --all-targets --all-features --locked -- -D warnings &
clippy=$!
trap 'kill "$clippy" 2>/dev/null || true' EXIT INT TERM

cargo test --workspace --all-features --locked

if ! wait "$clippy"; then
  echo "clippy failed" >&2
  exit 1
fi
trap - EXIT INT TERM

cargo run --package spielorders-checks --locked
swift_tests
if [[ "$(uname -s)" == Darwin ]]; then
  python3 -m unittest discover -s scripts/tests -p test_cli_install.py
fi

echo "check passed"
