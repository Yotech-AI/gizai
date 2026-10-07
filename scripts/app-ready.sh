# scripts/app-ready.sh: sourced by the headless UI scripts (ui-test.sh, shot-cage.sh) from the repository's root, so
# they also work in a brand-new worktree.
#   gz_demo  makes the demo data (.devdata/demo, with `example demo`) when it is missing;
#   gz_app   builds the app (npm run tauri build -- --no-bundle, after npm ci when node_modules/ is missing) when it is
#            missing or older than the files it is built from.

gz_demo() {
  [ -f .devdata/demo/gizai.db ] && return 0
  echo "making the demo data in .devdata/demo"
  (source scripts/env.sh && cargo run -q -p gizai-core --example demo -- .devdata/demo)
}

# The files the app is built from (extra find tests in "$@").
gz_sources() {
  find src index.html package.json package-lock.json vite.config.ts tsconfig.json Cargo.toml Cargo.lock \
    src-tauri/src src-tauri/build.rs src-tauri/Cargo.toml src-tauri/tauri.conf.json src-tauri/capabilities src-tauri/icons \
    crates/*/src crates/*/migrations crates/*/Cargo.toml -type f "$@" 2>/dev/null
}

gz_app() {
  local bin=target/release/gizai
  if [ -x "$bin" ] && [ -z "$(gz_sources -newer "$bin" -print -quit)" ]; then return 0; fi
  echo "building the app (npm run tauri build -- --no-bundle)"
  (source scripts/env.sh && { [ -d node_modules ] || npm ci; } && npm run tauri build -- --no-bundle) || return 1
  # a build that had nothing to do leaves the binary's time as it was
  touch "$bin"
}
