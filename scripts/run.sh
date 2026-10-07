#!/usr/bin/env bash
# Build Gizai (release) if needed and start it on dev data (.devdata/dev). Applies the Mesa EGL fix on machines
# without an NVIDIA GPU.
set -euo pipefail
cd "$(dirname "$0")/.."
source scripts/env.sh
[ -d node_modules ] || npm ci
# Rebuild when a binary is missing or any source file is newer than it. gizai-mcp (the helper Claude Code
# starts in a chat turn) must sit next to gizai.
if [ ! -x target/release/gizai ] || [ -n "$(find src src-tauri/src src-tauri/tauri.conf.json src-tauri/capabilities crates index.html package.json -newer target/release/gizai -print -quit)" ]; then
  npm run tauri build -- --no-bundle
fi
if [ ! -x target/release/gizai-mcp ] || [ -n "$(find crates/gizai-mcp -newer target/release/gizai-mcp -print -quit)" ]; then
  cargo build --release -q -p gizai-mcp
fi
# The dev build runs on its own data, next to the Gizai you use (the app launcher one, on your real data), and never
# changes that one. Update it on purpose with ./install.sh --skip-build: that backs up your data first.
export GIZAI_DATA_DIR="${GIZAI_DATA_DIR:-$PWD/.devdata/dev}"
echo "Dev build on the data in ${GIZAI_DATA_DIR/#$HOME/\~}. The Gizai in your app launcher is separate (update it: ./install.sh --skip-build)."
if ! lspci 2>/dev/null | grep -qi nvidia; then
  export __EGL_VENDOR_LIBRARY_FILENAMES=/usr/share/glvnd/egl_vendor.d/50_mesa.json
fi
exec target/release/gizai "$@"
