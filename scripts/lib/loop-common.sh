# Shared helpers for scripts/loop.sh and scripts/review-loop.sh; source, don't run.
# The sourcing script must set log_file before calling log_section, and log_file and
# state_file before calling run_pi.

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

# --- Claude subscription quota -----------------------------------------------
# A Claude subscription (Pro/Max) answers 429 once its rolling 5-hour window, or its
# weekly window, is spent, and pi's error carries no reset time. The windows are
# readable from the OAuth usage endpoint, whose resets_at is the authoritative refresh
# time; that is what quota_wait sleeps until.

PI_BIN=${PI_BIN:-pi}
CURL_BIN=${CURL_BIN:-curl}
CLAUDE_USAGE_URL=${CLAUDE_USAGE_URL:-https://api.anthropic.com/api/oauth/usage}
QUOTA_POLL_SECONDS=${QUOTA_POLL_SECONDS:-900}       # usage endpoint itself unavailable
QUOTA_THROTTLE_SECONDS=${QUOTA_THROTTLE_SECONDS:-300} # 429 while windows have headroom
QUOTA_CUSHION_SECONDS=${QUOTA_CUSHION_SECONDS:-90}    # slack past resets_at
QUOTA_RECHECK_SECONDS=${QUOTA_RECHECK_SECONDS:-60}    # floor for how often it re-probes

resume_session="" # session an earlier run left unfinished; see load_interrupted_session
state_file=${state_file:-}

# The subscription usage payload: five_hour/seven_day/limits, each with a utilization
# and a resets_at. Fails when the endpoint or the OAuth token is unavailable.
claude_usage() {
  local token
  token=$("$PI_BIN" auth print-bearer-token --provider anthropic 2>/dev/null) || return 1
  [[ -n $token ]] || return 1
  "$CURL_BIN" -fsS --max-time 20 "$CLAUDE_USAGE_URL" \
    -H "Authorization: Bearer $token" \
    -H "anthropic-beta: oauth-2025-04-20" \
    -H "User-Agent: claude-code/2.0.32" \
    -H "Content-Type: application/json"
}

# Classifies the usage payload in $1: prints "five-hour <epoch>" or "weekly <epoch>"
# for an exhausted window (the latest of its resets), or "throttled" while every
# window still has headroom.
quota_state() {
  python3 - "$1" <<'PY'
import datetime, json, sys

SATURATED = 99.0  # utilization and percent are percentages; allow rounding slack
WEEKLY = ("seven_day", "week")

def epoch(value):
    if value is None:
        return None
    if isinstance(value, (int, float)):
        return int(value)
    try:
        return int(float(value))
    except ValueError:
        return int(datetime.datetime.fromisoformat(str(value).replace("Z", "+00:00")).timestamp())

try:
    usage = json.loads(sys.argv[1])
except ValueError:  # an empty or non-JSON answer; the caller decides what to do
    raise SystemExit(1)

windows = []  # (scope, percent, resets_at)
for name in ("five_hour", "seven_day", "seven_day_opus", "seven_day_sonnet",
             "seven_day_oauth_apps", "seven_day_cowork"):
    window = usage.get(name)
    if isinstance(window, dict) and window.get("utilization") is not None:
        windows.append((name, float(window["utilization"]), window.get("resets_at")))
for limit in usage.get("limits") or []:  # the same windows under their reporting names
    if isinstance(limit, dict) and limit.get("percent") is not None:
        windows.append((str(limit.get("kind") or ""), float(limit["percent"]), limit.get("resets_at")))

exhausted = [(scope, epoch(reset)) for scope, percent, reset in windows if percent >= SATURATED]
if not exhausted:
    print("throttled")
    raise SystemExit
weekly = [reset for scope, reset in exhausted if reset and any(word in scope for word in WEEKLY)]
if weekly:
    print("weekly", max(weekly))
else:
    resets = [reset for _, reset in exhausted if reset]
    print("five-hour", max(resets) if resets else "")
PY
}

# Sleeps until local epoch $1 in short chunks, so Ctrl-C and a suspended laptop behave.
sleep_until() {
  local remaining
  while :; do
    remaining=$(($1 - $(date +%s)))
    ((remaining > 0)) || return 0
    sleep $((remaining < 300 ? remaining : 300))
  done
}

# Formats epoch $1 in the local timezone.
human_time() {
  date -r "$1" '+%Y-%m-%d %H:%M' 2>/dev/null ||
    date -d "@$1" '+%Y-%m-%d %H:%M' 2>/dev/null ||
    printf '@%s' "$1"
}

# Blocks until the subscription has room again: sleeps out an exhausted 5-hour window,
# stops the loop when the weekly window is the exhausted one (waiting days is not this
# loop's job), and retries a plain throttle after a short pause.
quota_wait() {
  local payload state reset until probe_at now waited=0 waited_until=""
  while :; do
    if ! payload=$(claude_usage); then
      ((waited)) && return 0 # no usage endpoint: retry the session rather than block forever
      status_set "Claude quota unavailable; retrying in $((QUOTA_POLL_SECONDS / 60))m"
      sleep "$QUOTA_POLL_SECONDS"
      status_stop
      waited=1
      continue
    fi
    state=$(quota_state "$payload") || state=throttled

    case $state in
      five-hour*)
        reset=${state#five-hour }
        if [[ -n $reset ]]; then
          # A resets_at that has already passed would otherwise be probed in a hot loop.
          now=$(date +%s)
          probe_at=$((reset + QUOTA_CUSHION_SECONDS))
          ((probe_at < now + QUOTA_RECHECK_SECONDS)) && probe_at=$((now + QUOTA_RECHECK_SECONDS))
          until=$(human_time "$probe_at")
          if [[ $until != "$waited_until" ]]; then # log each target once, not each probe
            log_section "Claude 5-hour window exhausted; waiting until $until" "$payload"
            status_set "Claude 5-hour window exhausted; waiting until $until"
            waited_until=$until
          fi
          sleep_until "$probe_at"
          waited=1
          continue
        fi
        ;;
      weekly*)
        reset=${state#weekly }
        status_stop
        log_section "Claude weekly window exhausted until $(human_time "$reset"); stopping" "$payload"
        printf 'Claude weekly limit exhausted until %s; re-run the loop after that (usage in %s)\n' \
          "$(human_time "$reset")" "$log_file" >&2
        exit 1
        ;;
    esac

    ((waited)) && return 0 # the window has reset: let the caller retry right away
    log_section "Claude throttled with headroom left; retrying in $((QUOTA_THROTTLE_SECONDS / 60))m" "$payload"
    status_set "Claude throttled; retrying in $((QUOTA_THROTTLE_SECONDS / 60))m"
    sleep_until $(($(date +%s) + QUOTA_THROTTLE_SECONDS))
    waited=1
  done
}

# Waits before a task starts when the subscription has no headroom left, so a session is
# not spent on an immediate 429. A failed probe changes nothing; pi then finds out.
quota_await_headroom() {
  local state
  state=$(quota_state "$(claude_usage 2>/dev/null)" 2>/dev/null) || return 0
  case $state in
    five-hour* | weekly*) quota_wait ;;
  esac
}

