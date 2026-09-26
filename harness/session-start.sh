#!/bin/sh
# Thin, fail-open launcher. No build, install, network or permission changes.
project_root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd) || exit 0
if [ ! -x "$project_root/target/debug/xtask" ]; then
    printf '%s\n' '{"hookSpecificOutput":{"hookEventName":"SessionStart","additionalContext":"agent-router-harness:UNKNOWN-missing-binary. Build with cargo build -p xtask --locked --target-dir target; then run the parity check. Host permissions remain authoritative."}}'
    exit 0
fi
"$project_root/target/debug/xtask" --root "$project_root" hook "$1" || exit 0
