#!/bin/sh
# Repetition-filtered launcher over the sealed manifest (see run_r1.py). Real model
# calls: launch it outside the Claude Code sandbox.
#   ./run-pilot-r1.sh plan --repetitions 1
#   ./run-pilot-r1.sh run --repetitions 1 --budget-cap-usd 'claude=USD,codex=USD'
set -eu
cd "$(dirname "$0")"
PATH="$PILOT_HOME/.nvm/versions/node/v24.16.0/bin:$PILOT_HOME/.local/bin:/usr/bin:/bin:/usr/sbin:/sbin"
export PATH
exec $PILOT_HOME/.pyenv/versions/3.13.0/bin/python3 run_r1.py "$@"
