#!/bin/sh
# The node fake Claude Code (fake-claude-node.cjs) as a program on Linux and macOS, as npm links one into its bin.
exec node "$(dirname "$0")/fake-claude-node.cjs" "$@"
