# Helpers for the ruleset probe of #87. Every call is logged with its status.
R=repos/sapientpants/sf-scratch
LOG="${LOG:-$(dirname "$0")/../results/ruleset-responses.log}"
# call <label> <gh api arguments...>: logs the call, the status line and the body.
call() {
  label="$1"; shift
  printf '\n=== %s\n$ gh api %s\n' "$label" "$*" >> "$LOG"
  out=$(gh api -i "$@" 2>&1); code=$?
  printf '%s\n' "$out" | awk 'NR==1 || body {print} /^\r?$/ {body=1}' >> "$LOG"
  printf '(gh exit %s)\n' "$code" >> "$LOG"
  printf '%s\n' "$out" | awk 'body {print} /^\r?$/ {body=1}'
}
