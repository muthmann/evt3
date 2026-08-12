#!/usr/bin/env bash
set -euo pipefail

repository_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
profile_dir="$(mktemp -d "${TMPDIR:-/tmp}/evt3-pgo-data.XXXXXX")"
generate_target="$(mktemp -d "${TMPDIR:-/tmp}/evt3-pgo-target.XXXXXX")"
use_target="${repository_root}/target/pgo"

if command -v llvm-profdata >/dev/null 2>&1; then
    llvm_profdata="$(command -v llvm-profdata)"
elif command -v xcrun >/dev/null 2>&1; then
    llvm_profdata="$(xcrun --find llvm-profdata)"
else
    echo "llvm-profdata is required for a PGO build" >&2
    exit 1
fi

cd "${repository_root}"
echo "Building and running the representative real-file decode workload..."
CARGO_TARGET_DIR="${generate_target}" \
RUSTFLAGS="-Cprofile-generate=${profile_dir}" \
    cargo test --release -p evt3 test_decode_performance -- --nocapture

"${llvm_profdata}" merge -o "${profile_dir}/merged.profdata" "${profile_dir}"

echo "Building the workspace with the collected optimization profile..."
CARGO_TARGET_DIR="${use_target}" \
RUSTFLAGS="-Cprofile-use=${profile_dir}/merged.profdata -Cllvm-args=-pgo-warn-missing-function" \
    cargo build --release -p evt3 -p evt3-cli

echo "PGO artifacts: ${use_target}/release"
