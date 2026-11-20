#!/bin/sh
# Probe P-1, third part: create factory/issue-9001 while a branch named
# `factory` exists. These commands were typed, not run from this file; it
# holds them as they were sent. Run after the refs of probe.sh and race.sh
# are deleted. Usage: collide.sh <output-dir>
set -u

REPO=sapientpants/sf-scratch
OUT=$1
MAIN=$(gh api "repos/$REPO/git/ref/heads/main" --jq .object.sha)
mkdir -p "$OUT"

gh api -i --method POST "repos/$REPO/git/refs" -H "X-GitHub-Api-Version: 2026-03-10" \
  -f ref=refs/heads/factory -f sha="$MAIN" > "$OUT/d1-create-factory.txt" 2>&1

for v in 2026-03-10 2022-11-28; do
  gh api -i --method POST "repos/$REPO/git/refs" -H "X-GitHub-Api-Version: $v" \
    -f ref=refs/heads/factory/issue-9001 -f sha="$MAIN" > "$OUT/d2-below-factory-$v.txt" 2>&1
  gh api -i "repos/$REPO/git/ref/heads/factory/issue-9001" -H "X-GitHub-Api-Version: $v" \
    > "$OUT/d3-read-$v.txt" 2>&1
done

gh api -i --method DELETE "repos/$REPO/git/refs/heads/factory" > "$OUT/d4-delete.txt" 2>&1
