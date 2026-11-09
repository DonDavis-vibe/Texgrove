#!/usr/bin/env bash
# Builds texgrove.wasm, the Meshgrove plugin:
#
#   scripts/build.sh            → target/plugin/texgrove.wasm (release)
#   scripts/build.sh --debug    → the same, a fast build with slow geometry
#
# Needs `rustup target add wasm32-wasip2` once. A distribution's Rust (Fedora's /usr/bin/cargo)
# often comes first in PATH and has no wasm targets, so rustup's stable toolchain is used.
set -euo pipefail
root=$(realpath "$(dirname "$0")/..")
profile=(--release)
dir=release
if [ "${1:-}" = "--debug" ]; then
    profile=()
    dir=debug
fi
rustup=$(command -v rustup || echo "$HOME/.cargo/bin/rustup")
cargo=(cargo)
if [ -x "$rustup" ]; then
    cargo=("$rustup" run stable cargo)
    RUSTC=$("$rustup" which --toolchain stable rustc)
    export RUSTC
fi
# SIMD in WebAssembly: every runtime Meshgrove uses has it, and the mesh work gets faster.
export RUSTFLAGS="${RUSTFLAGS:-} -C target-feature=+simd128"
# A local checkout of the plugin API, while the Meshgrove version that has it is not tagged.
local_api=()
[ -f "$root/crates/texgrove-plugin/local-api.toml" ] && local_api=(--config "$root/crates/texgrove-plugin/local-api.toml")
"${cargo[@]}" build ${local_api[@]+"${local_api[@]}"} --manifest-path "$root/crates/texgrove-plugin/Cargo.toml" --target wasm32-wasip2 \
    --target-dir "$root/target/plugin" ${profile[@]+"${profile[@]}"}
cp "$root/target/plugin/wasm32-wasip2/$dir/texgrove_plugin.wasm" "$root/target/plugin/texgrove.wasm"
ls -l "$root/target/plugin/texgrove.wasm"
