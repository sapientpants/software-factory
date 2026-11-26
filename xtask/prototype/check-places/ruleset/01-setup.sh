#!/bin/sh
# Creates the base branch and three head branches, each one commit ahead.
. "$(dirname "$0")/lib.sh"
main=$(gh api $R/git/ref/heads/main --jq .object.sha)
echo "main=$main" >> "$LOG"
call "create base" -X POST $R/git/refs -f ref=refs/heads/probe/p-38-base -f sha=$main >/dev/null
for n in a b c; do
  call "create head $n" -X POST $R/git/refs -f ref=refs/heads/probe/p-38-$n -f sha=$main >/dev/null
  call "commit on head $n" -X PUT $R/contents/probe-p-38/$n.txt -f message="P-38: head $n" -f content="$(printf 'head %s\n' $n | base64)" -f branch=probe/p-38-$n --jq .commit.sha
done