# Whether pi's output in $1 reports a Claude rate limit, which a window reset clears.
rate_limited() {
  grep -qE '^429 |rate_limit_error' <<<"$1"
}

# Prints $1 with the extra words in $2 and later as one session id: lowercase
# alphanumerics, dots, dashes and underscores, trimmed to start and end alphanumeric.
session_name() {
  local words=$* name
  name=$(tr '[:upper:]' '[:lower:]' <<<"$words" | sed -E 's/[^a-z0-9._-]+/-/g; s/^[^a-z0-9]+//; s/[^a-z0-9]+$//')
  printf '%s' "$name"
}

# Picks up the session an interrupted earlier run left behind; run_pi continues it
# with "continue" instead of starting it over.
load_interrupted_session() {
  resume_session=$(cat "$state_file" 2>/dev/null)
  [[ -n $resume_session ]] || return 0
  echo "Continuing interrupted session $resume_session"
}

# Usage: run_pi <provider> <model> <thinking> <prompt> <session-id>
# Runs pi headlessly in that session, so an interrupted run resumes where it stopped.
# Waits out the subscription's rate limits and continues the same session afterwards.
# Sets pi_output to the combined pi output; exits on failure.
run_pi() {
  local provider=$1 model=$2 thinking=$3 prompt=$4 session=$5 code

  if [[ $session == "$resume_session" ]]; then
    prompt="continue" # the earlier run stopped mid-task; let pi pick the thread up
    resume_session=""
  fi

  while :; do
    printf '%s\n' "$session" >"$state_file"
    pi_output=$("$PI_BIN" -p "$prompt" --session-id "$session" \
      --provider "$provider" --model "$model" --thinking "$thinking" </dev/null 2>&1)
    code=$?
    if ((code == 0)); then
      rm -f "$state_file"
      return 0
    fi
    if ! rate_limited "$pi_output"; then
      status_stop
      printf 'pi exited with status %s\n\n%s\n' "$code" "$pi_output" >&2
      exit 1
    fi
    status_stop
    log_section "pi hit a Claude rate limit (status $code)" "$pi_output"
    quota_wait
    prompt="continue"
  done
}

# Appends a titled section to the log file.
log_section() {
  printf '%s\n\n%s\n\n---\n\n' "$1" "$2" >>"$log_file"
}
