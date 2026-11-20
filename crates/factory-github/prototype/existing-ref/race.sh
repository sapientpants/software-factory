#!/bin/sh
# Probe P-1, second part: two creates of one new ref sent at once by one curl
# process, each on its own connection, and the reads around a name conflict.
# Usage: race.sh <output-dir>
set -u

REPO=sapientpants/sf-scratch
OUT=$1
N=${2:-7001}
V=2026-03-10
API=https://api.github.com/repos/$REPO
mkdir -p "$OUT"

MAIN=$(gh api "repos/$REPO/git/ref/heads/main" --jq .object.sha)
OTHER=$(gh api "repos/$REPO/git/ref/heads/probe/p-16" --jq .object.sha)

# the credential goes to curl in a config file, never on a command line
CFG=$OUT/.curl-headers
( umask 077; printf 'header = "Authorization: Bearer %s"\nheader = "Accept: application/vnd.github+json"\nheader = "X-GitHub-Api-Version: %s"\n' "$(gh auth token)" "$V" > "$CFG" )
trap 'rm -f "$CFG"' EXIT

W='%{http_code} start=%{time_starttransfer} total=%{time_total} conn=%{num_connects}\n'
i=0
while [ $i -lt 20 ]; do
  ref="refs/heads/factory/issue-$((N+i))"
  if [ $((i % 2)) -eq 0 ]; then second=$MAIN; else second=$OTHER; fi
  curl -sS --http2 --parallel --parallel-immediate --parallel-max 2 \
    -K "$CFG" -o "$OUT/t$i-x.json" -w "x $W" -d "{\"ref\":\"$ref\",\"sha\":\"$MAIN\"}" "$API/git/refs" \
    --next \
    -K "$CFG" -o "$OUT/t$i-y.json" -w "y $W" -d "{\"ref\":\"$ref\",\"sha\":\"$second\"}" "$API/git/refs" \
    > "$OUT/t$i-status.txt" 2>&1
  printf 'round %s (%s): %s | %s | x: %s | y: %s\n' "$i" \
    "$([ "$second" = "$MAIN" ] && echo same || echo different)" \
    "$(sed -n 1p "$OUT/t$i-status.txt")" "$(sed -n 2p "$OUT/t$i-status.txt")" \
    "$(grep -o '"message":"[^"]*"' "$OUT/t$i-x.json" || echo created)" \
    "$(grep -o '"message":"[^"]*"' "$OUT/t$i-y.json" || echo created)"
  i=$((i+1))
done

echo "--- reads around a name conflict (factory/issue-9001 exists)"
for path in git/ref/heads/factory git/ref/heads/factory/issue-9001/sub \
            git/matching-refs/heads/factory/issue-9001 git/matching-refs/heads/factory/issue-900; do
  name=$(echo "$path" | tr '/' '_')
  gh api -i "repos/$REPO/$path" -H "X-GitHub-Api-Version: $V" > "$OUT/read-$name.txt" 2>&1
  printf '%s\t%s\t%s\n' "$path" "$(head -1 "$OUT/read-$name.txt" | tr -d '\r')" \
    "$(awk 'BEGIN{b=0} /^\r?$/{b=1; next} b' "$OUT/read-$name.txt" | tr -d '\n' | grep -o '"ref":"[^"]*"\|"message":"[^"]*"' | tr '\n' ' ')"
done
