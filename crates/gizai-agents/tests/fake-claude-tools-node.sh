#!/bin/sh
# The node fake Claude Code of ask_claude_tree_test.rs (fake-claude-tools-node.cjs) as a program on Linux and macOS, as
# npm links one into its bin.
exec node "$(dirname "$0")/fake-claude-tools-node.cjs" "$@"
