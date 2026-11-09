#!/usr/bin/env bash
# Copies the built plugin into Meshgrove's plugin folder (enable it in Meshgrove's settings).
set -euo pipefail
root=$(realpath "$(dirname "$0")/..")
dest=${XDG_DATA_HOME:-$HOME/.local/share}/meshgrove/plugins
mkdir -p "$dest"
cp "$root/target/plugin/texgrove.wasm" "$dest/texgrove.wasm"
echo "installed: $dest/texgrove.wasm"
