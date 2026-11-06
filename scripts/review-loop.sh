#!/usr/bin/env bash
# Usage: scripts/review-loop.sh <review-directory>
#
# Works through the review tickets in <review-directory>/issues/ (e.g.
# .scratch/v1-review/issues/) one pi session each, in file order. Each ticket
# lists the implementation issues it covers under `## Reviews`; the matching
# sections of the implementation loop's log are cut out into a small context
# file so a session doesn't have to read the whole log.

set -uo pipefail

source "$(dirname "$0")/lib/loop-common.sh"

directory=${1:-} # kept as given, so paths stay relative to the cwd
if [[ -z $directory ]]; then
  echo "Usage: scripts/review-loop.sh <review-directory>" >&2
  exit 2
fi

tickets_dir="$directory/issues"
plan_file="$directory/spec.md"
for required in "$tickets_dir" "$plan_file"; do
  if [[ ! -e $required ]]; then
    echo "No such file or directory: $required" >&2
    exit 1
  fi
done

# Every agent commit goes through the pre-commit guardrail (make lint + Rust tests).
git config core.hooksPath .githooks

review_name=$(basename "$(cd "$directory" && pwd)")
mkdir -p "logs/$review_name"
log_file="logs/$review_name.loop.log"
state_file="${log_file%.log}.state"
load_interrupted_session

status_pattern() { printf '^\\*{0,2}Status:\\*{0,2}[ \t]*%s[ \t]*\r?$' "$1"; }

# Prints the issue paths listed under the ticket's `## Reviews` heading.
reviewed_issues() {
  awk '/^## /{in_reviews = ($0 ~ /^## Reviews/); next} in_reviews' "$1" |
    grep -oE '\.scratch/[^` ]+/issues/[^` ]+\.md'
}

# Usage: write_context <ticket> <context-file>
# Copies the implementation log sections of the reviewed issues into the context file.
write_context() {
  : >"$2"
  local issue impl_log
  while read -r issue; do
    # .scratch/<project>/issues/NN-x.md was implemented by scripts/loop.sh into logs/<project>.loop.log
    impl_log="logs/$(basename "$(dirname "$(dirname "$issue")")").loop.log"
    if [[ -f $impl_log ]] && grep -q "^$issue implemented in " "$impl_log"; then
      awk -v issue="$issue" '
        /^(\.scratch\/[^ ]+\.md implemented in |make test failed after |[^ ]+ reviewed in )/ {
          printing = (index($0, issue " implemented in ") == 1)
        }
        printing
      ' "$impl_log" >>"$2"
    else
      printf '%s\n\nNo implementation log section found; rely on the issue file and git history.\n\n---\n\n' "$issue" >>"$2"
    fi
  done < <(reviewed_issues "$1")
  if [[ ! -s $2 ]]; then
    printf 'This ticket lists no single issues. The earlier review sessions are logged in %s; grep it rather than reading it whole.\n' "$log_file" >"$2"
  fi
}

shopt -s nullglob
tickets=("$tickets_dir"/*.md) # glob expansion is sorted
shopt -u nullglob

reviewed=0

for ticket in ${tickets[@]+"${tickets[@]}"}; do
  grep -Eiq "$(status_pattern ready-for-agent)" "$ticket" || continue

  context_file="logs/$review_name/$(basename "$ticket" .md).context.log"
  write_context "$ticket" "$context_file"

  quota_await_headroom
  status_set "Reviewing $ticket"

  run_pi anthropic claude-opus-5-5 medium \
    "/skill:software-design review the implementation described in @$ticket, following the review procedure in @$plan_file. The implementation notes of the reviewed issues are in @$context_file. This is a headless run: don't ask questions; record open decisions in the ticket instead. Pay special attention to correctness and simplicity, and **fix found issues right away**. Update the ticket and the reviewed issue files in place (.scratch/ is untracked), set the ticket's Status line to done when you're finished, and commit your changes using the commit-message 'Review $ticket'." \
    "$(session_name review "$review_name" "$(basename "$ticket" .md)")"

  reviewed=$((reviewed + 1))
  status_stop
  log_section "$ticket reviewed in $status_elapsed" "$pi_output"

  # A ticket still waiting for an agent means the session gave up; don't pile more reviews on top.
  if grep -Eiq "$(status_pattern ready-for-agent)" "$ticket"; then
    printf '%s is still ready-for-agent after its session; stopping (see %s)\n' "$ticket" "$log_file" >&2
    exit 1
  fi

  # Guardrail: the pre-commit hook skips the Apple tests, so gate on them here.
  status_set "Testing after $ticket"
  if ! test_output=$(make test 2>&1); then
    status_stop
    log_section "make test failed after $ticket" "$test_output"
    printf 'make test failed after %s; stopping (see %s)\n\n%s\n' "$ticket" "$log_file" "$(tail -n 40 <<<"$test_output")" >&2
    exit 1
  fi
  status_stop
done

if ((reviewed == 0)); then
  echo "No ready-for-agent review tickets found."
fi

if [[ -n $resume_session ]]; then
  printf 'Interrupted session %s was not resumed; its ticket is no longer ready-for-agent. Open it with: pi --session-id %s\n' \
    "$resume_session" "$resume_session" >&2
fi
