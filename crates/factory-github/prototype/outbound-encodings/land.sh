#!/bin/bash
# Probe P-15, fourth part: put the probe's commits and pull requests on main,
# where a closing keyword acts. Run observe.sh after each half.
#   commits  fast-forward main to the head of probe/p-15-commits, as the operator
#   pulls    merge the fourteen pull requests with a merge commit, as the operator
# Usage: land.sh <results-dir> commits|pulls
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
. "$HERE/lib.sh"

OUT=$1
mkdir -p "$OUT/land"

case $2 in
commits)
  HEAD=$(gh api "repos/$REPO/git/ref/heads/probe/p-15-commits" --jq .object.sha)
  echo "commits-head=$HEAD" >> "$OUT/00-inputs.txt"
  W=$(mktemp -d)
  git clone --quiet "https://github.com/$REPO.git" "$W/s"
  git -C "$W/s" push origin "${HEAD}:refs/heads/main"
  rm -rf "$W"
  date -u +landed-at=%Y-%m-%dT%H:%M:%SZ >> "$OUT/00-inputs.txt"
  ;;
pulls)
  for kind in prc pre; do
    for e in $ENCODINGS; do
      rest "$OUT/land/merge-$kind-$e.txt" PUT "pulls/$(id "$OUT" "pr-$kind-$e")/merge" -f merge_method=merge
      sleep 2
    done
  done
  date -u +merged-at=%Y-%m-%dT%H:%M:%SZ >> "$OUT/00-inputs.txt"
  ;;
esac
