#!/usr/bin/env bash
# Stands in for `claude` in the GA-182 screenshots flow tests (src-tauri/tests/screenshots_flow_test.rs), never the real
# one. It finishes like fake-claude-mcp-run.sh (the run-ok fixture as a stream), and before it prints anything it keeps
# what it was given in $FAKE_MCP_OUT/<its worktree's folder name>/ (FAKE_MCP_OUT is one of the CLI's environment lines in
# Settings → Coding CLIs; the subfolder keeps runs on several cards at once apart):
#   argv       its arguments, one per line
#   prompt     the prompt it read from stdin
#   pwd        the folder it runs in (the card's worktree)
#   env        NAME=value for each of GIZAI_PREVIEW_PORT, GIZAI_PREVIEW_URL, GIZAI_PREVIEW_DIR, PORT, TMPDIR it has
#   answered   yes/no: whether something answered on 127.0.0.1:$GIZAI_PREVIEW_PORT when it started (only with that port)
#   answer     what an HTTP GET / on that port answered (only when it answered)
#   preview.log  a copy of $GIZAI_PREVIEW_DIR/preview.log as it was when it started (only when there was one)
#   shot       written once it has saved its screenshots (see below)
# When the prompt has the Screenshots section with the preview's address ("## Screenshots" … "served at
# http://127.0.0.1:"), it saves two tiny PNGs (only their signature and IHDR header) as $TMPDIR/screenshots/
# 01-board-desktop.png (32 x 20) and 02-card-mobile.png (24 x 48), and captions.json with a caption for the first only.
# FAKE_SHOTS_GATE in the prompt (from the card's description): after that, it waits until $FAKE_MCP_OUT/gate exists (at
# most 30 s) before it finishes, so a test can hold several runs live at once.
# FAKE_SHOTS_HANG in the prompt: after that, it prints its init line and waits until interrupted (exits on SIGINT, as
# fake-claude-mcp-run.sh's FAKE_MCP_HANG does), so a test can Stop it; at most 120 s, so a test that fails before
# its Stop leaves nothing behind for long.
here="$(cd "$(dirname "$0")" && pwd)"
# Asked for the model list (stream-json input): answer the initialize request, then exit when stdin closes.
case " $* " in *" --input-format stream-json "*)
  IFS= read -r _request
  cat "$here/fixtures/models-init.jsonl"
  cat > /dev/null
  exit 0 ;;
esac
prompt="$(cat)"   # the prompt comes in on stdin, like claude -p
base="${FAKE_MCP_OUT:?FAKE_MCP_OUT must name a folder}"
out="$base/$(basename "$PWD")"
mkdir -p "$out"
printf '%s\n' "$@" > "$out/argv"
printf '%s' "$prompt" > "$out/prompt"
printf '%s' "$PWD" > "$out/pwd"
: > "$out/env"
for name in GIZAI_PREVIEW_PORT GIZAI_PREVIEW_URL GIZAI_PREVIEW_DIR PORT TMPDIR; do
  if [ -n "${!name+set}" ]; then
    printf '%s=%s\n' "$name" "${!name}" >> "$out/env"
  fi
done
if [ -n "${GIZAI_PREVIEW_PORT:-}" ]; then
  if (exec 3<>"/dev/tcp/127.0.0.1/$GIZAI_PREVIEW_PORT") 2>/dev/null; then
    echo yes > "$out/answered"
    (
      exec 3<>"/dev/tcp/127.0.0.1/$GIZAI_PREVIEW_PORT"
      printf 'GET / HTTP/1.0\r\nHost: 127.0.0.1\r\n\r\n' >&3
      # HTTP/1.0: the server closes the connection after its answer (no `timeout`: macOS has none)
      cat <&3
    ) > "$out/answer" 2>/dev/null
  else
    echo no > "$out/answered"
  fi
fi
if [ -n "${GIZAI_PREVIEW_DIR:-}" ] && [ -f "$GIZAI_PREVIEW_DIR/preview.log" ]; then
  cp "$GIZAI_PREVIEW_DIR/preview.log" "$out/preview.log"
fi
case "$prompt" in *"## Screenshots"*"served at http://127.0.0.1:"*)
  shots="${TMPDIR:-$PWD/.gizai-tmp}/screenshots"
  mkdir -p "$shots"
  # PNG signature, then the IHDR chunk's length and type, width and height (big-endian), bit depth, colour type,
  # compression, filter, interlace and a CRC Gizai doesn't check
  printf '\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR\x00\x00\x00\x20\x00\x00\x00\x14\x08\x06\x00\x00\x00\x00\x00\x00\x00' > "$shots/01-board-desktop.png"
  printf '\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR\x00\x00\x00\x18\x00\x00\x00\x30\x08\x06\x00\x00\x00\x00\x00\x00\x00' > "$shots/02-card-mobile.png"
  printf '{"01-board-desktop.png": "The board"}' > "$shots/captions.json"
  echo yes > "$out/shot" ;;
esac
echo "fake claude (shots) started" >&2
fixture="$here/fixtures/run-ok.jsonl"
case "$prompt" in *FAKE_SHOTS_GATE*)
  n=0
  until [ -e "$base/gate" ] || [ "$n" -ge 600 ]; do
    sleep 0.05
    n=$((n + 1))
  done ;;
esac
case "$prompt" in *FAKE_SHOTS_HANG*)
  trap 'exit 130' INT
  IFS= read -r first < "$fixture"
  perl -e '$| = 1; print "$ARGV[0]\n"; sleep 120' -- "$first"
  exit 0 ;;
esac
tail -n +1 "$fixture" | while IFS= read -r line; do
  printf '%s\n' "$line"
  sleep 0.02
done
echo "fake claude (shots) done" >&2
exit 0
