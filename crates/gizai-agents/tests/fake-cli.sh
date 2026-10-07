#!/usr/bin/env bash
# Stands in for Codex, Gemini or another coding CLI in tests (GA-3), never the real ones. FAKE_KIND (one of the CLI's
# environment lines in Settings → Coding CLIs) picks which: codex (codex exec --json), gemini (stream-json) or other
# (plain text). It writes its argv, FAKE_ACCOUNT and the prompt's size to stderr, then that CLI's output for a card it
# finished. A prompt containing FAKE_NOT_LOGGED_IN fails the way the CLI does without a login; FAKE_NO_RESULT ends
# without the GIZAI_RESULT line.
prompt="$(cat)"   # the prompt on stdin (empty when it came as an argument)
echo "argv: $*" >&2
echo "account: ${FAKE_ACCOUNT:-none}" >&2
echo "prompt chars: ${#prompt}" >&2
all="$prompt $*"
case "$FAKE_KIND" in
codex)
  echo '{"type":"thread.started","thread_id":"019a-fake-thread"}'
  if [[ "$all" == *FAKE_NOT_LOGGED_IN* ]]; then
    echo '{"type":"error","message":"Not logged in: run codex login"}'
    echo '{"type":"turn.failed","error":{"message":"Not logged in: run codex login"}}'
    exit 1
  fi
  echo '{"type":"turn.started"}'
  echo '{"type":"item.started","item":{"id":"i1","type":"command_execution","command":"git status","aggregated_output":"","status":"in_progress"}}'
  echo '{"type":"item.completed","item":{"id":"i1","type":"command_execution","command":"git status","aggregated_output":"clean","exit_code":0,"status":"completed"}}'
  echo '{"type":"item.completed","item":{"id":"i2","type":"file_change","changes":[{"path":"src/a.rs","kind":"update"}],"status":"completed"}}'
  echo '{"type":"item.completed","item":{"id":"i3","type":"agent_message","text":"Done.\nGIZAI_RESULT: {\"outcome\":\"ready_for_testing\",\"summary\":\"done on codex\",\"issues\":[]}"}}'
  echo '{"type":"turn.completed","usage":{"input_tokens":1500,"cached_input_tokens":0,"output_tokens":250}}'
  ;;
gemini)
  echo '{"type":"init","timestamp":"2026-10-07T00:00:00Z","session_id":"gemini-own-id","model":"gemini-2.5-pro"}'
  echo '{"type":"tool_use","tool_name":"run_shell_command","tool_id":"t1","parameters":{"command":"git status"}}'
  echo '{"type":"tool_result","tool_id":"t1","status":"success","output":"clean"}'
  echo '{"type":"message","role":"assistant","content":"Done.\nGIZAI_RESULT: {\"outcome\":\"ready_for_testing\",\"summary\":\"done on gemini\",\"issues\":[]}","delta":true}'
  echo '{"type":"result","status":"success","stats":{"total_tokens":1000,"input_tokens":800,"output_tokens":200,"duration_ms":5,"tool_calls":1}}'
  ;;
*)
  printf '\033[1mWorking\033[0m on it\n'
  echo "progress 50%"$'\r'"progress 100%"
  if [[ "$all" != *FAKE_NO_RESULT* ]]; then
    echo 'GIZAI_RESULT: {"outcome":"ready_for_testing","summary":"done on other","issues":[]}'
  fi
  ;;
esac
