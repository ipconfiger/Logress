#!/bin/bash
set -e
cd "$(dirname "$0")/.."

echo "=== Building graftail for local platform ==="
cargo build --release
cp target/release/graftail dist/graftail-linux-x64
echo "Binary: dist/graftail-linux-x64 ($(du -h dist/graftail-linux-x64 | cut -f1))"

echo ""
echo "=== Update npm bin for local testing ==="
cp dist/graftail-linux-x64 npm/bin/graftail-linux-x64
echo "Done. For global install:"
echo "  cd npm && npm install -g ."
echo ""
echo "=== To release a new version ==="
echo "  1. Update version in Cargo.toml and npm/package.json"
echo "  2. git tag v0.1.0 && git push --tags"
echo "  3. Wait for GitHub Actions to build all platforms"
echo "  4. cd npm && npm publish"
