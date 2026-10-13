#!/usr/bin/env bash
# The product tour's Codex (scripts/tour-capture.sh, GA-149): a fake, so Settings → Coding CLIs finds a program at
# ~/.local/bin/codex in the fake home. No agent runs on it during the capture (the Frontend and QA Agents are paused).
case " $* " in *" --version "*|*" -V "*) echo "codex-cli 0.130.0"; exit 0 ;; esac
echo "codex: this is the product tour's fake" >&2
exit 1
