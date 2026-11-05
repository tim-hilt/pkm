#!/usr/bin/env bash
# Usage: scripts/loop.sh <project-directory>

set -uo pipefail

source "$(dirname "$0")/lib/loop-common.sh"

directory=${1:-} # kept as given, so paths stay relative to the cwd
if [[ -z $directory ]]; then
  echo "Usage: scripts/loop.sh <project-directory>" >&2
  exit 2
fi

tasks_dir="$directory/issues"
if [[ ! -d $tasks_dir ]]; then
  echo "No such directory: $tasks_dir" >&2
  exit 1
fi

# Every agent commit goes through the pre-commit guardrail (make lint + Rust tests).
git config core.hooksPath .githooks

mkdir -p logs
log_file="logs/$(basename "$(cd "$directory" && pwd)").loop.log"

shopt -s nullglob dotglob
tasks=()
for path in "$tasks_dir"/*.md; do # glob expansion is sorted
  [[ -f $path ]] && tasks+=("$path")
done
shopt -u nullglob dotglob

# Whether the current `**Status:**` line of $1 is one of the statuses in the regex $2.
has_status() {
  grep -Eiq $'^\\*{0,2}Status:\\*{0,2}[ \t]*('"$2"$')[ \t]*\r?$' "$1"
}

# The ticket numbers on the `**Blocked by:**` line of $1 that are not yet
# implemented (Status in-review or done), space-separated; empty if none.
pending_blockers() {
  local number blocker pending=""
  for number in $(grep -Ei '^\*{0,2}Blocked by:' "$1" | grep -oE '\b[0-9]{2}\b'); do
    blocker=$(compgen -G "$tasks_dir/$number-*.md" | head -n 1)
    if [[ -z $blocker ]] || ! has_status "$blocker" 'in-review|done'; then
      pending+="$number "
    fi
  done
  echo "${pending% }"
}

# Implements $1 with an agent, then gates on the Apple tests.
implement() {
  local task_file=$1
  status_set "Implementing $task_file"

  run_pi anthropic claude-sonnet-5-5 medium \
    "/skill:tdd implement the task described in @$task_file. This is a headless run: the seams named in the task and its spec are the confirmed seams. Apply software-design best practices during the implementation. Tick off all completed bullet points in $task_file and set its Status line to in-review when you're finished; edit it in place, since .scratch/ is untracked. Commit the implementation using the commit-message 'Implement $task_file'."

  status_stop
  log_section "$task_file implemented in $status_elapsed" "$pi_output"

  # Guardrail: the pre-commit hook skips the Apple tests, so gate on them here.
  status_set "Testing after $task_file"
  if ! test_output=$(make test 2>&1); then
    status_stop
    log_section "make test failed after $task_file" "$test_output"
    printf 'make test failed after %s; stopping (see %s)\n\n%s\n' "$task_file" "$log_file" "$(tail -n 40 <<<"$test_output")" >&2
    exit 1
  fi
  status_stop
}

implemented=0
attempted=" " # tasks already run: one whose Status the agent did not update runs only once

# Passes repeat until one implements nothing, so a task blocked by a
# later-numbered task runs in the pass after its blocker.
progress=1
while ((progress)); do
  progress=0
  for task_file in ${tasks[@]+"${tasks[@]}"}; do
    # Check the current `**Status:**` line only; earlier implementations may change later tasks.
    has_status "$task_file" 'ready-for-agent' || continue
    [[ $attempted == *" $task_file "* ]] && continue
    [[ -z $(pending_blockers "$task_file") ]] || continue
    attempted+="$task_file "
    implement "$task_file"
    implemented=$((implemented + 1))
    progress=1
  done
done

still_blocked=""
for task_file in ${tasks[@]+"${tasks[@]}"}; do
  has_status "$task_file" 'ready-for-agent' || continue
  [[ $attempted == *" $task_file "* ]] && continue
  still_blocked+="$task_file (waits for $(pending_blockers "$task_file"))"$'\n'
done
if [[ -n $still_blocked ]]; then
  log_section "Still blocked, not implemented" "$still_blocked"
  printf 'Still blocked, not implemented:\n%s' "$still_blocked" >&2
fi

if ((implemented == 0)); then
  echo "No ready-for-agent tasks found."
  exit 0
fi

spec_file="$directory/spec.md"

status_set "Reviewing $spec_file"

run_pi anthropic claude-opus-5-5 high \
  "/skill:software-design the last commits implemented the tickets in @$tasks_dir. Please review these commits according to @$spec_file. Also consider @$log_file to understand how we got to the current state. Pay special attention to correctness and simplicity. Fix found issues right away. If an issue is fully implemented, then please set its Status line to 'done'. Commit your changes using the commit-message 'Reviewed $spec_file'."

status_stop
log_section "$spec_file reviewed in $status_elapsed" "$pi_output"
