#!/usr/bin/env bash
# Stands in for `claude` in tests: prints the run-ok fixture as a stream. Depending on the prompt (read
# from stdin): "hang" waits until interrupted (and, like Claude Code, exits on SIGINT); "stubborn"
# ignores SIGINT so only SIGTERM ends it; "orphan" leaves a background child holding stdout open; "leftover" finishes
# but leaves a background child that ignores SIGTERM (not on stdout).
# A prompt containing FAKE_HANG (e.g. from a task description) also hangs, FAKE_STUBBORN is stubborn; FAKE_CRASH exits 1
# with an error on stderr.
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
case "$prompt" in *FAKE_HANG*) prompt=hang ;; *FAKE_STUBBORN*) prompt=stubborn ;; *FAKE_CRASH*) prompt=crash ;; *FAKE_NOT_LOGGED_IN*) prompt=nologin ;; esac
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
  head -n1 "$here/fixtures/run-ok.jsonl"
  sleep 600 &
  wait $!
  exit 0
fi
if [ "$prompt" = "orphan" ]; then
  sleep 600 &
fi
if [ "$prompt" = "leftover" ]; then
  ( trap '' TERM; exec sleep 600 ) > /dev/null 2>&1 &
fi
while IFS= read -r line; do
  printf '%s\n' "$line"
  sleep 0.05
done < "$here/fixtures/run-ok.jsonl"
echo "fake claude done" >&2
exit 0
