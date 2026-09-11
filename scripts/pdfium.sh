#!/bin/sh
# Fetch libpdfium into ./pdfium, which is where `cargo run` (via
# .cargo/config.toml) and `cargo packager` (via Cargo.toml) both look for it.
# The tag matches PDFIUM_TAG in .github/workflows/bundle.yml.
set -eu
TAG=chromium%2F8021

case "$(uname -s)" in
    Darwin) os=mac ;;
    Linux)  os=linux ;;
    *) echo "Unknown system $(uname -s) — use scripts/pdfium.ps1 on Windows." >&2; exit 1 ;;
esac
case "$(uname -m)" in
    arm64|aarch64) arch=arm64 ;;
    x86_64|amd64)  arch=x64 ;;
    *) echo "No pdfium build for $(uname -m)." >&2; exit 1 ;;
esac

cd "$(dirname "$0")/.."
mkdir -p pdfium
echo "pdfium-$os-$arch"
curl -sSfL "https://github.com/bblanchon/pdfium-binaries/releases/download/$TAG/pdfium-$os-$arch.tgz" | tar xz -C pdfium
ls pdfium/lib
