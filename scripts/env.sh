# scripts/env.sh — source before any cargo/npm command (keeps caches inside the repo)
export GZ="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")/.." && pwd)"
# In a git worktree (an agent's card) the download caches are the main checkout's, so a card doesn't fetch every
# crate and package again. Build output (target/, node_modules/) stays per checkout.
GZ_MAIN="$(git -C "$GZ" rev-parse --path-format=absolute --git-common-dir 2>/dev/null)"
GZ_MAIN="${GZ_MAIN%/.git}"
[ -n "$GZ_MAIN" ] && [ -d "$GZ_MAIN/.cargo" ] || GZ_MAIN="$GZ"
export CARGO_HOME=$GZ_MAIN/.cargo
export npm_config_cache=$GZ_MAIN/.npm-cache
export npm_config_update_notifier=false npm_config_fund=false npm_config_audit=false
export TAURI_TELEMETRY_DISABLED=1
