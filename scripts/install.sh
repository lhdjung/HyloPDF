#!/bin/sh
# Build Moonowl from this checkout and install it as a real app. One command,
# nothing to piece together: pdfium, the release build, the bundle, and the
# install. Windows has scripts/install.ps1.
set -eu
cd "$(dirname "$0")/.."

sh scripts/pdfium.sh
command -v cargo-packager >/dev/null 2>&1 || cargo install cargo-packager --locked
cargo build --release

case "$(uname -s)" in
Darwin)
    cargo packager --release --formats app
    rm -rf /Applications/Moonowl.app
    cp -R target/release/Moonowl.app /Applications/
    echo "Installed /Applications/Moonowl.app — open it from Launchpad or Spotlight."
    # Copied rather than downloaded, so it carries no quarantine flag and
    # Gatekeeper does not ask, ad-hoc signature and all.
    ;;
Linux)
    if command -v apt >/dev/null 2>&1; then
        cargo packager --release --formats deb
        sudo apt install -y ./target/release/*.deb
    elif command -v dnf >/dev/null 2>&1; then
        cargo packager --release --formats rpm
        sudo dnf install -y ./target/release/*.rpm
    else
        cargo packager --release --formats appimage
        mkdir -p "$HOME/.local/bin"
        cp target/release/*.AppImage "$HOME/.local/bin/Moonowl"
        chmod +x "$HOME/.local/bin/Moonowl"
        echo "Installed ~/.local/bin/Moonowl — run 'Moonowl' if that is on your PATH."
    fi
    ;;
*) echo "Unknown system $(uname -s) — use scripts/install.ps1 on Windows." >&2; exit 1 ;;
esac
