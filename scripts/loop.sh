#!/usr/bin/env bash
# Usage: scripts/loop.sh <project-directory>
#
# Implements the ready-for-agent tasks in <project-directory>/issues/ one pi session
# each, in file order and after their blockers, then reviews them all against
# <project-directory>/spec.md.

set -uo pipefail

source "$(dirname "$0")/lib/loop-common.sh"

directory=${1:-} # kept as given, so paths stay relative to the cwd
init_loop "$directory"
tasks_dir="$directory/issues"
spec_file="$directory/spec.md"
review_session="review-$name"

shopt -s nullglob
tasks=("$tasks_dir"/*.md) # glob expansion is sorted
shopt -u nullglob

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

implemented=0
attempted=" " # tasks already run: one whose Status the agent did not update runs only once

# Passes repeat until one implements nothing, so a task blocked by a
# later-numbered task runs in the pass after its blocker.
progress=1
while ((progress)); do
  progress=0
  for task_file in ${tasks[@]+"${tasks[@]}"}; do
    # Re-read the Status every time; earlier implementations may change later tasks.
    has_status "$task_file" 'ready-for-agent' || continue
    [[ $attempted == *" $task_file "* ]] && continue
    [[ -z $(pending_blockers "$task_file") ]] || continue
    attempted+="$task_file "

    run_pi "Implementing $task_file" claude-sonnet-5-5 medium "loop-$name-$(basename "$task_file" .md)" \
      "/skill:tdd implement the task described in @$task_file. This is a headless run: the seams named in the task and its spec are the confirmed seams. Apply software-design best practices during the implementation. Tick off all completed bullet points in $task_file and set its Status line to in-review when you're finished; edit it in place, since .scratch/ is untracked. Commit the implementation using the commit-message 'Implement $task_file'."
    log_section "$task_file implemented in $status_elapsed" "$pi_output"
    gate_on_tests "$task_file"

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

# An interrupted review is finished even when there is nothing new to review.
if ((implemented == 0)) && [[ $resume_session != "$review_session" ]]; then
  echo "No ready-for-agent tasks found."
  exit 0
fi

run_pi "Reviewing $spec_file" claude-opus-5-5 high "$review_session" \
  "/skill:software-design the last commits implemented the tickets in @$tasks_dir. Please review these commits according to @$spec_file. Also consider @$log_file to understand how we got to the current state. Pay special attention to correctness and simplicity. Fix found issues right away. If an issue is fully implemented, then please set its Status line to 'done'. Commit your changes using the commit-message 'Reviewed $spec_file'."
log_section "$spec_file reviewed in $status_elapsed" "$pi_output"

if [[ -n $resume_session ]]; then
  printf 'Interrupted session %s was not resumed; its task is no longer ready-for-agent. Open it with: pi --session-id %s\n' \
    "$resume_session" "$resume_session" >&2
fi
