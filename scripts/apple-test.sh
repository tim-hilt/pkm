#!/usr/bin/env bash
# Usage: scripts/apple-test.sh <xcodebuild args>, e.g. -destination ... -only-testing:...
# Runs the SCHEME (default pkm) scheme's tests and prints only compile errors and test
# output; the full log stays in $LOG. A test that runs over a minute fails (a hang
# would otherwise stall the run).
set -uo pipefail
LOG=${LOG:-/tmp/pkm-test.log}

cd "$(dirname "$0")/../apple" || exit 1
xcodebuild -allowProvisioningUpdates -scheme "${SCHEME:-pkm}" test -collect-test-diagnostics never \
  -test-timeouts-enabled YES -default-test-execution-time-allowance 60 "$@" >"$LOG" 2>&1
status=$?

awk -v zwsp="$(printf '\342\200\213')" '
    index($0, zwsp) == 1 { $0 = substr($0, length(zwsp) + 1) }
    /^[0-9-]+ [0-9:.+]+ [A-Za-z]+\[[0-9]+:[0-9]+\]/ { next }
    /Test run started/ { testing = 1; next }
    /^✔ Test run / { print; next }
    /^(◇ |✔ (Test|Suite) |↳ (Testing Library|Target Platform))/ { next }
    /^Test session results/ { testing = 0 }
    /exceeded execution time allowance/ ||
    testing || /(^|: )error: / || /^\*\* .* \*\*$/ { print }
' "$LOG"
[ $status -eq 0 ] || echo "Full log: $LOG"
exit $status
