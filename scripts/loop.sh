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

implemented=0

for task_file in ${tasks[@]+"${tasks[@]}"}; do
  # Check the current `**Status:**` line only; earlier implementations may change later tasks.
  grep -Eiq $'^\\*{0,2}Status:\\*{0,2}[ \t]*ready-for-agent[ \t]*\r?$' "$task_file" || continue

  status_set "Implementing $task_file"

  run_pi anthropic claude-opus-5-5 low \
    "/skill:tdd implement the task described in @$task_file. This is a headless run: the seams named in the task and its spec are the confirmed seams. Apply software-design best practices during the implementation. Tick off all completed bullet points in $task_file and set its Status line to in-review when you're finished; edit it in place, since .scratch/ is untracked. Commit the implementation using the commit-message 'Implement $task_file'."

  implemented=$((implemented + 1))
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
done

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
