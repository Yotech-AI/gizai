#!/usr/bin/env bash
# Stands in for `claude` in tests: prints the run-ok fixture as a stream. Depending on the prompt (read
# from stdin): "hang" waits until interrupted (and, like Claude Code, exits on SIGINT); "stubborn"
# ignores SIGINT so only SIGTERM ends it; "orphan" leaves a background child holding stdout open; "leftover" finishes
# but leaves a background child that ignores SIGTERM (not on stdout).
# A prompt containing FAKE_HANG (e.g. from a task description) also hangs, FAKE_STUBBORN is stubborn; FAKE_CRASH exits 1
# with an error on stderr.
# FAKE_COMMIT_TWICE commits twice in its working folder (the run's worktree), "First change" then "Second change",
# and then finishes like run-ok. FAKE_REFUSED finishes like run-refused (Claude Code refusing four tool calls).
# FAKE_REFUSED_THEN_HANG prints run-refused up to its result line, then hangs like FAKE_HANG (a run stopped part-way).
# FAKE_TEMP=1 in its environment (a CLI's environment line, GA-48) also writes TMPDIR, TMP and TEMP, whether that
# folder is there and the whole prompt to stderr, and leaves a file and a folder in it for Gizai to empty.
# FAKE_NO_RESULT in the prompt, or FAKE_NO_RESULT=1 in its environment (every run, Gizai's nudge too), finishes like
# run-no-result: the agent pushes, starts a background wait for CI and ends its message without the GIZAI_RESULT line
# (GA-54). FAKE_GATE=<file> in its environment waits (at most 30 s) until that file exists before it finishes, so a test
# can change things while the run is still live.
# FAKE_RUN_FOR_ME in the prompt finishes like run-for-me: a needs_decision whose result line asks the user to run two
# commands (`run_for_me`, GA-31). FAKE_ASKS finishes like run-asks: a needs_decision without them (an older result line).
# FAKE_LEARNED in the prompt finishes like run-learned: run-ok with two `learned` lines on its result line (GA-19).
here="$(cd "$(dirname "$0")" && pwd)"
# Asked for the model list (stream-json input): answer the initialize request, then exit when stdin closes.
case " $* " in *" --input-format stream-json "*)
  IFS= read -r _request
  cat "$here/fixtures/models-init.jsonl"
  cat > /dev/null
  exit 0 ;;
esac
prompt="$(cat)"   # the prompt comes in on stdin, like claude -p
echo "prompt chars: ${#prompt}" >&2
echo "argv: $*" >&2
if [ -n "${FAKE_TEMP:-}" ]; then
  echo "temp: TMPDIR=${TMPDIR:-} TMP=${TMP:-} TEMP=${TEMP:-}" >&2
  if [ -n "${TMPDIR:-}" ] && [ -d "$TMPDIR" ]; then
    echo "temp exists: yes" >&2
    mkdir -p "$TMPDIR/scratch" && echo x > "$TMPDIR/scratch/test.db" && echo y > "$TMPDIR/left.txt"
  else
    echo "temp exists: no" >&2
  fi
  printf 'prompt>>%s<<prompt\n' "$prompt" >&2
fi
fixture="$here/fixtures/run-ok.jsonl"
case "$prompt" in *FAKE_REFUSED*) fixture="$here/fixtures/run-refused.jsonl" ;; esac
case "$prompt" in *FAKE_NO_RESULT*) fixture="$here/fixtures/run-no-result.jsonl" ;; esac
case "$prompt" in *FAKE_RUN_FOR_ME*) fixture="$here/fixtures/run-for-me.jsonl" ;; *FAKE_ASKS*) fixture="$here/fixtures/run-asks.jsonl" ;; esac
case "$prompt" in *FAKE_LEARNED*) fixture="$here/fixtures/run-learned.jsonl" ;; esac
if [ -n "${FAKE_NO_RESULT:-}" ]; then fixture="$here/fixtures/run-no-result.jsonl"; fi
case "$prompt" in *FAKE_HANG*) prompt=hang ;; *FAKE_STUBBORN*) prompt=stubborn ;; *FAKE_CRASH*) prompt=crash ;; *FAKE_NOT_LOGGED_IN*) prompt=nologin ;; esac
case "$prompt" in *FAKE_REFUSED_THEN_HANG*) prompt=refusedhang ;; esac
if [ "$prompt" = "refusedhang" ]; then
  trap 'exit 130' INT
  sed '$d' "$here/fixtures/run-refused.jsonl"
  sleep 600 &
  wait $!
  exit 0
fi
if [ "$prompt" = "nologin" ]; then
  # What Claude Code 2.1.289 prints without a login: subtype "success" but is_error, and the reason as the result.
  head -n1 "$here/fixtures/run-ok.jsonl"
  echo '{"type":"result","subtype":"success","is_error":true,"result":"Not logged in · Please run /login","total_cost_usd":0,"num_turns":1,"terminal_reason":"api_error"}'
  exit 1
fi
if [ "$prompt" = "crash" ]; then
  echo "error: unknown option '--frobnicate'" >&2
  exit 1
fi
if [ "$prompt" = "hang" ] || [ "$prompt" = "stubborn" ]; then
  if [ "$prompt" = "hang" ]; then trap 'exit 130' INT; else trap '' INT; fi
  # The first line with builtins, not `head -n1`: a test stops the run as soon as it reads that line, and bash 3.2
  # (macOS) drops a SIGINT that comes while it waits for a command that then exits normally, so it would miss its trap.
  IFS= read -r first < "$here/fixtures/run-ok.jsonl"
  printf '%s\n' "$first"
  sleep 600 &
  wait $!
  exit 0
fi
case "$prompt" in *FAKE_COMMIT_TWICE*)
  for subject in "First change" "Second change"; do
    git -c user.email=t@t -c user.name=t commit -q --allow-empty -m "$subject" || exit 1
  done ;;
esac
if [ "$prompt" = "orphan" ]; then
  sleep 600 &
fi
if [ "$prompt" = "leftover" ]; then
  ( trap '' TERM; exec sleep 600 ) > /dev/null 2>&1 &
fi
if [ -n "${FAKE_GATE:-}" ]; then
  for _ in $(seq 600); do [ -e "$FAKE_GATE" ] && break; sleep 0.05; done
fi
while IFS= read -r line; do
  printf '%s\n' "$line"
  sleep 0.05
done < "$fixture"
echo "fake claude done" >&2
exit 0
