#!/usr/bin/env bash
# Perch 本地门禁（六条，单一事实源）。CI 仅跑跨平台子集（frontend / core）。
set -euo pipefail
cd "$(dirname "$0")/.."

echo "== cargo fmt --all --check"
cargo fmt --all --check
echo "== cargo clippy -p perch-core"
cargo clippy -p perch-core --all-targets -- -D warnings
echo "== cargo test -p perch-core"
cargo test -p perch-core
echo "== cargo test -p perch --lib"
cargo test -p perch --lib
echo "== npm run check"
npm run check
echo "== npm run build"
npm run build
echo "== 六条门禁全部通过"