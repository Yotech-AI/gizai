# Gizai (repo rules for Claude Code sessions and agents)

Gizai is Jeffrey's (they/them) desktop app: clients, projects and tasks, worked on by local Claude Code agents. It is built with Tauri 2, a React/TypeScript UI, Rust core crates and local SQLite. Jeffrey uses the installed Gizai every day, and Gizai's own agents now work on this repository.

## Start here

1. `docs/HANDOFF-2026-10-07.md`: what is built, where it stands, and the decisions so far.
2. `README.md`: what Gizai does, how a card moves, and the Development section (commands and repository layout).
3. `docs/v0.3-notes.md`: the latest build notes, rulings, deferred minors and Jeffrey's answers.
4. Then the code that your card touches, and its tests.

Background, only when the card needs it:

- `docs/superpowers/specs/` and `docs/superpowers/plans/` (the chat feature's spec and the plans);
- `docs/PAID-FEATURES.md`;
- `design/` (tokens and the design system generator).

## Never

- **Never touch the Gizai Jeffrey uses.**
  - Don't run `./install.sh`.
  - Don't start `~/.local/bin/gizai` or anything in `~/.local/lib/gizai`.
  - Don't open, copy or change `~/.local/share/gizai`: that is his real data, and an older build refuses a newer database.
  - Test Gizai's own update only with a scratch install and a fake release, with `HOME` and `XDG_DATA_HOME` in the scratch folder too (`docs/RELEASING.md` → Testing an update).
- **Never start a GUI on Jeffrey's screen.** UI tests run headless in `cage` (`scripts/ui-test.sh`, `scripts/shot-cage.sh`).
- **Never kill by process name or pattern** (`pkill`, `killall`): end only the exact PIDs you started.
- **Never run against Brave or its profile.**
- **No `git push`, no remotes, no GitHub posts.**
- **Never write in `~/.claude`.** Never run `src-tauri/examples/chat_probe.rs`: it spends money and writes a session there.
- **No sudo and no global installs.** If a system package is needed, finish what you can and ask for the exact command in `run_for_me` on a `needs_decision` result line (Run this for me): Jeffrey runs it and presses Done, continue, which resumes your run.

## Working rules

- **Commands:** run `source scripts/env.sh` before any cargo or npm command. In a worktree it shares the main checkout's download caches, and it uses `sccache` when that is installed.
- **Layering:**
  - Core logic lives in `crates/` and never imports Tauri.
  - `src-tauri/` is a thin adapter.
  - The UI talks to it only through `src/api.ts`.
- **TDD:** write the failing test first and watch it fail. Before you say you're done, run:
  - `cargo test --workspace`
  - `npm test`
  - `npm run build`
  - `scripts/ui-test.sh` when the UI changed

  The tests use fake Claude Code scripts (`crates/gizai-agents/tests/fake-claude*.{sh,py}`), never the real one.
- **Release build:** always use `npm run tauri build -- --no-bundle`. A plain `cargo build --release -p gizai` can produce a binary that loads the dev URL.
- **Git:** `main` is development (cards start from it); `production` is the released code, protected, changed only by a pull request from `main`. A release is a `vX.Y.Z` tag on `production`, which installed Gizais offer as an update (`docs/RELEASING.md` says how and when). Work on your card's branch. Commit as you go: Gizai stops a run at its limits. Write commit messages in plain English that say what changed for the person using Gizai.
- **Writing:** UI text and docs are plain and short, in sentence case, and say what happens.
- **Design rules:**
  - teal `--live` only for an agent working now;
  - magenta `--needs` only for "needs you";
  - dark first, blue accent;
  - fonts Atkinson Hyperlegible Next and Mono.

## GUI and browser tests (Jeffrey's rules, 2026-10-05)

- Run GUI and browser tests headless or inside `cage` with the headless backend, never on Jeffrey's screen or workspaces.
- Never run anything against Brave or its profile.
- Never `pkill`/`killall` or kill by process name or pattern: end only the exact PIDs you started.
- Why: Brave quit during Chromium test runs on 2026-10-05, and Jeffrey asked for safer tests.
- Don't suggest removing `nvidia-utils`: Hyprland depends on it on this PC. The launcher sets `__EGL_VENDOR_LIBRARY_FILENAMES` to Mesa instead.
