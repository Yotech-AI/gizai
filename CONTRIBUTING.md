# Contributing to GizAi

GizAi is a free, open-source desktop app where a team of AI coding agents works through your board, on Claude Code, Codex CLI or Gemini CLI. It is built with Tauri 2, Rust and React, and runs on Linux, macOS and Windows.

Questions and ideas go to [Discussions](https://github.com/Yotech-AI/gizai/discussions), and bugs to [issues](https://github.com/Yotech-AI/gizai/issues/new/choose). A security problem is reported privately, as [SECURITY.md](SECURITY.md) says.

## Before you start

- For a fix, say on its issue that you're working on it, so nobody does the same work twice.
- For anything bigger than a fix, open an issue first and say what you'd like to build.
- New here? Issues labelled [`good first issue`](https://github.com/Yotech-AI/gizai/labels/good%20first%20issue) are a good place to start.

## Set up

1. Install what GizAi needs to build on your system: git, Rust, Node.js 20 or newer, and a few things per system ([docs/install.md](docs/install.md)). In a clone, `./install.sh --check` (on Windows `.\install.ps1 -Check`) names what is missing.
2. Clone the repository, then build and test it:

   ```sh
   git clone https://github.com/Yotech-AI/gizai.git
   cd gizai
   source scripts/env.sh
   npm ci
   npm run build
   cargo test --workspace && npm test
   ```

   Run `source scripts/env.sh` before any cargo or npm command: it keeps the download caches inside the repository.
3. For a change to the UI, also run `scripts/ui-test.sh`. It runs the UI tests headless in `cage` on Linux, never on your desktop.

## Never test against your own GizAi

The GizAi you use keeps your real data. Don't run `./install.sh` to try a change, and never point a test at your own data folder.

- On Linux and macOS, `scripts/run.sh` builds GizAi and starts it on data of its own in `.devdata/dev`, next to the GizAi you use.
- For anything else, set `GIZAI_DATA_DIR` to a scratch folder.
- To test an update, follow [Testing an update](docs/RELEASING.md#testing-an-update-without-touching-a-real-gizai) in `docs/RELEASING.md`: a scratch install and a fake release, with `HOME` and `XDG_DATA_HOME` in the scratch folder too.

## How the code is laid out

These are the rules in [CLAUDE.md](CLAUDE.md), which GizAi's own agents follow too:

- Core logic lives in `crates/` and never imports Tauri.
- `src-tauri/` is a thin adapter, and the UI talks to it only through `src/api.ts`.
- Tests first: write the failing test and watch it fail. The tests use the fake CLIs in `crates/gizai-agents/tests/` (`fake-claude*`), never a real one.
- Code for one system goes behind `cfg(unix)`, `cfg(windows)` or `cfg(target_os = …)`, and leaves Linux's behaviour as it is. CI builds and tests every pull request on Linux, macOS and Windows.

## Writing

- UI text and docs are plain and short, in sentence case, and say what happens.
- Commit messages say, in plain English, what changed for the person using GizAi.

## Pull requests

- Open them into `main`. The `production` branch and the releases are for the maintainer ([docs/RELEASING.md](docs/RELEASING.md)).
- The pull request's template asks what changed for the person using GizAi, how you tested it, and on which systems.
- AI-written work is welcome: GizAi's own agents write most of this repository. The person who opens the pull request answers for it, so read and test what you send.

## Licence

Your contributions come under the [Apache License 2.0](LICENSE), as its section 5 says, without a separate contributor agreement (no CLA). The licence gives no rights to the GizAi name and logo.
