#!/usr/bin/env bash
# Everything CI would check: format, clippy and tests of the native workspace and, once the
# plugin crate exists, clippy of the plugin for wasm32-wasip2 and that it builds.
#
# For the wasm target the whole stable toolchain of rustup has to come first in PATH: with a
# distribution's rustc first (Fedora), clippy-driver takes its sysroot and finds no std for wasm.
set -euo pipefail
root=$(realpath "$(dirname "$0")/..")
cd "$root"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
[ -f crates/texgrove-plugin/Cargo.toml ] || exit 0
rustup=$(command -v rustup || echo "$HOME/.cargo/bin/rustup")
if [ -x "$rustup" ]; then
    toolchain_bin=$(dirname "$("$rustup" which --toolchain stable rustc)")
    export PATH="$toolchain_bin:$PATH" RUSTUP_TOOLCHAIN=stable
fi
local_api=()
[ -f crates/texgrove-plugin/local-api.toml ] && local_api=(--config crates/texgrove-plugin/local-api.toml)
cargo fmt --manifest-path crates/texgrove-plugin/Cargo.toml --check
cargo clippy ${local_api[@]+"${local_api[@]}"} --manifest-path crates/texgrove-plugin/Cargo.toml --target wasm32-wasip2 \
    --target-dir target/plugin -- -D warnings
scripts/build.sh
