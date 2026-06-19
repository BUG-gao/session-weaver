#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "Session Weaver compatibility smoke test"
rustc --version
claude --version
codex --version

cargo test --all-targets
cargo run --quiet -- doctor --json

echo "Offline compatibility checks passed."
echo "See docs/TEST_REPORT.zh-CN.md for bounded native-client probes."
