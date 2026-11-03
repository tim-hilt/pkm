# Shared helpers for scripts/loop.sh and scripts/review-loop.sh; source, don't run.
# The sourcing script must set log_file before calling log_section.

FRAMES=("⠋" "⠙" "⠹" "⠸" "⠼" "⠴" "⠦" "⠧" "⠇" "⠏")

format_elapsed() {
  local total=$1 minutes=$(($1 / 60)) seconds=$(($1 % 60))
  if ((minutes > 0)); then echo "${minutes}m ${seconds}s"; else echo "${seconds}s"; fi
}

# Most recent status message, rendered with a spinner and an elapsed timer.
status_message=""
status_start=0
status_active=0
spinner_pid=""

status_set() {
  status_stop >/dev/null
  status_message=$1
  status_start=$(date +%s)
  status_active=1
  [[ -t 1 ]] || return 0

  (
    frame=0
    while :; do
      printf '\r\033[K%s %s (%s)' "${FRAMES[frame++ % ${#FRAMES[@]}]}" "$status_message" \
        "$(format_elapsed $(($(date +%s) - status_start)))"
      sleep 0.1
    done
  ) &
  spinner_pid=$!
}

# Replaces the live line with a final line without spinner.
# Sets status_elapsed to the final elapsed time; no-op if no status is active.
status_stop() {
  ((status_active)) || return 0
  status_active=0
  if [[ -n $spinner_pid ]]; then
    kill "$spinner_pid" 2>/dev/null
    wait "$spinner_pid" 2>/dev/null
    spinner_pid=""
  fi
  status_elapsed=$(format_elapsed $(($(date +%s) - status_start)))
  [[ -t 1 ]] && printf '\r\033[K'
  echo "$status_message ($status_elapsed)"
}

trap '[[ -n $spinner_pid ]] && kill "$spinner_pid" 2>/dev/null' EXIT

# Usage: run_pi <provider> <model> <thinking> <prompt>
# Sets pi_output to the combined pi output; exits on failure.
run_pi() {
  pi_output=$(pi -p "$4" --provider "$1" --model "$2" --thinking "$3" </dev/null 2>&1)
  local code=$?
  if ((code != 0)); then
    status_stop
    printf 'pi exited with status %s\n\n%s\n' "$code" "$pi_output" >&2
    exit 1
  fi
}

# Appends a titled section to the log file.
log_section() {
  printf '%s\n\n%s\n\n---\n\n' "$1" "$2" >>"$log_file"
}
