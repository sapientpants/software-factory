# Shared by the scripts of probe P-15. Sourced, not run.
# Everything here talks to the scratch repository only.

OWNER=sapientpants
NAME=sf-scratch
REPO=$OWNER/$NAME
V=${V:-2026-03-10}                # REST API version (GH-17 pins 2026-03-10)
ENCODINGS="plain fence code backslash zwj zwsp entity"

# keep: cut response headers down to status, date, API version and request id.
# The rest names the scopes of the token and its rate limit.
keep() {
  awk 'BEGIN { h = 1 }
       h && /^\r?$/ { h = 0; print ""; next }
       h { l = tolower($0)
           if (/^HTTP\// || l ~ /^date:/ || l ~ /^x-github-api-version-selected:/ || l ~ /^x-github-request-id:/) print
           next }
       { print }' | tr -d '\r'
}

# rest <out-file> <method> <path> [gh api field arguments]: one REST call, response kept
rest() {
  out=$1; method=$2; path=$3; shift 3
  gh api -i --method "$method" "repos/$REPO/$path" \
    -H "X-GitHub-Api-Version: $V" "$@" 2>&1 | keep > "$out"
  printf '%s\t%s %s\t%s\n' "$(basename "$out")" "$method" "$path" "$(head -1 "$out")"
}

# field <response-file> <jq-filter>: a field of the body of a kept response
field() { sed '1,/^$/d' "$1" | jq -r "$2"; }

# id <results-dir> <name>: the issue or pull request number recorded under a name
id() { awk -F'\t' -v k="$2" '$1 == k { print $2 }' "$1/ids.tsv"; }
