#!/bin/sh
# Router value pilot launcher. Real model calls: launch it outside the Claude
# Code sandbox (Codex cannot write ~/.codex inside it).
#   ./run-pilot.sh calibrate   selfcheck + init (seals the manifest) + one calibration block, then stops
#   ./run-pilot.sh run --budget-cap-usd 'claude=USD,codex=USD'   remaining runs, resumable; no default cap (or PILOT_BUDGET_CAP_USD)
#   ./run-pilot.sh report|status
# Add --fake to any command to use the synthetic hosts (no tokens, results-fake/).
set -eu
cd "$(dirname "$0")"
PATH="$PILOT_HOME/.nvm/versions/node/v24.16.0/bin:$PILOT_HOME/.local/bin:/usr/bin:/bin:/usr/sbin:/sbin"
export PATH
PY=$PILOT_HOME/.pyenv/versions/3.13.0/bin/python3
command="${1:-status}"
[ "$#" -gt 0 ] && shift
case "$command" in
  calibrate)
    "$PY" run_pilot.py selfcheck "$@"
    "$PY" run_pilot.py init "$@"
    "$PY" run_pilot.py calibrate "$@"
    ;;
  *)
    "$PY" run_pilot.py "$command" "$@"
    ;;
esac
